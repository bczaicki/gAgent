use crate::{
    notification::{NotificationManager, RalphNotification},
    plan::{ImplementationPlan, TaskStatus},
};
use gagent_core::{Config, GagentError, Result, SystemPrompt};
use gagent_harness::{AgentHarness, Session};
use gagent_llm::LlmProvider;
use gagent_tools::ToolRegistry;
use std::path::PathBuf;
use tokio::fs;

/// Configuration for the RALPH loop.
#[derive(Debug, Clone)]
pub struct RalphConfig {
    /// Maximum number of iterations in the BUILDING phase.
    pub max_iterations: usize,

    /// Whether to wait for external acknowledgment after each iteration.
    pub backpressure: bool,

    /// Path to the .ralph directory.
    pub ralph_dir: PathBuf,

    /// Path to the specification file (for PLANNING phase).
    pub spec_path: Option<PathBuf>,

    /// Path to the implementation plan.
    pub plan_path: PathBuf,
}

impl Default for RalphConfig {
    fn default() -> Self {
        Self {
            max_iterations: 50,
            backpressure: false,
            ralph_dir: PathBuf::from(".ralph"),
            spec_path: None,
            plan_path: PathBuf::from("IMPLEMENTATION_PLAN.md"),
        }
    }
}

/// The RALPH loop orchestrates a two-phase development process:
/// 1. PLANNING: Generate an implementation plan from a specification
/// 2. BUILDING: Iteratively implement tasks from the plan
pub struct RalphLoop {
    config: Config,
    ralph_config: RalphConfig,
    notification_manager: NotificationManager,
}

impl RalphLoop {
    /// Create a new RALPH loop.
    pub fn new(config: Config, ralph_config: RalphConfig) -> Self {
        let notification_manager = NotificationManager::new(&ralph_config.ralph_dir);

        Self {
            config,
            ralph_config,
            notification_manager,
        }
    }

    /// Run the PLANNING phase.
    ///
    /// Takes a specification file, generates an IMPLEMENTATION_PLAN.md,
    /// and emits PLANNING_COMPLETE notification.
    pub async fn run_planning(
        &self,
        provider: &dyn LlmProvider,
        registry: &ToolRegistry,
        system_prompt: SystemPrompt,
    ) -> Result<()> {
        tracing::info!("Starting PLANNING phase");

        // Load spec file
        let spec_path = self
            .ralph_config
            .spec_path
            .as_ref()
            .ok_or_else(|| GagentError::Other("No spec file provided".to_string()))?;

        let spec_content = fs::read_to_string(spec_path).await?;

        // Create planning session
        let mut session = Session::new();

        // Build planning prompt
        let planning_prompt = format!(
            r#"You are in PLANNING mode. Your task is to analyze the specification below and create a detailed implementation plan.

# Specification

{}

# Instructions

1. Break down the specification into concrete, actionable tasks
2. Order tasks by dependency (what must be done first)
3. Write your plan to `{}` in markdown format
4. Use checkbox syntax for tasks:
   - [ ] Task description
5. Group related tasks under headings if helpful
6. Be specific about file paths, function names, etc.
7. After writing the plan file, respond with: "PLANNING COMPLETE"

Generate the implementation plan now using the FileWrite tool."#,
            spec_content, self.ralph_config.plan_path.display()
        );

        // Run planning loop
        let harness = AgentHarness::new(self.config.clone(), system_prompt);
        let response = harness
            .run(&planning_prompt, &mut session, provider, registry)
            .await?;

        tracing::info!("Planning response: {}", response);

        // Verify plan was created
        if !self.ralph_config.plan_path.exists() {
            return Err(GagentError::Other(
                "Implementation plan was not created".to_string(),
            ));
        }

        // Load plan to get task count
        let plan = ImplementationPlan::load(&self.ralph_config.plan_path).await?;
        let total_tasks = plan.tasks.len();

        // Emit notification
        self.notification_manager
            .emit(RalphNotification::PlanningComplete {
                plan_path: self.ralph_config.plan_path.clone(),
                total_tasks,
            })
            .await?;

        tracing::info!(
            "PLANNING phase complete. {} tasks created.",
            total_tasks
        );

        Ok(())
    }

    /// Run the BUILDING phase.
    ///
    /// Iteratively picks tasks from the plan, implements them,
    /// and marks them complete.
    pub async fn run_building(
        &self,
        provider: &dyn LlmProvider,
        registry: &ToolRegistry,
        system_prompt: SystemPrompt,
    ) -> Result<()> {
        tracing::info!("Starting BUILDING phase");

        // Load plan
        let mut plan = ImplementationPlan::load(&self.ralph_config.plan_path).await?;

        let mut iteration = 0;
        let mut success_count = 0;
        let mut failure_count = 0;

        loop {
            iteration += 1;

            if iteration > self.ralph_config.max_iterations {
                tracing::warn!("Max iterations reached");
                break;
            }

            // Get next pending task
            let next_task = match plan.next_pending_task() {
                Some(task) => task.clone(),
                None => {
                    tracing::info!("No more pending tasks");
                    break;
                }
            };

            tracing::info!(
                "Iteration {}: Starting task {} - {}",
                iteration,
                next_task.id,
                next_task.description
            );

            // Emit iteration started notification
            self.notification_manager
                .emit(RalphNotification::IterationStarted {
                    iteration,
                    task_id: Some(next_task.id.clone()),
                    task_description: Some(next_task.description.clone()),
                })
                .await?;

            // Mark task as in progress
            plan.update_task_status(&next_task.id, TaskStatus::InProgress)?;
            plan.save().await?;

            // Create fresh session for this iteration
            let mut session = Session::new();

            // Build task prompt
            let task_prompt = self.build_task_prompt(&next_task, &plan).await?;

            // Run the task
            let harness = AgentHarness::new(self.config.clone(), system_prompt.clone());
            let result = harness
                .run(&task_prompt, &mut session, provider, registry)
                .await;

            // Handle result
            let (success, message) = match result {
                Ok(response) => {
                    success_count += 1;
                    plan.update_task_status(&next_task.id, TaskStatus::Done)?;
                    (true, response)
                }
                Err(e) => {
                    failure_count += 1;
                    tracing::error!("Task failed: {}", e);
                    plan.update_task_status(&next_task.id, TaskStatus::Skipped)?;
                    (false, format!("Error: {}", e))
                }
            };

            plan.save().await?;

            // Emit iteration complete notification
            self.notification_manager
                .emit(RalphNotification::IterationComplete {
                    iteration,
                    success,
                    message: message.clone(),
                })
                .await?;

            tracing::info!(
                "Iteration {} complete. Success: {}. Message: {}",
                iteration,
                success,
                message
            );

            // Check backpressure
            if self.ralph_config.backpressure {
                self.wait_for_acknowledgment().await?;
            }
        }

        // Emit building complete notification
        self.notification_manager
            .emit(RalphNotification::BuildingComplete {
                total_iterations: iteration,
                success_count,
                failure_count,
            })
            .await?;

        tracing::info!(
            "BUILDING phase complete. {} successes, {} failures across {} iterations.",
            success_count,
            failure_count,
            iteration
        );

        Ok(())
    }

