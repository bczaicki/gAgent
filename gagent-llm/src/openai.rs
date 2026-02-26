//! OpenAI-compatible LLM provider.
//!
//! Works with any API that follows the OpenAI `/v1/chat/completions` format —
//! including OpenAI itself, Azure OpenAI, Together AI, Groq, Mistral, etc.

use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::message::{ChatMessage, Role, StreamChunk, ToolCall};
use crate::provider::{ChatRequest, ChatResponse, LlmFunctionDefinition, LlmProvider, LlmToolDefinition};
use gagent_core::GagentError;

/// OpenAI-compatible LLM provider.
pub struct OpenAiProvider {
    client: Client,
    base_url: String,
    model: String,
    api_key: Option<String>,
}

impl OpenAiProvider {
    /// Create a provider pointing at the given base URL.
    ///
    /// For OpenAI: `base_url = "https://api.openai.com"`, provide the API key.
    /// For local servers (LM Studio, Ollama /v1): `base_url = "http://localhost:1234"`.
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: Option<String>,
    ) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            model: model.into(),
            api_key,
        }
    }
}

// ── OpenAI API types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<LlmToolDefinition>>,
    stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiToolCall {
    id: String,
    r#type: String,
    function: OpenAiFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String, // JSON string in OpenAI format
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    total_tokens: Option<u64>,
}

