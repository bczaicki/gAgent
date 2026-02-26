use gagent_core::{GagentError, Result};
use gagent_llm::ChatMessage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// A chat session with message history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session ID.
    pub id: String,

    /// Message history (including system prompt, user messages, assistant messages, tool results).
    pub messages: Vec<ChatMessage>,

    /// Path to the JSONL file where this session is persisted.
    #[serde(skip)]
    pub file_path: Option<PathBuf>,
}

impl Session {
    /// Create a new session with a generated ID.
    pub fn new() -> Self {
        let id = generate_session_id();
        Self {
            id,
            messages: Vec::new(),
            file_path: None,
        }
    }

    /// Create a session with a specific ID.
    pub fn with_id(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            messages: Vec::new(),
            file_path: None,
        }
    }

    /// Add a message to the session.
    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    /// Get the total number of messages.
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Get the total character count across all messages.
    pub fn total_chars(&self) -> usize {
        self.messages.iter().map(|m| m.char_count()).sum()
    }

    /// Save the session to a JSONL file.
    ///
    /// Each message is written as a separate line.
    pub async fn save(&self, path: &Path) -> Result<()> {
        let mut file = tokio::fs::File::create(path).await?;

        for message in &self.messages {
            let json = serde_json::to_string(message)?;
            file.write_all(json.as_bytes()).await?;
            file.write_all(b"\n").await?;
        }

        file.flush().await?;
        Ok(())
    }

    /// Load a session from a JSONL file.
    pub async fn load(path: &Path) -> Result<Self> {
        let file = tokio::fs::File::open(path).await?;
        let reader = BufReader::new(file);
        let mut lines = reader.lines();

        let mut messages = Vec::new();

        while let Some(line) = lines.next_line().await? {
            if line.trim().is_empty() {
                continue;
            }

            let message: ChatMessage = serde_json::from_str(&line).map_err(|e| {
                GagentError::Session(format!("Failed to parse message from session file: {}", e))
            })?;

            messages.push(message);
        }

        // Extract session ID from filename
        let id = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        Ok(Self {
            id,
            messages,
            file_path: Some(path.to_path_buf()),
        })
    }

    /// Get the default session file path for a session ID.
    pub fn default_path(sessions_dir: &Path, session_id: &str) -> PathBuf {
        sessions_dir.join(format!("{}.jsonl", session_id))
    }

    /// Serialize the session to a compact JSON string (for crash recovery).
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self).map_err(GagentError::Json)
    }

    /// Deserialize a session from a JSON string (from crash recovery).
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json).map_err(|e| {
            GagentError::Session(format!("Failed to parse session JSON: {e}"))
        })
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate a unique session ID using timestamp and random suffix.
fn generate_session_id() -> String {
    use std::time::SystemTime;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Simple random suffix using timestamp's lower bits
    let suffix = timestamp % 10000;

    format!("session-{}-{}", timestamp, suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gagent_llm::Role;
    use tempfile::TempDir;

    #[test]
    fn test_new_session() {
        let session = Session::new();
        assert!(session.id.starts_with("session-"));
        assert_eq!(session.message_count(), 0);
        assert_eq!(session.total_chars(), 0);
    }

    #[test]
    fn test_add_message() {
        let mut session = Session::new();
        session.add_message(ChatMessage::user("Hello"));
        session.add_message(ChatMessage::assistant("Hi there!"));

        assert_eq!(session.message_count(), 2);
        assert!(session.total_chars() > 0);
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("test-session.jsonl");

        let mut session = Session::with_id("test-session");
        session.add_message(ChatMessage::system("System prompt"));
        session.add_message(ChatMessage::user("Hello"));
        session.add_message(ChatMessage::assistant("Hi there!"));

        // Save
        session.save(&session_path).await.unwrap();
        assert!(session_path.exists());

        // Load
        let loaded = Session::load(&session_path).await.unwrap();
        assert_eq!(loaded.id, "test-session");
        assert_eq!(loaded.message_count(), 3);
        assert_eq!(loaded.messages[0].content, "System prompt");
        assert_eq!(loaded.messages[1].content, "Hello");
        assert_eq!(loaded.messages[2].content, "Hi there!");
    }

    #[tokio::test]
    async fn test_load_nonexistent_file() {
        let temp_dir = TempDir::new().unwrap();
        let session_path = temp_dir.path().join("nonexistent.jsonl");

        let result = Session::load(&session_path).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_total_chars() {
        let mut session = Session::new();
        session.add_message(ChatMessage::user("12345")); // 5 chars
        session.add_message(ChatMessage::assistant("1234567890")); // 10 chars

        assert_eq!(session.total_chars(), 15);
    }

    #[test]
    fn test_default_path() {
        let sessions_dir = Path::new("/tmp/sessions");
        let path = Session::default_path(sessions_dir, "test-id");

        assert_eq!(path, Path::new("/tmp/sessions/test-id.jsonl"));
    }
}
