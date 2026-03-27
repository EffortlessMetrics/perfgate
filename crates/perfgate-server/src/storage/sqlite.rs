//! SQLite storage implementation for persistent baseline storage.

use async_trait::async_trait;
use rusqlite::{OptionalExtension, params};
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::{ArtifactStore, BaselineStore, StorageHealth};
use crate::error::StoreError;
use crate::models::{
    BaselineRecord, BaselineSource, BaselineVersion, ListBaselinesQuery, ListBaselinesResponse,
    ListVerdictsQuery, ListVerdictsResponse, PaginationInfo, VerdictRecord,
};
use perfgate_types::{VerdictCounts, VerdictStatus};

/// SQLite storage backend for baselines.
#[derive(Debug)]
pub struct SqliteStore {
    /// Path to the database file
    _path: std::path::PathBuf,

    /// Connection pool (simplified: single connection wrapped in Mutex)
    conn: Arc<Mutex<rusqlite::Connection>>,

    /// Optional artifact store for raw receipts
    artifacts: Option<Arc<dyn ArtifactStore>>,
}

impl SqliteStore {
    /// Opens or creates a SQLite database at the specified path.
    pub fn new<P: AsRef<Path>>(
        path: P,
        artifacts: Option<Arc<dyn ArtifactStore>>,
    ) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent().filter(|p| !p.exists()) {
            std::fs::create_dir_all(parent)?;
        }

        let conn = rusqlite::Connection::open(&path)?;
        Self::configure_pragmas(&conn)?;

        let store = Self {
            _path: path,
            conn: Arc::new(Mutex::new(conn)),
            artifacts,
        };

