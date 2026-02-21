use serde::{Deserialize, Serialize};

/// Agent identity loaded from bootstrap files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdentity {
    /// Display name of the agent.
    pub name: String,

    /// Emoji/icon for the agent.
    pub emoji: String,

    /// Personality description (from SOUL.md).
    pub personality: Option<String>,

    /// User profile (from USER.md).
    pub user_profile: Option<String>,
}

impl Default for AgentIdentity {
    fn default() -> Self {
        Self {
            name: "gAgent".to_string(),
            emoji: "\u{1f331}".to_string(), // 🌱
            personality: None,
            user_profile: None,
        }
    }
}

impl AgentIdentity {
    /// Format the agent's display prefix (e.g. "🌱 gAgent").
    pub fn display_prefix(&self) -> String {
        format!("{} {}", self.emoji, self.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_identity() {
        let id = AgentIdentity::default();
        assert_eq!(id.name, "gAgent");
        assert_eq!(id.display_prefix(), "\u{1f331} gAgent");
    }
}
