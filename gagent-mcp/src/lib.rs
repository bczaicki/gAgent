//! MCP gateway: tool bridge to external MCP servers.
//!
//! This crate implements the Model Context Protocol (MCP) client side.
//! It spawns MCP server processes and communicates via JSON-RPC 2.0 over stdio,
//! then bridges discovered tools into the gAgent ToolRegistry.

pub mod mcp_bridge;
pub mod mcp_client;
pub mod protocol;

pub use mcp_bridge::{McpBridge, McpServerConfig, parse_mcp_servers};
pub use mcp_client::McpClient;
