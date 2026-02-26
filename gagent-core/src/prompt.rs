use crate::{BootstrapFiles, Config};

/// Assembles system prompts from bootstrap files and configuration.
pub struct PromptAssembler {
    config: Config,
    bootstrap: BootstrapFiles,
}

/// Assembled system prompt with metadata.
#[derive(Debug, Clone)]
pub struct SystemPrompt {
    /// Complete assembled prompt text.
    pub text: String,

    /// Total character count.
    pub char_count: usize,
}

impl SystemPrompt {
    /// Create a minimal system prompt for testing.
    pub fn minimal() -> Self {
        let text = "You are a helpful AI agent.".to_string();
        let char_count = text.chars().count();
        Self { text, char_count }
    }
}

impl PromptAssembler {
    /// Create a new prompt assembler with config and bootstrap files.
    pub fn new(config: Config, bootstrap: BootstrapFiles) -> Self {
        Self { config, bootstrap }
    }

    /// Assemble the complete system prompt.
    ///
    /// Section order (as per CLAUDE.md):
    /// 1. Identity — "You are {emoji} {name}, a local-first AI agent"
    /// 2. Personality — SOUL.md content (if present)
    /// 3. Tooling — TOOLS.md content + placeholder for Phase 2
    /// 4. Safety — Built-in guardrails about file access, timeouts
    /// 5. Workspace — Working directory, timeout, sandbox mode from config
    /// 6. Bootstrap context — USER.md, AGENTS.md, MEMORY.md formatted as sections
    /// 7. Runtime metadata — Timestamp, model, provider, temperature
    pub fn assemble(&self) -> SystemPrompt {
        let mut sections = Vec::new();

        sections.push(self.build_identity());
        sections.push(self.build_personality());
        sections.push(self.build_tooling());
        sections.push(self.build_safety());
        sections.push(self.build_workspace());
        sections.push(self.build_bootstrap_context());
        sections.push(self.build_runtime_metadata());

        let text = sections.join("\n\n");
        let char_count = text.chars().count();

        SystemPrompt { text, char_count }
    }

    /// Build identity section.
    fn build_identity(&self) -> String {
        format!(
            "# Identity\n\nYou are {} {}, a local-first AI agent that respects user privacy and autonomy.\n\
            All data stays on the user's machine. No cloud dependencies for core functionality.",
            self.bootstrap.identity.emoji,
            self.bootstrap.identity.name
        )
    }

    /// Build personality section from SOUL.md.
    fn build_personality(&self) -> String {
        if let Some(ref soul) = self.bootstrap.soul {
            format!("# Personality\n\n{}", soul.trim())
        } else {
            String::new()
        }
    }

    /// Build tooling section from TOOLS.md and phase 2 placeholder.
    fn build_tooling(&self) -> String {
        let mut content = String::from("# Tools\n\n");

        if let Some(ref tools) = self.bootstrap.tools {
            content.push_str(tools.trim());
            content.push_str("\n\n");
        }

        content.push_str(
            "Tool execution is powered by the agent harness (Phase 2). \
            Available tools will be provided in the context when ready."
        );

        content
    }

    /// Build safety guidelines section.
    fn build_safety(&self) -> String {
        format!(
            "# Safety Guidelines\n\n\
            - Always validate file paths against workspace boundaries\n\
            - Never execute commands without timeout protection ({}s default)\n\
            - Sanitize all tool parameters before execution\n\
            - Log all tool executions for audit trails\n\
            - Respect sandbox mode settings: {}\n\
            - Never send data to external services without explicit user configuration",
            self.config.agent.timeout_secs,
            self.config.sandbox.mode
        )
    }

    /// Build workspace configuration section.
    fn build_workspace(&self) -> String {
        format!(
            "# Workspace Configuration\n\n\
            - Working directory: {}\n\
            - Timeout: {}s\n\
            - Sandbox mode: {}\n\
            - Sessions directory: {}",
            self.config.agent.workspace_dir.display(),
            self.config.agent.timeout_secs,
            self.config.sandbox.mode,
            self.config.session.sessions_dir.display()
        )
    }

    /// Build bootstrap context from USER.md, AGENTS.md, MEMORY.md.
    fn build_bootstrap_context(&self) -> String {
        let mut sections = Vec::new();

        if let Some(ref user) = self.bootstrap.user {
            sections.push(format!("## User Profile\n\n{}", user.trim()));
        }

        if let Some(ref agents) = self.bootstrap.agents {
            sections.push(format!("## Multi-Agent Context\n\n{}", agents.trim()));
        }

        if let Some(ref memory) = self.bootstrap.memory {
            sections.push(format!("## Memory\n\n{}", memory.trim()));
        }

        if sections.is_empty() {
            String::new()
        } else {
            format!("# Bootstrap Context\n\n{}", sections.join("\n\n"))
        }
    }

