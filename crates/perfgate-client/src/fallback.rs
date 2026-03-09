//! Fallback storage implementation.
//!
//! This module provides fallback storage when the server is unavailable.
//! It wraps the `BaselineClient` and falls back to local file storage on errors.

use crate::client::BaselineClient;
use crate::config::FallbackStorage;
use crate::error::ClientError;
use crate::types::*;
use std::path::PathBuf;
use tokio::fs;
use tracing::debug;

/// Client with fallback storage support.
///
/// This client wraps the main `BaselineClient` and provides automatic
/// fallback to local storage when the server is unavailable.
#[derive(Debug)]
pub struct FallbackClient {
    client: BaselineClient,
    fallback: Option<LocalFallbackStorage>,
}

impl FallbackClient {
    /// Creates a new fallback client.
    pub fn new(client: BaselineClient, fallback: Option<FallbackStorage>) -> Self {
        let local_fallback = fallback.map(|f| match f {
            FallbackStorage::Local { dir } => LocalFallbackStorage::new(dir),
        });

        Self {
            client,
            fallback: local_fallback,
        }
    }

    /// Gets the underlying client.
    pub fn inner(&self) -> &BaselineClient {
        &self.client
    }

    /// Gets the latest baseline with fallback support.
    ///
    /// First tries the server, then falls back to local storage if available.
    pub async fn get_latest_baseline(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<BaselineRecord, ClientError> {
        match self.client.get_latest_baseline(project, benchmark).await {
            Ok(record) => Ok(record),
            Err(e) if e.is_connection_error() => {
                if let Some(fallback) = &self.fallback {
                    debug!(
                        project = %project,
                        benchmark = %benchmark,
                        "Server unavailable, falling back to local storage"
                    );
                    fallback.get_latest_baseline(project, benchmark).await
                } else {
                    Err(e)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Gets a specific baseline version with fallback support.
    pub async fn get_baseline_version(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<BaselineRecord, ClientError> {
        match self
            .client
            .get_baseline_version(project, benchmark, version)
            .await
        {
            Ok(record) => Ok(record),
            Err(e) if e.is_connection_error() => {
                if let Some(fallback) = &self.fallback {
                    debug!(
                        project = %project,
                        benchmark = %benchmark,
                        version = %version,
                        "Server unavailable, falling back to local storage"
                    );
                    fallback
                        .get_baseline_version(project, benchmark, version)
                        .await
                } else {
                    Err(e)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Uploads a baseline with fallback support.
    ///
    /// If the server is unavailable and fallback is configured, saves to local storage.
    pub async fn upload_baseline(
        &self,
        project: &str,
        request: &UploadBaselineRequest,
    ) -> Result<UploadBaselineResponse, ClientError> {
        match self.client.upload_baseline(project, request).await {
            Ok(response) => Ok(response),
            Err(e) if e.is_connection_error() => {
                if let Some(fallback) = &self.fallback {
                    debug!(
                        project = %project,
                        benchmark = %request.benchmark,
                        "Server unavailable, saving to local fallback storage"
                    );
                    fallback.save_baseline(project, request).await
                } else {
                    Err(e)
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Lists baselines (server only, no fallback).
    pub async fn list_baselines(
        &self,
        project: &str,
        query: &ListBaselinesQuery,
    ) -> Result<ListBaselinesResponse, ClientError> {
        self.client.list_baselines(project, query).await
    }

    /// Deletes a baseline (server only, no fallback).
    pub async fn delete_baseline(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<(), ClientError> {
        self.client
            .delete_baseline(project, benchmark, version)
            .await
    }

    /// Promotes a baseline (server only, no fallback).
    pub async fn promote_baseline(
        &self,
        project: &str,
        benchmark: &str,
        request: &PromoteBaselineRequest,
    ) -> Result<PromoteBaselineResponse, ClientError> {
        self.client
            .promote_baseline(project, benchmark, request)
            .await
    }

    /// Checks server health.
    pub async fn health_check(&self) -> Result<HealthResponse, ClientError> {
        self.client.health_check().await
    }

    /// Returns true if the server is healthy.
    pub async fn is_healthy(&self) -> bool {
        self.client.is_healthy().await
    }

    /// Checks if fallback storage is available.
    pub fn has_fallback(&self) -> bool {
        self.fallback.is_some()
    }
}

/// Local filesystem fallback storage.
#[derive(Debug)]
pub struct LocalFallbackStorage {
    dir: PathBuf,
}

impl LocalFallbackStorage {
    /// Creates a new local fallback storage.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Gets the latest baseline from local storage.
    pub async fn get_latest_baseline(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<BaselineRecord, ClientError> {
        let project_dir = self.dir.join(project);

        let mut entries = match fs::read_dir(&project_dir).await {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Directory doesn't exist means no baselines
                return Err(ClientError::NotFoundError(format!(
                    "No baseline found for {}/{}",
                    project, benchmark
                )));
            }
            Err(e) => {
                return Err(ClientError::FallbackError(format!(
                    "Failed to read directory: {}",
                    e
                )));
            }
        };

        let mut latest: Option<(String, BaselineRecord)> = None;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| ClientError::FallbackError(format!("Failed to read entry: {}", e)))?
        {
            let file_name = entry.file_name();
            let name = file_name.to_string_lossy();

            // Check if file matches pattern
            if name.starts_with(&format!("{}-", benchmark)) && name.ends_with(".json") {
                let path = entry.path();
                let content = fs::read_to_string(&path).await.map_err(|e| {
                    ClientError::FallbackError(format!("Failed to read file: {}", e))
                })?;

                let record: BaselineRecord =
                    serde_json::from_str(&content).map_err(ClientError::ParseError)?;

                // Compare by created_at timestamp
                match &latest {
                    None => latest = Some((name.to_string(), record)),
                    Some((_, existing)) => {
                        if record.created_at > existing.created_at {
                            latest = Some((name.to_string(), record));
                        }
                    }
                }
            }
        }

        latest.map(|(_, record)| record).ok_or_else(|| {
            ClientError::NotFoundError(format!("No baseline found for {}/{}", project, benchmark))
        })
    }

    /// Gets a specific baseline version from local storage.
    pub async fn get_baseline_version(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<BaselineRecord, ClientError> {
        let file_name = format!("{}-{}.json", benchmark, version);
        let path = self.dir.join(project).join(&file_name);

        let content = fs::read_to_string(&path).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ClientError::NotFoundError(format!(
                    "Baseline {}/{} not found in fallback storage",
                    benchmark, version
                ))
            } else {
                ClientError::FallbackError(format!("Failed to read file: {}", e))
            }
        })?;

        serde_json::from_str(&content).map_err(ClientError::ParseError)
    }

    /// Saves a baseline to local storage.
    pub async fn save_baseline(
        &self,
        project: &str,
        request: &UploadBaselineRequest,
    ) -> Result<UploadBaselineResponse, ClientError> {
        // Ensure directory exists
        let project_dir = self.dir.join(project);
        fs::create_dir_all(&project_dir).await.map_err(|e| {
            ClientError::FallbackError(format!("Failed to create directory: {}", e))
        })?;

        // Generate version if not provided
        let version = request
            .version
            .clone()
            .unwrap_or_else(|| chrono::Utc::now().format("%Y%m%d-%H%M%S").to_string());

        // Create a baseline record
        let now = chrono::Utc::now();
        let record = BaselineRecord {
            schema: "perfgate.baseline.v1".to_string(),
            id: format!("local_{}", uuid::Uuid::new_v4()),
            project: project.to_string(),
            benchmark: request.benchmark.clone(),
            version: version.clone(),
            git_ref: request.git_ref.clone(),
            git_sha: request.git_sha.clone(),
            receipt: request.receipt.clone(),
            metadata: request.metadata.clone(),
            tags: request.tags.clone(),
            created_at: now,
            updated_at: now,
            content_hash: "local".to_string(),
            source: BaselineSource::Upload,
            deleted: false,
        };

        // Write to file
        let file_name = format!("{}-{}.json", request.benchmark, version);
        let path = project_dir.join(&file_name);
        let content = serde_json::to_string_pretty(&record).map_err(ClientError::ParseError)?;

        fs::write(&path, content)
            .await
            .map_err(|e| ClientError::FallbackError(format!("Failed to write file: {}", e)))?;

        debug!(
            project = %project,
            benchmark = %request.benchmark,
            version = %version,
            path = %path.display(),
            "Saved baseline to local fallback storage"
        );

        Ok(UploadBaselineResponse {
            id: record.id,
            benchmark: request.benchmark.clone(),
            version,
            created_at: now,
            etag: "\"local\"".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ClientConfig, RetryConfig};
    use perfgate_types::{BenchMeta, HostInfo, RunMeta, RunReceipt, Stats, ToolInfo, U64Summary};
    use tempfile::tempdir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn create_test_receipt(benchmark: &str) -> RunReceipt {
        RunReceipt {
            schema: "perfgate.run.v1".to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.1.0".to_string(),
            },
            run: RunMeta {
                id: "test".to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                ended_at: "2026-01-01T00:01:00Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: Some(8),
                    memory_bytes: Some(16000000000),
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: benchmark.to_string(),
                cwd: None,
                command: vec!["./bench.sh".to_string()],
                repeat: 5,
                warmup: 1,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![],
            stats: Stats {
                wall_ms: U64Summary {
                    median: 100,
                    min: 90,
                    max: 110,
                },
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        }
    }

    fn create_test_upload_request(benchmark: &str) -> UploadBaselineRequest {
        UploadBaselineRequest {
            benchmark: benchmark.to_string(),
            version: Some("v1.0.0".to_string()),
            git_ref: None,
            git_sha: None,
            receipt: create_test_receipt(benchmark),
            metadata: Default::default(),
            tags: vec![],
            normalize: false,
        }
    }

    #[tokio::test]
    async fn test_fallback_get_latest_from_server() {
        let mock_server = MockServer::start().await;
        let temp_dir = tempdir().unwrap();

        Mock::given(method("GET"))
            .and(path("/projects/test-project/baselines/my-bench/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(BaselineRecord {
                schema: "perfgate.baseline.v1".to_string(),
                id: "bl_123".to_string(),
                project: "test-project".to_string(),
                benchmark: "my-bench".to_string(),
                version: "v1.0.0".to_string(),
                git_ref: None,
                git_sha: None,
                receipt: create_test_receipt("my-bench"),
                metadata: Default::default(),
                tags: vec![],
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                content_hash: "abc123".to_string(),
                source: BaselineSource::Upload,
                deleted: false,
            }))
            .mount(&mock_server)
            .await;

        let config = ClientConfig::new(mock_server.uri())
            .with_retry(RetryConfig {
                max_retries: 0,
                ..Default::default()
            })
            .with_fallback(FallbackStorage::local(temp_dir.path()));

        let client = BaselineClient::new(config).unwrap();
        let fallback_client = FallbackClient::new(client, None);

        let result = fallback_client
            .get_latest_baseline("test-project", "my-bench")
            .await
            .unwrap();

        assert_eq!(result.id, "bl_123");
    }

    #[tokio::test]
    async fn test_fallback_get_latest_from_local() {
        let temp_dir = tempdir().unwrap();

        // Create a local baseline file
        let project_dir = temp_dir.path().join("test-project");
        fs::create_dir_all(&project_dir).await.unwrap();

        let record = BaselineRecord {
            schema: "perfgate.baseline.v1".to_string(),
            id: "local_123".to_string(),
            project: "test-project".to_string(),
            benchmark: "my-bench".to_string(),
            version: "v1.0.0".to_string(),
            git_ref: None,
            git_sha: None,
            receipt: create_test_receipt("my-bench"),
            metadata: Default::default(),
            tags: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            content_hash: "abc123".to_string(),
            source: BaselineSource::Upload,
            deleted: false,
        };

        let file_path = project_dir.join("my-bench-v1.0.0.json");
        fs::write(&file_path, serde_json::to_string_pretty(&record).unwrap())
            .await
            .unwrap();

        // Use a non-existent server to trigger fallback
        let config = ClientConfig::new("http://localhost:59999")
            .with_retry(RetryConfig {
                max_retries: 0,
                ..Default::default()
            })
            .with_fallback(FallbackStorage::local(temp_dir.path()));

        let client = BaselineClient::new(config).unwrap();
        let fallback_client =
            FallbackClient::new(client, Some(FallbackStorage::local(temp_dir.path())));

        let result = fallback_client
            .get_latest_baseline("test-project", "my-bench")
            .await
            .unwrap();

        assert_eq!(result.id, "local_123");
    }

    #[tokio::test]
    async fn test_fallback_save_to_local() {
        let temp_dir = tempdir().unwrap();

        // Use a non-existent server to trigger fallback
        let config = ClientConfig::new("http://localhost:59999")
            .with_retry(RetryConfig {
                max_retries: 0,
                ..Default::default()
            })
            .with_fallback(FallbackStorage::local(temp_dir.path()));

        let client = BaselineClient::new(config).unwrap();
        let fallback_client =
            FallbackClient::new(client, Some(FallbackStorage::local(temp_dir.path())));

        let request = create_test_upload_request("my-bench");
        let response = fallback_client
            .upload_baseline("test-project", &request)
            .await
            .unwrap();

        assert!(response.id.starts_with("local_"));
        assert_eq!(response.benchmark, "my-bench");

        // Verify file was created
        let project_dir = temp_dir.path().join("test-project");
        let file_path = project_dir.join("my-bench-v1.0.0.json");
        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_fallback_not_found_error() {
        let temp_dir = tempdir().unwrap();

        // Use a non-existent server to trigger fallback
        let config = ClientConfig::new("http://localhost:59999")
            .with_retry(RetryConfig {
                max_retries: 0,
                ..Default::default()
            })
            .with_fallback(FallbackStorage::local(temp_dir.path()));

        let client = BaselineClient::new(config).unwrap();
        let fallback_client =
            FallbackClient::new(client, Some(FallbackStorage::local(temp_dir.path())));

        let result = fallback_client
            .get_latest_baseline("test-project", "nonexistent")
            .await;

        assert!(matches!(result, Err(ClientError::NotFoundError(_))));
    }
}
