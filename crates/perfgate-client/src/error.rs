//! Error types for the perfgate client.
//!
//! This module defines all error conditions that can occur when interacting
//! with the baseline service.

use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;

/// Client error type.
#[derive(Debug, Error)]
pub enum ClientError {
    /// HTTP request failed.
    #[error("HTTP error: {status} - {message}")]
    HttpError {
        /// HTTP status code.
        status: u16,
        /// Error message from the server.
        message: String,
        /// Error code from the server.
        code: Option<String>,
    },

    /// Authentication failed.
    #[error("Authentication failed: {0}")]
    AuthError(String),

    /// Baseline not found.
    #[error("Baseline not found: {0}")]
    NotFoundError(String),

    /// Invalid request data.
    #[error("Validation error: {0}")]
    ValidationError(String),

    /// Response parsing failed.
    #[error("Failed to parse response: {0}")]
    ParseError(#[source] serde_json::Error),

    /// Server unreachable or connection failed.
    #[error("Connection error: {0}")]
    ConnectionError(String),

    /// Request timed out.
    #[error("Request timed out after {0:?}")]
    TimeoutError(Duration),

    /// Server returned an error after all retries.
    #[error("Request failed after {retries} retries: {message}")]
    RetryExhausted {
        /// Number of retry attempts.
        retries: u32,
        /// Final error message.
        message: String,
    },

    /// Baseline already exists (conflict).
    #[error("Baseline already exists: {0}")]
    AlreadyExistsError(String),

    /// Fallback storage error.
    #[error("Fallback storage error: {0}")]
    FallbackError(String),

    /// No fallback storage available.
    #[error("No fallback storage available")]
    NoFallbackAvailable,

    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(#[source] std::io::Error),

    /// URL parsing error.
    #[error("Invalid URL: {0}")]
    UrlError(#[from] url::ParseError),

    /// Generic request error from reqwest.
    #[error("Request error: {0}")]
    RequestError(#[source] reqwest::Error),
}

impl ClientError {
    /// Creates an HTTP error from a status code and response body.
    pub fn from_http(status: u16, body: &str) -> Self {
        // Try to parse the body as an API error response
        if let Ok(api_error) = serde_json::from_str::<ApiErrorResponse>(body) {
            let code = api_error.error.code.clone();
            let message = api_error.error.message;

            // Map specific error codes to specific error types
            match api_error.error.code.as_str() {
                "UNAUTHORIZED" => ClientError::AuthError(message),
                "FORBIDDEN" => ClientError::AuthError(message),
                "NOT_FOUND" => ClientError::NotFoundError(message),
                "VALIDATION_ERROR" => ClientError::ValidationError(message),
                "ALREADY_EXISTS" => ClientError::AlreadyExistsError(message),
                _ => ClientError::HttpError {
                    status,
                    message,
                    code: Some(code),
                },
            }
        } else {
            // Fallback to generic HTTP error
            ClientError::HttpError {
                status,
                message: body.to_string(),
                code: None,
            }
        }
    }

    /// Returns true if this error indicates the server is unavailable.
    pub fn is_connection_error(&self) -> bool {
        matches!(
            self,
            ClientError::ConnectionError(_)
                | ClientError::TimeoutError(_)
                | ClientError::RetryExhausted { .. }
        )
    }

    /// Returns true if this error could be retried.
    pub fn is_retryable(&self) -> bool {
        match self {
            ClientError::HttpError { status, .. } => {
                // Retry on 5xx errors and 429 (rate limited)
                *status >= 500 || *status == 429
            }
            ClientError::ConnectionError(_) => true,
            ClientError::TimeoutError(_) => true,
            _ => false,
        }
    }
}

/// API error response from the server.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorResponse {
    /// Error details.
    pub error: ApiErrorBody,
}

/// API error body.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    /// Error code.
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Additional details.
    #[serde(default)]
    pub details: Option<serde_json::Value>,
    /// Request ID for tracing.
    #[serde(default)]
    pub request_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_http_unauthorized() {
        let body = r#"{"error":{"code":"UNAUTHORIZED","message":"Invalid API key"}}"#;
        let error = ClientError::from_http(401, body);
        assert!(matches!(error, ClientError::AuthError(_)));
    }

    #[test]
    fn test_from_http_not_found() {
        let body = r#"{"error":{"code":"NOT_FOUND","message":"Baseline not found"}}"#;
        let error = ClientError::from_http(404, body);
        assert!(matches!(error, ClientError::NotFoundError(_)));
    }

    #[test]
    fn test_from_http_validation_error() {
        let body = r#"{"error":{"code":"VALIDATION_ERROR","message":"Invalid benchmark name"}}"#;
        let error = ClientError::from_http(400, body);
        assert!(matches!(error, ClientError::ValidationError(_)));
    }

    #[test]
    fn test_from_http_generic() {
        let body = r#"{"error":{"code":"INTERNAL_ERROR","message":"Something went wrong"}}"#;
        let error = ClientError::from_http(500, body);
        assert!(matches!(error, ClientError::HttpError { .. }));
    }

    #[test]
    fn test_from_http_malformed() {
        let body = "Not JSON";
        let error = ClientError::from_http(500, body);
        assert!(matches!(
            error,
            ClientError::HttpError {
                status: 500,
                message: _,
                code: None,
            }
        ));
    }

    #[test]
    fn test_is_connection_error() {
        assert!(ClientError::ConnectionError("failed".to_string()).is_connection_error());
        assert!(ClientError::TimeoutError(Duration::from_secs(30)).is_connection_error());
        assert!(!ClientError::NotFoundError("not found".to_string()).is_connection_error());
    }

    #[test]
    fn test_is_retryable() {
        // 5xx errors are retryable
        assert!(ClientError::from_http(500, "error").is_retryable());
        assert!(ClientError::from_http(502, "error").is_retryable());
        assert!(ClientError::from_http(503, "error").is_retryable());

        // 429 is retryable
        assert!(ClientError::from_http(429, "rate limited").is_retryable());

        // 4xx errors (except 429) are not retryable
        assert!(!ClientError::from_http(400, "bad request").is_retryable());
        assert!(!ClientError::from_http(404, "not found").is_retryable());

        // Connection errors are retryable
        assert!(ClientError::ConnectionError("failed".to_string()).is_retryable());
    }
}
