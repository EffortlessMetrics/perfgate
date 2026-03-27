//! PostgreSQL storage implementation for persistent baseline storage.
//!
//! This module provides a robust, asynchronous PostgreSQL backend for storing
//! and querying perfgate baseline records using sqlx.

use super::{ArtifactStore, BaselineStore, StorageHealth};
use crate::error::StoreError;
use crate::models::{
    BaselineRecord, BaselineSource, BaselineVersion, ListBaselinesQuery, ListBaselinesResponse,
    ListVerdictsQuery, ListVerdictsResponse, PaginationInfo, VerdictRecord,
};
use crate::server::PostgresPoolConfig;
use async_trait::async_trait;
use perfgate_types::VerdictStatus;
use sqlx::{Connection, Executor, PgPool, Row, postgres::PgPoolOptions};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

/// PostgreSQL storage backend for baselines.
#[derive(Debug, Clone)]
pub struct PostgresStore {
    pool: PgPool,
    artifacts: Option<Arc<dyn ArtifactStore>>,
}

/// Maximum number of retry attempts for transient connection failures.
const MAX_RETRIES: u32 = 3;

/// Initial backoff delay for connection retries.
const INITIAL_BACKOFF: Duration = Duration::from_millis(250);

/// Returns `true` when an sqlx error looks transient (connection refused,
/// timeout, reset, etc.) -- i.e. worth retrying.
fn is_transient(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::PoolTimedOut => true,
        sqlx::Error::PoolClosed => false,
        sqlx::Error::Io(_) => true,
        sqlx::Error::Database(db_err) => {
            // Postgres error codes starting with 08 are connection exceptions.
            db_err
                .code()
                .map(|c| c.starts_with("08") || c.starts_with("57P01"))
                .unwrap_or(false)
        }
        _ => {
            let msg = err.to_string().to_lowercase();
            msg.contains("connection refused")
                || msg.contains("connection reset")
                || msg.contains("broken pipe")
                || msg.contains("timed out")
        }
    }
}

impl PostgresStore {
    /// Creates a new PostgreSQL storage backend and runs initial schema migrations.
    ///
    /// The connection pool is configured according to the supplied
    /// [`PostgresPoolConfig`]. An `after_connect` callback sets
    /// `statement_timeout` on every new connection, and a `before_acquire`
    /// callback pings the connection to verify it is still alive.
    pub async fn new(
        url: &str,
        artifacts: Option<Arc<dyn ArtifactStore>>,
        pool_config: &PostgresPoolConfig,
    ) -> Result<Self, StoreError> {
        let stmt_timeout_ms = pool_config.statement_timeout.as_millis() as u64;

        let pool = PgPoolOptions::new()
            .max_connections(pool_config.max_connections)
            .min_connections(pool_config.min_connections)
            .idle_timeout(pool_config.idle_timeout)
            .max_lifetime(pool_config.max_lifetime)
            .acquire_timeout(pool_config.acquire_timeout)
            .after_connect(move |conn, _meta| {
                Box::pin(async move {
                    conn.execute(format!("SET statement_timeout = '{}'", stmt_timeout_ms).as_str())
                        .await?;
                    Ok(())
                })
            })
            .before_acquire(|conn, _meta| {
                Box::pin(async move {
                    // Quick ping to verify the connection is alive.
                    conn.ping().await?;
                    Ok(true)
                })
            })
            .connect(url)
            .await
            .map_err(|e| StoreError::ConnectionError(e.to_string()))?;

        info!(
            max = pool_config.max_connections,
            min = pool_config.min_connections,
            idle_timeout_s = pool_config.idle_timeout.as_secs(),
            max_lifetime_s = pool_config.max_lifetime.as_secs(),
            acquire_timeout_s = pool_config.acquire_timeout.as_secs(),
            statement_timeout_ms = stmt_timeout_ms,
            "PostgreSQL connection pool configured"
        );

        let store = Self { pool, artifacts };
        store.init_schema().await?;
        Ok(store)
    }

    /// Returns current pool metrics (idle, active, max connections).
    fn pg_pool_metrics(&self) -> crate::models::PoolMetrics {
        let size = self.pool.size();
        let num_idle = self.pool.num_idle() as u32;
        crate::models::PoolMetrics {
            idle: num_idle,
            active: size.saturating_sub(num_idle),
            max: self.pool.options().get_max_connections(),
        }
    }

