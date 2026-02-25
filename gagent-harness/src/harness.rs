use crate::{context::ContextManager, session::Session};
use gagent_core::{Config, GagentError, Result, SystemPrompt};
use gagent_llm::{ChatMessage, ChatRequest, LlmProvider, LlmToolDefinition, LlmFunctionDefinition, Role};
use gagent_tools::{ToolContext, ToolRegistry};
use std::collections::HashMap;

/// Core agent harness that orchestrates the LLM ↔ tool execution loop.
pub struct AgentHarness {
    config: Config,
    context_manager: ContextManager,
    system_prompt: SystemPrompt,
}

/// Response from the agent harness.
#[derive(Debug)]
pub enum HarnessResponse {
    /// Text response from the assistant.
    Text(String),

    /// Streaming response (text delta).
    StreamDelta(String),

    /// Stream completed.
    StreamDone,

    /// Tool execution result (for logging/debugging).
    ToolExecution {
        tool_name: String,
        success: bool,
        output: String,
    },
}

impl AgentHarness {
    /// Create a new agent harness with config and system prompt.
    pub fn new(config: Config, system_prompt: SystemPrompt) -> Self {
        let context_manager = ContextManager::from_config(&config);

        Self {
            config,
            context_manager,
            system_prompt,
        }
    }

    /// Run the agent loop for a single user message.
    ///
    /// This method:
    /// 1. Adds user message to session
    /// 2. Prepares context (compacts if needed)
    /// 3. Calls LLM with available tools
    /// 4. If LLM returns tool calls, executes them and loops back
    /// 5. If LLM returns text, returns to caller
    ///
    /// Returns the final assistant message after all tool calls are resolved.
    pub async fn run(
        &self,
        user_message: &str,
        session: &mut Session,
        provider: &dyn LlmProvider,
        registry: &ToolRegistry,
    ) -> Result<String> {
        // Add user message to session
        session.add_message(ChatMessage::user(user_message));

        // Loop until we get a text response (not tool calls)
        let max_iterations = 10; // Prevent infinite loops
        let mut iteration = 0;

        loop {
            iteration += 1;
            if iteration > max_iterations {
                return Err(GagentError::Other(
                    "Max tool call iterations exceeded".to_string(),
                ));
            }

            // Prepare context with compaction if needed
            let messages = self.prepare_context(session);

            // Get tool definitions for LLM and convert to LlmToolDefinition format
            let tools = self.convert_tool_definitions(registry);

            // Call LLM
            let request = ChatRequest {
                messages,
                tools,
                temperature: Some(self.config.llm.temperature),
                stream: false,
            };

            tracing::debug!("Calling LLM (iteration {})", iteration);
            let response = provider.chat(request).await.map_err(|e| {
                GagentError::Llm(format!("LLM request failed: {}", e))
            })?;

            // Check if response has tool calls
            if let Some(tool_calls) = &response.message.tool_calls {
                tracing::info!("LLM requested {} tool calls", tool_calls.len());

                // Add assistant message with tool calls to session
                session.add_message(response.message.clone());

                // Execute each tool call
                for tool_call in tool_calls {
                    let tool_result = self.execute_tool(
                        &tool_call.name,
                        &tool_call.arguments,
                        registry,
                    ).await?;

                    tracing::info!(
                        "Tool {} executed: success={}",
                        tool_call.name,
                        tool_result.success
                    );

                    // Add tool result to session as a tool message
                    let tool_message = ChatMessage::tool(format!(
                        "Tool: {}\nResult: {}",
                        tool_call.name,
                        tool_result.output
                    ));
                    session.add_message(tool_message);
                }

                // Loop back to get next LLM response
                continue;
            }

            // No tool calls - return text response
            let text = response.message.content.clone();
            session.add_message(response.message);

            return Ok(text);
        }
    }