// ── Streaming types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct OpenAiStreamResponse {
    choices: Vec<OpenAiStreamChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamToolCall {
    function: OpenAiStreamFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

// ── Conversions ──────────────────────────────────────────────────────────────

fn to_openai_messages(messages: &[ChatMessage]) -> Vec<OpenAiMessage> {
    messages
        .iter()
        .map(|m| {
            let role = match m.role {
                Role::System => "system",
                Role::User => "user",
                Role::Assistant => "assistant",
                Role::Tool => "tool",
            };

            let tool_calls = m.tool_calls.as_ref().map(|calls| {
                calls
                    .iter()
                    .enumerate()
                    .map(|(i, tc)| OpenAiToolCall {
                        id: format!("call_{i}"),
                        r#type: "function".to_string(),
                        function: OpenAiFunctionCall {
                            name: tc.name.clone(),
                            arguments: tc.arguments.to_string(),
                        },
                    })
                    .collect()
            });

            OpenAiMessage {
                role: role.to_string(),
                content: m.content.clone(),
                tool_calls,
                tool_call_id: None,
            }
        })
        .collect()
}

fn from_openai_message(msg: OpenAiMessage) -> ChatMessage {
    let role = match msg.role.as_str() {
        "system" => Role::System,
        "user" => Role::User,
        "tool" => Role::Tool,
        _ => Role::Assistant,
    };

    let tool_calls = msg.tool_calls.map(|calls| {
        calls
            .into_iter()
            .map(|tc| {
                let arguments =
                    serde_json::from_str(&tc.function.arguments).unwrap_or(serde_json::json!({}));
                ToolCall {
                    name: tc.function.name,
                    arguments,
                }
            })
            .collect()
    });

    ChatMessage {
        role,
        content: msg.content,
        tool_calls,
    }
}

// ── LlmProvider impl ─────────────────────────────────────────────────────────

#[async_trait]
impl LlmProvider for OpenAiProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, GagentError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let openai_req = OpenAiRequest {
            model: self.model.clone(),
            messages: to_openai_messages(&request.messages),
            temperature: request.temperature,
            tools: if request.tools.is_empty() {
                None
            } else {
                Some(request.tools)
            },
            stream: false,
        };

        debug!("Sending chat request to OpenAI-compatible API: {url}");

        let mut req_builder = self.client.post(&url).json(&openai_req);

        if let Some(key) = &self.api_key {
            req_builder = req_builder.bearer_auth(key);
        }

        let resp = req_builder
            .send()
            .await
            .map_err(|e| GagentError::Http(format!("OpenAI request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(GagentError::Llm(format!(
                "OpenAI API returned {status}: {body}"
            )));
        }

        let openai_resp: OpenAiResponse = resp
            .json()
            .await
            .map_err(|e| GagentError::Llm(format!("Failed to parse OpenAI response: {e}")))?;

        let message = openai_resp
            .choices
            .into_iter()
            .next()
            .map(|c| from_openai_message(c.message))
            .unwrap_or_else(|| ChatMessage::assistant(""));

        let total_tokens = openai_resp.usage.and_then(|u| u.total_tokens);

        Ok(ChatResponse {
            message,
            total_tokens,
        })
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk, GagentError>>, GagentError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        let openai_req = OpenAiRequest {
            model: self.model.clone(),
            messages: to_openai_messages(&request.messages),
            temperature: request.temperature,
            tools: if request.tools.is_empty() {
                None
            } else {
                Some(request.tools)
            },
            stream: true,
        };

        debug!("Sending streaming chat request to OpenAI-compatible API: {url}");

        let mut req_builder = self.client.post(&url).json(&openai_req);

        if let Some(key) = &self.api_key {
            req_builder = req_builder.bearer_auth(key);
        }

        let resp = req_builder
            .send()
            .await
            .map_err(|e| GagentError::Http(format!("OpenAI streaming request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(GagentError::Llm(format!(
                "OpenAI API returned {status}: {body}"
            )));
        }

        let stream = resp.bytes_stream();

        let mapped = stream.map(move |chunk_result| {
            let chunk = chunk_result
                .map_err(|e| GagentError::Http(format!("Stream error: {e}")))?;

            let text = String::from_utf8_lossy(&chunk);

            // OpenAI streams SSE: "data: {json}\n\n" or "data: [DONE]\n\n"
            let mut last_result: Option<Result<StreamChunk, GagentError>> = None;

            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() || line == "data: [DONE]" {
                    if line == "data: [DONE]" {
                        last_result = Some(Ok(StreamChunk::Done { total_tokens: None }));
                    }
                    continue;
                }

                let json_str = if let Some(s) = line.strip_prefix("data: ") {
                    s
                } else {
                    continue;
                };

                match serde_json::from_str::<OpenAiStreamResponse>(json_str) {
                    Ok(resp) => {
                        if let Some(choice) = resp.choices.into_iter().next() {
                            if choice.finish_reason.as_deref() == Some("stop") {
                                let total_tokens = resp.usage.and_then(|u| u.total_tokens);
                                last_result = Some(Ok(StreamChunk::Done { total_tokens }));
                            } else if let Some(content) = choice.delta.content {
                                if !content.is_empty() {
                                    last_result = Some(Ok(StreamChunk::Text(content)));
                                }
                            } else if let Some(tool_calls) = choice.delta.tool_calls {
                                if let Some(tc) = tool_calls.into_iter().next() {
                                    if let Some(name) = tc.function.name {
                                        last_result =
                                            Some(Ok(StreamChunk::ToolCallStart { name }));
                                    } else if let Some(args) = tc.function.arguments {
                                        last_result = Some(Ok(StreamChunk::ToolCallDelta {
                                            arguments_delta: args,
                                        }));
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse OpenAI stream chunk: {e}");
                    }
                }
            }

            Ok(last_result
                .unwrap_or(Ok(StreamChunk::Text(String::new())))
                .unwrap_or(StreamChunk::Text(String::new())))
        });

        Ok(Box::pin(mapped))
    }

    fn name(&self) -> &str {
        "openai-compatible"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_creation() {
        let provider = OpenAiProvider::new("https://api.openai.com", "gpt-4o", None);
        assert_eq!(provider.name(), "openai-compatible");
        assert_eq!(provider.model(), "gpt-4o");
    }

    #[test]
    fn test_provider_with_api_key() {
        let provider =
            OpenAiProvider::new("https://api.openai.com", "gpt-4o", Some("sk-test".to_string()));
        assert_eq!(provider.api_key.as_deref(), Some("sk-test"));
    }

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            ChatMessage::system("You are helpful."),
            ChatMessage::user("Hello"),
            ChatMessage::assistant("Hi!"),
        ];
        let openai_msgs = to_openai_messages(&messages);
        assert_eq!(openai_msgs.len(), 3);
        assert_eq!(openai_msgs[0].role, "system");
        assert_eq!(openai_msgs[1].role, "user");
        assert_eq!(openai_msgs[2].role, "assistant");
    }

    #[test]
    fn test_from_openai_message_with_tool_calls() {
        let msg = OpenAiMessage {
            role: "assistant".to_string(),
            content: String::new(),
            tool_calls: Some(vec![OpenAiToolCall {
                id: "call_0".to_string(),
                r#type: "function".to_string(),
                function: OpenAiFunctionCall {
                    name: "my_tool".to_string(),
                    arguments: r#"{"key":"value"}"#.to_string(),
                },
            }]),
            tool_call_id: None,
        };

        let chat_msg = from_openai_message(msg);
        assert_eq!(chat_msg.role, Role::Assistant);
        let calls = chat_msg.tool_calls.unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "my_tool");
        assert_eq!(calls[0].arguments["key"], "value");
    }

    #[test]
    fn test_response_parsing() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {"role": "assistant", "content": "Hello!"},
                "finish_reason": "stop"
            }],
            "usage": {"total_tokens": 15}
        }"#;

        let resp: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.content, "Hello!");
        assert_eq!(resp.usage.unwrap().total_tokens, Some(15));
    }
}
