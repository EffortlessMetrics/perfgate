//! Persistent key store trait and implementations.
//!
//! Provides [`KeyStore`] for managing API keys in a database, with
//! implementations for SQLite and in-memory backends.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use perfgate_auth::Role;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use crate::error::StoreError;

/// A persistent API key record stored in the database.
#[derive(Debug, Clone)]
pub struct KeyRecord {
    /// Unique identifier
    pub id: String,
    /// SHA-256 hash of the plaintext key
    pub key_hash: String,
    /// First 12 characters of the key (for display)
    pub key_prefix: String,
    /// Assigned role
    pub role: Role,
    /// Project scope
    pub project: String,
    /// Optional benchmark glob pattern
    pub pattern: Option<String>,
    /// Human-readable description
    pub description: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Expiration timestamp
    pub expires_at: Option<DateTime<Utc>>,
    /// Revocation timestamp (None if active)
    pub revoked_at: Option<DateTime<Utc>>,
}

impl KeyRecord {
    /// Returns true if this key has been revoked.
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }

    /// Returns true if this key has expired.
    pub fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|exp| exp < Utc::now())
    }

    /// Returns true if this key is currently valid (not revoked and not expired).
    pub fn is_active(&self) -> bool {
        !self.is_revoked() && !self.is_expired()
    }
}

/// Hashes a plaintext API key for storage.
pub fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Extracts a display prefix from a plaintext key.
pub fn key_prefix(key: &str) -> String {
    let prefix_len = 12.min(key.len());
    format!("{}...***", &key[..prefix_len])
}

/// Trait for persistent API key storage.
#[async_trait]
pub trait KeyStore: Send + Sync {
    /// Stores a new key record. The `key_hash` field must already be set.
    async fn create_key(&self, record: &KeyRecord) -> Result<(), StoreError>;

    /// Lists all keys (including revoked, for admin view).
    async fn list_keys(&self) -> Result<Vec<KeyRecord>, StoreError>;

    /// Revokes a key by its ID. Returns the revocation timestamp.
    async fn revoke_key(&self, id: &str) -> Result<Option<DateTime<Utc>>, StoreError>;

    /// Validates a plaintext key and returns its record if active.
    async fn validate_key(&self, raw_key: &str) -> Result<Option<KeyRecord>, StoreError>;
}

// ── In-Memory Implementation ──────────────────────────────────────────

/// In-memory key store for testing and development.
#[derive(Debug, Default)]
pub struct InMemoryKeyStore {
    /// Records keyed by ID
    records: Arc<RwLock<HashMap<String, KeyRecord>>>,
}

impl InMemoryKeyStore {
    /// Creates a new empty in-memory key store.
    pub fn new() -> Self {
        Self {
            records: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait]
impl KeyStore for InMemoryKeyStore {
    async fn create_key(&self, record: &KeyRecord) -> Result<(), StoreError> {
        let mut records = self.records.write().await;
        if records.contains_key(&record.id) {
            return Err(StoreError::AlreadyExists(format!("key id={}", record.id)));
        }
        records.insert(record.id.clone(), record.clone());
        Ok(())
    }

    async fn list_keys(&self) -> Result<Vec<KeyRecord>, StoreError> {
        let records = self.records.read().await;
        let mut keys: Vec<_> = records.values().cloned().collect();
        keys.sort_by_key(|b| std::cmp::Reverse(b.created_at));
        Ok(keys)
    }

    async fn revoke_key(&self, id: &str) -> Result<Option<DateTime<Utc>>, StoreError> {
        let mut records = self.records.write().await;
        if let Some(record) = records.get_mut(id) {
            if record.revoked_at.is_some() {
                return Ok(record.revoked_at);
            }
            let now = Utc::now();
            record.revoked_at = Some(now);
            Ok(Some(now))
        } else {
            Ok(None)
        }
    }

    async fn validate_key(&self, raw_key: &str) -> Result<Option<KeyRecord>, StoreError> {
        let hash = hash_key(raw_key);
        let records = self.records.read().await;
        let record = records.values().find(|r| r.key_hash == hash).cloned();
        match record {
            Some(r) if r.is_active() => Ok(Some(r)),
            _ => Ok(None),
        }
    }
}

// ── SQLite Implementation ─────────────────────────────────────────────

/// SQLite-backed persistent key store.
#[derive(Debug)]
pub struct SqliteKeyStore {
    conn: Arc<Mutex<rusqlite::Connection>>,
}

impl SqliteKeyStore {
    /// Opens or creates a key store backed by the given SQLite connection.
    /// The `api_keys` table is created if it does not exist.
    pub fn new(conn: Arc<Mutex<rusqlite::Connection>>) -> Result<Self, StoreError> {
        {
            let c = conn
                .lock()
                .map_err(|e| StoreError::LockError(e.to_string()))?;
            c.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS api_keys (
                    id TEXT PRIMARY KEY,
                    key_hash TEXT NOT NULL UNIQUE,
                    key_prefix TEXT NOT NULL,
                    role TEXT NOT NULL,
                    project TEXT NOT NULL,
                    pattern TEXT,
                    description TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    expires_at TEXT,
                    revoked_at TEXT
                );
                CREATE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash);
                "#,
            )?;
        }
        Ok(Self { conn })
    }

