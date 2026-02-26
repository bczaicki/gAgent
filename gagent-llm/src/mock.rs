use crate::{
    provider::{ChatRequest, ChatResponse, LlmProvider},
    ChatMessage, Role, StreamChunk,
};
use async_trait::async_trait;
use futures::stream::BoxStream;
use gagent_core::{GagentError, Result};

/// Mock LLM provider for testing.
///
/// Returns pre-configured responses in order.
pub struct MockProvider {
    responses: Vec<String>,
    call_count: std::sync::Arc<std::sync::Mutex<usize>>,
}

impl MockProvider {
    /// Create a new mock provider with the given responses.
    ///
    /// Each call to `chat()` will return the next response in order.
    /// If all responses are exhausted, returns "Mock response".
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses,
            call_count: std::sync::Arc::new(std::sync::Mutex::new(0)),
        }
    }

    /// Create a mock provider that always returns the same response.
    pub fn with_response(response: impl Into<String>) -> Self {
        Self::new(vec![response.into()])
    }

    /// Get the number of times the provider was called.
    pub fn call_count(&self) -> usize {
        *self.call_count.lock().unwrap()
    }
}

#[async_trait]
impl LlmProvider for MockProvider {
    async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse> {
        let mut count = self.call_count.lock().unwrap();
        let index = *count;
        *count += 1;
        drop(count);

        let content = if index < self.responses.len() {
            self.responses[index].clone()
        } else {
            "Mock response".to_string()
        };

        Ok(ChatResponse {
            message: ChatMessage {
                role: Role::Assistant,
                content,
                tool_calls: None,
            },
            total_tokens: Some(100),
        })
    }

    async fn chat_stream(&self, request: ChatRequest) -> std::result::Result<BoxStream<'static, std::result::Result<StreamChunk, GagentError>>, GagentError> {
        // For mock, just return the response as a single chunk
        let response = self.chat(request).await?;
        let content = response.message.content;

        let stream = futures::stream::once(async move {
            Ok(StreamChunk::Text(content))
        });

        Ok(Box::pin(stream))
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        "mock-model"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_mock_provider() {
        let provider = MockProvider::new(vec!["First".to_string(), "Second".to_string()]);

        let request = ChatRequest {
            messages: vec![ChatMessage::user("test")],
            tools: vec![],
            temperature: None,
            stream: false,
        };

        let response1 = provider.chat(request.clone()).await.unwrap();
        assert_eq!(response1.message.content, "First");

        let response2 = provider.chat(request.clone()).await.unwrap();
        assert_eq!(response2.message.content, "Second");

        // Exhausted responses, should return default
        let response3 = provider.chat(request.clone()).await.unwrap();
        assert_eq!(response3.message.content, "Mock response");

        assert_eq!(provider.call_count(), 3);
    }

    #[tokio::test]
    async fn test_mock_with_response() {
        let provider = MockProvider::with_response("Hello");

        let request = ChatRequest {
            messages: vec![ChatMessage::user("test")],
            tools: vec![],
            temperature: None,
            stream: false,
        };

        let response = provider.chat(request).await.unwrap();
        assert_eq!(response.message.content, "Hello");
    }

    #[tokio::test]
    async fn test_mock_stream() {
        let provider = MockProvider::with_response("Streamed content");

        let request = ChatRequest {
            messages: vec![ChatMessage::user("test")],
            tools: vec![],
            temperature: None,
            stream: true,
        };

        let mut stream = provider.chat_stream(request).await.unwrap();
        let chunk = stream.next().await.unwrap().unwrap();

        match chunk {
            StreamChunk::Text(text) => {
                assert_eq!(text, "Streamed content");
            }
            _ => panic!("Expected text chunk"),
        }
    }
}
