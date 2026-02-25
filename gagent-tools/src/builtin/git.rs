use crate::definition::{Tool, ToolContext, ToolDefinition, ToolParam, ToolResult};
use async_trait::async_trait;
use gagent_core::GagentError;
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;

/// Tool for Git operations.
pub struct GitTool;

impl GitTool {
    pub fn new() -> Self {
        Self
    }

    async fn execute_git_command(
        &self,
        args: &[&str],
        context: &ToolContext,
    ) -> Result<(bool, String), String> {
        let mut cmd = Command::new("git");
        cmd.args(args)
            .current_dir(&context.working_dir)
            .kill_on_drop(true);

        let timeout = Duration::from_secs(60); // Git commands get 60s timeout
        let result = tokio::time::timeout(timeout, cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let combined_output = if stderr.is_empty() {
                    stdout.to_string()
                } else if stdout.is_empty() {
                    stderr.to_string()
                } else {
                    format!("{}\n{}", stdout, stderr)
                };

                Ok((
                    output.status.success(),
                    if combined_output.is_empty() {
                        "(no output)".to_string()
                    } else {
                        combined_output
                    },
                ))
            }
            Ok(Err(e)) => Err(format!("Failed to execute git command: {}", e)),
            Err(_) => Err("Git command timed out after 60s".to_string()),
        }
    }
}

#[async_trait]
impl Tool for GitTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "git".to_string(),
            description: "Execute git commands. Common operations: status, diff, log, add, commit, push, pull.".to_string(),
            parameters: vec![
                ToolParam {
                    name: "operation".to_string(),
                    description: "Git operation to perform (e.g., 'status', 'diff', 'log', 'add', 'commit')".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                },
                ToolParam {
                    name: "args".to_string(),
                    description: "Additional arguments for the git command (e.g., file paths, commit message)".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                },
            ],
        }
    }

    async fn execute(
        &self,
        params: HashMap<String, serde_json::Value>,
        context: &ToolContext,
    ) -> Result<ToolResult, GagentError> {
        let operation = params
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GagentError::Tool("Missing or invalid 'operation' parameter".to_string()))?;

        let args_str = params
            .get("args")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        tracing::info!(
            "Executing git {} {} in {}",
            operation,
            args_str,
            context.working_dir.display()
        );

        // Build git command arguments
        let mut git_args = vec![operation];
        if !args_str.is_empty() {
            // Simple whitespace split - in production, would want proper shell parsing
            let arg_parts: Vec<&str> = args_str.split_whitespace().collect();
            git_args.extend(arg_parts);
        }

        let result = self.execute_git_command(&git_args, context).await;

        match result {
            Ok((success, output)) => Ok(ToolResult { success, output }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: e,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use tokio;

    async fn init_git_repo(path: &std::path::Path) {
        let mut cmd = Command::new("git");
        cmd.args(["init"])
            .current_dir(path)
            .output()
            .await
            .unwrap();

        // Configure user for commits
        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(path)
            .output()
            .await
            .unwrap();

        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(path)
            .output()
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_git_status() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path()).await;

        let tool = GitTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("operation".to_string(), serde_json::json!("status"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("On branch") || result.output.contains("No commits yet"));
    }

    #[tokio::test]
    async fn test_git_add_and_status() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path()).await;

        // Create a file
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let tool = GitTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        // Add the file
        let mut params = HashMap::new();
        params.insert("operation".to_string(), serde_json::json!("add"));
        params.insert("args".to_string(), serde_json::json!("test.txt"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);

        // Check status
        let mut params = HashMap::new();
        params.insert("operation".to_string(), serde_json::json!("status"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("test.txt"));
    }

    #[tokio::test]
    async fn test_git_invalid_operation() {
        let temp_dir = TempDir::new().unwrap();
        init_git_repo(temp_dir.path()).await;

        let tool = GitTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("operation".to_string(), serde_json::json!("invalid-op"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(!result.success);
    }
}
