//! Client for the perfgate baseline service.

use crate::config::ClientConfig;
use crate::error::ClientError;
use crate::types::*;
use reqwest::header::{self, HeaderMap, HeaderValue};
use tracing::debug;

/// High-level client for the perfgate baseline service.
#[derive(Clone, Debug)]
pub struct BaselineClient {
    config: ClientConfig,
    inner: reqwest::Client,
}

impl BaselineClient {
    /// Creates a new BaselineClient from the given configuration.
    pub fn new(config: ClientConfig) -> Result<Self, ClientError> {
        config.validate().map_err(ClientError::ValidationError)?;

        let mut headers = HeaderMap::new();

        if let Some(auth_val) = config.auth.header_value() {
            let mut auth_value = HeaderValue::from_str(&auth_val)
                .map_err(|e| ClientError::ValidationError(format!("Invalid auth header: {}", e)))?;
            auth_value.set_sensitive(true);
            headers.insert(header::AUTHORIZATION, auth_value);
        }

        let inner = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(config.timeout)
            .build()
            .map_err(|e| ClientError::ConnectionError(e.to_string()))?;

        Ok(Self { config, inner })
    }

    /// Uploads a new baseline to the server.
    pub async fn upload_baseline(
        &self,
        project: &str,
        request: &UploadBaselineRequest,
    ) -> Result<UploadBaselineResponse, ClientError> {
        self.execute_with_retry(|| {
            let url = self.url(&format!("projects/{}/baselines", project));
            debug!(url = %url, benchmark = %request.benchmark, "Uploading baseline");

            let client = self.inner.clone();
            let request = request.clone();
            async move {
                let response = client
                    .post(url)
                    .json(&request)
                    .send()
                    .await
                    .map_err(ClientError::RequestError)?;

                if !response.status().is_success() {
                    let status = response.status().as_u16();
                    let body = response.text().await.unwrap_or_default();
                    return Err(ClientError::from_http(status, &body));
                }

                let body = response
                    .json::<UploadBaselineResponse>()
                    .await
                    .map_err(ClientError::RequestError)?;
                Ok(body)
            }
        })
        .await
    }

