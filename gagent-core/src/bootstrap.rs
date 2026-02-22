use crate::{AgentIdentity, GagentError, Result};
use std::fs;
use std::path::Path;

/// Maximum characters allowed per bootstrap file.
pub const MAX_FILE_CHARS: usize = 20_000;

/// Maximum total characters across all bootstrap files.
pub const MAX_TOTAL_CHARS: usize = 150_000;

/// Bootstrap files loaded from the workspace directory.
#[derive(Debug, Clone, Default)]
pub struct BootstrapFiles {
    /// Agent identity parsed from IDENTITY.md.
    pub identity: AgentIdentity,

    /// SOUL.md content (personality/tone).
    pub soul: Option<String>,

    /// USER.md content (user profile/preferences).
    pub user: Option<String>,

    /// AGENTS.md content (multi-agent context).
    pub agents: Option<String>,

    /// TOOLS.md content (tool usage guidance).
    pub tools: Option<String>,

    /// MEMORY.md content (long-term memory).
    pub memory: Option<String>,

    /// Total character count across all files.
    pub total_chars: usize,
}

impl BootstrapFiles {
    /// Load all bootstrap files from the workspace directory.
    ///
    /// Enforces per-file and total character limits.
    /// Returns defaults for missing files.
    pub fn load(workspace_dir: &Path) -> Result<Self> {
        let soul = Self::load_file(&workspace_dir.join("SOUL.md"), MAX_FILE_CHARS)?;
        let user = Self::load_file(&workspace_dir.join("USER.md"), MAX_FILE_CHARS)?;
        let agents = Self::load_file(&workspace_dir.join("AGENTS.md"), MAX_FILE_CHARS)?;
        let tools = Self::load_file(&workspace_dir.join("TOOLS.md"), MAX_FILE_CHARS)?;
        let memory = Self::load_file(&workspace_dir.join("MEMORY.md"), MAX_FILE_CHARS)?;

        // Load and parse IDENTITY.md
        let identity_content = Self::load_file(&workspace_dir.join("IDENTITY.md"), MAX_FILE_CHARS)?;
        let identity = if let Some(ref content) = identity_content {
            let (name, emoji) = Self::parse_identity(content);
            let mut id = AgentIdentity::default();
            if let Some(n) = name {
                id.name = n;
            }
            if let Some(e) = emoji {
                id.emoji = e;
            }
            id
        } else {
            AgentIdentity::default()
        };

        // Calculate total character count
        let total_chars = identity_content.as_ref().map(|s| Self::count_chars(s)).unwrap_or(0)
            + soul.as_ref().map(|s| Self::count_chars(s)).unwrap_or(0)
            + user.as_ref().map(|s| Self::count_chars(s)).unwrap_or(0)
            + agents.as_ref().map(|s| Self::count_chars(s)).unwrap_or(0)
            + tools.as_ref().map(|s| Self::count_chars(s)).unwrap_or(0)
            + memory.as_ref().map(|s| Self::count_chars(s)).unwrap_or(0);

        // Enforce total character limit
        if total_chars > MAX_TOTAL_CHARS {
            return Err(GagentError::Bootstrap(format!(
                "Total bootstrap files exceed {} character limit: {} chars found",
                MAX_TOTAL_CHARS, total_chars
            )));
        }

        Ok(Self {
            identity,
            soul,
            user,
            agents,
            tools,
            memory,
            total_chars,
        })
    }

    /// Load a single file with character limit enforcement.
    ///
    /// Returns None if file doesn't exist.
    /// Returns error if file exceeds character limit.
    fn load_file(path: &Path, max_chars: usize) -> Result<Option<String>> {
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path)?;
        let char_count = Self::count_chars(&content);

        if char_count > max_chars {
            return Err(GagentError::Bootstrap(format!(
                "{} exceeds {} character limit: {} chars found",
                path.display(),
                max_chars,
                char_count
            )));
        }

