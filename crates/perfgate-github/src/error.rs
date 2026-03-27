//! Error types for the GitHub API client.

/// Errors that can occur when interacting with the GitHub API.
#[derive(Debug, thiserror::Error)]
pub enum GitHubError {
    /// Configuration error (e.g., invalid token).
    #[error("GitHub configuration error: {0}")]
    Config(String),

    /// HTTP request error.
    #[error("GitHub request error: {0}")]
    Request(#[from] reqwest::Error),

    /// GitHub API returned an error status code.
    #[error("GitHub API error (HTTP {status}): {message}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Error message from the API.
        message: String,
    },
}
