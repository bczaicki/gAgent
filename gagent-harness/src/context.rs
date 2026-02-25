use gagent_core::Config;
use gagent_llm::ChatMessage;

/// Context manager for handling message history and compaction.
pub struct ContextManager {
    max_messages: usize,
    max_chars: usize,
}

impl ContextManager {
    /// Create a new context manager from config.
    pub fn from_config(config: &Config) -> Self {
        Self {
            max_messages: config.session.max_messages,
            max_chars: config.session.max_context_chars,
        }
    }

    /// Create a context manager with specific limits.
    pub fn new(max_messages: usize, max_chars: usize) -> Self {
        Self {
            max_messages,
            max_chars,
        }
    }

    /// Check if messages need compaction based on limits.
    pub fn needs_compaction(&self, messages: &[ChatMessage]) -> bool {
        if messages.len() > self.max_messages {
            return true;
        }

        let total_chars: usize = messages.iter().map(|m| m.char_count()).sum();
        total_chars > self.max_chars
    }

    /// Compact messages by keeping system prompt and recent messages.
    ///
    /// Strategy:
    /// 1. Always keep the system prompt (first message if it's a system message)
    /// 2. Keep the most recent messages up to the limit
    /// 3. Add a marker message indicating compaction occurred
    pub fn compact(&self, messages: &[ChatMessage]) -> Vec<ChatMessage> {
        if !self.needs_compaction(messages) {
            return messages.to_vec();
        }

        let mut result = Vec::new();

        // Keep system prompt if present
        if let Some(first) = messages.first() {
            if matches!(first.role, gagent_llm::Role::System) {
                result.push(first.clone());
            }
        }

        // Calculate how many recent messages to keep
        // Start from the end and work backwards until we hit limits
        let start_index = if result.is_empty() { 0 } else { 1 };
        let remaining_messages = &messages[start_index..];

        let mut kept_messages = Vec::new();
        let mut current_chars = result.iter().map(|m| m.char_count()).sum::<usize>();
        let mut current_count = result.len();

        // Iterate from the end backwards
        for message in remaining_messages.iter().rev() {
            let msg_chars = message.char_count();

            // Check if adding this message would exceed limits
            if current_count + 1 > self.max_messages
                || current_chars + msg_chars > self.max_chars
            {
                break;
            }

            kept_messages.push(message.clone());
            current_chars += msg_chars;
            current_count += 1;
        }

        // Reverse to restore chronological order
        kept_messages.reverse();

        // Add compaction marker if we dropped messages
        let dropped_count = remaining_messages.len() - kept_messages.len();
        if dropped_count > 0 {
            result.push(ChatMessage::system(format!(
                "[Context compacted: {} earlier messages omitted]",
                dropped_count
            )));
        }

        // Add the kept messages
        result.extend(kept_messages);

        tracing::info!(
            "Compacted context: {} -> {} messages, dropped {}",
            messages.len(),
            result.len(),
            dropped_count
        );

        result
    }

