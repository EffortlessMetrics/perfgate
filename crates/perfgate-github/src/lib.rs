//! # perfgate-github
//!
//! GitHub API client and comment rendering for perfgate PR comments.
//!
//! This crate provides:
//!
//! - A GitHub REST API client for creating, updating, and finding PR comments
//! - Rich Markdown comment rendering with verdict badges, metric tables, trend indicators,
//!   and blame attribution
//! - Idempotent comment updates via a hidden marker comment
//! - Support for both GitHub Actions (GITHUB_TOKEN) and personal access tokens

pub mod client;
pub mod comment;
pub mod error;
pub mod types;

pub use client::{COMMENT_MARKER, GitHubClient};
pub use comment::{
    CommentOptions, parse_github_repository, parse_pr_number_from_ref, prepare_comment_body,
    render_comment, render_comment_from_report,
};
pub use error::GitHubError;
pub use types::GitHubComment;
