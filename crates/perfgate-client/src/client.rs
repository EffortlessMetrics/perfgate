//! Baseline client implementation.
//!
//! This module provides the main client for interacting with the baseline service.

use crate::config::ClientConfig;
use crate::error::ClientError;
use crate::types::*;
use reqwest::{Client, Response, StatusCode};
use tracing::{debug, warn};
use url::Url;

/// Client for interacting with the baseline service.
#[derive(Debug)]
pub struct BaselineClient {
    config: ClientConfig,
    http: Client,
    base_url: Url,
}

impl BaselineClient {
    /// Creates a new baseline client with the given configuration.
    pub fn new(config: ClientConfig) -> Result<Self, ClientError> {
        config.validate().map_err(ClientError::ValidationError)?;

        let http = Client::builder()
            .timeout(config.timeout)
            .user_agent(format!("perfgate-client/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(ClientError::RequestError)?;

        let base_url = Url::parse(&config.server_url)?;

        Ok(Self {
            config,
            http,
            base_url,
        })
    }

    /// Creates a client with default configuration for the given server URL.
    pub fn with_server_url(server_url: impl Into<String>) -> Result<Self, ClientError> {
        Self::new(ClientConfig::new(server_url))
    }

    /// Uploads a new baseline to the server.
    ///
    /// # Arguments
    /// * `project` - The project/namespace identifier.
    /// * `request` - The upload request containing baseline data.
    pub async fn upload_baseline(
        &self,
        project: &str,
        request: &UploadBaselineRequest,
    ) -> Result<UploadBaselineResponse, ClientError> {
        let url = self.url(&format!("projects/{}/baselines", project));
        debug!(url = %url, "Uploading baseline");

        let response = self
            .execute_with_retry(|| {
                let mut builder = self.http.post(url.clone()).json(request);
                if let Some(header) = self.config.auth.header_value() {
                    builder = builder.header("Authorization", header);
                }
                builder
            })
            .await?;

        let status = response.status();
        let body = response.text().await.map_err(ClientError::RequestError)?;

        if status == StatusCode::CREATED {
            serde_json::from_str(&body).map_err(ClientError::ParseError)
        } else {
            Err(ClientError::from_http(status.as_u16(), &body))
        }
    }

    /// Gets the latest baseline for a benchmark.
    ///
    /// # Arguments
    /// * `project` - The project/namespace identifier.
    /// * `benchmark` - The benchmark name.
    pub async fn get_latest_baseline(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<BaselineRecord, ClientError> {
        let url = self.url(&format!(
            "projects/{}/baselines/{}/latest",
            project, benchmark
        ));
        debug!(url = %url, "Getting latest baseline");

        let response = self
            .execute_with_retry(|| {
                let mut builder = self.http.get(url.clone());
                if let Some(header) = self.config.auth.header_value() {
                    builder = builder.header("Authorization", header);
                }
                builder
            })
            .await?;

        let status = response.status();
        let body = response.text().await.map_err(ClientError::RequestError)?;

        if status == StatusCode::OK {
            serde_json::from_str(&body).map_err(ClientError::ParseError)
        } else {
            Err(ClientError::from_http(status.as_u16(), &body))
        }
    }

    /// Gets a specific baseline version.
    ///
    /// # Arguments
    /// * `project` - The project/namespace identifier.
    /// * `benchmark` - The benchmark name.
    /// * `version` - The version identifier (string, not u64).
    pub async fn get_baseline_version(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<BaselineRecord, ClientError> {
        let url = self.url(&format!(
            "projects/{}/baselines/{}/versions/{}",
            project, benchmark, version
        ));
        debug!(url = %url, "Getting baseline version");

        let response = self
            .execute_with_retry(|| {
                let mut builder = self.http.get(url.clone());
                if let Some(header) = self.config.auth.header_value() {
                    builder = builder.header("Authorization", header);
                }
                builder
            })
            .await?;

        let status = response.status();
        let body = response.text().await.map_err(ClientError::RequestError)?;

        if status == StatusCode::OK {
            serde_json::from_str(&body).map_err(ClientError::ParseError)
        } else {
            Err(ClientError::from_http(status.as_u16(), &body))
        }
    }

    /// Lists baselines with optional filtering.
    ///
    /// # Arguments
    /// * `project` - The project/namespace identifier.
    /// * `query` - Query parameters for filtering and pagination.
    pub async fn list_baselines(
        &self,
        project: &str,
        query: &ListBaselinesQuery,
    ) -> Result<ListBaselinesResponse, ClientError> {
        let mut url = self.url(&format!("projects/{}/baselines", project));

        // Add query parameters
        {
            let mut pairs = url.query_pairs_mut();
            for (key, value) in query.to_query_params() {
                pairs.append_pair(&key, &value);
            }
        }

        debug!(url = %url, "Listing baselines");

        let response = self
            .execute_with_retry(|| {
                let mut builder = self.http.get(url.clone());
                if let Some(header) = self.config.auth.header_value() {
                    builder = builder.header("Authorization", header);
                }
                builder
            })
            .await?;

        let status = response.status();
        let body = response.text().await.map_err(ClientError::RequestError)?;

        if status == StatusCode::OK {
            serde_json::from_str(&body).map_err(ClientError::ParseError)
        } else {
            Err(ClientError::from_http(status.as_u16(), &body))
        }
    }

    /// Deletes a baseline version.
    ///
    /// # Arguments
    /// * `project` - The project/namespace identifier.
    /// * `benchmark` - The benchmark name.
    /// * `version` - The version identifier (string, not u64).
    pub async fn delete_baseline(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<(), ClientError> {
        let url = self.url(&format!(
            "projects/{}/baselines/{}/versions/{}",
            project, benchmark, version
        ));
        debug!(url = %url, "Deleting baseline");

        let response = self
            .execute_with_retry(|| {
                let mut builder = self.http.delete(url.clone());
                if let Some(header) = self.config.auth.header_value() {
                    builder = builder.header("Authorization", header);
                }
                builder
            })
            .await?;

        let status = response.status();
        let body = response.text().await.map_err(ClientError::RequestError)?;

        if status == StatusCode::OK {
            Ok(())
        } else {
            Err(ClientError::from_http(status.as_u16(), &body))
        }
    }

    /// Promotes a baseline version.
    ///
    /// # Arguments
    /// * `project` - The project/namespace identifier.
    /// * `benchmark` - The benchmark name.
    /// * `request` - The promotion request.
    pub async fn promote_baseline(
        &self,
        project: &str,
        benchmark: &str,
        request: &PromoteBaselineRequest,
    ) -> Result<PromoteBaselineResponse, ClientError> {
        let url = self.url(&format!(
            "projects/{}/baselines/{}/promote",
            project, benchmark
        ));
        debug!(url = %url, "Promoting baseline");

        let response = self
            .execute_with_retry(|| {
                let mut builder = self.http.post(url.clone()).json(request);
                if let Some(header) = self.config.auth.header_value() {
                    builder = builder.header("Authorization", header);
                }
                builder
            })
            .await?;

        let status = response.status();
        let body = response.text().await.map_err(ClientError::RequestError)?;

        if status == StatusCode::CREATED {
            serde_json::from_str(&body).map_err(ClientError::ParseError)
        } else {
            Err(ClientError::from_http(status.as_u16(), &body))
        }
    }

    /// Checks server health.
    pub async fn health_check(&self) -> Result<HealthResponse, ClientError> {
        let url = self.url("health");
        debug!(url = %url, "Checking health");

        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| ClientError::ConnectionError(e.to_string()))?;

        let status = response.status();
        let body = response.text().await.map_err(ClientError::RequestError)?;

        if status == StatusCode::OK {
            serde_json::from_str(&body).map_err(ClientError::ParseError)
        } else {
            Err(ClientError::from_http(status.as_u16(), &body))
        }
    }

    /// Returns true if the server is healthy.
    pub async fn is_healthy(&self) -> bool {
        self.health_check().await.is_ok()
    }

    /// Builds a full URL from a path.
    fn url(&self, path: &str) -> Url {
        self.base_url.join(path).expect("Invalid URL path")
    }

    /// Executes a request with retry logic.
    async fn execute_with_retry<F>(&self, request_fn: F) -> Result<Response, ClientError>
    where
        F: Fn() -> reqwest::RequestBuilder,
    {
        let retry_config = &self.config.retry;
        let mut last_error = None;

        for attempt in 0..=retry_config.max_retries {
            if attempt > 0 {
                let delay = retry_config.delay_for_attempt(attempt - 1);
                debug!(attempt, delay_ms = delay.as_millis(), "Retrying request");
                tokio::time::sleep(delay).await;
            }

            let builder = request_fn();
            match builder.send().await {
                Ok(response) => {
                    let status = response.status();

                    // Check if we should retry based on status code
                    if retry_config.retry_status_codes.contains(&status.as_u16())
                        && attempt < retry_config.max_retries
                    {
                        warn!(
                            attempt,
                            status = status.as_u16(),
                            "Request failed with retryable status"
                        );
                        last_error = Some(ClientError::from_http(
                            status.as_u16(),
                            &format!("HTTP {}", status),
                        ));
                        continue;
                    }

                    return Ok(response);
                }
                Err(e) => {
                    let is_connect_error = e.is_connect() || e.is_timeout() || e.is_request();

                    if is_connect_error {
                        if attempt < retry_config.max_retries {
                            warn!(attempt, error = %e, "Request failed, will retry");
                        }
                        last_error = Some(ClientError::ConnectionError(e.to_string()));
                        if attempt < retry_config.max_retries {
                            continue;
                        }
                        // Return ConnectionError on final attempt for connection errors
                        return Err(ClientError::ConnectionError(e.to_string()));
                    }

                    return Err(ClientError::RequestError(e));
                }
            }
        }

        Err(ClientError::RetryExhausted {
            retries: retry_config.max_retries,
            message: last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown error".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RetryConfig;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(server_url: &str) -> ClientConfig {
        ClientConfig::new(server_url)
            .with_api_key("test-key")
            .with_retry(RetryConfig {
                max_retries: 0, // Disable retries for tests
                ..Default::default()
            })
    }

    #[tokio::test]
    async fn test_health_check() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(HealthResponse {
                status: "healthy".to_string(),
                version: "2.0.0".to_string(),
                storage: StorageHealth {
                    backend: "memory".to_string(),
                    status: "connected".to_string(),
                },
            }))
            .mount(&mock_server)
            .await;

        let client = BaselineClient::new(test_config(&mock_server.uri())).unwrap();
        let health = client.health_check().await.unwrap();

        assert_eq!(health.status, "healthy");
        assert_eq!(health.version, "2.0.0");
    }

    #[tokio::test]
    async fn test_is_healthy() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(HealthResponse {
                status: "healthy".to_string(),
                version: "2.0.0".to_string(),
                storage: StorageHealth {
                    backend: "memory".to_string(),
                    status: "connected".to_string(),
                },
            }))
            .mount(&mock_server)
            .await;

        let client = BaselineClient::new(test_config(&mock_server.uri())).unwrap();
        assert!(client.is_healthy().await);
    }

    #[tokio::test]
    async fn test_get_latest_baseline_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/my-project/baselines/my-bench/latest"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": {
                    "code": "NOT_FOUND",
                    "message": "Baseline 'my-bench/latest' not found"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = BaselineClient::new(test_config(&mock_server.uri())).unwrap();
        let result = client.get_latest_baseline("my-project", "my-bench").await;

        assert!(matches!(result, Err(ClientError::NotFoundError(_))));
    }

    #[tokio::test]
    async fn test_upload_baseline_success() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/projects/my-project/baselines"))
            .respond_with(
                ResponseTemplate::new(201).set_body_json(UploadBaselineResponse {
                    id: "bl_123".to_string(),
                    benchmark: "my-bench".to_string(),
                    version: "v1.0.0".to_string(),
                    created_at: chrono::Utc::now(),
                    etag: "\"sha256:abc123\"".to_string(),
                }),
            )
            .mount(&mock_server)
            .await;

        let client = BaselineClient::new(test_config(&mock_server.uri())).unwrap();

        // Create a minimal upload request
        let request = UploadBaselineRequest {
            benchmark: "my-bench".to_string(),
            version: Some("v1.0.0".to_string()),
            git_ref: None,
            git_sha: None,
            receipt: perfgate_types::RunReceipt {
                schema: "perfgate.run.v1".to_string(),
                tool: perfgate_types::ToolInfo {
                    name: "perfgate".to_string(),
                    version: "0.1.0".to_string(),
                },
                run: perfgate_types::RunMeta {
                    id: "test".to_string(),
                    started_at: "2026-01-01T00:00:00Z".to_string(),
                    ended_at: "2026-01-01T00:01:00Z".to_string(),
                    host: perfgate_types::HostInfo {
                        os: "linux".to_string(),
                        arch: "x86_64".to_string(),
                        cpu_count: Some(8),
                        memory_bytes: Some(16000000000),
                        hostname_hash: None,
                    },
                },
                bench: perfgate_types::BenchMeta {
                    name: "my-bench".to_string(),
                    cwd: None,
                    command: vec!["./bench.sh".to_string()],
                    repeat: 5,
                    warmup: 1,
                    work_units: None,
                    timeout_ms: None,
                },
                samples: vec![],
                stats: perfgate_types::Stats {
                    wall_ms: perfgate_types::U64Summary {
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
            },
            metadata: Default::default(),
            tags: vec![],
            normalize: false,
        };

        let response = client
            .upload_baseline("my-project", &request)
            .await
            .unwrap();
        assert_eq!(response.id, "bl_123");
        assert_eq!(response.benchmark, "my-bench");
    }

    #[tokio::test]
    async fn test_connection_error() {
        let client = BaselineClient::new(test_config("http://localhost:59999")).unwrap();

        let result = client.health_check().await;
        assert!(matches!(result, Err(ClientError::ConnectionError(_))));
    }

    #[tokio::test]
    async fn test_list_baselines() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/my-project/baselines"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(ListBaselinesResponse {
                    baselines: vec![],
                    pagination: PaginationInfo {
                        total: 0,
                        limit: 50,
                        offset: 0,
                        has_more: false,
                    },
                }),
            )
            .mount(&mock_server)
            .await;

        let client = BaselineClient::new(test_config(&mock_server.uri())).unwrap();
        let query = ListBaselinesQuery::new();
        let response = client.list_baselines("my-project", &query).await.unwrap();

        assert_eq!(response.baselines.len(), 0);
        assert_eq!(response.pagination.total, 0);
    }
}
