//! Artifact storage implementations using object_store.

use super::{ArtifactMeta, ArtifactStore};
use crate::error::StoreError;
use async_trait::async_trait;
use futures::TryStreamExt;
use object_store::{ObjectStore, path::Path};
use std::sync::Arc;

/// Artifact storage using a generic ObjectStore (S3, GCS, Azure, Local).
#[derive(Debug)]
pub struct ObjectArtifactStore {
    inner: Arc<dyn ObjectStore>,
}

impl ObjectArtifactStore {
    /// Creates a new ObjectArtifactStore from an existing ObjectStore.
    pub fn new(inner: Arc<dyn ObjectStore>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl ArtifactStore for ObjectArtifactStore {
    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), StoreError> {
        let path = Path::from(path);
        self.inner
            .put(&path, data.into())
            .await
            .map_err(|e| StoreError::Other(format!("ObjectStore put failed: {}", e)))?;
        Ok(())
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>, StoreError> {
        let path = Path::from(path);
        let result = self
            .inner
            .get(&path)
            .await
            .map_err(|e| StoreError::Other(format!("ObjectStore get failed: {}", e)))?;

        let bytes = result
            .bytes()
            .await
            .map_err(|e| StoreError::Other(format!("ObjectStore bytes failed: {}", e)))?;

        Ok(bytes.to_vec())
    }

    async fn delete(&self, path: &str) -> Result<(), StoreError> {
        let path = Path::from(path);
        self.inner
            .delete(&path)
            .await
            .map_err(|e| StoreError::Other(format!("ObjectStore delete failed: {}", e)))?;
        Ok(())
    }

    async fn list(&self, prefix: Option<&str>) -> Result<Vec<ArtifactMeta>, StoreError> {
        let prefix = prefix.map(Path::from);
        let stream = self.inner.list(prefix.as_ref());

        let objects: Vec<_> = stream
            .try_collect()
            .await
            .map_err(|e| StoreError::Other(format!("ObjectStore list failed: {}", e)))?;

        Ok(objects
            .into_iter()
            .map(|meta| ArtifactMeta {
                path: meta.location.to_string(),
                last_modified: meta.last_modified,
                size: meta.size as u64,
            })
            .collect())
    }
}
