use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Describes a tool parameter for the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParam {
    pub name: String,
    pub description: String,
    pub param_type: String,
    pub required: bool,
}

/// JSON-schema-like definition of a tool, sent to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParam>,
}

/// Context passed to tool execution.
#[derive(Debug, Clone)]
pub struct ToolContext {
    /// Current working directory for the tool.
    pub working_dir: std::path::PathBuf,

    /// Allowed paths for file operations (empty = no restriction).
    pub allowed_paths: Vec<std::path::PathBuf>,
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
}

/// Trait that all tools must implement.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Return the tool definition (name, description, parameters).
    fn definition(&self) -> ToolDefinition;

    /// Execute the tool with the given parameters.
    async fn execute(
        &self,
        params: HashMap<String, serde_json::Value>,
        context: &ToolContext,
    ) -> Result<ToolResult, gagent_core::GagentError>;
}