    /// Executes a query with automatic retry on transient errors.
    ///
    /// The closure receives a reference to the pool and should return the
    /// query result. On transient failure the call is retried up to
    /// [`MAX_RETRIES`] times with exponential backoff.
    pub(crate) async fn with_retry<F, Fut, T>(&self, operation: F) -> Result<T, StoreError>
    where
        F: Fn(PgPool) -> Fut,
        Fut: std::future::Future<Output = Result<T, sqlx::Error>>,
    {
        let mut last_err = None;
        let mut backoff = INITIAL_BACKOFF;

        for attempt in 0..=MAX_RETRIES {
            match operation(self.pool.clone()).await {
                Ok(val) => return Ok(val),
                Err(e) if is_transient(&e) && attempt < MAX_RETRIES => {
                    warn!(
                        attempt = attempt + 1,
                        max = MAX_RETRIES,
                        backoff_ms = backoff.as_millis() as u64,
                        error = %e,
                        "Transient database error, retrying"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff *= 2;
                    last_err = Some(e);
                }
                Err(e) => {
                    return Err(StoreError::QueryError(e.to_string()));
                }
            }
        }

        Err(StoreError::QueryError(
            last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown error after retries".to_string()),
        ))
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

            CREATE TABLE IF NOT EXISTS verdicts (
                id VARCHAR(26) PRIMARY KEY,
                schema_id VARCHAR(64) NOT NULL,
                project VARCHAR(255) NOT NULL,
                benchmark VARCHAR(255) NOT NULL,
                run_id VARCHAR(255) NOT NULL,
                status VARCHAR(32) NOT NULL,
                counts JSONB NOT NULL,
                reasons JSONB NOT NULL,
                git_ref VARCHAR(255),
                git_sha VARCHAR(40),
                created_at TIMESTAMPTZ NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_verdicts_project_benchmark
            ON verdicts(project, benchmark);

            CREATE INDEX IF NOT EXISTS idx_verdicts_created_at
            ON verdicts(created_at);
        "#;

        sqlx::query(sql)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::ConnectionError(format!("Failed to init schema: {}", e)))?;

        Ok(())
    }

    async fn store_artifact(&self, record: &BaselineRecord) -> Result<Option<String>, StoreError> {
        if let Some(store) = &self.artifacts {
            let path = format!(
                "{}/{}/{}.json",
                record.project, record.benchmark, record.version
            );
            let data =
                serde_json::to_vec(&record.receipt).map_err(StoreError::SerializationError)?;
            store.put(&path, data).await?;
            Ok(Some(path))
        } else {
            Ok(None)
        }
    }

    fn row_to_record(
        row: sqlx::postgres::PgRow,
    ) -> Result<(BaselineRecord, Option<String>), StoreError> {
        let artifact_path: Option<String> = row.get("artifact_path");

        let receipt = if let Some(receipt_json) = row.get::<Option<serde_json::Value>, _>("receipt")
        {
            serde_json::from_value(receipt_json).map_err(StoreError::SerializationError)?
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
            }))
            .unwrap()
        };

        let metadata_json: serde_json::Value = row.get("metadata");
        let metadata =
            serde_json::from_value(metadata_json).map_err(StoreError::SerializationError)?;

        let tags_json: serde_json::Value = row.get("tags");
        let tags = serde_json::from_value(tags_json).map_err(StoreError::SerializationError)?;

        let source_str: String = row.get("source");
        let source = serde_json::from_value(serde_json::Value::String(source_str))
            .unwrap_or(BaselineSource::Upload);

        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let updated_at: chrono::DateTime<chrono::Utc> = row.get("updated_at");

        Ok((
            BaselineRecord {
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
            },
            artifact_path,
        ))
    }

    async fn load_artifact(
        &self,
        path: Option<String>,
        mut record: BaselineRecord,
    ) -> Result<BaselineRecord, StoreError> {
        if let (Some(store), Some(path)) = (&self.artifacts, path) {
            let data = store.get(&path).await?;
            record.receipt =
                serde_json::from_slice(&data).map_err(StoreError::SerializationError)?;
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

        let metadata_json =
            serde_json::to_value(&record.metadata).map_err(StoreError::SerializationError)?;
        let tags_json =
            serde_json::to_value(&record.tags).map_err(StoreError::SerializationError)?;
        let source_json =
            serde_json::to_value(&record.source).map_err(StoreError::SerializationError)?;
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
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => Err(
                StoreError::already_exists(&record.project, &record.benchmark, &record.version),
            ),
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
            }
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
            }
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        project: &str,
        query: &ListBaselinesQuery,
    ) -> Result<ListBaselinesResponse, StoreError> {
        let mut sql =
            String::from("SELECT * FROM baselines WHERE project = $1 AND deleted = FALSE");

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
        let receipt_json =
            serde_json::to_value(&record.receipt).map_err(StoreError::SerializationError)?;
        let metadata_json =
            serde_json::to_value(&record.metadata).map_err(StoreError::SerializationError)?;
        let tags_json =
            serde_json::to_value(&record.tags).map_err(StoreError::SerializationError)?;

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
            return Err(StoreError::not_found(
                &record.project,
                &record.benchmark,
                &record.version,
            ));
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
        match self
            .with_retry(|pool| async move { sqlx::query("SELECT 1").execute(&pool).await })
            .await
        {
            Ok(_) => Ok(StorageHealth::Healthy),
            Err(_) => Ok(StorageHealth::Unhealthy),
        }
    }

    fn backend_type(&self) -> &'static str {
        "postgres"
    }

    fn pool_metrics(&self) -> Option<crate::models::PoolMetrics> {
        Some(self.pg_pool_metrics())
    }

    async fn create_verdict(&self, record: &VerdictRecord) -> Result<(), StoreError> {
        let sql = r#"
            INSERT INTO verdicts (
                id, schema_id, project, benchmark, run_id, status, counts, reasons,
                git_ref, git_sha, created_at
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        "#;

        let counts_json =
            serde_json::to_value(&record.counts).map_err(StoreError::SerializationError)?;
        let reasons_json =
            serde_json::to_value(&record.reasons).map_err(StoreError::SerializationError)?;
        let status_str = record.status.as_str();

        sqlx::query(sql)
            .bind(&record.id)
            .bind(&record.schema)
            .bind(&record.project)
            .bind(&record.benchmark)
            .bind(&record.run_id)
            .bind(status_str)
            .bind(counts_json)
            .bind(reasons_json)
            .bind(&record.git_ref)
            .bind(&record.git_sha)
            .bind(record.created_at)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::QueryError(e.to_string()))?;

        Ok(())
    }

    async fn list_verdicts(
        &self,
        project: &str,
        query: &ListVerdictsQuery,
    ) -> Result<ListVerdictsResponse, StoreError> {
        let mut sql = "SELECT * FROM verdicts WHERE project = $1".to_string();
        let mut params_count = 1;

        if let Some(_bench) = &query.benchmark {
            params_count += 1;
            sql.push_str(&format!(" AND benchmark = ${}", params_count));
        }

        if let Some(_status) = &query.status {
            params_count += 1;
            sql.push_str(&format!(" AND status = ${}", params_count));
        }

        if let Some(_since) = &query.since {
            params_count += 1;
            sql.push_str(&format!(" AND created_at >= ${}", params_count));
        }

        if let Some(_until) = &query.until {
            params_count += 1;
            sql.push_str(&format!(" AND created_at <= ${}", params_count));
        }

        sql.push_str(" ORDER BY created_at DESC");

        // Limit and offset
        params_count += 1;
        sql.push_str(&format!(" LIMIT ${}", params_count));
        params_count += 1;
        sql.push_str(&format!(" OFFSET ${}", params_count));

        let mut q = sqlx::query(&sql).bind(project);

        if let Some(bench) = &query.benchmark {
            q = q.bind(bench);
        }
        if let Some(status) = &query.status {
            q = q.bind(status.as_str());
        }
        if let Some(since) = &query.since {
            q = q.bind(since);
        }
        if let Some(until) = &query.until {
            q = q.bind(until);
        }

        q = q.bind(query.limit as i64);
        q = q.bind(query.offset as i64);

        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::QueryError(e.to_string()))?;

        let mut verdicts = Vec::with_capacity(rows.len());
        for row in rows {
            verdicts.push(self.row_to_verdict(row)?);
        }

        // For total count
        let count_sql = "SELECT COUNT(*) FROM verdicts WHERE project = $1";
        let total: i64 = sqlx::query_scalar(count_sql)
            .bind(project)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| StoreError::QueryError(e.to_string()))?;

        Ok(ListVerdictsResponse {
            verdicts,
            pagination: PaginationInfo {
                total: total as u64,
                offset: query.offset,
                limit: query.limit,
                has_more: (query.offset + query.limit as u64) < total as u64,
            },
        })
    }
}

