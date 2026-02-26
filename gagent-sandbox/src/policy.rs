//! Execution policy — determines whether a shell command may be run.
//!
//! The policy is driven by two lists from the gAgent config:
//! - `denied_commands`: patterns that are always blocked.
//! - `confirm_commands`: patterns that require explicit user confirmation.
//!
//! Each pattern is matched against the first token (the command name) of the
//! command string being executed.

use gagent_core::{GagentError, Result};

/// Decision made by the policy engine for a given command.
#[derive(Debug, Clone, PartialEq)]
pub enum PolicyDecision {
    /// The command is permitted to run without extra confirmation.
    Allow,

    /// The command matches a confirm pattern — the caller should prompt the
    /// user before executing.
    Confirm,

    /// The command is permanently denied.
    Deny(String),
}

/// Policy engine built from the sandbox configuration.
#[derive(Debug, Clone)]
pub struct ExecutionPolicy {
    /// Commands that are always denied (matched against the command name).
    denied: Vec<String>,

    /// Commands that require confirmation before execution.
    confirm: Vec<String>,
}

impl ExecutionPolicy {
    /// Create a policy from deny and confirm lists.
    pub fn new(denied: Vec<String>, confirm: Vec<String>) -> Self {
        Self { denied, confirm }
    }

    /// Create a policy that allows everything (sandbox off).
    pub fn permissive() -> Self {
        Self {
            denied: vec![],
            confirm: vec![],
        }
    }

    /// Evaluate the policy for a given shell command string.
    ///
    /// The command is expected to be the full shell command (e.g. `rm -rf /`).
    /// Only the first token (the command name) is matched against the lists.
    pub fn evaluate(&self, command: &str) -> Result<PolicyDecision> {
        let command_name = extract_command_name(command);

        // Check deny list first
        for denied in &self.denied {
            if matches_pattern(command_name, denied) {
                return Ok(PolicyDecision::Deny(format!(
                    "Command '{}' is denied by policy",
                    command_name
                )));
            }
        }

        // Check confirm list
        for confirm in &self.confirm {
            if matches_pattern(command_name, confirm) {
                return Ok(PolicyDecision::Confirm);
            }
        }

        Ok(PolicyDecision::Allow)
    }

    /// Convenience method: return `Err` if the command is denied.
    pub fn enforce(&self, command: &str) -> Result<PolicyDecision> {
        let decision = self.evaluate(command)?;
        if let PolicyDecision::Deny(ref reason) = decision {
            return Err(GagentError::Tool(reason.clone()));
        }
        Ok(decision)
    }
}

/// Extract the command name (first whitespace-delimited token) from a shell command.
fn extract_command_name(command: &str) -> &str {
    command.trim().split_whitespace().next().unwrap_or("")
}

/// Match a command name against a pattern.
///
/// Patterns are simple: an exact match OR a glob-style `*` wildcard at the
/// end (e.g. `rm*` matches `rm`, `rmdir`, `rm -rf`, etc.).
fn matches_pattern(command_name: &str, pattern: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        command_name.starts_with(prefix)
    } else {
        command_name == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permissive_allows_everything() {
        let policy = ExecutionPolicy::permissive();
        assert_eq!(policy.evaluate("rm -rf /").unwrap(), PolicyDecision::Allow);
        assert_eq!(policy.evaluate("sudo shutdown").unwrap(), PolicyDecision::Allow);
    }

    #[test]
    fn test_deny_list_blocks_command() {
        let policy = ExecutionPolicy::new(
            vec!["rm".to_string()],
            vec![],
        );
        assert!(matches!(
            policy.evaluate("rm -rf /tmp").unwrap(),
            PolicyDecision::Deny(_)
        ));
        assert_eq!(policy.evaluate("ls -la").unwrap(), PolicyDecision::Allow);
    }

    #[test]
    fn test_confirm_list_returns_confirm() {
        let policy = ExecutionPolicy::new(
            vec![],
            vec!["git".to_string()],
        );
        assert_eq!(policy.evaluate("git push").unwrap(), PolicyDecision::Confirm);
        assert_eq!(policy.evaluate("ls").unwrap(), PolicyDecision::Allow);
    }

    #[test]
    fn test_deny_takes_priority_over_confirm() {
        let policy = ExecutionPolicy::new(
            vec!["sudo".to_string()],
            vec!["sudo".to_string()],
        );
        assert!(matches!(
            policy.evaluate("sudo rm -rf /").unwrap(),
            PolicyDecision::Deny(_)
        ));
    }

    #[test]
    fn test_wildcard_pattern() {
        let policy = ExecutionPolicy::new(
            vec!["rm*".to_string()],
            vec![],
        );
        assert!(matches!(
            policy.evaluate("rm /tmp/x").unwrap(),
            PolicyDecision::Deny(_)
        ));
        assert!(matches!(
            policy.evaluate("rmdir /tmp/x").unwrap(),
            PolicyDecision::Deny(_)
        ));
        assert_eq!(policy.evaluate("cat file").unwrap(), PolicyDecision::Allow);
    }

    #[test]
    fn test_enforce_returns_err_for_denied() {
        let policy = ExecutionPolicy::new(vec!["dd".to_string()], vec![]);
        let result = policy.enforce("dd if=/dev/zero of=/dev/sda");
        assert!(result.is_err());
    }

    #[test]
    fn test_enforce_returns_ok_for_allowed() {
        let policy = ExecutionPolicy::permissive();
        let result = policy.enforce("echo hello");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PolicyDecision::Allow);
    }

    #[test]
    fn test_extract_command_name() {
        assert_eq!(extract_command_name("rm -rf /tmp"), "rm");
        assert_eq!(extract_command_name("  sudo apt install vim  "), "sudo");
        assert_eq!(extract_command_name(""), "");
    }

    #[test]
    fn test_empty_command() {
        let policy = ExecutionPolicy::permissive();
        assert_eq!(policy.evaluate("").unwrap(), PolicyDecision::Allow);
    }
}
