use crate::definition::{Tool, ToolContext, ToolDefinition, ToolParam, ToolResult};
use async_trait::async_trait;
use gagent_core::GagentError;
use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;

/// Tool for executing shell commands.
pub struct ShellTool {
    default_timeout_secs: u64,
}

impl ShellTool {
    pub fn new() -> Self {
        Self {
            default_timeout_secs: 600, // 10 minutes default
        }
    }

    pub fn with_timeout(timeout_secs: u64) -> Self {
        Self {
            default_timeout_secs: timeout_secs,
        }
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "shell".to_string(),
            description: format!(
                "Execute a shell command. Commands are executed with a {}s timeout. Returns stdout and stderr.",
                self.default_timeout_secs
            ),
            parameters: vec![
                ToolParam {
                    name: "command".to_string(),
                    description: "Shell command to execute".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                },
                ToolParam {
                    name: "timeout_secs".to_string(),
                    description: format!("Timeout in seconds (default: {})", self.default_timeout_secs),
                    param_type: "number".to_string(),
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
        let command_str = params
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GagentError::Tool("Missing or invalid 'command' parameter".to_string()))?;

        let timeout_secs = params
            .get("timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(self.default_timeout_secs);

        tracing::info!(
            "Executing shell command in {}: {}",
            context.working_dir.display(),
            command_str
        );

        // Execute command with timeout
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(command_str)
            .current_dir(&context.working_dir)
            .kill_on_drop(true);

        let timeout = Duration::from_secs(timeout_secs);
        let result = tokio::time::timeout(timeout, cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let combined_output = if stderr.is_empty() {
                    stdout.to_string()
                } else if stdout.is_empty() {
                    format!("stderr:\n{}", stderr)
                } else {
                    format!("stdout:\n{}\n\nstderr:\n{}", stdout, stderr)
                };

                Ok(ToolResult {
                    success: output.status.success(),
                    output: if combined_output.is_empty() {
                        "(no output)".to_string()
                    } else {
                        combined_output
                    },
                })
            }
            Ok(Err(e)) => Ok(ToolResult {
                success: false,
                output: format!("Failed to execute command: {}", e),
            }),
            Err(_) => Err(GagentError::Timeout(timeout_secs)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use tokio;

    #[tokio::test]
    async fn test_shell_success() {
        let temp_dir = TempDir::new().unwrap();

        let tool = ShellTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("command".to_string(), serde_json::json!("echo 'Hello, world!'"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Hello, world!"));
    }

    #[tokio::test]
    async fn test_shell_failure() {
        let temp_dir = TempDir::new().unwrap();

        let tool = ShellTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("command".to_string(), serde_json::json!("exit 1"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_shell_working_directory() {
        let temp_dir = TempDir::new().unwrap();

        let tool = ShellTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("command".to_string(), serde_json::json!("pwd"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains(&temp_dir.path().to_string_lossy().to_string()));
    }

    #[tokio::test]
    async fn test_shell_timeout() {
        let temp_dir = TempDir::new().unwrap();

        let tool = ShellTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("command".to_string(), serde_json::json!("sleep 10"));
        params.insert("timeout_secs".to_string(), serde_json::json!(1));

        let result = tool.execute(params, &context).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), GagentError::Timeout(_)));
    }
}
