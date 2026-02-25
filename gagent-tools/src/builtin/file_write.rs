use crate::definition::{Tool, ToolContext, ToolDefinition, ToolParam, ToolResult};
use async_trait::async_trait;
use gagent_core::GagentError;
use std::collections::HashMap;
use std::path::Path;

/// Tool for writing file contents.
pub struct FileWriteTool;

impl FileWriteTool {
    pub fn new() -> Self {
        Self
    }

    fn validate_path(&self, path: &Path, context: &ToolContext) -> Result<(), GagentError> {
        // If allowed_paths is empty, allow all paths
        if context.allowed_paths.is_empty() {
            return Ok(());
        }

        // For write operations, we need to check parent directory
        // since the file might not exist yet
        let parent = path.parent().ok_or_else(|| {
            GagentError::Tool("Cannot determine parent directory".to_string())
        })?;

        let canonical_parent = parent.canonicalize().map_err(|e| {
            GagentError::Tool(format!("Failed to canonicalize parent path: {}", e))
        })?;

        for allowed in &context.allowed_paths {
            // Canonicalize allowed path for comparison
            if let Ok(canonical_allowed) = allowed.canonicalize() {
                if canonical_parent.starts_with(canonical_allowed) {
                    return Ok(());
                }
            }
        }

        Err(GagentError::PathNotAllowed(
            path.display().to_string(),
        ))
    }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_write".to_string(),
            description: "Write content to a file. Creates the file if it doesn't exist, overwrites if it does.".to_string(),
            parameters: vec![
                ToolParam {
                    name: "path".to_string(),
                    description: "Path to the file to write (relative to working directory)".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                },
                ToolParam {
                    name: "content".to_string(),
                    description: "Content to write to the file".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                },
            ],
        }
    }

    async fn execute(
        &self,
        params: HashMap<String, serde_json::Value>,
        context: &ToolContext,
    ) -> Result<ToolResult, GagentError> {
        let path_str = params
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GagentError::Tool("Missing or invalid 'path' parameter".to_string()))?;

        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GagentError::Tool("Missing or invalid 'content' parameter".to_string()))?;

        let path = context.working_dir.join(path_str);

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                GagentError::Tool(format!("Failed to create parent directories: {}", e))
            })?;
        }

        // Validate path permissions
        self.validate_path(&path, context)?;

        // Write the file
        match tokio::fs::write(&path, content).await {
            Ok(_) => Ok(ToolResult {
                success: true,
                output: format!("Successfully wrote {} bytes to {}", content.len(), path.display()),
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: format!("Failed to write file {}: {}", path.display(), e),
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

    #[tokio::test]
    async fn test_file_write_success() {
        let temp_dir = TempDir::new().unwrap();

        let tool = FileWriteTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("path".to_string(), serde_json::json!("test.txt"));
        params.insert("content".to_string(), serde_json::json!("Hello, world!"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Successfully wrote"));

        // Verify file was written
        let content = std::fs::read_to_string(temp_dir.path().join("test.txt")).unwrap();
        assert_eq!(content, "Hello, world!");
    }

    #[tokio::test]
    async fn test_file_write_creates_directories() {
        let temp_dir = TempDir::new().unwrap();

        let tool = FileWriteTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("path".to_string(), serde_json::json!("subdir/test.txt"));
        params.insert("content".to_string(), serde_json::json!("content"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);

        // Verify directory and file were created
        let file_path = temp_dir.path().join("subdir/test.txt");
        assert!(file_path.exists());
        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "content");
    }

    #[tokio::test]
    async fn test_file_write_overwrites() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "old content").unwrap();

        let tool = FileWriteTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("path".to_string(), serde_json::json!("test.txt"));
        params.insert("content".to_string(), serde_json::json!("new content"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);

        // Verify file was overwritten
        let content = std::fs::read_to_string(file_path).unwrap();
        assert_eq!(content, "new content");
    }
}
