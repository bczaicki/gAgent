use crate::definition::{Tool, ToolContext, ToolDefinition, ToolParam, ToolResult};
use async_trait::async_trait;
use gagent_core::{GagentError, MemoryStore};
use std::collections::HashMap;

/// Tool for searching agent memory files.
pub struct MemorySearchTool;

impl MemorySearchTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for MemorySearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "memory_search".to_string(),
            description: "Search all agent memory files (MEMORY.md and memory/*.md) for lines \
                           containing the given query. Returns matching lines with file and line \
                           number references. Search is case-insensitive."
                .to_string(),
            parameters: vec![
                ToolParam {
                    name: "query".to_string(),
                    description: "Search term to look for in memory files.".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                },
                ToolParam {
                    name: "max_results".to_string(),
                    description: "Maximum number of results to return. Default: 20.".to_string(),
                    param_type: "integer".to_string(),
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
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GagentError::Tool("Missing 'query' parameter".to_string()))?;

        let max_results = params
            .get("max_results")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;

        let store = MemoryStore::new(&context.working_dir);

        match store.search(query) {
            Ok(results) => {
                if results.is_empty() {
                    Ok(ToolResult {
                        success: true,
                        output: format!("No results found for query: '{query}'"),
                    })
                } else {
                    let limited: Vec<_> = results.iter().take(max_results).collect();
                    let total = results.len();

                    let mut output = format!(
                        "Found {} result(s) for '{query}'{}:\n\n",
                        limited.len(),
                        if total > max_results {
                            format!(" (showing first {max_results} of {total})")
                        } else {
                            String::new()
                        }
                    );

                    for r in &limited {
                        output.push_str(&format!("  {r}\n"));
                    }

                    Ok(ToolResult {
                        success: true,
                        output,
                    })
                }
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: format!("Memory search failed: {e}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_search_finds_results() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("MEMORY.md"), "Remember: Rust is fast.").unwrap();
        std::fs::create_dir(dir.path().join("memory")).unwrap();
        std::fs::write(dir.path().join("memory/notes.md"), "Rust is memory-safe.").unwrap();

        let tool = MemorySearchTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("query".to_string(), serde_json::json!("rust"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("2 result"));
    }

    #[tokio::test]
    async fn test_search_no_results() {
        let dir = TempDir::new().unwrap();
        let tool = MemorySearchTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("query".to_string(), serde_json::json!("xyzzy"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("No results"));
    }

    #[tokio::test]
    async fn test_search_max_results() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("memory")).unwrap();
        // Write 10 lines containing "match"
        let content: String = (1..=10).map(|i| format!("match line {i}\n")).collect();
        std::fs::write(dir.path().join("memory/many.md"), content).unwrap();

        let tool = MemorySearchTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("query".to_string(), serde_json::json!("match"));
        params.insert("max_results".to_string(), serde_json::json!(3));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("showing first 3 of 10"));
    }

    #[tokio::test]
    async fn test_search_missing_query_errors() {
        let dir = TempDir::new().unwrap();
        let tool = MemorySearchTool::new();
        let context = ToolContext {
            working_dir: dir.path().to_path_buf(),
            allowed_paths: vec![],
        };
        let result = tool.execute(HashMap::new(), &context).await;
        assert!(result.is_err());
    }
}
