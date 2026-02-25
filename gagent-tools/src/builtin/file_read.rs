use crate::definition::{Tool, ToolContext, ToolDefinition, ToolParam, ToolResult};
use async_trait::async_trait;
use gagent_core::GagentError;
use std::collections::HashMap;
use std::path::Path;

/// Tool for reading file contents.
pub struct FileReadTool;

impl FileReadTool {
    pub fn new() -> Self {
        Self
    }

    fn validate_path(&self, path: &Path, context: &ToolContext) -> Result<(), GagentError> {
        // If allowed_paths is empty, allow all paths
        if context.allowed_paths.is_empty() {
            return Ok(());
        }

        // Check if path is within allowed paths
        let canonical = path.canonicalize().map_err(|e| {
            GagentError::Tool(format!("Failed to canonicalize path: {}", e))
        })?;

        for allowed in &context.allowed_paths {
            // Canonicalize allowed path for comparison
            if let Ok(canonical_allowed) = allowed.canonicalize() {
                if canonical.starts_with(canonical_allowed) {
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
impl Tool for FileReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_read".to_string(),
            description: "Read the contents of a file. Returns the file contents as a string."
                .to_string(),
            parameters: vec![ToolParam {
                name: "path".to_string(),
                description: "Path to the file to read (relative to working directory)"
                    .to_string(),
                param_type: "string".to_string(),
                required: true,
            }],
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

        let path = context.working_dir.join(path_str);

        // Validate path permissions
        self.validate_path(&path, context)?;

        // Read the file
        match tokio::fs::read_to_string(&path).await {
            Ok(contents) => Ok(ToolResult {
                success: true,
                output: contents,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: format!("Failed to read file {}: {}", path.display(), e),
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
    async fn test_file_read_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "Hello, world!").unwrap();

        let tool = FileReadTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("path".to_string(), serde_json::json!("test.txt"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output, "Hello, world!");
    }

    #[tokio::test]
    async fn test_file_read_not_found() {
        let temp_dir = TempDir::new().unwrap();

        let tool = FileReadTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("path".to_string(), serde_json::json!("nonexistent.txt"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(!result.success);
        assert!(result.output.contains("Failed to read file"));
    }

    #[tokio::test]
    async fn test_file_read_path_validation() {
        let temp_dir = TempDir::new().unwrap();
        let allowed_dir = temp_dir.path().join("allowed");
        std::fs::create_dir(&allowed_dir).unwrap();
        let file_path = allowed_dir.join("test.txt");
        std::fs::write(&file_path, "content").unwrap();

        let tool = FileReadTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![allowed_dir.clone()],
        };

        // Should succeed - file is in allowed path
        let mut params = HashMap::new();
        params.insert("path".to_string(), serde_json::json!("allowed/test.txt"));
        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output, "content");
    }
}
