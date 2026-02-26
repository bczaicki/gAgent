use gagent_core::{GagentError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Status of a task in the implementation plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TaskStatus {
    /// Task is pending.
    Pending,
    /// Task is in progress.
    InProgress,
    /// Task is completed.
    Done,
    /// Task was skipped.
    Skipped,
}

impl TaskStatus {
    /// Parse task status from a checkbox line.
    ///
    /// Examples:
    /// - `- [ ] Task description` → Pending
    /// - `- [x] Task description` → Done
    /// - `- [~] Task description` → InProgress
    /// - `- [-] Task description` → Skipped
    pub fn from_checkbox(checkbox: &str) -> Self {
        match checkbox.trim() {
            "x" | "X" => TaskStatus::Done,
            "~" => TaskStatus::InProgress,
            "-" => TaskStatus::Skipped,
            _ => TaskStatus::Pending,
        }
    }

    /// Convert task status to checkbox format.
    pub fn to_checkbox(&self) -> &str {
        match self {
            TaskStatus::Pending => " ",
            TaskStatus::InProgress => "~",
            TaskStatus::Done => "x",
            TaskStatus::Skipped => "-",
        }
    }
}

/// A single task in the implementation plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    /// Original line number in the file (for updating).
    pub line_number: usize,
}

/// Implementation plan parser and updater.
pub struct ImplementationPlan {
    pub path: PathBuf,
    pub tasks: Vec<Task>,
}

impl ImplementationPlan {
    /// Load an implementation plan from disk.
    ///
    /// Parses checkbox lists in markdown format:
    /// ```markdown
    /// - [ ] Task 1
    /// - [x] Task 2
    /// - [~] Task 3
    /// ```
    pub async fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let contents = fs::read_to_string(&path).await?;

        let mut tasks = Vec::new();
        let mut task_counter = 1;

        for (line_num, line) in contents.lines().enumerate() {
            if let Some(task) = Self::parse_task_line(line, line_num, &mut task_counter) {
                tasks.push(task);
            }
        }

        if tasks.is_empty() {
            return Err(GagentError::Other(
                "No tasks found in implementation plan".to_string(),
            ));
        }

        Ok(Self { path, tasks })
    }

    /// Parse a single task line.
    ///
    /// Returns Some(Task) if the line is a checkbox task, None otherwise.
    fn parse_task_line(line: &str, line_num: usize, task_counter: &mut usize) -> Option<Task> {
        let line = line.trim();

        // Match checkbox pattern: - [ ], - [x], - [~], etc.
        if !line.starts_with("- [") {
            return None;
        }

        // Extract checkbox and description
        let checkbox_end = line.find(']')?;
        let checkbox = &line[3..checkbox_end]; // Extract char between [ and ]
        let description = line[checkbox_end + 1..].trim();

        if description.is_empty() {
            return None;
        }

        let status = TaskStatus::from_checkbox(checkbox);
        let id = format!("task-{}", task_counter);
        *task_counter += 1;

        Some(Task {
            id,
            description: description.to_string(),
            status,
            line_number: line_num,
        })
    }

    /// Update a task's status in memory.
    pub fn update_task_status(&mut self, task_id: &str, new_status: TaskStatus) -> Result<()> {
        let task = self
            .tasks
            .iter_mut()
            .find(|t| t.id == task_id)
            .ok_or_else(|| GagentError::Other(format!("Task not found: {}", task_id)))?;

        task.status = new_status;
        Ok(())
    }

    /// Write the updated plan back to disk.
    ///
    /// This preserves all non-task lines and only updates checkbox status.
    pub async fn save(&self) -> Result<()> {
        let original_contents = fs::read_to_string(&self.path).await?;
        let mut new_lines: Vec<String> = original_contents.lines().map(|s| s.to_string()).collect();

        // Update task lines
        for task in &self.tasks {
            if task.line_number < new_lines.len() {
                let line = &new_lines[task.line_number];
                if let Some(updated) = Self::update_checkbox_in_line(line, &task.status) {
                    new_lines[task.line_number] = updated;
                }
            }
        }

        let new_contents = new_lines.join("\n") + "\n";
        fs::write(&self.path, new_contents).await?;

        Ok(())
    }

    /// Update the checkbox in a line, preserving everything else.
    fn update_checkbox_in_line(line: &str, status: &TaskStatus) -> Option<String> {
        let checkbox_start = line.find("- [")?;
        let checkbox_end = line.find(']')?;

        if checkbox_end <= checkbox_start + 2 {
            return None;
        }

        let mut result = String::new();
        result.push_str(&line[..checkbox_start + 3]); // "- ["
        result.push_str(status.to_checkbox());
        result.push_str(&line[checkbox_end..]); // "] description"

        Some(result)
    }

    /// Get the next pending task.
    pub fn next_pending_task(&self) -> Option<&Task> {
        self.tasks
            .iter()
            .find(|t| t.status == TaskStatus::Pending)
    }

    /// Get task statistics.
    pub fn stats(&self) -> TaskStats {
        let total = self.tasks.len();
        let done = self.tasks.iter().filter(|t| t.status == TaskStatus::Done).count();
        let pending = self.tasks.iter().filter(|t| t.status == TaskStatus::Pending).count();
        let in_progress = self.tasks.iter().filter(|t| t.status == TaskStatus::InProgress).count();
        let skipped = self.tasks.iter().filter(|t| t.status == TaskStatus::Skipped).count();

        TaskStats {
            total,
            done,
            pending,
            in_progress,
            skipped,
        }
    }
}