    /// Creates an in-memory SQLite key store (for testing).
    pub fn in_memory() -> Result<Self, StoreError> {
        let conn = rusqlite::Connection::open_in_memory()?;
        Self::new(Arc::new(Mutex::new(conn)))
    }

    fn row_to_record(row: &rusqlite::Row) -> Result<KeyRecord, rusqlite::Error> {
        let role_str: String = row.get(3)?;
        let role = match role_str.as_str() {
            "admin" => Role::Admin,
            "promoter" => Role::Promoter,
            "contributor" => Role::Contributor,
            _ => Role::Viewer,
        };

        let created_at_str: String = row.get(7)?;
        let expires_at_str: Option<String> = row.get(8)?;
        let revoked_at_str: Option<String> = row.get(9)?;

        Ok(KeyRecord {
            id: row.get(0)?,
            key_hash: row.get(1)?,
            key_prefix: row.get(2)?,
            role,
            project: row.get(4)?,
            pattern: row.get(5)?,
            description: row.get(6)?,
            created_at: parse_dt(&created_at_str),
            expires_at: expires_at_str.as_deref().map(parse_dt),
            revoked_at: revoked_at_str.as_deref().map(parse_dt),
        })
    }
}

fn parse_dt(s: &str) -> DateTime<Utc> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn role_str(role: &Role) -> &'static str {
    match role {
        Role::Admin => "admin",
        Role::Promoter => "promoter",
        Role::Contributor => "contributor",
        Role::Viewer => "viewer",
    }
}

