//! In-memory storage implementation for testing and development.

use async_trait::async_trait;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::error::StoreError;
use crate::models::{BaselineRecord, BaselineSummary, BaselineVersion, ListBaselinesQuery, ListBaselinesResponse, PaginationInfo};
use super::{BaselineStore, StorageHealth};

/// In-memory storage backend for baselines.
#[derive(Debug, Default)]
pub struct InMemoryStore {
    baselines: Arc<RwLock<BTreeMap<(String, String, String), BaselineRecord>>>,
}

impl InMemoryStore {
    /// Creates a new empty in-memory store.
    pub fn new() -> Self {
        Self {
            baselines: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    fn key(project: &str, benchmark: &str, version: &str) -> (String, String, String) {
        (project.to_string(), benchmark.to_string(), version.to_string())
    }
}

#[async_trait]
impl BaselineStore for InMemoryStore {
    async fn create(&self, record: &BaselineRecord) -> Result<(), StoreError> {
        let key = Self::key(&record.project, &record.benchmark, &record.version);
        let mut baselines = self.baselines.write().await;

        if baselines.contains_key(&key) {
            return Err(StoreError::AlreadyExists(format!(
                "project={}, benchmark={}, version={}",
                record.project, record.benchmark, record.version
            )));
        }

        baselines.insert(key, record.clone());
        Ok(())
    }

    async fn get(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        let key = Self::key(project, benchmark, version);
        let baselines = self.baselines.read().await;
        Ok(baselines.get(&key).filter(|r| !r.deleted).cloned())
    }

    async fn get_latest(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        let baselines = self.baselines.read().await;
        let latest = baselines
            .values()
            .filter(|r| r.project == project && r.benchmark == benchmark && !r.deleted)
            .max_by_key(|r| r.created_at);
        Ok(latest.cloned())
    }

    async fn list(
        &self,
        project: &str,
        query: &ListBaselinesQuery,
    ) -> Result<ListBaselinesResponse, StoreError> {
        let baselines = self.baselines.read().await;

        let mut filtered: Vec<_> = baselines
            .values()
            .filter(|r| r.project == project && !r.deleted)
            .collect();

        filtered.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let total = filtered.len() as u64;
        let offset = query.offset as usize;
        let limit = query.limit as usize;

        let paginated: Vec<_> = filtered
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|r| {
                let mut summary: BaselineSummary = r.clone().into();
                if query.include_receipt {
                    summary.receipt = Some(r.receipt.clone());
                }
                summary
            })
            .collect();

        let has_more = (offset + paginated.len()) < total as usize;

        Ok(ListBaselinesResponse {
            baselines: paginated,
            pagination: PaginationInfo {
                total,
                limit: query.limit,
                offset: query.offset,
                has_more,
            },
        })
    }

    async fn update(&self, record: &BaselineRecord) -> Result<(), StoreError> {
        let key = Self::key(&record.project, &record.benchmark, &record.version);
        let mut baselines = self.baselines.write().await;

        if !baselines.contains_key(&key) {
            return Err(StoreError::NotFound(format!(
                "project={}, benchmark={}, version={}",
                record.project, record.benchmark, record.version
            )));
        }

        baselines.insert(key, record.clone());
        Ok(())
    }

    async fn delete(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<bool, StoreError> {
        let key = Self::key(project, benchmark, version);
        let mut baselines = self.baselines.write().await;

        if let Some(record) = baselines.get_mut(&key) {
            if record.deleted {
                return Ok(false);
            }
            record.deleted = true;
            return Ok(true);
        }

        Ok(false)
    }

    async fn hard_delete(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<bool, StoreError> {
        let key = Self::key(project, benchmark, version);
        let mut baselines = self.baselines.write().await;
        Ok(baselines.remove(&key).is_some())
    }

    async fn list_versions(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<Vec<BaselineVersion>, StoreError> {
        let baselines = self.baselines.read().await;

        let mut versions: Vec<_> = baselines
            .values()
            .filter(|r| r.project == project && r.benchmark == benchmark && !r.deleted)
            .map(|r| BaselineVersion {
                version: r.version.clone(),
                git_ref: r.git_ref.clone(),
                git_sha: r.git_sha.clone(),
                created_at: r.created_at,
                created_by: None,
                is_current: false,
                source: r.source.clone(),
            })
            .collect();

        versions.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        if let Some(first) = versions.first_mut() {
            first.is_current = true;
        }

        Ok(versions)
    }

    async fn health_check(&self) -> Result<StorageHealth, StoreError> {
        Ok(StorageHealth::Healthy)
    }

    fn backend_type(&self) -> &'static str {
        "memory"
    }
}
