//! Sandbox: path containment, Docker sandboxing, and execution policies.
//!
//! This crate provides three layers of security for gAgent tool execution:
//!
//! 1. **PathGuard** (`path_guard`) — prevents path traversal by canonicalizing
//!    and comparing against allowed root directories.
//!
//! 2. **ExecutionPolicy** (`policy`) — allow/confirm/deny shell commands based
//!    on configurable patterns from `.gagent/config.toml`.
//!
//! 3. **Docker sandbox** (`docker`) — optionally runs shell commands inside an
//!    ephemeral Docker container for strong OS-level isolation.

pub mod docker;
pub mod path_guard;
pub mod policy;

pub use docker::{DockerConfig, DockerResult, is_docker_available, run_in_docker};
pub use path_guard::PathGuard;
pub use policy::{ExecutionPolicy, PolicyDecision};

/// Sandbox mode from configuration.
#[derive(Debug, Clone, PartialEq)]
pub enum SandboxMode {
    /// No sandboxing — all commands execute directly.
    Off,

    /// Docker sandbox for non-main branch operations.
    NonMain,

    /// Docker sandbox for all shell commands.
    All,
}

impl SandboxMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "non-main" => Self::NonMain,
            "all" => Self::All,
            _ => Self::Off,
        }
    }
}

/// Combined sandbox context used during tool execution.
///
/// Callers (e.g. `AgentHarness`) create a `SandboxContext` from the config
/// and use it to validate paths and check execution policy before running
/// any tool.
pub struct SandboxContext {
    pub path_guard: PathGuard,
    pub policy: ExecutionPolicy,
    pub mode: SandboxMode,
    pub docker_config: DockerConfig,
}

impl SandboxContext {
    /// Build a sandbox context from the gAgent `SandboxConfig`.
    pub fn from_config(config: &gagent_core::config::SandboxConfig) -> Self {
        let path_guard = if config.allowed_paths.is_empty() {
            PathGuard::allow_all()
        } else {
            PathGuard::new(&config.allowed_paths)
        };

        let policy = ExecutionPolicy::new(
            config.denied_commands.clone(),
            config.confirm_commands.clone(),
        );

        let mode = SandboxMode::from_str(&config.mode);

        Self {
            path_guard,
            policy,
            mode,
            docker_config: DockerConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gagent_core::config::SandboxConfig;
    use std::path::PathBuf;

    #[test]
    fn test_sandbox_mode_from_str() {
        assert_eq!(SandboxMode::from_str("off"), SandboxMode::Off);
        assert_eq!(SandboxMode::from_str("non-main"), SandboxMode::NonMain);
        assert_eq!(SandboxMode::from_str("all"), SandboxMode::All);
        assert_eq!(SandboxMode::from_str("unknown"), SandboxMode::Off);
    }

    #[test]
    fn test_sandbox_context_from_default_config() {
        let config = SandboxConfig {
            mode: "off".to_string(),
            allowed_paths: vec![],
            confirm_commands: vec![],
            denied_commands: vec![],
        };

        let ctx = SandboxContext::from_config(&config);
        assert_eq!(ctx.mode, SandboxMode::Off);
        assert!(ctx.path_guard.allowed_roots().is_empty());
    }

    #[test]
    fn test_sandbox_context_with_restrictions() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();

        let config = SandboxConfig {
            mode: "all".to_string(),
            allowed_paths: vec![dir.path().to_path_buf()],
            confirm_commands: vec!["git".to_string()],
            denied_commands: vec!["rm".to_string()],
        };

        let ctx = SandboxContext::from_config(&config);
        assert_eq!(ctx.mode, SandboxMode::All);
        assert!(!ctx.path_guard.allowed_roots().is_empty());

        // Policy should deny "rm"
        assert!(matches!(
            ctx.policy.evaluate("rm /tmp/x").unwrap(),
            PolicyDecision::Deny(_)
        ));

        // Policy should confirm "git"
        assert_eq!(
            ctx.policy.evaluate("git push").unwrap(),
            PolicyDecision::Confirm
        );
    }
}