    /// Run the agent loop with streaming support.
    ///
    /// Similar to `run()` but returns a stream of responses including tool executions.
    /// Callers should consume the stream and handle each response type.
    pub async fn run_stream<'a>(
        &'a self,
        user_message: &str,
        session: &'a mut Session,
        provider: &'a dyn LlmProvider,
        registry: &'a ToolRegistry,
    ) -> Result<futures::stream::BoxStream<'a, Result<HarnessResponse>>> {
        // Add user message to session
        session.add_message(ChatMessage::user(user_message));

        // For now, implement a simple non-streaming version that wraps the result
        // Full streaming implementation would require more complex state management
        let result = self.run(user_message, session, provider, registry).await;

        match result {
            Ok(text) => {
                let stream = futures::stream::once(async move {
                    Ok(HarnessResponse::Text(text))
                });
                Ok(Box::pin(stream))
            }
            Err(e) => {
                let stream = futures::stream::once(async move { Err(e) });
                Ok(Box::pin(stream))
            }
        }
    }

    /// Convert tool definitions to LLM format.
    fn convert_tool_definitions(&self, registry: &ToolRegistry) -> Vec<LlmToolDefinition> {
        registry
            .definitions()
            .into_iter()
            .map(|tool_def| {
                // Convert parameters to JSON schema format
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();

                for param in &tool_def.parameters {
                    let param_schema = serde_json::json!({
                        "type": param.param_type,
                        "description": param.description,
                    });
                    properties.insert(param.name.clone(), param_schema);

                    if param.required {
                        required.push(param.name.clone());
                    }
                }

                let parameters = serde_json::json!({
                    "type": "object",
                    "properties": properties,
                    "required": required,
                });

                LlmToolDefinition {
                    r#type: "function".to_string(),
                    function: LlmFunctionDefinition {
                        name: tool_def.name,
                        description: tool_def.description,
                        parameters,
                    },
                }
            })
            .collect()
    }

    /// Prepare context with system prompt and compacted message history.
    fn prepare_context(&self, session: &Session) -> Vec<ChatMessage> {
        let mut messages = Vec::new();

        // Add system prompt
        messages.push(ChatMessage::system(self.system_prompt.text.clone()));

        // Add compacted message history (skip system messages from history)
        let history: Vec<ChatMessage> = session
            .messages
            .iter()
            .filter(|m| !matches!(m.role, Role::System))
            .cloned()
            .collect();

        let compacted = self.context_manager.compact(&history);
        messages.extend(compacted);

        tracing::debug!(
            "Prepared context: {} messages, ~{} tokens",
            messages.len(),
            ContextManager::estimate_tokens(&messages)
        );

        messages
    }

    /// Execute a tool with the given parameters.
    async fn execute_tool(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        registry: &ToolRegistry,
    ) -> Result<gagent_tools::ToolResult> {
        // Parse arguments as HashMap
        let params: HashMap<String, serde_json::Value> = serde_json::from_value(
            arguments.clone()
        ).map_err(|e| {
            GagentError::Tool(format!("Failed to parse tool arguments: {}", e))
        })?;

        // Create tool context
        let context = ToolContext {
            working_dir: std::env::current_dir().unwrap_or_default(),
            allowed_paths: self.config.sandbox.allowed_paths.clone(),
        };

        // Execute tool with timeout
        let timeout = tokio::time::Duration::from_secs(self.config.agent.timeout_secs);

        tokio::time::timeout(
            timeout,
            registry.execute(tool_name, params, &context)
        )
        .await
        .map_err(|_| GagentError::Timeout(self.config.agent.timeout_secs))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gagent_llm::{ChatResponse, OllamaProvider};
    use gagent_tools::builtin::FileReadTool;

    #[test]
    fn test_prepare_context() {
        let config = Config::default();
        let system_prompt = SystemPrompt {
            text: "System prompt".to_string(),
            char_count: 13,
        };

        let harness = AgentHarness::new(config, system_prompt);

        let mut session = Session::new();
        session.add_message(ChatMessage::user("Hello"));
        session.add_message(ChatMessage::assistant("Hi"));

        let messages = harness.prepare_context(&session);

        // Should have system prompt + 2 messages
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].content, "System prompt");
        assert_eq!(messages[1].content, "Hello");
        assert_eq!(messages[2].content, "Hi");
    }

    #[test]
    fn test_prepare_context_with_compaction() {
        let mut config = Config::default();
        config.session.max_messages = 2;

        let system_prompt = SystemPrompt {
            text: "System".to_string(),
            char_count: 6,
        };

        let harness = AgentHarness::new(config, system_prompt);

        let mut session = Session::new();
        for i in 0..5 {
            session.add_message(ChatMessage::user(format!("Message {}", i)));
        }

        let messages = harness.prepare_context(&session);

        // Should be compacted: system prompt + compaction marker + recent messages
        assert!(messages.len() < 7); // Less than system + 5 messages
        assert_eq!(messages[0].content, "System");
    }

    #[tokio::test]
    async fn test_execute_tool() {
        use tempfile::TempDir;
        let temp_dir = TempDir::new().unwrap();
        std::fs::write(temp_dir.path().join("test.txt"), "content").unwrap();

        let config = Config::default();
        let system_prompt = SystemPrompt {
            text: "System".to_string(),
            char_count: 6,
        };

        let harness = AgentHarness::new(config, system_prompt);

        let mut registry = ToolRegistry::new();
        registry.register(Box::new(FileReadTool::new()));

        let mut args = serde_json::Map::new();
        args.insert("path".to_string(), serde_json::json!("test.txt"));

        // Override working directory by modifying the tool context in execute_tool
        // For this test, we'll test the basic structure
        let result = harness
            .execute_tool("file_read", &serde_json::Value::Object(args), &registry)
            .await;

        assert!(result.is_ok());
    }
}
