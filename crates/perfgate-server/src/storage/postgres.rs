//! PostgreSQL storage implementation for persistent baseline storage.
//!
//! This module is a placeholder for future PostgreSQL support.
//! It currently returns errors for all operations.

use super::{BaselineStore, StorageHealth};
use crate::error::StoreError;
use crate::models::{BaselineRecord, BaselineVersion, ListBaselinesQuery, ListBaselinesResponse};
use async_trait::async_trait;

/// PostgreSQL storage backend for baselines.
#[derive(Debug, Default)]
pub struct PostgresStore;

impl PostgresStore {
    /// Creates a new PostgreSQL storage backend.
    pub fn new(_url: &str) -> Self {
        Self
    }
}

#[async_trait]
impl BaselineStore for PostgresStore {
    async fn create(&self, _record: &BaselineRecord) -> Result<(), StoreError> {
        Err(StoreError::ConnectionError(
            "PostgreSQL storage is not yet implemented".to_string(),
        ))
    }

    async fn get(
        &self,
        _project: &str,
        _benchmark: &str,
        _version: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        Err(StoreError::ConnectionError(
            "PostgreSQL storage is not yet implemented".to_string(),
        ))
    }

    async fn get_latest(
        &self,
        _project: &str,
        _benchmark: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        Err(StoreError::ConnectionError(
            "PostgreSQL storage is not yet implemented".to_string(),
        ))
    }

    async fn list(
        &self,
        _project: &str,
        _query: &ListBaselinesQuery,
    ) -> Result<ListBaselinesResponse, StoreError> {
        Err(StoreError::ConnectionError(
            "PostgreSQL storage is not yet implemented".to_string(),
        ))
    }

    async fn update(&self, _record: &BaselineRecord) -> Result<(), StoreError> {
        Err(StoreError::ConnectionError(
            "PostgreSQL storage is not yet implemented".to_string(),
        ))
    }

    async fn delete(
        &self,
        _project: &str,
        _benchmark: &str,
        _version: &str,
    ) -> Result<bool, StoreError> {
        Err(StoreError::ConnectionError(
            "PostgreSQL storage is not yet implemented".to_string(),
        ))
    }

    async fn hard_delete(
        &self,
        _project: &str,
        _benchmark: &str,
        _version: &str,
    ) -> Result<bool, StoreError> {
        Err(StoreError::ConnectionError(
            "PostgreSQL storage is not yet implemented".to_string(),
        ))
    }

    async fn list_versions(
        &self,
        _project: &str,
        _benchmark: &str,
    ) -> Result<Vec<BaselineVersion>, StoreError> {
        Err(StoreError::ConnectionError(
            "PostgreSQL storage is not yet implemented".to_string(),
        ))
    }

    async fn health_check(&self) -> Result<StorageHealth, StoreError> {
        Ok(StorageHealth::Unhealthy)
    }

    fn backend_type(&self) -> &'static str {
        "postgres"
    }
}
