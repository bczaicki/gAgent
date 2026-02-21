use async_trait::async_trait;
use futures::stream::BoxStream;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::message::{ChatMessage, Role, StreamChunk, ToolCall};
use crate::provider::{ChatRequest, ChatResponse, LlmProvider, LlmToolDefinition};
use gagent_core::GagentError;

/// Ollama LLM provider using the /api/chat endpoint.
pub struct OllamaProvider {
    client: Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.into(),
            model: model.into(),
        }
    }
}

// --- Ollama API types ---

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<LlmToolDefinition>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaToolCall {
    function: OllamaFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    #[serde(default)]
    message: Option<OllamaMessage>,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    eval_count: Option<u64>,
    #[serde(default)]
    prompt_eval_count: Option<u64>,
}

// --- Conversions ---

fn to_ollama_messages(messages: &[ChatMessage]) -> Vec<OllamaMessage> {
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
                    .map(|tc| OllamaToolCall {
                        function: OllamaFunctionCall {
                            name: tc.name.clone(),
                            arguments: tc.arguments.clone(),
                        },
                    })
                    .collect()
            });
            OllamaMessage {
                role: role.to_string(),
                content: m.content.clone(),
                tool_calls,
            }
        })
        .collect()
}

fn from_ollama_message(msg: OllamaMessage) -> ChatMessage {
    let role = match msg.role.as_str() {
        "system" => Role::System,
        "user" => Role::User,
        "assistant" => Role::Assistant,
        "tool" => Role::Tool,
        _ => Role::Assistant,
    };
    let tool_calls = msg.tool_calls.map(|calls| {
        calls
            .into_iter()
            .map(|tc| ToolCall {
                name: tc.function.name,
                arguments: tc.function.arguments,
            })
            .collect()
    });
    ChatMessage {
        role,
        content: msg.content,
        tool_calls,
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse, GagentError> {
        let url = format!("{}/api/chat", self.base_url);
        let ollama_req = OllamaChatRequest {
            model: self.model.clone(),
            messages: to_ollama_messages(&request.messages),
            stream: false,
            options: request.temperature.map(|t| OllamaOptions {
                temperature: Some(t),
            }),
            tools: if request.tools.is_empty() {
                None
            } else {
                Some(request.tools)
            },
        };

        debug!("Sending chat request to Ollama: {url}");

        let resp = self
            .client
            .post(&url)
            .json(&ollama_req)
            .send()
            .await
            .map_err(|e| GagentError::Http(format!("Ollama request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "no body".to_string());
            return Err(GagentError::Llm(format!(
                "Ollama returned {status}: {body}"
            )));
        }

        let ollama_resp: OllamaChatResponse = resp
            .json()
            .await
            .map_err(|e| GagentError::Llm(format!("Failed to parse Ollama response: {e}")))?;

        let message = ollama_resp
            .message
            .map(from_ollama_message)
            .unwrap_or_else(|| ChatMessage::assistant(""));

        let total_tokens = match (ollama_resp.eval_count, ollama_resp.prompt_eval_count) {
            (Some(eval), Some(prompt)) => Some(eval + prompt),
            (Some(eval), None) => Some(eval),
            _ => None,
        };

        Ok(ChatResponse {
            message,
            total_tokens,
        })
    }

    async fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> Result<BoxStream<'static, Result<StreamChunk, GagentError>>, GagentError> {
        let url = format!("{}/api/chat", self.base_url);
        let ollama_req = OllamaChatRequest {
            model: self.model.clone(),
            messages: to_ollama_messages(&request.messages),
            stream: true,
            options: request.temperature.map(|t| OllamaOptions {
                temperature: Some(t),
            }),
            tools: if request.tools.is_empty() {
                None
            } else {
                Some(request.tools)
            },
        };

        debug!("Sending streaming chat request to Ollama: {url}");

        let resp = self
            .client
            .post(&url)
            .json(&ollama_req)
            .send()
            .await
            .map_err(|e| GagentError::Http(format!("Ollama streaming request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "no body".to_string());
            return Err(GagentError::Llm(format!(
                "Ollama returned {status}: {body}"
            )));
        }

        let stream = resp.bytes_stream();

        let mapped = stream.map(move |chunk_result| {
            let chunk = chunk_result.map_err(|e| {
                GagentError::Http(format!("Stream error: {e}"))
            })?;

            let text = String::from_utf8_lossy(&chunk);

            // Ollama streams newline-delimited JSON
            // A single chunk may contain multiple JSON lines
            let mut last_result: Option<Result<StreamChunk, GagentError>> = None;

            for line in text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                match serde_json::from_str::<OllamaChatResponse>(line) {
                    Ok(resp) => {
                        if resp.done {
                            let total_tokens =
                                match (resp.eval_count, resp.prompt_eval_count) {
                                    (Some(eval), Some(prompt)) => Some(eval + prompt),
                                    (Some(eval), None) => Some(eval),
                                    _ => None,
                                };
                            last_result = Some(Ok(StreamChunk::Done { total_tokens }));
                        } else if let Some(msg) = resp.message {
                            if let Some(tool_calls) = &msg.tool_calls {
                                if let Some(tc) = tool_calls.first() {
                                    last_result = Some(Ok(StreamChunk::ToolCallStart {
                                        name: tc.function.name.clone(),
                                    }));
                                }
                            } else if !msg.content.is_empty() {
                                last_result = Some(Ok(StreamChunk::Text(msg.content)));
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to parse Ollama stream chunk: {e}");
                        last_result =
                            Some(Err(GagentError::Llm(format!("Stream parse error: {e}"))));
                    }
                }
            }

            last_result.unwrap_or(Ok(StreamChunk::Text(String::new())))
        });

        Ok(Box::pin(mapped))
    }

    fn name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_conversion() {
        let messages = vec![
            ChatMessage::system("You are helpful."),
            ChatMessage::user("Hello"),
        ];
        let ollama_msgs = to_ollama_messages(&messages);
        assert_eq!(ollama_msgs.len(), 2);
        assert_eq!(ollama_msgs[0].role, "system");
        assert_eq!(ollama_msgs[1].role, "user");
        assert_eq!(ollama_msgs[1].content, "Hello");
    }

    #[test]
    fn test_ollama_response_parsing() {
        let json = r#"{"model":"llama3.2","message":{"role":"assistant","content":"Hi there!"},"done":true,"eval_count":10,"prompt_eval_count":5}"#;
        let resp: OllamaChatResponse = serde_json::from_str(json).unwrap();
        assert!(resp.done);
        let msg = resp.message.unwrap();
        assert_eq!(msg.content, "Hi there!");
        assert_eq!(resp.eval_count, Some(10));
    }

    #[test]
    fn test_provider_creation() {
        let provider = OllamaProvider::new("http://localhost:11434", "llama3.2");
        assert_eq!(provider.name(), "ollama");
        assert_eq!(provider.model(), "llama3.2");
    }
}
