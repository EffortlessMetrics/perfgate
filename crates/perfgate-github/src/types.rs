//! Types for GitHub API responses and requests.

use serde::{Deserialize, Serialize};

/// A GitHub issue/PR comment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubComment {
    /// The comment ID.
    pub id: u64,

    /// The comment body (Markdown).
    pub body: String,

    /// The HTML URL for the comment.
    pub html_url: String,

    /// The user who created the comment.
    pub user: GitHubUser,
}

/// A GitHub user (minimal fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubUser {
    /// The user's login name.
    pub login: String,
}

/// Request body for creating/updating a comment.
#[derive(Debug, Serialize)]
pub(crate) struct GitHubCommentRequest {
    pub body: String,
}
