use crate::definition::{Tool, ToolContext, ToolDefinition, ToolParam, ToolResult};
use async_trait::async_trait;
use gagent_core::GagentError;
use std::collections::HashMap;

/// Tool for searching files (glob patterns and grep).
pub struct FileSearchTool;

impl FileSearchTool {
    pub fn new() -> Self {
        Self
    }

    async fn glob_search(&self, pattern: &str, context: &ToolContext) -> Result<String, String> {
        let search_dir = context.working_dir.clone();

        // Use walkdir for recursive glob-like searching
        let pattern_lower = pattern.to_lowercase();
        let mut matches = Vec::new();

        // Simple pattern matching: if pattern contains *, treat as wildcard
        // Otherwise, match filename contains pattern
        match std::fs::read_dir(&search_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                    let matches_pattern = if pattern.contains('*') {
                        // Simple wildcard matching
                        let parts: Vec<&str> = pattern.split('*').collect();
                        if parts.len() == 2 && parts[0].is_empty() {
                            // *.txt pattern
                            file_name.ends_with(parts[1])
                        } else if parts.len() == 2 && parts[1].is_empty() {
                            // test* pattern
                            file_name.starts_with(parts[0])
                        } else {
                            // *pattern* or more complex
                            file_name.to_lowercase().contains(&pattern_lower.replace('*', ""))
                        }
                    } else {
                        // Exact or contains match
                        file_name.to_lowercase().contains(&pattern_lower)
                    };

                    if matches_pattern {
                        matches.push(path.display().to_string());
                    }
                }
            }
            Err(e) => return Err(format!("Failed to read directory: {}", e)),
        }

        if matches.is_empty() {
            Ok("No files found matching pattern".to_string())
        } else {
            Ok(matches.join("\n"))
        }
    }

    async fn grep_search(&self, pattern: &str, context: &ToolContext) -> Result<String, String> {
        let search_dir = context.working_dir.clone();
        let pattern_lower = pattern.to_lowercase();
        let mut results = Vec::new();

        match std::fs::read_dir(&search_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let mut matching_lines = Vec::new();
                            for (line_num, line) in content.lines().enumerate() {
                                if line.to_lowercase().contains(&pattern_lower) {
                                    matching_lines.push(format!("{}:{}: {}",
                                        path.display(),
                                        line_num + 1,
                                        line.trim()
                                    ));
                                }
                            }
                            if !matching_lines.is_empty() {
                                results.extend(matching_lines);
                            }
                        }
                    }
                }
            }
            Err(e) => return Err(format!("Failed to read directory: {}", e)),
        }

        if results.is_empty() {
            Ok("No matches found".to_string())
        } else {
            Ok(results.join("\n"))
        }
    }
}

#[async_trait]
impl Tool for FileSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "file_search".to_string(),
            description: "Search for files by name pattern (glob) or content (grep). Use mode='glob' for filename search, mode='grep' for content search.".to_string(),
            parameters: vec![
                ToolParam {
                    name: "pattern".to_string(),
                    description: "Search pattern (filename pattern for glob, text for grep)".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                },
                ToolParam {
                    name: "mode".to_string(),
                    description: "Search mode: 'glob' for filename search, 'grep' for content search".to_string(),
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
        let pattern = params
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GagentError::Tool("Missing or invalid 'pattern' parameter".to_string()))?;

        let mode = params
            .get("mode")
            .and_then(|v| v.as_str())
            .ok_or_else(|| GagentError::Tool("Missing or invalid 'mode' parameter".to_string()))?;

        let result = match mode {
            "glob" => self.glob_search(pattern, context).await,
            "grep" => self.grep_search(pattern, context).await,
            _ => Err(format!("Invalid mode '{}'. Use 'glob' or 'grep'", mode)),
        };

        match result {
            Ok(output) => Ok(ToolResult {
                success: true,
                output,
            }),
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

    #[tokio::test]
    async fn test_glob_search() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();
        std::fs::write(temp_dir.path().join("test.md"), "content").unwrap();
        std::fs::write(temp_dir.path().join("other.txt"), "content").unwrap();

        let tool = FileSearchTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("pattern".to_string(), serde_json::json!("*.txt"));
        params.insert("mode".to_string(), serde_json::json!("glob"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("test.txt"));
        assert!(result.output.contains("other.txt"));
        assert!(!result.output.contains("test.md"));
    }

    #[tokio::test]
    async fn test_grep_search() {
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("file1.txt"), "Hello world\nGoodbye world").unwrap();
        std::fs::write(temp_dir.path().join("file2.txt"), "No match here").unwrap();

        let tool = FileSearchTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("pattern".to_string(), serde_json::json!("world"));
        params.insert("mode".to_string(), serde_json::json!("grep"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Hello world"));
        assert!(result.output.contains("Goodbye world"));
        assert!(!result.output.contains("No match"));
    }

    #[tokio::test]
    async fn test_invalid_mode() {
        let temp_dir = TempDir::new().unwrap();

        let tool = FileSearchTool::new();
        let context = ToolContext {
            working_dir: temp_dir.path().to_path_buf(),
            allowed_paths: vec![],
        };

        let mut params = HashMap::new();
        params.insert("pattern".to_string(), serde_json::json!("test"));
        params.insert("mode".to_string(), serde_json::json!("invalid"));

        let result = tool.execute(params, &context).await.unwrap();
        assert!(!result.success);
        assert!(result.output.contains("Invalid mode"));
    }
}
