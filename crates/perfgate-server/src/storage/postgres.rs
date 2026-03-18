//! PostgreSQL storage implementation for persistent baseline storage.
//!
//! This module provides a robust, asynchronous PostgreSQL backend for storing
//! and querying perfgate baseline records using sqlx.

use super::{ArtifactStore, BaselineStore, StorageHealth};
use crate::error::StoreError;
use crate::models::{
    BaselineRecord, BaselineSource, BaselineVersion, ListBaselinesQuery,
    ListBaselinesResponse, PaginationInfo,
};
use async_trait::async_trait;
use sqlx::{PgPool, postgres::PgPoolOptions, Row};
use std::sync::Arc;
use std::time::Duration;

/// PostgreSQL storage backend for baselines.
#[derive(Debug, Clone)]
pub struct PostgresStore {
    pool: PgPool,
    artifacts: Option<Arc<dyn ArtifactStore>>,
}

impl PostgresStore {
    /// Creates a new PostgreSQL storage backend and runs initial schema migrations.
    pub async fn new(url: &str, artifacts: Option<Arc<dyn ArtifactStore>>) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await
            .map_err(|e| StoreError::ConnectionError(e.to_string()))?;

        let store = Self { pool, artifacts };
        store.init_schema().await?;
        Ok(store)
    }

    async fn init_schema(&self) -> Result<(), StoreError> {
        let sql = r#"
            CREATE TABLE IF NOT EXISTS baselines (
                id VARCHAR(26) PRIMARY KEY,
                project VARCHAR(255) NOT NULL,
                benchmark VARCHAR(255) NOT NULL,
                version VARCHAR(64) NOT NULL,
                schema_id VARCHAR(64) NOT NULL,
                git_ref VARCHAR(255),
                git_sha VARCHAR(40),
                receipt JSONB,
                artifact_path TEXT,
                metadata JSONB NOT NULL,
                tags JSONB NOT NULL,
                created_at TIMESTAMPTZ NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL,
                content_hash VARCHAR(64) NOT NULL,
                source VARCHAR(32) NOT NULL,
                deleted BOOLEAN NOT NULL DEFAULT FALSE,
                UNIQUE (project, benchmark, version)
            );
            
            CREATE INDEX IF NOT EXISTS idx_baselines_project_benchmark 
            ON baselines(project, benchmark);
        "#;

        sqlx::query(sql)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::ConnectionError(format!("Failed to init schema: {}", e)))?;

        Ok(())
    }

    async fn store_artifact(&self, record: &BaselineRecord) -> Result<Option<String>, StoreError> {
        if let Some(store) = &self.artifacts {
            let path = format!("{}/{}/{}.json", record.project, record.benchmark, record.version);
            let data = serde_json::to_vec(&record.receipt)
                .map_err(StoreError::SerializationError)?;
            store.put(&path, data).await?;
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    fn row_to_record(row: sqlx::postgres::PgRow) -> Result<(BaselineRecord, Option<String>), StoreError> {
        let artifact_path: Option<String> = row.get("artifact_path");
        
        let receipt = if let Some(receipt_json) = row.get:: <Option<serde_json::Value>, _>("receipt") {
            serde_json::from_value(receipt_json)
                .map_err(StoreError::SerializationError)?
        } else {
            // Placeholder, will be loaded from artifact store if needed
            serde_json::from_value(serde_json::json!({
                "schema": "perfgate.run.v1",
                "tool": {"name": "placeholder", "version": "0"},
                "run": {
                    "id": "placeholder",
                    "started_at": "1970-01-01T00:00:00Z",
                    "ended_at": "1970-01-01T00:00:00Z",
                    "host": {"os": "unknown", "arch": "unknown"}
                },
                "bench": {
                    "name": "placeholder",
                    "command": [],
                    "repeat": 0,
                    "warmup": 0
                },
                "samples": [],
                "stats": {
                    "wall_ms": {"median": 0, "min": 0, "max": 0}
                }
            })).unwrap()
        };
        
        let metadata_json: serde_json::Value = row.get("metadata");
        let metadata = serde_json::from_value(metadata_json)
            .map_err(StoreError::SerializationError)?;

        let tags_json: serde_json::Value = row.get("tags");
        let tags = serde_json::from_value(tags_json)
            .map_err(StoreError::SerializationError)?;

        let source_str: String = row.get("source");
        let source = serde_json::from_value(serde_json::Value::String(source_str))
            .unwrap_or(BaselineSource::Upload);

        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

        Ok((BaselineRecord {
            schema: row.get("schema_id"),
            id: row.get("id"),
            project: row.get("project"),
            benchmark: row.get("benchmark"),
            version: row.get("version"),
            git_ref: row.get("git_ref"),
            git_sha: row.get("git_sha"),
            receipt,
            metadata,
            tags,
            created_at,
            updated_at,
            content_hash: row.get("content_hash"),
            source,
            deleted: row.get("deleted"),
        }, artifact_path))
    }

    async fn load_artifact(&self, path: Option<String>, mut record: BaselineRecord) -> Result<BaselineRecord, StoreError> {
        if let (Some(store), Some(path)) = (&self.artifacts, path) {
            let data = store.get(&path).await?;
            record.receipt = serde_json::from_slice(&data)
                .map_err(StoreError::SerializationError)?;
        }
        Ok(record)
    }
}

#[async_trait]
impl BaselineStore for PostgresStore {
    async fn create(&self, record: &BaselineRecord) -> Result<(), StoreError> {
        let artifact_path = self.store_artifact(record).await?;
        
        let receipt_json = if artifact_path.is_none() {
            Some(serde_json::to_value(&record.receipt).map_err(StoreError::SerializationError)?)
        } else {
            None
        };

        let metadata_json = serde_json::to_value(&record.metadata)
            .map_err(StoreError::SerializationError)?;
        let tags_json = serde_json::to_value(&record.tags)
            .map_err(StoreError::SerializationError)?;
        let source_json = serde_json::to_value(&record.source)
            .map_err(StoreError::SerializationError)?;
        let source_str = source_json.as_str().unwrap_or("upload");

        let sql = r#"
            INSERT INTO baselines (
                id, project, benchmark, version, schema_id, 
                git_ref, git_sha, receipt, artifact_path, metadata, tags,
                created_at, updated_at, content_hash, source, deleted
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        "#;

        let result = sqlx::query(sql)
            .bind(&record.id)
            .bind(&record.project)
            .bind(&record.benchmark)
            .bind(&record.version)
            .bind(&record.schema)
            .bind(&record.git_ref)
            .bind(&record.git_sha)
            .bind(receipt_json)
            .bind(artifact_path)
            .bind(metadata_json)
            .bind(tags_json)
            .bind(record.created_at)
            .bind(record.updated_at)
            .bind(&record.content_hash)
            .bind(source_str)
            .bind(record.deleted)
            .execute(&self.pool)
            .await;

        match result {
            Ok(_) => Ok(()),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                Err(StoreError::already_exists(&record.project, &record.benchmark, &record.version))
            }
            Err(e) => Err(StoreError::QueryError(e.to_string())),
        }
    }

    async fn get(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        let sql = "SELECT * FROM baselines WHERE project = $1 AND benchmark = $2 AND version = $3 AND deleted = FALSE";
        
        let row_opt = sqlx::query(sql)
            .bind(project)
            .bind(benchmark)
            .bind(version)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::QueryError(e.to_string()))?;

        match row_opt {
            Some(row) => {
                let (record, artifact_path) = Self::row_to_record(row)?;
                Ok(Some(self.load_artifact(artifact_path, record).await?))
            },
            None => Ok(None),
        }
    }

    async fn get_latest(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        let sql = "SELECT * FROM baselines WHERE project = $1 AND benchmark = $2 AND deleted = FALSE ORDER BY created_at DESC LIMIT 1";
        
        let row_opt = sqlx::query(sql)
            .bind(project)
            .bind(benchmark)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::QueryError(e.to_string()))?;

        match row_opt {
            Some(row) => {
                let (record, artifact_path) = Self::row_to_record(row)?;
                Ok(Some(self.load_artifact(artifact_path, record).await?))
            },
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        project: &str,
        query: &ListBaselinesQuery,
    ) -> Result<ListBaselinesResponse, StoreError> {
        let mut sql = String::from("SELECT * FROM baselines WHERE project = $1 AND deleted = FALSE");
        
        if let Some(bench) = &query.benchmark {
            sql.push_str(" AND benchmark = '");
            sql.push_str(&bench.replace('\'', "''"));
            sql.push('\'');
        }

        sql.push_str(" ORDER BY created_at DESC");
        
        let limit = query.limit.min(100) as i64;
        sql.push_str(&format!(" LIMIT {}", limit + 1));
        
        let offset = query.offset as i64;
        sql.push_str(&format!(" OFFSET {}", offset));

        let rows = sqlx::query(&sql)
            .bind(project)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::QueryError(e.to_string()))?;

        let has_more = rows.len() > limit as usize;
        let take_count = if has_more { limit as usize } else { rows.len() };
        
        let mut baselines = Vec::with_capacity(take_count);
        for row in rows.into_iter().take(take_count) {
            let (mut record, artifact_path) = Self::row_to_record(row)?;
            if query.include_receipt {
                record = self.load_artifact(artifact_path, record).await?;
            }
            baselines.push(record.into());
        }

        // Determine total count
        let count_sql = "SELECT COUNT(*) FROM baselines WHERE project = $1 AND deleted = FALSE";
        let mut count_query = String::from(count_sql);
        if let Some(bench) = &query.benchmark {
            count_query.push_str(" AND benchmark = '");
            count_query.push_str(&bench.replace('\'', "''"));
            count_query.push('\'');
        }
        let total_row = sqlx::query(&count_query)
            .bind(project)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StoreError::QueryError(e.to_string()))?;
            
        let total: i64 = total_row.get(0);

        let pagination = PaginationInfo {
            limit: limit as u32,
            offset: query.offset,
            total: total as u64,
            has_more,
        };

        Ok(ListBaselinesResponse {
            baselines,
            pagination,
        })
    }

    async fn update(&self, record: &BaselineRecord) -> Result<(), StoreError> {
        let receipt_json = serde_json::to_value(&record.receipt)
            .map_err(StoreError::SerializationError)?;
        let metadata_json = serde_json::to_value(&record.metadata)
            .map_err(StoreError::SerializationError)?;
        let tags_json = serde_json::to_value(&record.tags)
            .map_err(StoreError::SerializationError)?;

        let sql = r#"
            UPDATE baselines 
            SET schema_id = $1, git_ref = $2, git_sha = $3, receipt = $4, 
                metadata = $5, tags = $6, updated_at = $7, content_hash = $8
            WHERE project = $9 AND benchmark = $10 AND version = $11 AND deleted = FALSE
        "#;

        let result = sqlx::query(sql)
            .bind(&record.schema)
            .bind(&record.git_ref)
            .bind(&record.git_sha)
            .bind(receipt_json)
            .bind(metadata_json)
            .bind(tags_json)
            .bind(record.updated_at)
            .bind(&record.content_hash)
            .bind(&record.project)
            .bind(&record.benchmark)
            .bind(&record.version)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::QueryError(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(StoreError::not_found(&record.project, &record.benchmark, &record.version));
        }

        Ok(())
    }

    async fn delete(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<bool, StoreError> {
        let sql = "UPDATE baselines SET deleted = TRUE, updated_at = NOW() WHERE project = $1 AND benchmark = $2 AND version = $3 AND deleted = FALSE";
        
        let result = sqlx::query(sql)
            .bind(project)
            .bind(benchmark)
            .bind(version)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::QueryError(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn hard_delete(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<bool, StoreError> {
        let sql = "DELETE FROM baselines WHERE project = $1 AND benchmark = $2 AND version = $3";
        
        let result = sqlx::query(sql)
            .bind(project)
            .bind(benchmark)
            .bind(version)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::QueryError(e.to_string()))?;

        Ok(result.rows_affected() > 0)
    }

    async fn list_versions(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<Vec<BaselineVersion>, StoreError> {
        let sql = "SELECT version, created_at, git_ref, git_sha, source FROM baselines WHERE project = $1 AND benchmark = $2 AND deleted = FALSE ORDER BY created_at DESC";
        
        let rows = sqlx::query(sql)
            .bind(project)
            .bind(benchmark)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::QueryError(e.to_string()))?;

        let mut versions = Vec::with_capacity(rows.len());
        for row in rows {
            let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
            let source_str: String = row.get("source");
            let source = serde_json::from_value(serde_json::Value::String(source_str))
                .unwrap_or(BaselineSource::Upload);
                
            versions.push(BaselineVersion {
                version: row.get("version"),
                created_at,
                git_ref: row.get("git_ref"),
                git_sha: row.get("git_sha"),
                created_by: None,
                is_current: false, // Could be determined by checking if it's the latest
                source,
            });
        }
        
        if let Some(first) = versions.first_mut() {
            first.is_current = true;
        }

        Ok(versions)
    }

    async fn health_check(&self) -> Result<StorageHealth, StoreError> {
        match sqlx::query("SELECT 1").execute(&self.pool).await {
            Ok(_) => Ok(StorageHealth::Healthy),
            Err(_) => Ok(StorageHealth::Unhealthy),
        }
    }

    fn backend_type(&self) -> &'static str {
        "postgres"
    }
}