        store.initialize()?;
        Ok(store)
    }

    /// Creates an in-memory SQLite database (for testing).
    pub fn in_memory() -> Result<Self, StoreError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        Self::configure_pragmas(&conn)?;

        let store = Self {
            _path: std::path::PathBuf::from(":memory:"),
            conn: Arc::new(Mutex::new(conn)),
            artifacts: None,
        };

        store.initialize()?;
        Ok(store)
    }

    /// Configures SQLite pragmas for performance and concurrent access.
    ///
    /// - `journal_mode=WAL`: enables write-ahead logging so readers do not
    ///   block writers and vice-versa.
    /// - `busy_timeout=5000`: waits up to 5 seconds when the database is
    ///   locked instead of returning SQLITE_BUSY immediately.
    fn configure_pragmas(conn: &rusqlite::Connection) -> Result<(), StoreError> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        Ok(())
    }

    fn initialize(&self) -> Result<(), StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS baselines (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                benchmark TEXT NOT NULL,
                version TEXT NOT NULL,
                git_ref TEXT,
                git_sha TEXT,
                receipt TEXT,
                artifact_path TEXT,
                metadata TEXT NOT NULL DEFAULT '{}',
                tags TEXT NOT NULL DEFAULT '[]',
                source TEXT NOT NULL DEFAULT 'upload',
                content_hash TEXT NOT NULL,
                deleted INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(project, benchmark, version)
            );
            CREATE INDEX IF NOT EXISTS idx_baselines_project_benchmark ON baselines(project, benchmark);
            CREATE INDEX IF NOT EXISTS idx_baselines_created_at ON baselines(created_at DESC);

            CREATE TABLE IF NOT EXISTS verdicts (
                id TEXT PRIMARY KEY,
                schema_id TEXT NOT NULL,
                project TEXT NOT NULL,
                benchmark TEXT NOT NULL,
                run_id TEXT NOT NULL,
                status TEXT NOT NULL,
                counts TEXT NOT NULL,
                reasons TEXT NOT NULL,
                git_ref TEXT,
                git_sha TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_verdicts_project_benchmark ON verdicts(project, benchmark);
            CREATE INDEX IF NOT EXISTS idx_verdicts_created_at ON verdicts(created_at DESC);
            "#,
        )?;
        Ok(())
    }

    fn row_to_record_tuple(
        row: &rusqlite::Row,
    ) -> Result<(BaselineRecord, Option<String>), rusqlite::Error> {
        let created_at_str: String = row.get(13)?;
        let updated_at_str: String = row.get(14)?;

        let receipt_json: Option<String> = row.get(6)?;
        let receipt = if let Some(json) = receipt_json {
            serde_json::from_str(&json).unwrap_or_else(|_| Self::placeholder_receipt())
        } else {
            Self::placeholder_receipt()
        };

        let record = BaselineRecord {
            schema: crate::models::BASELINE_SCHEMA_V1.to_string(),
            id: row.get(0)?,
            project: row.get(1)?,
            benchmark: row.get(2)?,
            version: row.get(3)?,
            git_ref: row.get(4)?,
            git_sha: row.get(5)?,
            receipt,
            metadata: serde_json::from_str(&row.get::<_, String>(8)?).unwrap_or_default(),
            tags: serde_json::from_str(&row.get::<_, String>(9)?).unwrap_or_default(),
            created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            updated_at: chrono::DateTime::parse_from_rfc3339(&updated_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
            content_hash: row.get(11)?,
            source: match row.get::<_, String>(10)?.as_str() {
                "promote" => BaselineSource::Promote,
                "migrate" => BaselineSource::Migrate,
                "rollback" => BaselineSource::Rollback,
                _ => BaselineSource::Upload,
            },
            deleted: row.get::<_, i64>(12)? != 0,
        };

        Ok((record, row.get(7)?))
    }

    fn placeholder_receipt() -> perfgate_types::RunReceipt {
        serde_json::from_value(serde_json::json!({
            "schema": "perfgate.run.v1",
            "tool": {"name": "placeholder", "version": "0"},
            "run": {
                "id": "placeholder",
                "started_at": "1970-01-01T00:00:00Z",
                "ended_at": "1970-01-01T00:00:00Z",
                "host": {"os": "unknown", "arch": "unknown"}
            },
            "bench": {"name": "placeholder", "command": [], "repeat": 0, "warmup": 0},
            "samples": [],
            "stats": {"wall_ms": {"median": 0, "min": 0, "max": 0}}
        }))
        .unwrap()
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
impl BaselineStore for SqliteStore {
    async fn create(&self, record: &BaselineRecord) -> Result<(), StoreError> {
        let artifact_path = self.store_artifact(record).await?;
        let receipt_json = if artifact_path.is_none() {
            Some(serde_json::to_string(&record.receipt)?)
        } else {
            None
        };

        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;
        conn.execute(
            r#"
            INSERT INTO baselines (
                id, project, benchmark, version, git_ref, git_sha,
                receipt, artifact_path, metadata, tags, source, content_hash,
                deleted, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            "#,
            params![
                record.id,
                record.project,
                record.benchmark,
                record.version,
                record.git_ref,
                record.git_sha,
                receipt_json,
                artifact_path,
                serde_json::to_string(&record.metadata)?,
                serde_json::to_string(&record.tags)?,
                format!("{:?}", record.source).to_lowercase(),
                record.content_hash,
                if record.deleted { 1i64 } else { 0i64 },
                record.created_at.to_rfc3339(),
                record.updated_at.to_rfc3339(),
            ],
        )
        .map_err(|e| match &e {
            rusqlite::Error::SqliteFailure(err, _)
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                StoreError::AlreadyExists(format!(
                    "project={}, benchmark={}, version={}",
                    record.project, record.benchmark, record.version
                ))
            }
            _ => StoreError::SqliteError(e),
        })?;
        Ok(())
    }

    async fn get(
        &self,
        project: &str,
        benchmark: &str,
        version: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        let res = {
            let conn = self
                .conn
                .lock()
                .map_err(|e| StoreError::LockError(e.to_string()))?;
            let mut stmt = conn.prepare(
                "SELECT * FROM baselines WHERE project = ?1 AND benchmark = ?2 AND version = ?3 AND deleted = 0"
            )?;
            stmt.query_row(
                params![project, benchmark, version],
                Self::row_to_record_tuple,
            )
            .optional()?
        };

        match res {
            Some((record, path)) => Ok(Some(self.load_artifact(path, record).await?)),
            None => Ok(None),
        }
    }

    async fn get_latest(
        &self,
        project: &str,
        benchmark: &str,
    ) -> Result<Option<BaselineRecord>, StoreError> {
        let res = {
            let conn = self
                .conn
                .lock()
                .map_err(|e| StoreError::LockError(e.to_string()))?;
            let mut stmt = conn.prepare(
                "SELECT * FROM baselines WHERE project = ?1 AND benchmark = ?2 AND deleted = 0 ORDER BY created_at DESC LIMIT 1"
            )?;
            stmt.query_row(params![project, benchmark], Self::row_to_record_tuple)
                .optional()?
        };

        match res {
            Some((record, path)) => Ok(Some(self.load_artifact(path, record).await?)),
            None => Ok(None),
        }
    }

    async fn list(
        &self,
        project: &str,
        query: &ListBaselinesQuery,
    ) -> Result<ListBaselinesResponse, StoreError> {
        let (records_with_paths, total) = {
            let conn = self
                .conn
                .lock()
                .map_err(|e| StoreError::LockError(e.to_string()))?;
            let mut sql =
                String::from("SELECT * FROM baselines WHERE project = ?1 AND deleted = 0");
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(project.to_string())];

            if let Some(ref b) = query.benchmark {
                sql.push_str(" AND benchmark = ?");
                params.push(Box::new(b.clone()));
            }

            let count_sql = format!("SELECT COUNT(*) FROM ({})", sql);
            let total: u64 =
                conn.query_row(&count_sql, rusqlite::params_from_iter(params.iter()), |r| {
                    r.get(0)
                })?;

            sql.push_str(" ORDER BY created_at DESC LIMIT ? OFFSET ?");
            params.push(Box::new(query.limit as i64));
            params.push(Box::new(query.offset as i64));

            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt
                .query_map(
                    rusqlite::params_from_iter(params.iter()),
                    Self::row_to_record_tuple,
                )?
                .collect::<Result<Vec<_>, _>>()?;
            (rows, total)
        };

        let mut baselines = Vec::with_capacity(records_with_paths.len());
        for (mut record, path) in records_with_paths {
            if query.include_receipt {
                record = self.load_artifact(path, record).await?;
            }
            baselines.push(record.into());
        }

        let count = baselines.len() as u64;

        Ok(ListBaselinesResponse {
            baselines,
            pagination: PaginationInfo {
                total,
                limit: query.limit,
                offset: query.offset,
                has_more: (query.offset + count) < total,
            },
        })
    }

    async fn update(&self, record: &BaselineRecord) -> Result<(), StoreError> {
        let artifact_path = self.store_artifact(record).await?;
        let receipt_json = if artifact_path.is_none() {
            Some(serde_json::to_string(&record.receipt)?)
        } else {
            None
        };

        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;
        conn.execute(
            "UPDATE baselines SET git_ref=?1, git_sha=?2, receipt=?3, artifact_path=?4, metadata=?5, tags=?6, source=?7, content_hash=?8, updated_at=?9 WHERE project=?10 AND benchmark=?11 AND version=?12",
            params![
                record.git_ref, record.git_sha, receipt_json, artifact_path,
                serde_json::to_string(&record.metadata)?, serde_json::to_string(&record.tags)?,
                format!("{:?}", record.source).to_lowercase(), record.content_hash,
                record.updated_at.to_rfc3339(), record.project, record.benchmark, record.version
            ]
        )?;
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
        let n = conn.execute("UPDATE baselines SET deleted = 1, updated_at = ?1 WHERE project = ?2 AND benchmark = ?3 AND version = ?4 AND deleted = 0",
            params![chrono::Utc::now().to_rfc3339(), project, benchmark, version])?;
        Ok(n > 0)
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
        let n = conn.execute(
            "DELETE FROM baselines WHERE project = ?1 AND benchmark = ?2 AND version = ?3",
            params![project, benchmark, version],
        )?;
        Ok(n > 0)
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
        let mut stmt = conn.prepare("SELECT version, git_ref, git_sha, source, created_at FROM baselines WHERE project = ?1 AND benchmark = ?2 AND deleted = 0 ORDER BY created_at DESC")?;
        let mut versions: Vec<BaselineVersion> = stmt
            .query_map(params![project, benchmark], |row| {
                let created_at_str: String = row.get(4)?;
                Ok(BaselineVersion {
                    version: row.get(0)?,
                    git_ref: row.get(1)?,
                    git_sha: row.get(2)?,
                    created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .unwrap_or_else(|_| chrono::Utc::now()),
                    created_by: None,
                    is_current: false,
                    source: match row.get::<_, String>(3)?.as_str() {
                        "promote" => BaselineSource::Promote,
                        "migrate" => BaselineSource::Migrate,
                        "rollback" => BaselineSource::Rollback,
                        _ => BaselineSource::Upload,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
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

    async fn create_verdict(&self, record: &VerdictRecord) -> Result<(), StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        let counts_json =
            serde_json::to_string(&record.counts).map_err(StoreError::SerializationError)?;
        let reasons_json =
            serde_json::to_string(&record.reasons).map_err(StoreError::SerializationError)?;
        let status_str = record.status.as_str();
        let created_at_str = record.created_at.to_rfc3339();

        conn.execute(
            r#"
            INSERT INTO verdicts (
                id, schema_id, project, benchmark, run_id, status, counts, reasons,
                git_ref, git_sha, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            params![
                record.id,
                record.schema,
                record.project,
                record.benchmark,
                record.run_id,
                status_str,
                counts_json,
                reasons_json,
                record.git_ref,
                record.git_sha,
                created_at_str
            ],
        )?;

        Ok(())
    }

    async fn list_verdicts(
        &self,
        project: &str,
        query: &ListVerdictsQuery,
    ) -> Result<ListVerdictsResponse, StoreError> {
        let mut sql = "SELECT * FROM verdicts WHERE project = ?".to_string();
        let mut params_vec: Vec<rusqlite::types::Value> = vec![project.to_string().into()];

        if let Some(bench) = &query.benchmark {
            sql.push_str(" AND benchmark = ?");
            params_vec.push(bench.clone().into());
        }

        if let Some(status) = &query.status {
            sql.push_str(" AND status = ?");
            params_vec.push(status.as_str().to_string().into());
        }

        if let Some(since) = &query.since {
            sql.push_str(" AND created_at >= ?");
            params_vec.push(since.to_rfc3339().into());
        }

        if let Some(until) = &query.until {
            sql.push_str(" AND created_at <= ?");
            params_vec.push(until.to_rfc3339().into());
        }

        sql.push_str(" ORDER BY created_at DESC");

        // Limit and offset
        sql.push_str(" LIMIT ? OFFSET ?");
        params_vec.push((query.limit as i64).into());
        params_vec.push((query.offset as i64).into());

        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| StoreError::QueryError(e.to_string()))?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(params_vec.iter()), |row| {
                Self::row_to_verdict(row)
            })
            .map_err(|e| StoreError::QueryError(e.to_string()))?;

        let mut verdicts = Vec::new();
        for row in rows {
            verdicts.push(row?);
        }

        // For total count
        let count_sql = "SELECT COUNT(*) FROM verdicts WHERE project = ?";
        let total: i64 = conn.query_row(count_sql, params![project], |row| row.get(0))?;

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

impl SqliteStore {
    fn row_to_verdict(row: &rusqlite::Row) -> Result<VerdictRecord, rusqlite::Error> {
        let status_str: String = row.get(5)?;
        let status = match status_str.as_str() {
            "pass" => VerdictStatus::Pass,
            "warn" => VerdictStatus::Warn,
            "fail" => VerdictStatus::Fail,
            "skip" => VerdictStatus::Skip,
            _ => VerdictStatus::Pass,
        };

        let counts_json: String = row.get(6)?;
        let counts = serde_json::from_str(&counts_json).unwrap_or(VerdictCounts {
            pass: 0,
            warn: 0,
            fail: 0,
            skip: 0,
        });

        let reasons_json: String = row.get(7)?;
        let reasons = serde_json::from_str(&reasons_json).unwrap_or_default();

        let created_at_str: String = row.get(10)?;
        let created_at = chrono::DateTime::parse_from_rfc3339(&created_at_str)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());

        Ok(VerdictRecord {
            id: row.get(0)?,
            schema: row.get(1)?,
            project: row.get(2)?,
            benchmark: row.get(3)?,
            run_id: row.get(4)?,
            status,
            counts,
            reasons,
            git_ref: row.get(8)?,
            git_sha: row.get(9)?,
            created_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{BaselineRecordExt, BaselineSource};
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
                wall_ms: U64Summary::new(100, 90, 110),
                cpu_ms: None,
                page_faults: None,
                ctx_switches: None,
                max_rss_kb: None,
                io_read_bytes: None,
                io_write_bytes: None,
                network_packets: None,
                energy_uj: None,
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
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_persistent_database() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        {
            let store = SqliteStore::new(&db_path, None).unwrap();
            let record = create_test_record("my-project", "my-bench", "v1.0.0");
            store.create(&record).await.unwrap();
        }
        {
            let store = SqliteStore::new(&db_path, None).unwrap();
            let retrieved = store.get("my-project", "my-bench", "v1.0.0").await.unwrap();
            assert!(retrieved.is_some());
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_wal_mode_enabled() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("wal_test.db");
        let store = SqliteStore::new(&db_path, None).unwrap();

        let conn = store.conn.lock().unwrap();
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");

        let busy_timeout: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(busy_timeout, 5000);
    }
}
