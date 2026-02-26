use crate::definition::{Tool, ToolContext, ToolDefinition, ToolParam, ToolResult};
use async_trait::async_trait;
use gagent_core::{GagentError, MemoryStore};
use std::collections::HashMap;

/// Tool for reading memory files.
pub struct MemoryReadTool;

impl MemoryReadTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for MemoryReadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_read".to_string(),
            description: "Read agent memory. If 'name' is omitted, reads the root MEMORY.md \
                           summary. Otherwise reads a specific file from the memory/ directory."
                .to_string(),
            parameters: vec![ToolParam {
                name: "name".to_string(),
                description: "Optional filename within the memory/ directory (e.g. '2024-01-15.md'). \
                               Omit to read the root MEMORY.md summary."
                    .to_string(),
                param_type: "string".to_string(),
                required: false,
            }],
        }
    }

    async fn execute(
        &self,
        params: HashMap<String, serde_json::Value>,
        context: &ToolContext,
    ) -> Result<ToolResult, GagentError> {
        let store = MemoryStore::new(&context.working_dir);

        let name = params.get("name").and_then(|v| v.as_str());

        match name {
            None => {
                // Read root MEMORY.md
                let summary = store.read_summary().map_err(|e| {
                    GagentError::Tool(format!("Failed to read memory summary: {e}"))
                })?;

                if summary.is_empty() {
                    Ok(ToolResult {
                        success: true,
                        output: "(Memory summary is empty)".to_string(),
                    })
                } else {
                    Ok(ToolResult {
                        success: true,
                        output: summary,
                    })
                }
            }
            Some(file_name) => {
                match store.read(file_name) {
                    Ok(entry) => Ok(ToolResult {
                        success: true,
                        output: entry.content,
                    }),
                    Err(e) => Ok(ToolResult {
                        success: false,
                        output: format!("Failed to read memory/{file_name}: {e}"),
                    }),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_read_summary_empty() {
        let dir = TempDir::new().unwrap();
        let tool = MemoryReadTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };
        let result = tool.execute(HashMap::new(), &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("empty"));
    }

    #[tokio::test]
    async fn test_read_summary_with_content() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("MEMORY.md"), "Key facts: A, B, C").unwrap();

        let tool = MemoryReadTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let result = tool.execute(HashMap::new(), &context).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output, "Key facts: A, B, C");
    }

    #[tokio::test]
    async fn test_read_named_file() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("memory")).unwrap();
        std::fs::write(dir.path().join("memory/notes.md"), "Important note.").unwrap();

        let tool = MemoryReadTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("name".to_string(), serde_json::json!("notes.md"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output, "Important note.");
    }

    #[tokio::test]
    async fn test_read_named_file_not_found() {
        let dir = TempDir::new().unwrap();
        let tool = MemoryReadTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("name".to_string(), serde_json::json!("missing.md"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(!result.success);
    }
}