impl PostgresStore {
    fn row_to_verdict(&self, row: sqlx::postgres::PgRow) -> Result<VerdictRecord, StoreError> {
        let status_str: String = row.get("status");
        let status = match status_str.as_str() {
            "pass" => VerdictStatus::Pass,
            "warn" => VerdictStatus::Warn,
            "fail" => VerdictStatus::Fail,
            "skip" => VerdictStatus::Skip,
            _ => VerdictStatus::Pass, // Default fallback
        };

        let counts_json: serde_json::Value = row.get("counts");
        let counts = serde_json::from_value(counts_json).map_err(StoreError::SerializationError)?;

        let reasons_json: serde_json::Value = row.get("reasons");
        let reasons =
            serde_json::from_value(reasons_json).map_err(StoreError::SerializationError)?;

        Ok(VerdictRecord {
            schema: row.get("schema_id"), // Wait, I didn't add schema_id to verdicts table
            id: row.get("id"),
            project: row.get("project"),
            benchmark: row.get("benchmark"),
            run_id: row.get("run_id"),
            status,
            counts,
            reasons,
            git_ref: row.get("git_ref"),
            git_sha: row.get("git_sha"),
            created_at: row.get("created_at"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_transient_pool_timed_out() {
        assert!(is_transient(&sqlx::Error::PoolTimedOut));
    }

    #[test]
    fn test_is_transient_pool_closed() {
        assert!(!is_transient(&sqlx::Error::PoolClosed));
    }

    #[test]
    fn test_is_transient_io_error() {
        let err = sqlx::Error::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "connection refused",
        ));
        assert!(is_transient(&err));
    }

    #[test]
    fn test_is_transient_non_transient() {
        let err = sqlx::Error::ColumnNotFound("missing".to_string());
        assert!(!is_transient(&err));
    }
}