    /// Build runtime metadata section.
    fn build_runtime_metadata(&self) -> String {
        use std::time::SystemTime;

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        format!(
            "# Runtime Metadata\n\n\
            - Timestamp: {}\n\
            - LLM Provider: {}\n\
            - Model: {}\n\
            - Temperature: {}\n\
            - Context Length: {} tokens (approx)",
            timestamp,
            self.config.llm.provider,
            self.config.llm.model,
            self.config.llm.temperature,
            self.config.llm.context_length
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AgentIdentity;

    #[test]
    fn test_assemble_minimal_prompt() {
        let config = Config::default();
        let bootstrap = BootstrapFiles::default();
        let assembler = PromptAssembler::new(config, bootstrap);
        let prompt = assembler.assemble();

        assert!(prompt.text.contains("You are 🌱 gAgent"));
        assert!(prompt.text.contains("local-first AI agent"));
        assert!(prompt.char_count > 0);
    }

    #[test]
    fn test_assemble_with_personality() {
        let config = Config::default();
        let mut bootstrap = BootstrapFiles::default();
        bootstrap.soul = Some("Friendly and helpful".to_string());

        let assembler = PromptAssembler::new(config, bootstrap);
        let prompt = assembler.assemble();

        assert!(prompt.text.contains("# Personality"));
        assert!(prompt.text.contains("Friendly and helpful"));
    }

    #[test]
    fn test_assemble_with_user_profile() {
        let config = Config::default();
        let mut bootstrap = BootstrapFiles::default();
        bootstrap.user = Some("Power user with Rust expertise".to_string());

        let assembler = PromptAssembler::new(config, bootstrap);
        let prompt = assembler.assemble();

        assert!(prompt.text.contains("## User Profile"));
        assert!(prompt.text.contains("Power user with Rust expertise"));
    }

    #[test]
    fn test_section_ordering() {
        let config = Config::default();
        let mut bootstrap = BootstrapFiles::default();
        bootstrap.soul = Some("SOUL".to_string());
        bootstrap.user = Some("USER".to_string());
        bootstrap.tools = Some("TOOLS".to_string());

        let assembler = PromptAssembler::new(config, bootstrap);
        let prompt = assembler.assemble();

        // Find positions of each section
        let identity_pos = prompt.text.find("# Identity").unwrap();
        let personality_pos = prompt.text.find("# Personality").unwrap();
        let tools_pos = prompt.text.find("# Tools").unwrap();
        let safety_pos = prompt.text.find("# Safety Guidelines").unwrap();
        let workspace_pos = prompt.text.find("# Workspace Configuration").unwrap();
        let bootstrap_pos = prompt.text.find("# Bootstrap Context").unwrap();
        let runtime_pos = prompt.text.find("# Runtime Metadata").unwrap();

        // Verify ordering
        assert!(identity_pos < personality_pos);
        assert!(personality_pos < tools_pos);
        assert!(tools_pos < safety_pos);
        assert!(safety_pos < workspace_pos);
        assert!(workspace_pos < bootstrap_pos);
        assert!(bootstrap_pos < runtime_pos);
    }

    #[test]
    fn test_custom_identity() {
        let config = Config::default();
        let mut bootstrap = BootstrapFiles::default();
        bootstrap.identity = AgentIdentity {
            name: "CustomBot".to_string(),
            emoji: "🤖".to_string(),
            personality: None,
            user_profile: None,
        };

        let assembler = PromptAssembler::new(config, bootstrap);
        let prompt = assembler.assemble();

        assert!(prompt.text.contains("You are 🤖 CustomBot"));
        assert!(!prompt.text.contains("gAgent"));
    }

    #[test]
    fn test_runtime_metadata_included() {
        let config = Config::default();
        let bootstrap = BootstrapFiles::default();
        let assembler = PromptAssembler::new(config, bootstrap);
        let prompt = assembler.assemble();

        assert!(prompt.text.contains("LLM Provider: ollama"));
        assert!(prompt.text.contains("Model: llama3.2"));
        assert!(prompt.text.contains("Temperature: 0.7"));
    }

    #[test]
    fn test_all_sections_present() {
        let config = Config::default();
        let mut bootstrap = BootstrapFiles::default();
        bootstrap.soul = Some("Soul".to_string());
        bootstrap.user = Some("User".to_string());
        bootstrap.agents = Some("Agents".to_string());
        bootstrap.memory = Some("Memory".to_string());
        bootstrap.tools = Some("Tools".to_string());

        let assembler = PromptAssembler::new(config, bootstrap);
        let prompt = assembler.assemble();

        // Check all required sections are present
        assert!(prompt.text.contains("# Identity"));
        assert!(prompt.text.contains("# Personality"));
        assert!(prompt.text.contains("# Tools"));
        assert!(prompt.text.contains("# Safety Guidelines"));
        assert!(prompt.text.contains("# Workspace Configuration"));
        assert!(prompt.text.contains("# Bootstrap Context"));
        assert!(prompt.text.contains("## User Profile"));
        assert!(prompt.text.contains("## Multi-Agent Context"));
        assert!(prompt.text.contains("## Memory"));
        assert!(prompt.text.contains("# Runtime Metadata"));
    }
}