    /// Gets the latest baseline for a benchmark.
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
                let client = self.inner.clone();
                let url = url.clone();
                async move {
                    let resp = client
                        .get(url)
                        .send()
                        .await
                        .map_err(ClientError::RequestError)?;

                    if !resp.status().is_success() {
                        let status = resp.status().as_u16();
                        let body = resp.text().await.unwrap_or_default();
                        return Err(ClientError::from_http(status, &body));
                    }

                    let body = resp
                        .json::<BaselineRecord>()
                        .await
                        .map_err(ClientError::RequestError)?;
                    Ok(body)
                }
            })
            .await?;

        Ok(response)
    }

    /// Gets a specific version of a baseline.
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
        debug!(url = %url, version = %version, "Getting baseline version");

        let response = self
            .execute_with_retry(|| {
                let client = self.inner.clone();
                let url = url.clone();
                async move {
                    let resp = client
                        .get(url)
                        .send()
                        .await
                        .map_err(ClientError::RequestError)?;

                    if !resp.status().is_success() {
                        let status = resp.status().as_u16();
                        let body = resp.text().await.unwrap_or_default();
                        return Err(ClientError::from_http(status, &body));
                    }

                    let body = resp
                        .json::<BaselineRecord>()
                        .await
                        .map_err(ClientError::RequestError)?;
                    Ok(body)
                }
            })
            .await?;

        Ok(response)
    }

    /// Promotes a baseline to a new version.
    pub async fn promote_baseline(
        &self,
        project: &str,
        benchmark: &str,
        request: &PromoteBaselineRequest,
    ) -> Result<PromoteBaselineResponse, ClientError> {
        self.execute_with_retry(|| {
            let url = self.url(&format!("projects/{}/baselines/{}/promote", project, benchmark));
            debug!(url = %url, from = %request.from_version, to = %request.to_version, "Promoting baseline");

            let client = self.inner.clone();
            let request = request.clone();
            async move {
                let response = client
                    .post(url)
                    .json(&request)
                    .send()
                    .await
                    .map_err(ClientError::RequestError)?;

                if !response.status().is_success() {
                    let status = response.status().as_u16();
                    let body = response.text().await.unwrap_or_default();
                    return Err(ClientError::from_http(status, &body));
                }

                let body = response.json::<PromoteBaselineResponse>().await
                    .map_err(ClientError::RequestError)?;
                Ok(body)
            }
        })
        .await
    }

    /// Lists baselines for a project.
    pub async fn list_baselines(
        &self,
        project: &str,
        query: &ListBaselinesQuery,
    ) -> Result<ListBaselinesResponse, ClientError> {
        let mut url = self.url(&format!("projects/{}/baselines", project));

        let params = query.to_query_params();
        if !params.is_empty() {
            let mut url_obj = url::Url::parse(&url).map_err(ClientError::UrlError)?;
            {
                let mut query_pairs = url_obj.query_pairs_mut();
                for (k, v) in params {
                    query_pairs.append_pair(&k, &v);
                }
            }
            url = url_obj.to_string();
        }

        debug!(url = %url, "Listing baselines");

        let response = self
            .execute_with_retry(|| {
                let client = self.inner.clone();
                let url = url.clone();
                async move {
                    let resp = client
                        .get(url)
                        .send()
                        .await
                        .map_err(ClientError::RequestError)?;

                    if !resp.status().is_success() {
                        let status = resp.status().as_u16();
                        let body = resp.text().await.unwrap_or_default();
                        return Err(ClientError::from_http(status, &body));
                    }

                    let body = resp
                        .json::<ListBaselinesResponse>()
                        .await
                        .map_err(ClientError::RequestError)?;
                    Ok(body)
                }
            })
            .await?;

        Ok(response)
    }

    /// Deletes a baseline from the server.
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
        debug!(url = %url, version = %version, "Deleting baseline version");

        self.execute_with_retry(|| {
            let client = self.inner.clone();
            let url = url.clone();
            async move {
                let resp = client
                    .delete(url)
                    .send()
                    .await
                    .map_err(ClientError::RequestError)?;

                if !resp.status().is_success() {
                    let status = resp.status().as_u16();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(ClientError::from_http(status, &body));
                }
                Ok(())
            }
        })
        .await?;

        Ok(())
    }

    /// Submits a benchmark verdict to the server.
    pub async fn submit_verdict(
        &self,
        project: &str,
        request: &SubmitVerdictRequest,
    ) -> Result<VerdictRecord, ClientError> {
        self.execute_with_retry(|| {
            let url = self.url(&format!("projects/{}/verdicts", project));
            debug!(url = %url, benchmark = %request.benchmark, "Submitting verdict");

            let client = self.inner.clone();
            let request = request.clone();
            async move {
                let response = client
                    .post(url)
                    .json(&request)
                    .send()
                    .await
                    .map_err(ClientError::RequestError)?;

                if !response.status().is_success() {
                    let status = response.status().as_u16();
                    let body = response.text().await.unwrap_or_default();
                    return Err(ClientError::from_http(status, &body));
                }

                let body = response
                    .json::<VerdictRecord>()
                    .await
                    .map_err(ClientError::RequestError)?;
                Ok(body)
            }
        })
        .await
    }

    /// Lists verdicts for a project.
    pub async fn list_verdicts(
        &self,
        project: &str,
        query: &ListVerdictsQuery,
    ) -> Result<ListVerdictsResponse, ClientError> {
        self.execute_with_retry(|| {
            let url = self.url(&format!("projects/{}/verdicts", project));
            debug!(url = %url, "Listing verdicts");

            let client = self.inner.clone();
            let query = query.clone();
            async move {
                let response = client
                    .get(url)
                    .query(&query)
                    .send()
                    .await
                    .map_err(ClientError::RequestError)?;

                if !response.status().is_success() {
                    let status = response.status().as_u16();
                    let body = response.text().await.unwrap_or_default();
                    return Err(ClientError::from_http(status, &body));
                }

                let body = response
                    .json::<ListVerdictsResponse>()
                    .await
                    .map_err(ClientError::RequestError)?;
                Ok(body)
            }
        })
        .await
    }

    /// Checks the health of the baseline service.
    pub async fn health_check(&self) -> Result<HealthResponse, ClientError> {
        let url = self.url("health");
        debug!(url = %url, "Checking health");

        let response = self
            .execute_with_retry(|| {
                let client = self.inner.clone();
                let url = url.clone();
                async move {
                    let resp = client
                        .get(url)
                        .send()
                        .await
                        .map_err(ClientError::RequestError)?;

                    if !resp.status().is_success() {
                        let status = resp.status().as_u16();
                        let body = resp.text().await.unwrap_or_default();
                        return Err(ClientError::from_http(status, &body));
                    }

                    let body = resp
                        .json::<HealthResponse>()
                        .await
                        .map_err(ClientError::RequestError)?;
                    Ok(body)
                }
            })
            .await?;

        Ok(response)
    }

    /// Returns true if the service is reachable and healthy.
    pub async fn is_healthy(&self) -> bool {
        match self.health_check().await {
            Ok(h) => h.status == "healthy",
            Err(_) => false,
        }
    }

    fn url(&self, path: &str) -> String {
        let mut base = self.config.server_url.clone();
        if !base.ends_with('/') {
            base.push('/');
        }
        format!("{}{}", base, path)
    }

    async fn execute_with_retry<F, Fut, T>(&self, mut operation: F) -> Result<T, ClientError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, ClientError>>,
    {
        let mut attempts = 0;

        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    attempts += 1;
                    let is_retryable = e.is_retryable();

                    if !is_retryable || attempts > self.config.retry.max_retries {
                        return Err(e);
                    }

                    debug!(error = %e, attempt = attempts, "Request failed, retrying");
                    tokio::time::sleep(self.config.retry.delay_for_attempt(attempts)).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_config(url: &str) -> ClientConfig {
        ClientConfig::new(url)
    }

    #[tokio::test]
    async fn test_get_latest_baseline() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/projects/my-project/baselines/my-bench/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "schema": "perfgate.baseline.v1",
                "id": "bl_123",
                "project": "my-project",
                "benchmark": "my-bench",
                "version": "v1.2.3",
                "receipt": {
                    "schema": "perfgate.run.v1",
                    "tool": {"name": "test", "version": "0"},
                    "run": {"id": "r1", "started_at": "2024-01-01T00:00:00Z", "ended_at": "2024-01-01T00:00:01Z", "host": {"os": "linux", "arch": "x86_64"}},
                    "bench": {"name": "my-bench", "command": [], "repeat": 1, "warmup": 0},
                    "samples": [],
                    "stats": {"wall_ms": {"median": 100, "min": 100, "max": 100}}
                },
                "metadata": {},
                "tags": [],
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z",
                "content_hash": "hash123",
                "source": "upload",
                "deleted": false
            })))
            .mount(&mock_server)
            .await;

        let client = BaselineClient::new(test_config(&mock_server.uri())).unwrap();
        let result = client
            .get_latest_baseline("my-project", "my-bench")
            .await
            .unwrap();

        assert_eq!(result.id, "bl_123");
        assert_eq!(result.version, "v1.2.3");
    }

    #[tokio::test]
    async fn test_promote_baseline() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/projects/my-project/baselines/my-bench/promote"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": "bl_new",
                "benchmark": "my-bench",
                "version": "v2.0.0",
                "promoted_from": "v1.0.0",
                "promoted_at": "2024-01-01T00:00:00Z",
                "created_at": "2024-01-01T00:00:00Z"
            })))
            .mount(&mock_server)
            .await;

        let client = BaselineClient::new(test_config(&mock_server.uri())).unwrap();
        let request = PromoteBaselineRequest {
            from_version: "v1.0.0".to_string(),
            to_version: "v2.0.0".to_string(),
            git_ref: None,
            git_sha: None,
            tags: vec![],
            normalize: true,
        };
        let response = client
            .promote_baseline("my-project", "my-bench", &request)
            .await
            .unwrap();

        assert_eq!(response.version, "v2.0.0");
        assert_eq!(response.promoted_from, "v1.0.0");
    }
}
