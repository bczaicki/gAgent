use async_trait::async_trait;
use crate::message::{ChatMessage, StreamChunk};
use gagent_core::GagentError;
use futures::stream::BoxStream;

/// Tool definition in the format expected by LLM APIs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmToolDefinition {
    pub r#type: String,
    pub function: LlmFunctionDefinition,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LlmFunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Options for an LLM chat request.
#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<LlmToolDefinition>,
    pub temperature: Option<f64>,
    pub stream: bool,
}

/// A complete (non-streaming) LLM response.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub message: ChatMessage,
    pub total_tokens: Option<u64>,
}

/// Trait for LLM providers (Ollama, OpenAI-compatible, etc.).
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat request and get a complete response.
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, GagentError>;

    /// Send a chat request and get a streaming response.
    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk, GagentError>>, GagentError>;

    /// Get the provider name (e.g., "ollama").
    fn name(&self) -> &str;

    /// Get the model name being used.
    fn model(&self) -> &str;
}
