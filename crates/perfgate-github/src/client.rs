//! GitHub REST API client for managing PR comments.

use crate::error::GitHubError;
use crate::types::{GitHubComment, GitHubCommentRequest};
use reqwest::header::{self, HeaderMap, HeaderValue};
use tracing::debug;

/// Canonical marker embedded in PR comments to identify perfgate comments for idempotent updates.
pub const COMMENT_MARKER: &str = "<!-- perfgate:comment kind=summary -->";

/// Legacy marker still recognized so old comments converge onto the sticky path.
pub const LEGACY_COMMENT_MARKER: &str = "<!-- perfgate -->";

fn has_comment_marker(body: &str) -> bool {
    body.contains(COMMENT_MARKER) || body.contains(LEGACY_COMMENT_MARKER)
}

/// Client for the GitHub REST API, focused on issue/PR comments.
#[derive(Clone, Debug)]
pub struct GitHubClient {
    base_url: String,
    inner: reqwest::Client,
}

impl GitHubClient {
    /// Creates a new GitHubClient with the given token.
    ///
    /// The `base_url` should be `https://api.github.com` for github.com
    /// or `https://github.example.com/api/v3` for GitHub Enterprise.
    pub fn new(base_url: &str, token: &str) -> Result<Self, GitHubError> {
        let mut headers = HeaderMap::new();

        let mut auth_value = HeaderValue::from_str(&format!("Bearer {}", token))
            .map_err(|e| GitHubError::Config(format!("Invalid token header: {}", e)))?;
        auth_value.set_sensitive(true);
        headers.insert(header::AUTHORIZATION, auth_value);

        headers.insert(
            header::ACCEPT,
            HeaderValue::from_static("application/vnd.github+json"),
        );
        headers.insert(
            "X-GitHub-Api-Version",
            HeaderValue::from_static("2022-11-28"),
        );
        headers.insert(header::USER_AGENT, HeaderValue::from_static("perfgate-bot"));

        let inner = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| GitHubError::Config(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            inner,
        })
    }

    /// List comments on a pull request / issue.
    pub async fn list_comments(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
    ) -> Result<Vec<GitHubComment>, GitHubError> {
        let mut all_comments = Vec::new();
        let mut page = 1u32;

        loop {
            let url = format!(
                "{}/repos/{}/{}/issues/{}/comments?per_page=100&page={}",
                self.base_url, owner, repo, pr_number, page,
            );
            debug!(url = %url, "Listing PR comments");

            let response = self
                .inner
                .get(&url)
                .send()
                .await
                .map_err(GitHubError::Request)?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response.text().await.unwrap_or_default();
                return Err(GitHubError::Api {
                    status,
                    message: body,
                });
            }

            let comments: Vec<GitHubComment> =
                response.json().await.map_err(GitHubError::Request)?;

            let is_last = comments.len() < 100;
            all_comments.extend(comments);

            if is_last {
                break;
            }
            page += 1;
        }

        Ok(all_comments)
    }

    /// Create a new comment on a pull request / issue.
    pub async fn create_comment(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
        body: &str,
    ) -> Result<GitHubComment, GitHubError> {
        let url = format!(
            "{}/repos/{}/{}/issues/{}/comments",
            self.base_url, owner, repo, pr_number,
        );
        debug!(url = %url, "Creating PR comment");

        let request = GitHubCommentRequest {
            body: body.to_string(),
        };

        let response = self
            .inner
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(GitHubError::Request)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(GitHubError::Api {
                status,
                message: body,
            });
        }

        response.json().await.map_err(GitHubError::Request)
    }

    /// Update an existing comment.
    pub async fn update_comment(
        &self,
        owner: &str,
        repo: &str,
        comment_id: u64,
        body: &str,
    ) -> Result<GitHubComment, GitHubError> {
        let url = format!(
            "{}/repos/{}/{}/issues/comments/{}",
            self.base_url, owner, repo, comment_id,
        );
        debug!(url = %url, comment_id = comment_id, "Updating PR comment");

        let request = GitHubCommentRequest {
            body: body.to_string(),
        };

        let response = self
            .inner
            .patch(&url)
            .json(&request)
            .send()
            .await
            .map_err(GitHubError::Request)?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().await.unwrap_or_default();
            return Err(GitHubError::Api {
                status,
                message: body,
            });
        }

        response.json().await.map_err(GitHubError::Request)
    }

    /// Find an existing perfgate comment on a PR by looking for the marker.
    pub async fn find_perfgate_comment(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
    ) -> Result<Option<GitHubComment>, GitHubError> {
        let comments = self.list_comments(owner, repo, pr_number).await?;
        Ok(comments
            .into_iter()
            .rev()
            .find(|c| has_comment_marker(&c.body)))
    }

    /// Create or update the perfgate comment on a PR (idempotent).
    ///
    /// If a comment with the perfgate marker already exists, it is updated.
    /// Otherwise, a new comment is created.
    ///
    /// Returns `(comment, created)` where `created` is true if a new comment was made.
    pub async fn upsert_comment(
        &self,
        owner: &str,
        repo: &str,
        pr_number: u64,
        body: &str,
    ) -> Result<(GitHubComment, bool), GitHubError> {
        let existing = self.find_perfgate_comment(owner, repo, pr_number).await?;

        match existing {
            Some(comment) => {
                debug!(
                    comment_id = comment.id,
                    "Updating existing perfgate comment"
                );
                let updated = self.update_comment(owner, repo, comment.id, body).await?;
                Ok((updated, false))
            }
            None => {
                debug!("Creating new perfgate comment");
                let created = self.create_comment(owner, repo, pr_number, body).await?;
                Ok((created, true))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{bearer_token, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_create_comment() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/issues/1/comments"))
            .and(bearer_token("test-token"))
            .and(header("Accept", "application/vnd.github+json"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 42,
                "body": "test body",
                "html_url": "https://github.com/owner/repo/pull/1#issuecomment-42",
                "user": {
                    "login": "perfgate-bot"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::new(&mock_server.uri(), "test-token").unwrap();
        let comment = client
            .create_comment("owner", "repo", 1, "test body")
            .await
            .unwrap();

        assert_eq!(comment.id, 42);
        assert_eq!(comment.body, "test body");
    }

    #[tokio::test]
    async fn test_update_comment() {
        let mock_server = MockServer::start().await;

        Mock::given(method("PATCH"))
            .and(path("/repos/owner/repo/issues/comments/42"))
            .and(bearer_token("test-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 42,
                "body": "updated body",
                "html_url": "https://github.com/owner/repo/pull/1#issuecomment-42",
                "user": {
                    "login": "perfgate-bot"
                }
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::new(&mock_server.uri(), "test-token").unwrap();
        let comment = client
            .update_comment("owner", "repo", 42, "updated body")
            .await
            .unwrap();

        assert_eq!(comment.id, 42);
        assert_eq!(comment.body, "updated body");
    }

    #[tokio::test]
    async fn test_find_perfgate_comment() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": 1,
                    "body": "unrelated comment",
                    "html_url": "https://github.com/owner/repo/pull/1#issuecomment-1",
                    "user": { "login": "someone" }
                },
                {
                    "id": 2,
                    "body": "<!-- perfgate:comment kind=summary -->\nperfgate results",
                    "html_url": "https://github.com/owner/repo/pull/1#issuecomment-2",
                    "user": { "login": "perfgate-bot" }
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::new(&mock_server.uri(), "test-token").unwrap();
        let found = client
            .find_perfgate_comment("owner", "repo", 1)
            .await
            .unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().id, 2);
    }

    #[tokio::test]
    async fn test_find_perfgate_comment_not_found() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": 1,
                    "body": "no marker here",
                    "html_url": "https://github.com/owner/repo/pull/1#issuecomment-1",
                    "user": { "login": "someone" }
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::new(&mock_server.uri(), "test-token").unwrap();
        let found = client
            .find_perfgate_comment("owner", "repo", 1)
            .await
            .unwrap();

        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_upsert_creates_when_no_existing() {
        let mock_server = MockServer::start().await;

        // List returns empty
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock_server)
            .await;

        // Create succeeds
        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/issues/1/comments"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({
                "id": 99,
                "body": "new comment",
                "html_url": "https://github.com/owner/repo/pull/1#issuecomment-99",
                "user": { "login": "perfgate-bot" }
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::new(&mock_server.uri(), "test-token").unwrap();
        let (comment, created) = client
            .upsert_comment("owner", "repo", 1, "new comment")
            .await
            .unwrap();

        assert!(created);
        assert_eq!(comment.id, 99);
    }

    #[tokio::test]
    async fn test_upsert_updates_when_existing() {
        let mock_server = MockServer::start().await;

        // List returns existing perfgate comment
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": 50,
                    "body": "<!-- perfgate:comment kind=summary -->\nold content",
                    "html_url": "https://github.com/owner/repo/pull/1#issuecomment-50",
                    "user": { "login": "perfgate-bot" }
                }
            ])))
            .mount(&mock_server)
            .await;

        // Update succeeds
        Mock::given(method("PATCH"))
            .and(path("/repos/owner/repo/issues/comments/50"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "id": 50,
                "body": "<!-- perfgate:comment kind=summary -->\nnew content",
                "html_url": "https://github.com/owner/repo/pull/1#issuecomment-50",
                "user": { "login": "perfgate-bot" }
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::new(&mock_server.uri(), "test-token").unwrap();
        let (comment, created) = client
            .upsert_comment(
                "owner",
                "repo",
                1,
                "<!-- perfgate:comment kind=summary -->\nnew content",
            )
            .await
            .unwrap();

        assert!(!created);
        assert_eq!(comment.id, 50);
    }

    #[tokio::test]
    async fn test_find_perfgate_comment_accepts_legacy_marker() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": 7,
                    "body": "<!-- perfgate -->\nlegacy content",
                    "html_url": "https://github.com/owner/repo/pull/1#issuecomment-7",
                    "user": { "login": "perfgate-bot" }
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::new(&mock_server.uri(), "test-token").unwrap();
        let found = client
            .find_perfgate_comment("owner", "repo", 1)
            .await
            .unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().id, 7);
    }

    #[tokio::test]
    async fn test_find_perfgate_comment_prefers_newest_marker_match() {
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/issues/1/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": 10,
                    "body": "<!-- perfgate -->\nold content",
                    "html_url": "https://github.com/owner/repo/pull/1#issuecomment-10",
                    "user": { "login": "perfgate-bot" }
                },
                {
                    "id": 11,
                    "body": "<!-- perfgate:comment kind=summary -->\nnew content",
                    "html_url": "https://github.com/owner/repo/pull/1#issuecomment-11",
                    "user": { "login": "perfgate-bot" }
                }
            ])))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::new(&mock_server.uri(), "test-token").unwrap();
        let found = client
            .find_perfgate_comment("owner", "repo", 1)
            .await
            .unwrap();

        assert!(found.is_some());
        assert_eq!(found.unwrap().id, 11);
    }

    #[tokio::test]
    async fn test_api_error() {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/repos/owner/repo/issues/1/comments"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "message": "Resource not accessible by integration"
            })))
            .mount(&mock_server)
            .await;

        let client = GitHubClient::new(&mock_server.uri(), "test-token").unwrap();
        let result = client.create_comment("owner", "repo", 1, "test").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, GitHubError::Api { status: 403, .. }));
    }
}
