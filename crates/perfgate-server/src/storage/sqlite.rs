//! SQLite storage implementation for persistent baseline storage.

use async_trait::async_trait;
use rusqlite::params;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::{BaselineStore, StorageHealth};
use crate::error::StoreError;
use crate::models::{
    BaselineRecord, BaselineSource, BaselineSummary, BaselineVersion, ListBaselinesQuery,
    ListBaselinesResponse, PaginationInfo,
};

/// SQLite storage backend for baselines.
///
/// This implementation persists all data to a SQLite database file,
/// making it suitable for production deployments that need durability
/// without the complexity of a full database server.
#[derive(Debug)]
pub struct SqliteStore {
    /// Path to the database file (for debugging/display purposes)
    _path: std::path::PathBuf,

    /// Connection pool (simplified: single connection wrapped in Mutex)
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl SqliteStore {
    /// Opens or creates a SQLite database at the specified path.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();

        // Create parent directories if needed
        if let Some(parent) = path.parent().filter(|p| !p.exists()) {
            std::fs::create_dir_all(parent)?;
        }

        let conn = rusqlite::Connection::open(&path)?;

        let store = Self {
            _path: path,
            conn: Arc::new(Mutex::new(conn)),
        };

        store.initialize()?;
        Ok(store)
    }

    /// Creates an in-memory SQLite database (for testing).
    pub fn in_memory() -> Result<Self, StoreError> {
        let conn = rusqlite::Connection::open_in_memory()?;

        let store = Self {
            _path: std::path::PathBuf::from(":memory:"),
            conn: Arc::new(Mutex::new(conn)),
        };

        store.initialize()?;
        Ok(store)
    }

    /// Initializes the database schema.
    fn initialize(&self) -> Result<(), StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        conn.execute_batch(
            r#"
            -- Baselines table
            CREATE TABLE IF NOT EXISTS baselines (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                benchmark TEXT NOT NULL,
                version TEXT NOT NULL,
                git_ref TEXT,
                git_sha TEXT,
                receipt TEXT NOT NULL,
                metadata TEXT NOT NULL DEFAULT '{}',
                tags TEXT NOT NULL DEFAULT '[]',
                source TEXT NOT NULL DEFAULT 'upload',
                content_hash TEXT NOT NULL,
                deleted INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                
                UNIQUE(project, benchmark, version)
            );

            -- Indexes for common queries
            CREATE INDEX IF NOT EXISTS idx_baselines_project_benchmark 
                ON baselines(project, benchmark);
            CREATE INDEX IF NOT EXISTS idx_baselines_project_git_ref 
                ON baselines(project, git_ref);
            CREATE INDEX IF NOT EXISTS idx_baselines_created_at 
                ON baselines(created_at DESC);
            "#,
        )?;

