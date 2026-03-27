//! Background artifact cleanup for retention policy enforcement.
//!
//! When `--retention-days` is set to a non-zero value, a background task
//! periodically scans the artifact store and removes objects older than
//! the configured retention period.

use crate::storage::ArtifactStore;
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Result of a cleanup operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupResult {
    /// Number of objects scanned.
    pub scanned: u64,
    /// Number of objects deleted.
    pub deleted: u64,
    /// Number of objects that failed to delete.
    pub errors: u64,
    /// The retention cutoff timestamp used.
    pub cutoff: String,
}

/// Runs a single cleanup pass against the artifact store.
///
/// Deletes all objects whose `last_modified` timestamp is older than
/// `retention_days` days from now.
pub async fn run_cleanup(
    store: &dyn ArtifactStore,
    retention_days: u64,
) -> Result<CleanupResult, String> {
    let cutoff = Utc::now() - Duration::days(retention_days as i64);
    info!(
        retention_days = retention_days,
        cutoff = %cutoff,
        "Starting artifact cleanup"
    );

    let objects = store
        .list(None)
        .await
        .map_err(|e| format!("Failed to list objects: {}", e))?;

    let scanned = objects.len() as u64;
    let mut deleted = 0u64;
    let mut errors = 0u64;

    for obj in &objects {
        if obj.last_modified < cutoff {
            match store.delete(&obj.path).await {
                Ok(()) => {
                    deleted += 1;
                    info!(
                        path = %obj.path,
                        last_modified = %obj.last_modified,
                        "Deleted expired artifact"
                    );
                }
                Err(e) => {
                    errors += 1;
                    warn!(
                        path = %obj.path,
                        error = %e,
                        "Failed to delete expired artifact"
                    );
                }
            }
        }
    }

    let result = CleanupResult {
        scanned,
        deleted,
        errors,
        cutoff: cutoff.to_rfc3339(),
    };

    info!(
        scanned = result.scanned,
        deleted = result.deleted,
        errors = result.errors,
        "Artifact cleanup completed"
    );

    Ok(result)
}

/// Spawns a background task that runs cleanup periodically.
///
/// The task runs every `interval_hours` hours. It is designed to be
/// cancelled via the tokio cancellation token / task abort.
pub fn spawn_cleanup_task(
    store: Arc<dyn ArtifactStore>,
    retention_days: u64,
    interval_hours: u64,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let interval = std::time::Duration::from_secs(interval_hours * 3600);
        info!(
            retention_days = retention_days,
            interval_hours = interval_hours,
            "Background cleanup task started"
        );

        loop {
            tokio::time::sleep(interval).await;

            match run_cleanup(store.as_ref(), retention_days).await {
                Ok(result) => {
                    info!(
                        deleted = result.deleted,
                        scanned = result.scanned,
                        "Background cleanup pass completed"
                    );
                }
                Err(e) => {
                    error!(error = %e, "Background cleanup pass failed");
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::StoreError;
    use crate::storage::ArtifactMeta;
    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use std::sync::Mutex;

    /// A simple in-memory artifact store for testing cleanup logic.
    #[derive(Debug)]
    struct MockArtifactStore {
        objects: Mutex<Vec<ArtifactMeta>>,
        deleted: Mutex<Vec<String>>,
    }

    impl MockArtifactStore {
        fn new(objects: Vec<ArtifactMeta>) -> Self {
            Self {
                objects: Mutex::new(objects),
                deleted: Mutex::new(Vec::new()),
            }
        }

        fn deleted_paths(&self) -> Vec<String> {
            self.deleted.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl ArtifactStore for MockArtifactStore {
        async fn put(&self, _path: &str, _data: Vec<u8>) -> Result<(), StoreError> {
            Ok(())
        }

        async fn get(&self, _path: &str) -> Result<Vec<u8>, StoreError> {
            Ok(vec![])
        }

        async fn delete(&self, path: &str) -> Result<(), StoreError> {
            let mut deleted = self.deleted.lock().unwrap();
            deleted.push(path.to_string());
            Ok(())
        }

        async fn list(&self, _prefix: Option<&str>) -> Result<Vec<ArtifactMeta>, StoreError> {
            let objects = self.objects.lock().unwrap();
            Ok(objects.clone())
        }
    }

    #[tokio::test]
    async fn test_cleanup_deletes_expired_objects() {
        let now = Utc::now();
        let objects = vec![
            ArtifactMeta {
                path: "old-receipt.json".to_string(),
                last_modified: now - Duration::days(10),
                size: 1024,
            },
            ArtifactMeta {
                path: "recent-receipt.json".to_string(),
                last_modified: now - Duration::days(1),
                size: 2048,
            },
            ArtifactMeta {
                path: "ancient-receipt.json".to_string(),
                last_modified: now - Duration::days(100),
                size: 512,
            },
        ];

        let store = MockArtifactStore::new(objects);
        let result = run_cleanup(&store, 7).await.unwrap();

        assert_eq!(result.scanned, 3);
        assert_eq!(result.deleted, 2);
        assert_eq!(result.errors, 0);

        let deleted = store.deleted_paths();
        assert!(deleted.contains(&"old-receipt.json".to_string()));
        assert!(deleted.contains(&"ancient-receipt.json".to_string()));
        assert!(!deleted.contains(&"recent-receipt.json".to_string()));
    }

    #[tokio::test]
    async fn test_cleanup_no_expired_objects() {
        let now = Utc::now();
        let objects = vec![ArtifactMeta {
            path: "fresh.json".to_string(),
            last_modified: now - Duration::hours(1),
            size: 100,
        }];

        let store = MockArtifactStore::new(objects);
        let result = run_cleanup(&store, 30).await.unwrap();

        assert_eq!(result.scanned, 1);
        assert_eq!(result.deleted, 0);
        assert_eq!(result.errors, 0);
    }

    #[tokio::test]
    async fn test_cleanup_empty_store() {
        let store = MockArtifactStore::new(vec![]);
        let result = run_cleanup(&store, 7).await.unwrap();

        assert_eq!(result.scanned, 0);
        assert_eq!(result.deleted, 0);
        assert_eq!(result.errors, 0);
    }

    /// Test that delete errors are counted but don't abort the scan.
    #[tokio::test]
    async fn test_cleanup_handles_delete_errors() {
        /// A store that fails on delete for a specific path.
        #[derive(Debug)]
        struct FailingDeleteStore;

        #[async_trait]
        impl ArtifactStore for FailingDeleteStore {
            async fn put(&self, _path: &str, _data: Vec<u8>) -> Result<(), StoreError> {
                Ok(())
            }
            async fn get(&self, _path: &str) -> Result<Vec<u8>, StoreError> {
                Ok(vec![])
            }
            async fn delete(&self, _path: &str) -> Result<(), StoreError> {
                Err(StoreError::Other("permission denied".to_string()))
            }
            async fn list(&self, _prefix: Option<&str>) -> Result<Vec<ArtifactMeta>, StoreError> {
                Ok(vec![ArtifactMeta {
                    path: "locked.json".to_string(),
                    last_modified: Utc::now() - Duration::days(30),
                    size: 100,
                }])
            }
        }

        let store = FailingDeleteStore;
        let result = run_cleanup(&store, 7).await.unwrap();

        assert_eq!(result.scanned, 1);
        assert_eq!(result.deleted, 0);
        assert_eq!(result.errors, 1);
    }
}
