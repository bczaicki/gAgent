use serde::{Deserialize, Serialize};

/// Role in a chat conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A tool call requested by the assistant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Tool function name.
    pub name: String,

    /// Arguments as a JSON object.
    pub arguments: serde_json::Value,
}

/// A chat message in the conversation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub content: String,

    /// Tool calls (only present for assistant messages).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_calls: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_calls: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_calls: None,
        }
    }

    pub fn tool(content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: content.into(),
            tool_calls: None,
        }
    }

    /// Approximate character count for context budgeting.
    pub fn char_count(&self) -> usize {
        self.content.len()
    }
}

/// A streamed chunk from the LLM.
#[derive(Debug, Clone)]
pub enum StreamChunk {
    /// A text delta.
    Text(String),

    /// A tool call being built up.
    ToolCallStart {
        name: String,
    },

    /// Partial arguments for an in-progress tool call.
    ToolCallDelta {
        arguments_delta: String,
    },

    /// The response is complete.
    Done {
        /// Total tokens used (if reported).
        total_tokens: Option<u64>,
    },
}