        Ok(())
    }

    /// Parses tags from JSON array string.
    fn parse_tags(tags_json: &str) -> Vec<String> {
        serde_json::from_str(tags_json).unwrap_or_default()
    }

    /// Serializes tags to JSON array string.
    fn serialize_tags(tags: &[String]) -> String {
        serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
    }

    /// Parses metadata from JSON object string.
    fn parse_metadata(metadata_json: &str) -> std::collections::BTreeMap<String, String> {
        serde_json::from_str(metadata_json).unwrap_or_default()
    }

    /// Serializes metadata to JSON object string.
    fn serialize_metadata(metadata: &std::collections::BTreeMap<String, String>) -> String {
        serde_json::to_string(metadata).unwrap_or_else(|_| "{}".to_string())
    }

    /// Maps a database row to a BaselineRecord.
    fn row_to_record(row: &rusqlite::Row) -> Result<BaselineRecord, rusqlite::Error> {
        // Column indices based on schema:
        // 0: id, 1: project, 2: benchmark, 3: version, 4: git_ref, 5: git_sha
        // 6: receipt, 7: metadata, 8: tags, 9: source, 10: content_hash
        // 11: deleted, 12: created_at, 13: updated_at
        let schema = crate::models::BASELINE_SCHEMA_V1.to_string();
        let source_str: String = row.get(9)?;
        let source = match source_str.as_str() {
            "upload" => BaselineSource::Upload,
            "promote" => BaselineSource::Promote,
            "migrate" => BaselineSource::Migrate,
            "rollback" => BaselineSource::Rollback,
            _ => BaselineSource::Upload,
        };

        let receipt_json: String = row.get(6)?;
        let receipt: perfgate_types::RunReceipt =
            serde_json::from_str(&receipt_json).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    6,
                    rusqlite::types::Type::Text,
                    Box::new(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Failed to parse receipt JSON: {}", e),
                    )),
                )
            })?;

        let created_at_str: String = row.get(12)?;
        let updated_at_str: String = row.get(13)?;

        Ok(BaselineRecord {
            schema,
            id: row.get(0)?,
            project: row.get(1)?,
            benchmark: row.get(2)?,
            version: row.get(3)?,
            git_ref: row.get(4)?,
            git_sha: row.get(5)?,
            receipt,
            metadata: Self::parse_metadata(&row.get::<_, String>(7)?),
            tags: Self::parse_tags(&row.get::<_, String>(8)?),
            created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            content_hash: row.get(10)?,
            source,
            deleted: row.get::<_, i64>(11)? != 0,
        })
    }
}

#[async_trait]
impl BaselineStore for SqliteStore {
    async fn create(&self, record: &BaselineRecord) -> Result<(), StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        let receipt_json = serde_json::to_string(&record.receipt)?;
        let metadata_json = Self::serialize_metadata(&record.metadata);
        let tags_json = Self::serialize_tags(&record.tags);
        let source_str = match record.source {
            BaselineSource::Upload => "upload",
            BaselineSource::Promote => "promote",
            BaselineSource::Migrate => "migrate",
            BaselineSource::Rollback => "rollback",
        };

        let result = conn.execute(
            r#"
            INSERT INTO baselines (
                id, project, benchmark, version, git_ref, git_sha,
                receipt, metadata, tags, source, content_hash,
                deleted, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            "#,
            params![
                record.id,
                record.project,
                record.benchmark,
                record.version,
                record.git_ref,
                record.git_sha,
                receipt_json,
                metadata_json,
                tags_json,
                source_str,
                record.content_hash,
                if record.deleted { 1i64 } else { 0i64 },
                record.created_at.to_rfc3339(),
                record.updated_at.to_rfc3339(),
            ],
        );

        match result {
            Ok(_) => Ok(()),
            Err(rusqlite::Error::SqliteFailure(err, _)) => {
                if err.code == rusqlite::ErrorCode::ConstraintViolation {
                    Err(StoreError::AlreadyExists(format!(
                        "project={}, benchmark={}, version={}",
                        record.project, record.benchmark, record.version
                    )))
                } else {
                    Err(StoreError::SqliteError(rusqlite::Error::SqliteFailure(
                        err, None,
                    )))
                }
            }
            Err(e) => Err(StoreError::SqliteError(e)),
        }
    }

    async fn get(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, project, benchmark, version, git_ref, git_sha,
                   receipt, metadata, tags, source, content_hash, deleted,
                   created_at, updated_at
            FROM baselines
            WHERE project = ?1 AND benchmark = ?2 AND version = ?3 AND deleted = 0
            "#,
        )?;

