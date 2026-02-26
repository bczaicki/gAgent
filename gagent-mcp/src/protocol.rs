//! JSON-RPC 2.0 and MCP message types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── JSON-RPC 2.0 ────────────────────────────────────────────────────────────

/// An outgoing JSON-RPC request.
#[derive(Debug, Serialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    pub params: Value,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

/// An outgoing JSON-RPC notification (no id, no response expected).
#[derive(Debug, Serialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: &'static str,
    pub method: String,
    pub params: Value,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            method: method.into(),
            params,
        }
    }
}

/// An incoming JSON-RPC response.
#[derive(Debug, Deserialize)]
pub struct JsonRpcResponse {
    pub id: Option<u64>,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error object.
#[derive(Debug, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
}

// ── MCP-specific types ───────────────────────────────────────────────────────

/// A tool discovered via `tools/list`.
#[derive(Debug, Clone, Deserialize)]
pub struct McpTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Value,
}

/// Response to `tools/list`.
#[derive(Debug, Deserialize)]
pub struct McpToolsListResult {
    pub tools: Vec<McpTool>,
}

/// A single content block from a `tools/call` response.
#[derive(Debug, Deserialize)]
pub struct McpContentBlock {
    #[serde(rename = "type")]
    pub content_type: String,
    #[serde(default)]
    pub text: String,
}

/// Response to `tools/call`.
#[derive(Debug, Deserialize)]
pub struct McpToolCallResult {
    pub content: Vec<McpContentBlock>,
    #[serde(rename = "isError", default)]
    pub is_error: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = JsonRpcRequest::new(1, "tools/list", serde_json::json!({}));
        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["jsonrpc"], "2.0");
        assert_eq!(json["id"], 1);
        assert_eq!(json["method"], "tools/list");
    }

    #[test]
    fn test_response_deserialization() {
        let json = r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#;
        let resp: JsonRpcResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.id, Some(1));
        assert!(resp.result.is_some());
        assert!(resp.error.is_none());
    }

    #[test]
    fn test_tool_deserialization() {
        let json = r#"{"name":"my_tool","description":"Does something","inputSchema":{"type":"object","properties":{}}}"#;
        let tool: McpTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.name, "my_tool");
        assert_eq!(tool.description, "Does something");
    }
}