    /// Approximate token count from character count.
    ///
    /// Uses a simple heuristic: ~4 characters per token on average.
    pub fn estimate_tokens(messages: &[ChatMessage]) -> usize {
        let total_chars: usize = messages.iter().map(|m| m.char_count()).sum();
        (total_chars as f64 / 4.0).ceil() as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gagent_llm::Role;

    #[test]
    fn test_needs_compaction_by_count() {
        let manager = ContextManager::new(5, 10000);

        let mut messages = Vec::new();
        for i in 0..3 {
            messages.push(ChatMessage::user(format!("Message {}", i)));
        }

        assert!(!manager.needs_compaction(&messages));

        // Add more messages to exceed limit
        for i in 3..7 {
            messages.push(ChatMessage::user(format!("Message {}", i)));
        }

        assert!(manager.needs_compaction(&messages));
    }

    #[test]
    fn test_needs_compaction_by_chars() {
        let manager = ContextManager::new(100, 50);

        let messages = vec![
            ChatMessage::user("Short"),
            ChatMessage::user("This is a much longer message that exceeds the character limit"),
        ];

        assert!(manager.needs_compaction(&messages));
    }

    #[test]
    fn test_compact_keeps_system_prompt() {
        let manager = ContextManager::new(3, 10000);

        let messages = vec![
            ChatMessage::system("System prompt"),
            ChatMessage::user("Msg 1"),
            ChatMessage::user("Msg 2"),
            ChatMessage::user("Msg 3"),
            ChatMessage::user("Msg 4"),
            ChatMessage::user("Msg 5"),
        ];

        let compacted = manager.compact(&messages);

        // Should keep system prompt + compaction marker + 2 recent messages = 4 total
        // (limited by max_messages = 3, but system + marker don't count toward limit)
        assert!(compacted.len() <= 5);
        assert_eq!(compacted[0].content, "System prompt");
        assert!(compacted.iter().any(|m| m.content.contains("compacted")));
    }

    #[test]
    fn test_compact_no_compaction_needed() {
        let manager = ContextManager::new(10, 10000);

        let messages = vec![
            ChatMessage::system("System"),
            ChatMessage::user("User message"),
            ChatMessage::assistant("Assistant response"),
        ];

        let compacted = manager.compact(&messages);
        assert_eq!(compacted.len(), messages.len());
        assert_eq!(compacted, messages);
    }

    #[test]
    fn test_compact_keeps_recent_messages() {
        let manager = ContextManager::new(4, 10000);

        let messages = vec![
            ChatMessage::system("System"),
            ChatMessage::user("Msg 1"),
            ChatMessage::user("Msg 2"),
            ChatMessage::user("Msg 3"),
            ChatMessage::user("Msg 4"),
            ChatMessage::user("Msg 5"),
        ];

        let compacted = manager.compact(&messages);

        // System + marker + recent messages
        assert!(compacted.len() <= 5);
        assert_eq!(compacted[0].content, "System");

        // Should contain most recent messages
        let last = compacted.last().unwrap();
        assert_eq!(last.content, "Msg 5");
    }

    #[test]
    fn test_estimate_tokens() {
        let messages = vec![
            ChatMessage::user("1234"), // 4 chars = ~1 token
            ChatMessage::user("12345678"), // 8 chars = ~2 tokens
        ];

        let tokens = ContextManager::estimate_tokens(&messages);
        // 12 chars / 4 = 3 tokens
        assert_eq!(tokens, 3);
    }

    #[test]
    fn test_compact_by_char_limit() {
        let manager = ContextManager::new(100, 60); // 60 char limit

        let messages = vec![
            ChatMessage::system("System"), // 6 chars
            ChatMessage::user("Message one"), // 11 chars
            ChatMessage::user("Message two"), // 11 chars
            ChatMessage::user("Message three"), // 13 chars
            ChatMessage::user("Message four"), // 12 chars
            ChatMessage::user("Last"), // 4 chars
        ];
        // Total: 6 + 11 + 11 + 13 + 12 + 4 = 57 chars, under limit
        // But let's make it exceed: 6 + 11*4 + 13 + 4 = 67 chars
        let messages = vec![
            ChatMessage::system("System"), // 6 chars
            ChatMessage::user("Message one!"), // 12 chars
            ChatMessage::user("Message two!"), // 12 chars
            ChatMessage::user("Message three!"), // 14 chars
            ChatMessage::user("Message four!"), // 13 chars
            ChatMessage::user("Last"), // 4 chars
        ];
        // Total: 6 + 12 + 12 + 14 + 13 + 4 = 61 chars, exceeds 60 limit

        let compacted = manager.compact(&messages);

        // Should have dropped messages (compacted should have fewer than original)
        // Original: 6 messages, Compacted: System + Marker + kept messages
        // Since we drop messages and add a marker, the count might be similar
        // Better assertion: check that compaction marker exists
        assert!(compacted.iter().any(|m| m.content.contains("compacted")));
        // Should keep system prompt
        assert_eq!(compacted[0].content, "System");
        // Should keep most recent message
        assert_eq!(compacted.last().unwrap().content, "Last");
    }
}
