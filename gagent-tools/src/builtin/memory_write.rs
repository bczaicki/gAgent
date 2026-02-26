use crate::definition::{Tool, ToolContext, ToolDefinition, ToolParam, ToolResult};
use async_trait::async_trait;
use gagent_core::{GagentError, MemoryStore};
use std::collections::HashMap;

/// Tool for writing or appending to agent memory files.
pub struct MemoryWriteTool;

impl MemoryWriteTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for MemoryWriteTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_write".to_string(),
            description: "Write or append to agent memory. Use 'name' to specify a file in the \
                           memory/ directory. Omit 'name' to update the root MEMORY.md summary. \
                           Set 'append' to true to add to existing content instead of overwriting."
                .to_string(),
            parameters: vec![
                ToolParam {
                    name: "content".to_string(),
                    description: "The text content to write to the memory file.".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                },
                ToolParam {
                    name: "name".to_string(),
                    description: "Optional filename within the memory/ directory (e.g. '2024-01-15.md'). \
                                   Omit to write to the root MEMORY.md summary."
                        .to_string(),
                    param_type: "string".to_string(),
                    required: false,
                },
                ToolParam {
                    name: "append".to_string(),
                    description: "If true, append content to existing file. Default: false (overwrite)."
                        .to_string(),
                    param_type: "boolean".to_string(),
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
        let content = params
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GagentError::Tool("Missing 'content' parameter".to_string()))?;

        let name = params.get("name").and_then(|v| v.as_str());
        let append = params
            .get("append")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let store = MemoryStore::new(&context.working_dir);

        let result = match name {
            None => {
                // Write to root MEMORY.md
                if append {
                    let existing = store.read_summary().unwrap_or_default();
                    let combined = if existing.is_empty() {
                        content.to_string()
                    } else {
                        format!("{existing}\n{content}")
                    };
                    store.write_summary(&combined)
                } else {
                    store.write_summary(content)
                }
            }
            Some(file_name) => {
                if append {
                    store.append(file_name, content)
                } else {
                    store.write(file_name, content)
                }
            }
        };

        match result {
            Ok(()) => {
                let target = name.map_or("MEMORY.md".to_string(), |n| format!("memory/{n}"));
                let action = if append { "appended to" } else { "written to" };
                Ok(ToolResult {
                    success: true,
                    output: format!("Successfully {action} {target} ({} chars)", content.chars().count()),
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: format!("Failed to write memory: {e}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_write_summary() {
        let dir = TempDir::new().unwrap();
        let tool = MemoryWriteTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("content".to_string(), serde_json::json!("Key fact: Rust is fast."));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);

        let written = std::fs::read_to_string(dir.path().join("MEMORY.md")).unwrap();
        assert_eq!(written, "Key fact: Rust is fast.");
    }

    #[tokio::test]
    async fn test_write_named_file() {
        let dir = TempDir::new().unwrap();
        let tool = MemoryWriteTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("content".to_string(), serde_json::json!("Session notes."));
        params.insert("name".to_string(), serde_json::json!("session.md"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);

        let written = std::fs::read_to_string(dir.path().join("memory/session.md")).unwrap();
        assert_eq!(written, "Session notes.");
    }

    #[tokio::test]
    async fn test_append_to_summary() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("MEMORY.md"), "Existing.").unwrap();

        let tool = MemoryWriteTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("content".to_string(), serde_json::json!("New line."));
        params.insert("append".to_string(), serde_json::json!(true));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("appended"));

        let written = std::fs::read_to_string(dir.path().join("MEMORY.md")).unwrap();
        assert!(written.contains("Existing."));
        assert!(written.contains("New line."));
    }

    #[tokio::test]
    async fn test_missing_content_errors() {
        let dir = TempDir::new().unwrap();
        let tool = MemoryWriteTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let result = tool.execute(HashMap::new(), &context).await;
        assert!(result.is_err());
    }
}
