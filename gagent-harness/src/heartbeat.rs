//! Heartbeat system for periodic agent maintenance tasks.
//!
//! A `Heartbeat` runs a user-supplied async callback on a regular interval.
//! Typical uses:
//! - Distilling session learnings into memory at end of session
//! - Health checks (e.g. pinging the LLM server)
//! - Consolidating memory files
//! - Writing session checkpoints for crash recovery

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

/// A periodic heartbeat that calls a callback on a fixed interval.
///
/// The heartbeat runs in a background Tokio task. It can be stopped by
/// calling `stop()` or by dropping the `HeartbeatHandle`.
pub struct HeartbeatHandle {
    stop_flag: Arc<AtomicBool>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl HeartbeatHandle {
    /// Signal the heartbeat to stop. The running callback is not interrupted;
    /// the heartbeat will stop after the current sleep finishes.
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    /// Stop the heartbeat and wait for the background task to finish.
    pub async fn shutdown(mut self) {
        self.stop();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for HeartbeatHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Spawn a heartbeat that calls `callback` every `interval`.
///
/// The callback is a closure returning a `Future`. It is called in a
/// background Tokio task, so it must be `Send + 'static`.
///
/// # Example
/// ```no_run
/// use gagent_harness::heartbeat::spawn_heartbeat;
/// use std::time::Duration;
///
/// # #[tokio::main]
/// # async fn main() {
/// let handle = spawn_heartbeat(Duration::from_secs(30), || async {
///     println!("Heartbeat tick!");
/// });
///
/// // ...agent work...
/// handle.stop();
/// # }
/// ```
pub fn spawn_heartbeat<F, Fut>(interval: Duration, callback: F) -> HeartbeatHandle
where
    F: Fn() -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    let stop_flag = Arc::new(AtomicBool::new(false));
    let stop_flag_clone = Arc::clone(&stop_flag);

    let task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;

            if stop_flag_clone.load(Ordering::Relaxed) {
                debug!("Heartbeat stopping");
                break;
            }

            debug!("Heartbeat tick");

            // Run the callback, catching any panics
            let result = tokio::spawn(callback()).await;
            if let Err(e) = result {
                warn!("Heartbeat callback panicked: {:?}", e);
            }
        }
    });

    HeartbeatHandle {
        stop_flag,
        task: Some(task),
    }
}

/// A session checkpoint writer for crash recovery.
///
/// Writes the current session state to a crash recovery file so that
/// sessions can be resumed after an unexpected termination.
pub struct CrashRecovery {
    /// Path to the crash recovery checkpoint file.
    checkpoint_path: std::path::PathBuf,
}

impl CrashRecovery {
    pub fn new(checkpoint_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            checkpoint_path: checkpoint_path.into(),
        }
    }

    /// Write a checkpoint with the given session JSON.
    pub fn write_checkpoint(&self, session_json: &str) -> std::io::Result<()> {
        if let Some(parent) = self.checkpoint_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.checkpoint_path, session_json)?;
        debug!("Crash recovery checkpoint written: {:?}", self.checkpoint_path);
        Ok(())
    }

    /// Remove the checkpoint file (called on clean exit).
    pub fn clear_checkpoint(&self) -> std::io::Result<()> {
        if self.checkpoint_path.exists() {
            std::fs::remove_file(&self.checkpoint_path)?;
            debug!("Crash recovery checkpoint cleared");
        }
        Ok(())
    }

    /// Check if a crash recovery file exists (indicating a previous crash).
    pub fn has_checkpoint(&self) -> bool {
        self.checkpoint_path.exists()
    }

    /// Read the checkpoint content (for session recovery).
    pub fn read_checkpoint(&self) -> std::io::Result<String> {
        std::fs::read_to_string(&self.checkpoint_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;
    use tempfile::TempDir;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_heartbeat_fires() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        let handle = spawn_heartbeat(Duration::from_millis(50), move || {
            let c = Arc::clone(&counter_clone);
            async move {
                c.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Wait for at least 2 ticks
        sleep(Duration::from_millis(160)).await;
        handle.stop();

        let count = counter.load(Ordering::Relaxed);
        assert!(count >= 2, "Expected at least 2 heartbeat ticks, got {}", count);
    }

    #[tokio::test]
    async fn test_heartbeat_stops() {
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = Arc::clone(&counter);

        let handle = spawn_heartbeat(Duration::from_millis(50), move || {
            let c = Arc::clone(&counter_clone);
            async move {
                c.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Let it tick once
        sleep(Duration::from_millis(70)).await;
        handle.stop();

        // Wait a bit and verify no more ticks
        let count_after_stop = counter.load(Ordering::Relaxed);
        sleep(Duration::from_millis(100)).await;
        let count_later = counter.load(Ordering::Relaxed);

        // After stopping, the count should not increase significantly
        assert!(
            count_later <= count_after_stop + 1,
            "Heartbeat continued after stop"
        );
    }

    #[test]
    fn test_crash_recovery_write_read() {
        let dir = TempDir::new().unwrap();
        let checkpoint = dir.path().join("checkpoint.json");
        let recovery = CrashRecovery::new(&checkpoint);

        assert!(!recovery.has_checkpoint());

        recovery.write_checkpoint(r#"{"session_id":"abc"}"#).unwrap();
        assert!(recovery.has_checkpoint());

        let content = recovery.read_checkpoint().unwrap();
        assert_eq!(content, r#"{"session_id":"abc"}"#);
    }

    #[test]
    fn test_crash_recovery_clear() {
        let dir = TempDir::new().unwrap();
        let recovery = CrashRecovery::new(dir.path().join("checkpoint.json"));

        recovery.write_checkpoint("{}").unwrap();
        assert!(recovery.has_checkpoint());

        recovery.clear_checkpoint().unwrap();
        assert!(!recovery.has_checkpoint());
    }

    #[test]
    fn test_crash_recovery_creates_dirs() {
        let dir = TempDir::new().unwrap();
        let nested_path = dir.path().join("a/b/c/checkpoint.json");
        let recovery = CrashRecovery::new(&nested_path);
        recovery.write_checkpoint("data").unwrap();
        assert!(nested_path.exists());
    }
}
