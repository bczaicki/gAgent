//! MCP client that communicates with an MCP server process via stdin/stdout.
//!
//! The MCP protocol is JSON-RPC 2.0 over stdio. Each message is a JSON object
//! followed by a newline. The client spawns the server as a subprocess and
//! communicates via the process's stdin/stdout.

use crate::protocol::{
    JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, McpTool, McpToolCallResult,
    McpToolsListResult,
};
use gagent_core::{GagentError, Result};
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};

/// Client for a single MCP server process.
pub struct McpClient {
    /// Next request ID.
    next_id: AtomicU64,

    /// Server process.
    _child: Child,

    /// Stdin of the server process.
    stdin: ChildStdin,

    /// Buffered reader over the server's stdout.
    reader: BufReader<ChildStdout>,
}

impl McpClient {
    /// Spawn an MCP server and perform the initialization handshake.
    ///
    /// `command` is the executable, `args` are its arguments.
    pub async fn connect(command: &str, args: &[&str]) -> Result<Self> {
        use tokio::process::Command;

        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                GagentError::Other(format!(
                    "Failed to spawn MCP server '{command}': {e}"
                ))
            })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            GagentError::Other("MCP server has no stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            GagentError::Other("MCP server has no stdout".to_string())
        })?;

        let mut client = Self {
            next_id: AtomicU64::new(1),
            _child: child,
            stdin,
            reader: BufReader::new(stdout),
        };

        // Perform initialization handshake
        client.initialize().await?;

        Ok(client)
    }

    /// Send the MCP `initialize` request and `initialized` notification.
    async fn initialize(&mut self) -> Result<()> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "gAgent",
                "version": "0.1.0"
            }
        });

        let response = self.request("initialize", params).await?;

        if response.error.is_some() {
            return Err(GagentError::Other(format!(
                "MCP initialize failed: {:?}",
                response.error
            )));
        }

        tracing::debug!("MCP server initialized");

        // Send the `initialized` notification (no response expected)
        self.notify("notifications/initialized", serde_json::json!({}))
            .await?;

        Ok(())
    }

    /// List all tools available on this MCP server.
    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>> {
        let response = self.request("tools/list", serde_json::json!({})).await?;

        if let Some(err) = response.error {
            return Err(GagentError::Other(format!(
                "MCP tools/list failed: {} (code {})",
                err.message, err.code
            )));
        }

        let result_value = response.result.ok_or_else(|| {
            GagentError::Other("MCP tools/list returned no result".to_string())
        })?;

        let list: McpToolsListResult = serde_json::from_value(result_value).map_err(|e| {
            GagentError::Other(format!("Failed to parse tools/list response: {e}"))
        })?;

        Ok(list.tools)
    }

    /// Call a tool on the MCP server.
    pub async fn call_tool(
        &mut self,
        tool_name: &str,
        arguments: Value,
    ) -> Result<McpToolCallResult> {
        let params = serde_json::json!({
            "name": tool_name,
            "arguments": arguments,
        });

        let response = self.request("tools/call", params).await?;

        if let Some(err) = response.error {
            return Err(GagentError::Other(format!(
                "MCP tools/call '{}' failed: {} (code {})",
                tool_name, err.message, err.code
            )));
        }

        let result_value = response.result.ok_or_else(|| {
            GagentError::Other(format!(
                "MCP tools/call '{tool_name}' returned no result"
            ))
        })?;

        let call_result: McpToolCallResult =
            serde_json::from_value(result_value).map_err(|e| {
                GagentError::Other(format!("Failed to parse tools/call response: {e}"))
            })?;

        Ok(call_result)
    }

    /// Send a JSON-RPC request and wait for the response.
    async fn request(&mut self, method: &str, params: Value) -> Result<JsonRpcResponse> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = JsonRpcRequest::new(id, method, params);

        let mut line = serde_json::to_string(&req).map_err(|e| {
            GagentError::Json(e)
        })?;
        line.push('\n');

        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(GagentError::Io)?;
        self.stdin.flush().await.map_err(GagentError::Io)?;

        tracing::debug!("MCP → {method} (id={id})");

        // Read lines until we get a response with matching id
        loop {
            let mut response_line = String::new();
            let bytes = self
                .reader
                .read_line(&mut response_line)
                .await
                .map_err(GagentError::Io)?;

            if bytes == 0 {
                return Err(GagentError::Other(
                    "MCP server closed connection unexpectedly".to_string(),
                ));
            }

            let response_line = response_line.trim();
            if response_line.is_empty() {
                continue;
            }

            // Try to parse as JSON-RPC response
            match serde_json::from_str::<JsonRpcResponse>(response_line) {
                Ok(resp) if resp.id == Some(id) => {
                    tracing::debug!("MCP ← {method} (id={id})");
                    return Ok(resp);
                }
                Ok(_) => {
                    // Response for a different id or a notification — skip it
                    tracing::debug!("MCP: skipping message for different id");
                    continue;
                }
                Err(e) => {
                    tracing::warn!("MCP: failed to parse response: {e}");
                    continue;
                }
            }
        }
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let notification = JsonRpcNotification::new(method, params);
        let mut line = serde_json::to_string(&notification).map_err(GagentError::Json)?;
        line.push('\n');

        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(GagentError::Io)?;
        self.stdin.flush().await.map_err(GagentError::Io)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that McpClient::connect returns an error when the command doesn't exist.
    #[tokio::test]
    async fn test_connect_nonexistent_command() {
        let result = McpClient::connect("nonexistent-mcp-server-xyz", &[]).await;
        assert!(result.is_err());
        match result {
            Err(e) => assert!(e.to_string().contains("nonexistent-mcp-server-xyz")),
            Ok(_) => panic!("Expected error"),
        }
    }
}