/// Statistics about tasks in the plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStats {
    pub total: usize,
    pub done: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs;

    #[tokio::test]
    async fn test_parse_plan() {
        let dir = tempdir().unwrap();
        let plan_path = dir.path().join("IMPLEMENTATION_PLAN.md");

        let content = r#"# Implementation Plan

## Phase 1
- [ ] Task one
- [x] Task two (done)
- [~] Task three (in progress)

Some text here.

## Phase 2
- [ ] Task four
- [-] Task five (skipped)
"#;

        tokio::fs::write(&plan_path, content).await.expect("Failed to write file");

        let plan: ImplementationPlan = ImplementationPlan::load(&plan_path).await.unwrap();

        assert_eq!(plan.tasks.len(), 5);
        assert_eq!(plan.tasks[0].status, TaskStatus::Pending);
        assert_eq!(plan.tasks[1].status, TaskStatus::Done);
        assert_eq!(plan.tasks[2].status, TaskStatus::InProgress);
        assert_eq!(plan.tasks[3].status, TaskStatus::Pending);
        assert_eq!(plan.tasks[4].status, TaskStatus::Skipped);

        assert_eq!(plan.tasks[0].description, "Task one");
        assert_eq!(plan.tasks[1].description, "Task two (done)");
    }

    #[tokio::test]
    async fn test_update_and_save() {
        let dir = tempdir().unwrap();
        let plan_path = dir.path().join("IMPLEMENTATION_PLAN.md");

        let content = r#"# Plan
- [ ] First task
- [ ] Second task
"#;

        tokio::fs::write(&plan_path, content).await.expect("Failed to write file");

        let mut plan: ImplementationPlan = ImplementationPlan::load(&plan_path).await.unwrap();
        assert_eq!(plan.tasks.len(), 2);

        // Update first task
        plan.update_task_status("task-1", TaskStatus::Done).unwrap();
        plan.save().await.expect("Failed to save plan");

        // Reload and verify
        let reloaded: ImplementationPlan = ImplementationPlan::load(&plan_path).await.unwrap();
        assert_eq!(reloaded.tasks[0].status, TaskStatus::Done);
        assert_eq!(reloaded.tasks[1].status, TaskStatus::Pending);
    }

    #[tokio::test]
    async fn test_next_pending() {
        let dir = tempdir().unwrap();
        let plan_path = dir.path().join("IMPLEMENTATION_PLAN.md");

        let content = r#"
- [x] Done task
- [~] In progress
- [ ] Next pending
- [ ] Another pending
"#;

        tokio::fs::write(&plan_path, content).await.expect("Failed to write file");

        let plan: ImplementationPlan = ImplementationPlan::load(&plan_path).await.unwrap();
        let next = plan.next_pending_task().unwrap();
        assert_eq!(next.id, "task-3");
        assert_eq!(next.description, "Next pending");
    }

    #[tokio::test]
    async fn test_stats() {
        let dir = tempdir().unwrap();
        let plan_path = dir.path().join("IMPLEMENTATION_PLAN.md");

        let content = r#"
- [ ] Task 1
- [x] Task 2
- [x] Task 3
- [~] Task 4
- [-] Task 5
"#;

        tokio::fs::write(&plan_path, content).await.expect("Failed to write file");

        let plan: ImplementationPlan = ImplementationPlan::load(&plan_path).await.unwrap();
        let stats = plan.stats();

        assert_eq!(stats.total, 5);
        assert_eq!(stats.done, 2);
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.in_progress, 1);
        assert_eq!(stats.skipped, 1);
    }

    #[test]
    fn test_checkbox_parsing() {
        assert_eq!(TaskStatus::from_checkbox(" "), TaskStatus::Pending);
        assert_eq!(TaskStatus::from_checkbox("x"), TaskStatus::Done);
        assert_eq!(TaskStatus::from_checkbox("X"), TaskStatus::Done);
        assert_eq!(TaskStatus::from_checkbox("~"), TaskStatus::InProgress);
        assert_eq!(TaskStatus::from_checkbox("-"), TaskStatus::Skipped);
    }

    #[test]
    fn test_checkbox_conversion() {
        assert_eq!(TaskStatus::Pending.to_checkbox(), " ");
        assert_eq!(TaskStatus::Done.to_checkbox(), "x");
        assert_eq!(TaskStatus::InProgress.to_checkbox(), "~");
        assert_eq!(TaskStatus::Skipped.to_checkbox(), "-");
    }
}
