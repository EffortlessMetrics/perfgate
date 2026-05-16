//! File and object-store storage helpers for the CLI.
//!
//! The CLI accepts both local filesystem paths and object-store URIs for a few
//! artifact flows. Keeping those details in this module lets command handlers
//! focus on orchestration instead of storage dispatch and atomic write details.

use anyhow::Context;
use object_store::{ObjectStore, path::Path as ObjectPath};
use perfgate::app::baseline_resolve::is_remote_storage_uri;
use perfgate_types::RunReceipt;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use url::Url;

struct RemoteLocation {
    store: Arc<dyn ObjectStore>,
    object_path: ObjectPath,
}

fn parse_remote_location(path: &Path) -> anyhow::Result<Option<RemoteLocation>> {
    let uri = path.to_string_lossy().to_string();
    if !is_remote_storage_uri(&uri) {
        return Ok(None);
    }

    let url = Url::parse(&uri).with_context(|| format!("invalid remote URI {}", uri))?;
    let (store, object_path) =
        object_store::parse_url(&url).with_context(|| format!("parse remote URI {}", uri))?;

    Ok(Some(RemoteLocation {
        store: store.into(),
        object_path,
    }))
}

pub(crate) fn with_tokio_runtime<T, F>(f: F) -> anyhow::Result<T>
where
    F: std::future::Future<Output = anyhow::Result<T>>,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("initialize async runtime")?;
    rt.block_on(f)
}

fn is_object_not_found(err: &object_store::Error) -> bool {
    matches!(err, object_store::Error::NotFound { .. })
        || err.to_string().to_ascii_lowercase().contains("not found")
}

pub(crate) fn location_exists(path: &Path) -> anyhow::Result<bool> {
    if let Some(remote) = parse_remote_location(path)? {
        let head = with_tokio_runtime(async move {
            remote
                .store
                .head(&remote.object_path)
                .await
                .map_err(anyhow::Error::from)
        });
        return match head {
            Ok(_) => Ok(true),
            Err(err) => {
                if err
                    .downcast_ref::<object_store::Error>()
                    .is_some_and(is_object_not_found)
                {
                    Ok(false)
                } else {
                    Err(err).with_context(|| format!("check existence {}", path.display()))
                }
            }
        };
    }
    Ok(path.exists())
}

pub(crate) fn read_json_from_location<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> anyhow::Result<T> {
    if let Some(remote) = parse_remote_location(path)? {
        let bytes = with_tokio_runtime(async move {
            let result = remote
                .store
                .get(&remote.object_path)
                .await
                .map_err(anyhow::Error::from)?;
            result.bytes().await.map_err(anyhow::Error::from)
        })
        .with_context(|| format!("read {}", path.display()))?;

        return serde_json::from_slice(&bytes)
            .with_context(|| format!("parse json {}", path.display()));
    }

    read_json(path)
}

pub(crate) fn write_json_to_location<T: serde::Serialize>(
    path: &Path,
    value: &T,
    pretty: bool,
) -> anyhow::Result<()> {
    if let Some(remote) = parse_remote_location(path)? {
        let bytes = if pretty {
            serde_json::to_vec_pretty(value)?
        } else {
            serde_json::to_vec(value)?
        };

        with_tokio_runtime(async move {
            remote
                .store
                .put(&remote.object_path, bytes.into())
                .await
                .map(|_| ())
                .map_err(anyhow::Error::from)
        })
        .with_context(|| format!("write {}", path.display()))?;
        return Ok(());
    }

    write_json(path, value, pretty)
}

pub(crate) fn load_optional_baseline_receipt(path: &Path) -> anyhow::Result<Option<RunReceipt>> {
    if location_exists(path)? {
        Ok(Some(read_json_from_location(path)?))
    } else {
        Ok(None)
    }
}

pub(crate) fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    Ok(perfgate_types::read_json_file(path)?)
}

pub(crate) fn write_json<T: serde::Serialize>(
    path: &Path,
    value: &T,
    pretty: bool,
) -> anyhow::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    if !parent.as_os_str().is_empty() {
        fs::create_dir_all(parent).with_context(|| format!("create dir {}", parent.display()))?;
    }

    let bytes = if pretty {
        serde_json::to_vec_pretty(value)?
    } else {
        serde_json::to_vec(value)?
    };

    atomic_write(path, &bytes)
}

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    use std::io::Write;

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = parent.to_path_buf();
    tmp.push(format!(".{}.tmp", uuid::Uuid::new_v4()));

    {
        let mut f =
            fs::File::create(&tmp).with_context(|| format!("create temp {}", tmp.display()))?;
        f.write_all(bytes)
            .with_context(|| format!("write temp {}", tmp.display()))?;
        f.sync_all().ok();
    }

    fs::rename(&tmp, path)
        .with_context(|| format!("rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}
