//! Storage trait and implementations for baseline persistence.
//!
//! This module provides the [`BaselineStore`] trait for abstracting storage
//! operations and implementations for different backends.

mod memory;
mod sqlite;

pub use memory::InMemoryStore;
pub use sqlite::SqliteStore;

use async_trait::async_trait;
use crate::models::{BaselineRecord, BaselineVersion, ListBaselinesQuery, ListBaselinesResponse};
use crate::error::StoreError;

/// Trait for baseline storage operations.
///
/// This trait abstracts the storage layer, allowing different backends
/// (in-memory, SQLite, PostgreSQL) to be used interchangeably.
#[async_trait]
pub trait BaselineStore: Send + Sync {
    /// Stores a new baseline record.
    async fn create(&self, record: &BaselineRecord) -> Result<(), StoreError>;

    /// Retrieves a baseline by project, benchmark, and version.
    async fn get(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<Option<BaselineRecord>, StoreError>;

    /// Retrieves the latest baseline for a project and benchmark.
    async fn get_latest(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<Option<BaselineRecord>, StoreError>;

    /// Lists baselines with optional filtering.
    async fn list(
        &self,
        project: &str,
        query: &ListBaselinesQuery,
    ) -> Result<ListBaselinesResponse, StoreError>;

    /// Updates an existing baseline record.
    async fn update(&self, record: &BaselineRecord) -> Result<(), StoreError>;

    /// Deletes a baseline (soft delete).
    async fn delete(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<bool, StoreError>;

    /// Permanently removes a deleted baseline.
    async fn hard_delete(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<bool, StoreError>;

    /// Lists all versions for a benchmark.
    async fn list_versions(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<Vec<BaselineVersion>, StoreError>;

    /// Checks if the storage backend is healthy.
    async fn health_check(&self) -> Result<StorageHealth, StoreError>;

    /// Returns the backend type name.
    fn backend_type(&self) -> &'static str;
}

/// Storage backend health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageHealth {
    /// Storage is healthy and operational
    Healthy,
    /// Storage is degraded but functional
    Degraded,
    /// Storage is unavailable
    Unhealthy,
}

impl StorageHealth {
    /// Returns the string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unhealthy => "unhealthy",
        }
    }
}

impl std::fmt::Display for StorageHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
