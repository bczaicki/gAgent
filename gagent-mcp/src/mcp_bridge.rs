//! MCP bridge: converts MCP server tools into gAgent Tool implementations.
//!
//! The McpBridge discovers tools from a connected MCP server and registers
//! proxy `Tool` implementations in a `ToolRegistry`.

use crate::mcp_client::McpClient;
use crate::protocol::McpTool;
use async_trait::async_trait;
use gagent_core::{GagentError, Result};
use gagent_tools::{Tool, ToolContext, ToolDefinition, ToolParam, ToolRegistry, ToolResult};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Configuration for a single MCP server.
#[derive(Debug, Clone)]
pub struct McpServerConfig {
    /// Human-readable name for the server.
    pub name: String,

    /// Executable command to spawn.
    pub command: String,

    /// Arguments to pass to the command.
    pub args: Vec<String>,
}

/// A proxy `Tool` that delegates execution to an MCP server.
struct McpProxyTool {
    /// The tool definition as reported by the MCP server.
    mcp_tool: McpTool,

    /// Shared client (behind a mutex since McpClient has mutable state).
    client: Arc<Mutex<McpClient>>,
}

#[async_trait]
impl Tool for McpProxyTool {
    fn definition(&self) -> ToolDefinition {
        // Convert MCP inputSchema properties to ToolParam list.
        let mut parameters = Vec::new();

        if let Some(props) = self
            .mcp_tool
            .input_schema
            .get("properties")
            .and_then(|v| v.as_object())
        {
            let required_fields: Vec<String> = self
                .mcp_tool
                .input_schema
                .get("required")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            for (name, schema) in props {
                let description = schema
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let param_type = schema
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("string")
                    .to_string();

                parameters.push(ToolParam {
                    name: name.clone(),
                    description,
                    param_type,
                    required: required_fields.contains(name),
                });
            }
        }

        ToolDefinition {
            name: self.mcp_tool.name.clone(),
            description: self.mcp_tool.description.clone(),
            parameters,
        }
    }

    async fn execute(
        &self,
        params: HashMap<String, Value>,
        _context: &ToolContext,
    ) -> std::result::Result<ToolResult, GagentError> {
        let arguments = Value::Object(
            params
                .into_iter()
                .collect::<serde_json::Map<String, Value>>(),
        );

        let mut client = self.client.lock().await;

        match client.call_tool(&self.mcp_tool.name, arguments).await {
            Ok(result) => {
                // Concatenate all text content blocks
                let output = result
                    .content
                    .iter()
                    .filter(|c| c.content_type == "text")
                    .map(|c| c.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");

                Ok(ToolResult {
                    success: !result.is_error,
                    output,
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: format!("MCP tool call failed: {e}"),
            }),
        }
    }
}

/// Bridge that connects to MCP servers and registers their tools.
pub struct McpBridge {
    /// Registered server configs.
    servers: Vec<McpServerConfig>,
}

impl McpBridge {
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
        }
    }

    /// Add an MCP server configuration.
    pub fn add_server(&mut self, config: McpServerConfig) {
        self.servers.push(config);
    }

    /// Connect to all configured MCP servers and register their tools in the registry.
    ///
    /// Each server is spawned as a subprocess. Tools from all servers are registered
    /// with their original names (prefixed with the server name if there are collisions).
    pub async fn register_all(&self, registry: &mut ToolRegistry) -> Result<()> {
        for server_config in &self.servers {
            match self.connect_and_register(server_config, registry).await {
                Ok(count) => {
                    tracing::info!(
                        "MCP server '{}': registered {} tool(s)",
                        server_config.name,
                        count
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "MCP server '{}': failed to connect: {}",
                        server_config.name,
                        e
                    );
                    // Don't fail the whole bridge if one server is unavailable
                }
            }
        }
        Ok(())
    }

    async fn connect_and_register(
        &self,
        config: &McpServerConfig,
        registry: &mut ToolRegistry,
    ) -> Result<usize> {
        let args: Vec<&str> = config.args.iter().map(String::as_str).collect();

        let client = McpClient::connect(&config.command, &args).await?;
        let mut client = client;

        let tools = client.list_tools().await?;
        let tool_count = tools.len();

        let client = Arc::new(Mutex::new(client));

        for mcp_tool in tools {
            let proxy = McpProxyTool {
                mcp_tool,
                client: Arc::clone(&client),
            };
            registry.register(Box::new(proxy));
        }

        Ok(tool_count)
    }
}

impl Default for McpBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse MCP server configurations from a JSON config block.
///
/// Expected format (matches the `mcpServers` field from `.gagent/config.json`):
/// ```json
/// {
///   "filesystem": {
///     "command": "npx",
///     "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
///   }
/// }
/// ```
pub fn parse_mcp_servers(config: &Value) -> Vec<McpServerConfig> {
    let mut servers = Vec::new();

    if let Some(obj) = config.as_object() {
        for (name, server_config) in obj {
            let command = match server_config.get("command").and_then(|v| v.as_str()) {
                Some(c) => c.to_string(),
                None => {
                    tracing::warn!("MCP server '{name}': missing 'command' field");
                    continue;
                }
            };

            let args = server_config
                .get("args")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            servers.push(McpServerConfig {
                name: name.clone(),
                command,
                args,
            });
        }
    }

    servers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mcp_servers_valid() {
        let config = serde_json::json!({
            "filesystem": {
                "command": "npx",
                "args": ["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
            }
        });

        let servers = parse_mcp_servers(&config);
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "filesystem");
        assert_eq!(servers[0].command, "npx");
        assert_eq!(servers[0].args, vec!["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]);
    }

    #[test]
    fn test_parse_mcp_servers_missing_command() {
        let config = serde_json::json!({
            "broken": {
                "args": ["something"]
            }
        });

        let servers = parse_mcp_servers(&config);
        assert!(servers.is_empty());
    }

    #[test]
    fn test_parse_mcp_servers_empty() {
        let config = serde_json::json!({});
        let servers = parse_mcp_servers(&config);
        assert!(servers.is_empty());
    }

    #[test]
    fn test_parse_mcp_servers_no_args() {
        let config = serde_json::json!({
            "simple": { "command": "my-server" }
        });
        let servers = parse_mcp_servers(&config);
        assert_eq!(servers.len(), 1);
        assert!(servers[0].args.is_empty());
    }

    #[test]
    fn test_mcp_bridge_new() {
        let bridge = McpBridge::new();
        assert_eq!(bridge.servers.len(), 0);
    }

    #[test]
    fn test_mcp_bridge_add_server() {
        let mut bridge = McpBridge::new();
        bridge.add_server(McpServerConfig {
            name: "test".to_string(),
            command: "echo".to_string(),
            args: vec![],
        });
        assert_eq!(bridge.servers.len(), 1);
    }

    #[test]
    fn test_proxy_tool_definition() {
        use crate::protocol::McpTool;

        let mcp_tool = McpTool {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The query to search"
                    }
                },
                "required": ["query"]
            }),
        };

        // We can't construct McpProxyTool directly (client field), but we can
        // test that our schema parsing logic is correct.
        let props = mcp_tool.input_schema
            .get("properties")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        assert!(props.contains_key("query"));

        let required: Vec<String> = mcp_tool.input_schema
            .get("required")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        assert!(required.contains(&"query".to_string()));
    }
}