    /// Build the prompt for a single task.
    async fn build_task_prompt(
        &self,
        task: &crate::plan::Task,
        plan: &ImplementationPlan,
    ) -> Result<String> {
        let stats = plan.stats();

        let prompt = format!(
            r#"You are in BUILDING mode. Your task is to implement the following item from the implementation plan:

# Current Task

**Task ID:** {}
**Description:** {}

# Plan Progress

- Total tasks: {}
- Completed: {}
- In progress: 1 (this task)
- Pending: {}

# Instructions

1. Implement this task completely
2. Follow the implementation plan guidelines
3. Use the available tools (FileRead, FileWrite, FileSearch, Shell, Git)
4. Test your implementation if applicable
5. When done, respond with a summary of what you implemented

Begin implementation now."#,
            task.id,
            task.description,
            stats.total,
            stats.done,
            stats.pending - 1 // Subtract 1 because current task is now in progress
        );

        Ok(prompt)
    }

    /// Wait for external acknowledgment (backpressure mode).
    ///
    /// In backpressure mode, the loop pauses after each iteration
    /// and waits for an external signal (e.g., user pressing Enter).
    async fn wait_for_acknowledgment(&self) -> Result<()> {
        tracing::info!("Waiting for acknowledgment (backpressure mode)");

        // In a real implementation, this could:
        // - Wait for a file to be deleted/modified
        // - Wait for user input on stdin
        // - Wait for a signal file
        //
        // For now, we just log and continue.
        // The CLI will handle the actual waiting logic.

        Ok(())
    }

    /// Run both PLANNING and BUILDING phases in sequence.
    pub async fn run_full_cycle(
        &self,
        provider: &dyn LlmProvider,
        registry: &ToolRegistry,
        system_prompt: SystemPrompt,
    ) -> Result<()> {
        // Run planning
        self.run_planning(provider, registry, system_prompt.clone())
            .await?;

        // Run building
        self.run_building(provider, registry, system_prompt).await?;

        Ok(())
    }

    /// Resume building from an existing plan.
    ///
    /// This is useful if the BUILDING phase was interrupted.
    pub async fn resume_building(
        &self,
        provider: &dyn LlmProvider,
        registry: &ToolRegistry,
        system_prompt: SystemPrompt,
    ) -> Result<()> {
        if !self.ralph_config.plan_path.exists() {
            return Err(GagentError::Other(
                "No implementation plan found to resume from".to_string(),
            ));
        }

        self.run_building(provider, registry, system_prompt).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gagent_llm::MockProvider;
    use tempfile::tempdir;
    use tokio::fs;

    #[tokio::test]
    async fn test_ralph_loop_creation() {
        let config = Config::default();
        let ralph_config = RalphConfig::default();
        let _ralph_loop = RalphLoop::new(config, ralph_config);
    }

    #[tokio::test]
    async fn test_build_task_prompt() {
        let dir = tempdir().unwrap();
        let plan_path = dir.path().join("IMPLEMENTATION_PLAN.md");

        let content = r#"
- [ ] First task
- [ ] Second task
- [x] Completed task
"#;

        tokio::fs::write(&plan_path, content).await.expect("Failed to write file");

        let plan: ImplementationPlan = ImplementationPlan::load(&plan_path).await.unwrap();
        let task = plan.tasks[0].clone();

        let config = Config::default();
        let mut ralph_config = RalphConfig::default();
        ralph_config.plan_path = plan_path;

        let ralph_loop = RalphLoop::new(config, ralph_config);
        let prompt = ralph_loop.build_task_prompt(&task, &plan).await.unwrap();

        assert!(prompt.contains("First task"));
        assert!(prompt.contains("Total tasks: 3"));
        assert!(prompt.contains("Completed: 1"));
    }

    #[tokio::test]
    async fn test_resume_building_no_plan() {
        let dir = tempdir().unwrap();
        let config = Config::default();
        let mut ralph_config = RalphConfig::default();
        ralph_config.plan_path = dir.path().join("nonexistent.md");

        let ralph_loop = RalphLoop::new(config, ralph_config);

        let registry = ToolRegistry::new();
        let provider = MockProvider::new(vec![]);
        let system_prompt = SystemPrompt::minimal();

        let result = ralph_loop
            .resume_building(&provider, &registry, system_prompt)
            .await;

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No implementation plan found"));
    }
}