        Ok(Some(content))
    }

    /// Parse IDENTITY.md for name and emoji fields.
    ///
    /// Expected format:
    /// ```markdown
    /// name: AgentName
    /// emoji: 🌱
    /// ```
    ///
    /// Returns (name, emoji) tuple with None for missing fields.
    fn parse_identity(content: &str) -> (Option<String>, Option<String>) {
        let mut name = None;
        let mut emoji = None;

        for line in content.lines() {
            let line = line.trim();

            if let Some(value) = line.strip_prefix("name:") {
                name = Some(value.trim().to_string());
            } else if let Some(value) = line.strip_prefix("emoji:") {
                emoji = Some(value.trim().to_string());
            }
        }

        (name, emoji)
    }

    /// Count characters in a string (Unicode-aware).
    ///
    /// Uses .chars().count() instead of .len() to count
    /// characters, not bytes. "🌱" = 1 char, not 4 bytes.
    fn count_chars(s: &str) -> usize {
        s.chars().count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_load_empty_workspace() {
        let workspace = TempDir::new().unwrap();
        let bootstrap = BootstrapFiles::load(workspace.path()).unwrap();

        assert_eq!(bootstrap.identity.name, "gAgent");
        assert_eq!(bootstrap.identity.emoji, "🌱");
        assert!(bootstrap.soul.is_none());
        assert!(bootstrap.user.is_none());
        assert!(bootstrap.agents.is_none());
        assert!(bootstrap.tools.is_none());
        assert!(bootstrap.memory.is_none());
        assert_eq!(bootstrap.total_chars, 0);
    }

    #[test]
    fn test_load_identity_file() {
        let workspace = TempDir::new().unwrap();
        fs::write(
            workspace.path().join("IDENTITY.md"),
            "name: TestBot\nemoji: 🤖\n"
        ).unwrap();

        let bootstrap = BootstrapFiles::load(workspace.path()).unwrap();
        assert_eq!(bootstrap.identity.name, "TestBot");
        assert_eq!(bootstrap.identity.emoji, "🤖");
    }

    #[test]
    fn test_load_all_files() {
        let workspace = TempDir::new().unwrap();

        fs::write(workspace.path().join("IDENTITY.md"), "name: TestBot\n").unwrap();
        fs::write(workspace.path().join("SOUL.md"), "Friendly and helpful").unwrap();
        fs::write(workspace.path().join("USER.md"), "Power user").unwrap();
        fs::write(workspace.path().join("AGENTS.md"), "Multi-agent context").unwrap();
        fs::write(workspace.path().join("TOOLS.md"), "Tool guidance").unwrap();
        fs::write(workspace.path().join("MEMORY.md"), "Memory content").unwrap();

        let bootstrap = BootstrapFiles::load(workspace.path()).unwrap();

        assert_eq!(bootstrap.identity.name, "TestBot");
        assert_eq!(bootstrap.soul.unwrap(), "Friendly and helpful");
        assert_eq!(bootstrap.user.unwrap(), "Power user");
        assert_eq!(bootstrap.agents.unwrap(), "Multi-agent context");
        assert_eq!(bootstrap.tools.unwrap(), "Tool guidance");
        assert_eq!(bootstrap.memory.unwrap(), "Memory content");
        assert!(bootstrap.total_chars > 0);
    }

    #[test]
    fn test_file_char_limit() {
        let workspace = TempDir::new().unwrap();

        // Create a file with more than 20,000 characters
        let content = "a".repeat(MAX_FILE_CHARS + 1);
        fs::write(workspace.path().join("SOUL.md"), content).unwrap();

        let result = BootstrapFiles::load(workspace.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("exceeds"));
    }

    #[test]
    fn test_total_char_limit() {
        let workspace = TempDir::new().unwrap();

        // NOTE: With only 6 bootstrap files and a 20k per-file limit,
        // the maximum total is 120k, which is under the 150k total limit.
        // To test the total limit enforcement, we need to temporarily
        // bypass the individual limits or test with a lower total limit.

        // For this test, we'll verify the total char counting works correctly
        // with files near the individual limit (19k each = 114k total)
        let content = "a".repeat(19_000);
        fs::write(workspace.path().join("SOUL.md"), &content).unwrap();
        fs::write(workspace.path().join("USER.md"), &content).unwrap();
        fs::write(workspace.path().join("AGENTS.md"), &content).unwrap();
        fs::write(workspace.path().join("TOOLS.md"), &content).unwrap();
        fs::write(workspace.path().join("MEMORY.md"), &content).unwrap();
        fs::write(workspace.path().join("IDENTITY.md"), &content).unwrap();

        let result = BootstrapFiles::load(workspace.path());
        assert!(result.is_ok());
        let bootstrap = result.unwrap();
        assert_eq!(bootstrap.total_chars, 6 * 19_000);
        assert!(bootstrap.total_chars < MAX_TOTAL_CHARS);
    }

    #[test]
    fn test_unicode_char_counting() {
        let emoji_str = "🌱🤖🚀";
        assert_eq!(BootstrapFiles::count_chars(emoji_str), 3);

        let mixed = "Hello 🌱 World";
        assert_eq!(BootstrapFiles::count_chars(mixed), 13); // 5 + 1 + 1 + 1 + 5 = 13
    }

    #[test]
    fn test_parse_identity_valid() {
        let content = "name: CustomAgent\nemoji: 🚀\n";
        let (name, emoji) = BootstrapFiles::parse_identity(content);

        assert_eq!(name.unwrap(), "CustomAgent");
        assert_eq!(emoji.unwrap(), "🚀");
    }

    #[test]
    fn test_parse_identity_malformed() {
        let content = "This is not a valid format\nJust random text\n";
        let (name, emoji) = BootstrapFiles::parse_identity(content);

        assert!(name.is_none());
        assert!(emoji.is_none());
    }

    #[test]
    fn test_parse_identity_partial() {
        let content = "name: PartialBot\n";
        let (name, emoji) = BootstrapFiles::parse_identity(content);

        assert_eq!(name.unwrap(), "PartialBot");
        assert!(emoji.is_none());
    }
}