        let result = stmt.query_row(params![project, benchmark, version], Self::row_to_record);

        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::SqliteError(e)),
        }
    }

    async fn get_latest(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT id, project, benchmark, version, git_ref, git_sha,
                   receipt, metadata, tags, source, content_hash, deleted,
                   created_at, updated_at
            FROM baselines
            WHERE project = ?1 AND benchmark = ?2 AND deleted = 0
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )?;

        let result = stmt.query_row(params![project, benchmark], Self::row_to_record);

        match result {
            Ok(record) => Ok(Some(record)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(StoreError::SqliteError(e)),
        }
    }

    async fn list(
        &self,
        project: &str,
        query: &ListBaselinesQuery,
    ) -> Result<ListBaselinesResponse, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        // Build WHERE clause conditions dynamically with numbered params
        let mut conditions = Vec::new();
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(project.to_string())];

        if let Some(ref b) = query.benchmark {
            conditions.push("benchmark = ?".to_string());
            params.push(Box::new(b.clone()));
        }
        if let Some(ref p) = query.benchmark_prefix {
            conditions.push("benchmark LIKE ? || '%'".to_string());
            params.push(Box::new(p.clone()));
        }
        if let Some(ref r) = query.git_ref {
            conditions.push("git_ref = ?".to_string());
            params.push(Box::new(r.clone()));
        }
        if let Some(ref s) = query.git_sha {
            conditions.push("git_sha = ?".to_string());
            params.push(Box::new(s.clone()));
        }

        // Build the WHERE clause string
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" AND {}", conditions.join(" AND "))
        };

        // Count query
        let count_sql = format!(
            "SELECT COUNT(*) FROM baselines WHERE project = ? AND deleted = 0{}",
            where_clause
        );

        // Get total count
        let count_param_count = params.len();
        let count_params: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let total: u64 = conn.query_row(&count_sql, &count_params[..count_param_count], |row| {
            row.get(0)
        })?;

        // Main query with pagination
        let sql = format!(
            r#"
            SELECT id, project, benchmark, version, git_ref, git_sha,
                   receipt, metadata, tags, source, content_hash, deleted,
                   created_at, updated_at
            FROM baselines
            WHERE project = ? AND deleted = 0{}
            ORDER BY created_at DESC
            LIMIT ? OFFSET ?
            "#,
            where_clause
        );

        // Add pagination params
        params.push(Box::new(query.limit as i64));
        params.push(Box::new(query.offset as i64));

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        let records = stmt
            .query_map(&param_refs[..], Self::row_to_record)?
            .collect::<Result<Vec<_>, _>>()?;

        let baselines: Vec<BaselineSummary> = records
            .into_iter()
            .map(|r| {
                let mut summary: BaselineSummary = r.clone().into();
                if query.include_receipt {
                    summary.receipt = Some(r.receipt);
                }
                summary
            })
            .collect();

        let has_more = (query.offset + baselines.len() as u64) < total;

        Ok(ListBaselinesResponse {
            baselines,
            pagination: PaginationInfo {
                total,
                limit: query.limit,
                offset: query.offset,
                has_more,
            },
        })
    }

    async fn update(&self, record: &BaselineRecord) -> Result<(), StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        let receipt_json = serde_json::to_string(&record.receipt)?;
        let metadata_json = Self::serialize_metadata(&record.metadata);
        let tags_json = Self::serialize_tags(&record.tags);
        let source_str = match record.source {
            BaselineSource::Upload => "upload",
            BaselineSource::Promote => "promote",
            BaselineSource::Migrate => "migrate",
            BaselineSource::Rollback => "rollback",
        };

        let rows_affected = conn.execute(
            r#"
            UPDATE baselines SET
                git_ref = ?1, git_sha = ?2, receipt = ?3,
                metadata = ?4, tags = ?5, source = ?6,
                content_hash = ?7, deleted = ?8, updated_at = ?9
            WHERE project_id = ?10 AND benchmark = ?11 AND version = ?12
            "#,
            params![
                record.git_ref,
                record.git_sha,
                receipt_json,
                metadata_json,
                tags_json,
                source_str,
                record.content_hash,
                if record.deleted { 1i64 } else { 0i64 },
                record.updated_at.to_rfc3339(),
                record.project,
                record.benchmark,
                record.version,
            ],
        )?;

        if rows_affected == 0 {
            return Err(StoreError::NotFound(format!(
                "project={}, benchmark={}, version={}",
                record.project, record.benchmark, record.version
            )));
        }

        Ok(())
    }

    async fn delete(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<bool, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        let rows_affected = conn.execute(
            r#"
            UPDATE baselines SET deleted = 1, updated_at = ?1
            WHERE project = ?2 AND benchmark = ?3 AND version = ?4 AND deleted = 0
            "#,
            params![chrono::Utc::now().to_rfc3339(), project, benchmark, version,],
        )?;

        Ok(rows_affected > 0)
    }

    async fn hard_delete(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<bool, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        let rows_affected = conn.execute(
            "DELETE FROM baselines WHERE project = ?1 AND benchmark = ?2 AND version = ?3",
            params![project, benchmark, version],
        )?;

        Ok(rows_affected > 0)
    }

    async fn list_versions(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<Vec<BaselineVersion>, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        let mut stmt = conn.prepare(
            r#"
            SELECT version, git_ref, git_sha, source, created_at
            FROM baselines
            WHERE project = ?1 AND benchmark = ?2 AND deleted = 0
            ORDER BY created_at DESC
            "#,
        )?;

        let versions: Vec<BaselineVersion> = stmt
            .query_map(params![project, benchmark], |row| {
                let source_str: String = row.get(3)?;
                let source = match source_str.as_str() {
                    "upload" => BaselineSource::Upload,
                    "promote" => BaselineSource::Promote,
                    "migrate" => BaselineSource::Migrate,
                    "rollback" => BaselineSource::Rollback,
                    _ => BaselineSource::Upload,
                };

                let created_at_str: String = row.get(4)?;
                let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());

                Ok(BaselineVersion {
                    version: row.get(0)?,
                    git_ref: row.get(1)?,
                    git_sha: row.get(2)?,
                    created_at,
                    created_by: None,
                    is_current: false,
                    source,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Mark first (latest) as current
        let mut versions = versions;
        if let Some(first) = versions.first_mut() {
            first.is_current = true;
        }

        Ok(versions)
    }

    async fn health_check(&self) -> Result<StorageHealth, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        match conn.query_row("SELECT 1", [], |_| Ok(())) {
            Ok(_) => Ok(StorageHealth::Healthy),
            Err(_) => Ok(StorageHealth::Unhealthy),
        }
    }

    fn backend_type(&self) -> &'static str {
        "sqlite"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BaselineSource;
    use perfgate_types::{BenchMeta, HostInfo, RunMeta, RunReceipt, Stats, ToolInfo, U64Summary};
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    fn create_test_receipt(name: &str) -> RunReceipt {
        RunReceipt {
            schema: "perfgate.run.v1".to_string(),
            tool: ToolInfo {
                name: "perfgate".to_string(),
                version: "0.3.0".to_string(),
            },
            run: RunMeta {
                id: "test-run-id".to_string(),
                started_at: "2026-01-01T00:00:00Z".to_string(),
                ended_at: "2026-01-01T00:01:00Z".to_string(),
                host: HostInfo {
                    os: "linux".to_string(),
                    arch: "x86_64".to_string(),
                    cpu_count: Some(8),
                    memory_bytes: Some(16 * 1024 * 1024 * 1024),
                    hostname_hash: None,
                },
            },
            bench: BenchMeta {
                name: name.to_string(),
                cwd: None,
                command: vec!["./bench.sh".to_string()],
                repeat: 5,
                warmup: 1,
                work_units: None,
                timeout_ms: None,
            },
            samples: vec![],
            stats: Stats {
                wall_ms: U64Summary {
                    median: 100,
                    min: 90,
                    max: 110,
                },
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                binary_bytes: None,
                throughput_per_s: None,
            },
        }
    }

    fn create_test_record(project: &str, benchmark: &str, version: &str) -> BaselineRecord {
        BaselineRecord::new(
            project.to_string(),
            benchmark.to_string(),
            version.to_string(),
            create_test_receipt(benchmark),
            Some("refs/heads/main".to_string()),
            Some("abc123".to_string()),
            BTreeMap::new(),
            vec!["test".to_string()],
            BaselineSource::Upload,
        )
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_in_memory_database() {
        let store = SqliteStore::in_memory().unwrap();

        let record = create_test_record("my-project", "my-bench", "v1.0.0");
        store.create(&record).await.unwrap();

        let retrieved = store.get("my-project", "my-bench", "v1.0.0").await.unwrap();

        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.project, "my-project");
        assert_eq!(retrieved.benchmark, "my-bench");
        assert_eq!(retrieved.version, "v1.0.0");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_persistent_database() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create and write
        {
            let store = SqliteStore::new(&db_path).unwrap();
            let record = create_test_record("my-project", "my-bench", "v1.0.0");
            store.create(&record).await.unwrap();
        }

        // Reopen and verify
        {
            let store = SqliteStore::new(&db_path).unwrap();
            let retrieved = store.get("my-project", "my-bench", "v1.0.0").await.unwrap();

            assert!(retrieved.is_some());
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_duplicate_fails() {
        let store = SqliteStore::in_memory().unwrap();
        let record = create_test_record("my-project", "my-bench", "v1.0.0");

        store.create(&record).await.unwrap();

        // Second create should fail
        let result = store.create(&record).await;
        assert!(result.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_latest() {
        let store = SqliteStore::in_memory().unwrap();

        let record1 = create_test_record("my-project", "my-bench", "v1.0.0");
        store.create(&record1).await.unwrap();

        // Small delay to ensure different timestamps
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let record2 = create_test_record("my-project", "my-bench", "v1.1.0");
        store.create(&record2).await.unwrap();

        let latest = store.get_latest("my-project", "my-bench").await.unwrap();

        assert!(latest.is_some());
        let latest = latest.unwrap();
        assert_eq!(latest.version, "v1.1.0");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_with_filters() {
        let store = SqliteStore::in_memory().unwrap();

        store
            .create(&create_test_record("my-project", "bench-a", "v1.0.0"))
            .await
            .unwrap();
        store
            .create(&create_test_record("my-project", "bench-b", "v1.0.0"))
            .await
            .unwrap();
        store
            .create(&create_test_record("my-project", "bench-a", "v2.0.0"))
            .await
            .unwrap();

        // List all
        let query = ListBaselinesQuery::default();
        let result = store.list("my-project", &query).await.unwrap();
        assert_eq!(result.baselines.len(), 3);

        // Filter by benchmark
        let query = ListBaselinesQuery {
            benchmark: Some("bench-a".to_string()),
            ..Default::default()
        };
        let result = store.list("my-project", &query).await.unwrap();
        assert_eq!(result.baselines.len(), 2);

        // Pagination
        let query = ListBaselinesQuery {
            limit: 2,
            offset: 0,
            ..Default::default()
        };
        let result = store.list("my-project", &query).await.unwrap();
        assert_eq!(result.baselines.len(), 2);
        assert!(result.pagination.has_more);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_delete() {
        let store = SqliteStore::in_memory().unwrap();
        let record = create_test_record("my-project", "my-bench", "v1.0.0");

        store.create(&record).await.unwrap();

        // Soft delete
        let deleted = store
            .delete("my-project", "my-bench", "v1.0.0")
            .await
            .unwrap();
        assert!(deleted);

        // Should not be retrievable
        let retrieved = store.get("my-project", "my-bench", "v1.0.0").await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_list_versions() {
        let store = SqliteStore::in_memory().unwrap();

        store
            .create(&create_test_record("my-project", "my-bench", "v1.0.0"))
            .await
            .unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        store
            .create(&create_test_record("my-project", "my-bench", "v1.1.0"))
            .await
            .unwrap();

        let versions = store.list_versions("my-project", "my-bench").await.unwrap();

        assert_eq!(versions.len(), 2);
        assert!(versions[0].is_current);
        assert!(!versions[1].is_current);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_health_check() {
        let store = SqliteStore::in_memory().unwrap();
        let health = store.health_check().await.unwrap();
        assert_eq!(health, StorageHealth::Healthy);
    }

    #[test]
    fn test_backend_type() {
        let store = SqliteStore::in_memory().unwrap();
        assert_eq!(store.backend_type(), "sqlite");
    }
}