#[async_trait]
impl KeyStore for SqliteKeyStore {
    async fn create_key(&self, record: &KeyRecord) -> Result<(), StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;
        conn.execute(
            r#"
            INSERT INTO api_keys (id, key_hash, key_prefix, role, project, pattern, description, created_at, expires_at, revoked_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            rusqlite::params![
                record.id,
                record.key_hash,
                record.key_prefix,
                role_str(&record.role),
                record.project,
                record.pattern,
                record.description,
                record.created_at.to_rfc3339(),
                record.expires_at.map(|t| t.to_rfc3339()),
                record.revoked_at.map(|t| t.to_rfc3339()),
            ],
        )
        .map_err(|e| match &e {
            rusqlite::Error::SqliteFailure(err, _)
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                StoreError::AlreadyExists(format!("key id={}", record.id))
            }
            _ => StoreError::SqliteError(e),
        })?;
        Ok(())
    }

    async fn list_keys(&self) -> Result<Vec<KeyRecord>, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;
        let mut stmt = conn.prepare("SELECT * FROM api_keys ORDER BY created_at DESC")?;
        let rows = stmt
            .query_map([], Self::row_to_record)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    async fn revoke_key(&self, id: &str) -> Result<Option<DateTime<Utc>>, StoreError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;
        let now = Utc::now();
        let n = conn.execute(
            "UPDATE api_keys SET revoked_at = ?1 WHERE id = ?2 AND revoked_at IS NULL",
            rusqlite::params![now.to_rfc3339(), id],
        )?;
        if n > 0 {
            Ok(Some(now))
        } else {
            // Check if the key exists at all
            let exists: bool = conn.query_row(
                "SELECT COUNT(*) > 0 FROM api_keys WHERE id = ?1",
                rusqlite::params![id],
                |row| row.get(0),
            )?;
            if exists {
                // Already revoked — return existing revoked_at
                let revoked_at: Option<String> = conn.query_row(
                    "SELECT revoked_at FROM api_keys WHERE id = ?1",
                    rusqlite::params![id],
                    |row| row.get(0),
                )?;
                Ok(revoked_at.as_deref().map(parse_dt))
            } else {
                Ok(None)
            }
        }
    }

    async fn validate_key(&self, raw_key: &str) -> Result<Option<KeyRecord>, StoreError> {
        let hash = hash_key(raw_key);
        let conn = self
            .conn
            .lock()
            .map_err(|e| StoreError::LockError(e.to_string()))?;
        let result = conn
            .query_row(
                "SELECT * FROM api_keys WHERE key_hash = ?1",
                rusqlite::params![hash],
                Self::row_to_record,
            )
            .optional()?;
        match result {
            Some(r) if r.is_active() => Ok(Some(r)),
            _ => Ok(None),
        }
    }
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_auth::generate_api_key;

    fn make_record(raw_key: &str, role: Role) -> KeyRecord {
        KeyRecord {
            id: uuid::Uuid::new_v4().to_string(),
            key_hash: hash_key(raw_key),
            key_prefix: key_prefix(raw_key),
            role,
            project: "default".to_string(),
            pattern: None,
            description: "test key".to_string(),
            created_at: Utc::now(),
            expires_at: None,
            revoked_at: None,
        }
    }

    #[tokio::test]
    async fn test_inmemory_crud() {
        let store = InMemoryKeyStore::new();
        let raw = generate_api_key(false);
        let rec = make_record(&raw, Role::Contributor);
        let id = rec.id.clone();

        store.create_key(&rec).await.unwrap();

        let keys = store.list_keys().await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, id);

        let found = store.validate_key(&raw).await.unwrap();
        assert!(found.is_some());

        let revoked = store.revoke_key(&id).await.unwrap();
        assert!(revoked.is_some());

        let found = store.validate_key(&raw).await.unwrap();
        assert!(found.is_none(), "revoked key should not validate");
    }

    #[tokio::test]
    async fn test_inmemory_expiration() {
        let store = InMemoryKeyStore::new();
        let raw = generate_api_key(false);
        let mut rec = make_record(&raw, Role::Viewer);
        rec.expires_at = Some(Utc::now() - chrono::Duration::hours(1));

        store.create_key(&rec).await.unwrap();
        let found = store.validate_key(&raw).await.unwrap();
        assert!(found.is_none(), "expired key should not validate");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sqlite_crud() {
        let store = SqliteKeyStore::in_memory().unwrap();
        let raw = generate_api_key(false);
        let rec = make_record(&raw, Role::Admin);
        let id = rec.id.clone();

        store.create_key(&rec).await.unwrap();

        let keys = store.list_keys().await.unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].role, Role::Admin);

        let found = store.validate_key(&raw).await.unwrap();
        assert!(found.is_some());

        let revoked = store.revoke_key(&id).await.unwrap();
        assert!(revoked.is_some());

        let found = store.validate_key(&raw).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sqlite_expiration() {
        let store = SqliteKeyStore::in_memory().unwrap();
        let raw = generate_api_key(false);
        let mut rec = make_record(&raw, Role::Viewer);
        rec.expires_at = Some(Utc::now() - chrono::Duration::hours(1));

        store.create_key(&rec).await.unwrap();
        let found = store.validate_key(&raw).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_sqlite_revoke_nonexistent() {
        let store = SqliteKeyStore::in_memory().unwrap();
        let result = store.revoke_key("nonexistent-id").await.unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_hash_key_deterministic() {
        let h1 = hash_key("pg_live_test123456789012345678901234567890");
        let h2 = hash_key("pg_live_test123456789012345678901234567890");
        assert_eq!(h1, h2);

        let h3 = hash_key("pg_live_different1234567890123456789012");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_key_prefix_display() {
        let prefix = key_prefix("pg_live_abcdefghijklmnopqrstuvwxyz123456");
        assert_eq!(prefix, "pg_live_abcd...***");
    }

    #[test]
    fn test_key_record_active() {
        let raw = generate_api_key(false);
        let rec = make_record(&raw, Role::Viewer);
        assert!(rec.is_active());
        assert!(!rec.is_revoked());
        assert!(!rec.is_expired());
    }
}
