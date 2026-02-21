use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Top-level configuration for a gAgent instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// LLM provider configuration.
    pub llm: LlmConfig,

    /// Agent identity and behavior.
    pub agent: AgentConfig,

    /// Session and history settings.
    pub session: SessionConfig,

    /// Security and sandboxing settings.
    pub sandbox: SandboxConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider type: "ollama" or "openai-compatible".
    pub provider: String,

    /// Base URL for the LLM API.
    pub base_url: String,

    /// Model name to use.
    pub model: String,

    /// Maximum context length in tokens (approximate).
    #[serde(default = "default_context_length")]
    pub context_length: usize,

    /// Temperature for generation.
    #[serde(default = "default_temperature")]
    pub temperature: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent display name.
    #[serde(default = "default_agent_name")]
    pub name: String,

    /// Agent emoji/icon.
    #[serde(default = "default_agent_emoji")]
    pub emoji: String,

    /// Path to the .gagent workspace directory.
    #[serde(default = "default_workspace_dir")]
    pub workspace_dir: PathBuf,

    /// Maximum timeout for agent operations in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Directory for session history files.
    #[serde(default = "default_sessions_dir")]
    pub sessions_dir: PathBuf,

    /// Maximum number of messages before auto-compaction.
    #[serde(default = "default_max_messages")]
    pub max_messages: usize,

    /// Maximum total character count before auto-compaction.
    #[serde(default = "default_max_context_chars")]
    pub max_context_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxConfig {
    /// Sandbox mode: "off", "non-main", "all".
    #[serde(default = "default_sandbox_mode")]
    pub mode: String,

    /// Allowed workspace paths for file operations.
    #[serde(default)]
    pub allowed_paths: Vec<PathBuf>,

    /// Commands that require confirmation before execution.
    #[serde(default)]
    pub confirm_commands: Vec<String>,

    /// Commands that are always denied.
    #[serde(default)]
    pub denied_commands: Vec<String>,
}

fn default_context_length() -> usize {
    8192
}
fn default_temperature() -> f64 {
    0.7
}
fn default_agent_name() -> String {
    "gAgent".to_string()
}
fn default_agent_emoji() -> String {
    "\u{1f331}".to_string() // 🌱
}
fn default_workspace_dir() -> PathBuf {
    PathBuf::from(".gagent")
}
fn default_timeout() -> u64 {
    600
}
fn default_sessions_dir() -> PathBuf {
    PathBuf::from(".gagent/sessions")
}
fn default_max_messages() -> usize {
    100
}
fn default_max_context_chars() -> usize {
    150_000
}
fn default_sandbox_mode() -> String {
    "off".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LlmConfig {
                provider: "ollama".to_string(),
                base_url: "http://localhost:11434".to_string(),
                model: "llama3.2".to_string(),
                context_length: default_context_length(),
                temperature: default_temperature(),
            },
            agent: AgentConfig {
                name: default_agent_name(),
                emoji: default_agent_emoji(),
                workspace_dir: default_workspace_dir(),
                timeout_secs: default_timeout(),
            },
            session: SessionConfig {
                sessions_dir: default_sessions_dir(),
                max_messages: default_max_messages(),
                max_context_chars: default_max_context_chars(),
            },
            sandbox: SandboxConfig {
                mode: default_sandbox_mode(),
                allowed_paths: vec![],
                confirm_commands: vec![],
                denied_commands: vec![],
            },
        }
    }
}

impl Config {
    /// Load config from a TOML file, falling back to defaults.
    pub fn load(path: &Path) -> crate::error::Result<Self> {
        if path.exists() {
            let contents = std::fs::read_to_string(path)?;
            let config: Config = toml::from_str(&contents).map_err(|e| {
                crate::error::GagentError::Config(format!("Failed to parse config: {e}"))
            })?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    /// Save config to a TOML file.
    pub fn save(&self, path: &Path) -> crate::error::Result<()> {
        let contents = toml::to_string_pretty(self).map_err(|e| {
            crate::error::GagentError::Config(format!("Failed to serialize config: {e}"))
        })?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, contents)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.llm.provider, "ollama");
        assert_eq!(config.llm.base_url, "http://localhost:11434");
        assert_eq!(config.agent.name, "gAgent");
        assert_eq!(config.agent.timeout_secs, 600);
    }

    #[test]
    fn test_config_roundtrip() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.llm.model, config.llm.model);
        assert_eq!(parsed.agent.name, config.agent.name);
    }
}
