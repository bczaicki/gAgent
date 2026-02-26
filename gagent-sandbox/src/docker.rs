//! Docker sandbox — runs shell commands inside a Docker container.
//!
//! When the sandbox mode is "all", all shell commands are routed through
//! Docker rather than executed directly on the host. The container is
//! ephemeral (--rm) and mounts the workspace directory read-write.
//!
//! This module does NOT depend on the `bollard` crate — it shells out to
//! the `docker` CLI, which avoids a heavy async Docker dependency while
//! still providing strong isolation.

use gagent_core::{GagentError, Result};
use std::path::Path;
use std::time::Duration;

/// Configuration for Docker sandbox execution.
#[derive(Debug, Clone)]
pub struct DockerConfig {
    /// Docker image to use (e.g. "ubuntu:22.04").
    pub image: String,

    /// Timeout for command execution in seconds.
    pub timeout_secs: u64,

    /// Network mode. Defaults to "none" for maximum isolation.
    pub network: String,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            image: "ubuntu:22.04".to_string(),
            timeout_secs: 60,
            network: "none".to_string(),
        }
    }
}

/// Result from running a command in Docker.
#[derive(Debug)]
pub struct DockerResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Check whether Docker is available on the host system.
pub async fn is_docker_available() -> bool {
    tokio::process::Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run a shell command inside a Docker container.
///
/// The workspace directory is mounted at `/workspace` inside the container.
/// The command is passed to `/bin/sh -c` inside the container.
pub async fn run_in_docker(
    command: &str,
    workspace_dir: &Path,
    config: &DockerConfig,
) -> Result<DockerResult> {
    let workspace_str = workspace_dir
        .canonicalize()
        .unwrap_or_else(|_| workspace_dir.to_path_buf())
        .to_string_lossy()
        .to_string();

    let mount = format!("{}:/workspace", workspace_str);

    let args = vec![
        "run",
        "--rm",
        "--network",
        &config.network,
        "-v",
        &mount,
        "-w",
        "/workspace",
        &config.image,
        "/bin/sh",
        "-c",
        command,
    ];

    tracing::info!("Docker exec: {}", command);

    let timeout = Duration::from_secs(config.timeout_secs);

    let output = tokio::time::timeout(
        timeout,
        tokio::process::Command::new("docker")
            .args(&args)
            .output(),
    )
    .await
    .map_err(|_| GagentError::Timeout(config.timeout_secs))?
    .map_err(|e| GagentError::Other(format!("Failed to run docker: {e}")))?;

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    tracing::debug!("Docker exit code: {}", exit_code);

    Ok(DockerResult {
        exit_code,
        stdout,
        stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_docker_config() {
        let config = DockerConfig::default();
        assert_eq!(config.image, "ubuntu:22.04");
        assert_eq!(config.timeout_secs, 60);
        assert_eq!(config.network, "none");
    }

    /// Test that Docker availability check runs without panicking.
    /// The actual result depends on whether Docker is installed.
    #[tokio::test]
    async fn test_docker_availability_check() {
        // Should not panic regardless of whether Docker is installed
        let _available = is_docker_available().await;
    }

    /// Test that run_in_docker returns an error when Docker is not available
    /// or when the command fails, rather than panicking.
    #[tokio::test]
    async fn test_run_in_docker_with_invalid_image() {
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let config = DockerConfig {
            image: "nonexistent-image-xyz-12345:latest".to_string(),
            timeout_secs: 5,
            network: "none".to_string(),
        };

        // This will either fail (Docker not installed) or return a Docker error
        // Either way it should not panic
        let result = run_in_docker("echo hello", dir.path(), &config).await;
        // We just ensure it doesn't panic — may succeed or fail depending on Docker
        let _ = result;
    }
}
