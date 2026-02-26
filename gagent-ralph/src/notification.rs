use gagent_core::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

/// A notification event emitted during the RALPH loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RalphNotification {
    /// Planning phase completed successfully.
    PlanningComplete {
        plan_path: PathBuf,
        total_tasks: usize,
    },

    /// An iteration has started.
    IterationStarted {
        iteration: usize,
        task_id: Option<String>,
        task_description: Option<String>,
    },

    /// An iteration has completed.
    IterationComplete {
        iteration: usize,
        success: bool,
        message: String,
    },

    /// All tasks completed.
    BuildingComplete {
        total_iterations: usize,
        success_count: usize,
        failure_count: usize,
    },

    /// An error occurred.
    Error {
        iteration: Option<usize>,
        message: String,
    },
}

/// Notification manager handles writing notifications to disk.
pub struct NotificationManager {
    notification_file: PathBuf,
}

impl NotificationManager {
    /// Create a new notification manager.
    ///
    /// Notifications are written to `{ralph_dir}/pending-notification.txt`.
    pub fn new(ralph_dir: impl AsRef<Path>) -> Self {
        let notification_file = ralph_dir.as_ref().join("pending-notification.txt");
        Self { notification_file }
    }

    /// Emit a notification by writing it to the pending notification file.
    ///
    /// The file contains a single JSON object per emission. Downstream
    /// consumers (like VS Code extensions or dashboards) can watch this file
    /// and consume notifications.
    pub async fn emit(&self, notification: RalphNotification) -> Result<()> {
        tracing::info!("Emitting notification: {:?}", notification);

        // Ensure parent directory exists
        if let Some(parent) = self.notification_file.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Serialize to JSON
        let json = serde_json::to_string_pretty(&notification)?;

        // Write to file (overwrites previous notification)
        let mut file = fs::File::create(&self.notification_file).await?;
        file.write_all(json.as_bytes()).await?;
        file.write_all(b"\n").await?;
        file.flush().await?;

        Ok(())
    }

    /// Clear the notification file.
    pub async fn clear(&self) -> Result<()> {
        if self.notification_file.exists() {
            fs::remove_file(&self.notification_file).await?;
        }
        Ok(())
    }

    /// Read the current pending notification, if any.
    pub async fn read(&self) -> Result<Option<RalphNotification>> {
        if !self.notification_file.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&self.notification_file).await?;
        if contents.trim().is_empty() {
            return Ok(None);
        }

        let notification: RalphNotification = serde_json::from_str(&contents)?;
        Ok(Some(notification))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::fs;

    #[tokio::test]
    async fn test_emit_notification() {
        let dir = tempdir().unwrap();
        let manager = NotificationManager::new(dir.path());

        let notification = RalphNotification::PlanningComplete {
            plan_path: PathBuf::from("IMPLEMENTATION_PLAN.md"),
            total_tasks: 5,
        };

        manager.emit(notification.clone()).await.unwrap();

        // Read it back
        let read_notification = manager.read().await.unwrap();
        assert!(read_notification.is_some());

        // Verify contents
        let file_contents = fs::read_to_string(&manager.notification_file)
            .await
            .unwrap();
        assert!(file_contents.contains("PLANNING_COMPLETE"));
        assert!(file_contents.contains("\"total_tasks\": 5"));
    }

    #[tokio::test]
    async fn test_clear_notification() {
        let dir = tempdir().unwrap();
        let manager = NotificationManager::new(dir.path());

        // Emit a notification
        manager
            .emit(RalphNotification::Error {
                iteration: Some(1),
                message: "test error".to_string(),
            })
            .await
            .unwrap();

        assert!(manager.notification_file.exists());

        // Clear it
        manager.clear().await.unwrap();

        // Should be gone
        assert!(!manager.notification_file.exists());
    }

    #[tokio::test]
    async fn test_multiple_notifications() {
        let dir = tempdir().unwrap();
        let manager = NotificationManager::new(dir.path());

        // Emit first notification
        manager
            .emit(RalphNotification::IterationStarted {
                iteration: 1,
                task_id: Some("task-1".to_string()),
                task_description: Some("First task".to_string()),
            })
            .await
            .unwrap();

        // Emit second notification (should overwrite)
        manager
            .emit(RalphNotification::IterationComplete {
                iteration: 1,
                success: true,
                message: "Done".to_string(),
            })
            .await
            .unwrap();

        // Only the second should be present
        let read_notification = manager.read().await.unwrap().unwrap();
        match read_notification {
            RalphNotification::IterationComplete { iteration, .. } => {
                assert_eq!(iteration, 1);
            }
            _ => panic!("Expected IterationComplete"),
        }
    }
}
