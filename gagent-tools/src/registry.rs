use crate::definition::{Tool, ToolDefinition, ToolResult};
use gagent_core::GagentError;
use std::collections::HashMap;

/// Registry that holds all available tools for the agent.
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Overwrites any existing tool with the same name.
    pub fn register(&mut self, tool: Box<dyn Tool>) {
        let name = tool.definition().name.clone();
        self.tools.insert(name, tool);
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    /// List all tool definitions (for sending to the LLM).
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    /// Execute a tool by name.
    pub async fn execute(
        &self,
        name: &str,
        params: HashMap<String, serde_json::Value>,
        context: &crate::definition::ToolContext,
    ) -> Result<ToolResult, GagentError> {
        let tool = self
            .tools
            .get(name)
            .ok_or_else(|| GagentError::Tool(format!("Unknown tool: {name}")))?;
        tool.execute(params, context).await
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_registry() {
        let registry = ToolRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.definitions().len(), 0);
        assert!(registry.get("nonexistent").is_none());
    }
}
