//! File I/O helpers for reading JSON files.

use serde::de::DeserializeOwned;
use std::path::Path;

/// Error returned by [`read_json_file`].
#[derive(Debug, thiserror::Error)]
pub enum ReadJsonError {
    /// Failed to read the file from disk.
    #[error("failed to read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },

    /// Failed to parse the file contents as JSON.
    #[error("failed to parse JSON from {path}: {source}")]
    Parse {
        path: String,
        source: serde_json::Error,
    },
}

/// Reads a JSON file from disk and deserializes it into `T`.
///
/// This replaces the common boilerplate of `fs::read_to_string` followed
/// by `serde_json::from_str`, with error messages that include the file path.
///
/// # Errors
///
/// Returns [`ReadJsonError::Read`] if the file cannot be read, or
/// [`ReadJsonError::Parse`] if the contents are not valid JSON for `T`.
///
/// # Example
///
/// ```no_run
/// use perfgate_types::read_json_file;
/// use perfgate_types::RunReceipt;
/// use std::path::Path;
///
/// let receipt: RunReceipt = read_json_file(Path::new("run.json")).unwrap();
/// ```
pub fn read_json_file<T: DeserializeOwned>(path: &Path) -> Result<T, ReadJsonError> {
    let contents = std::fs::read_to_string(path).map_err(|source| ReadJsonError::Read {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&contents).map_err(|source| ReadJsonError::Parse {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn read_json_file_nonexistent_returns_read_error() {
        let result = read_json_file::<serde_json::Value>(Path::new("does_not_exist.json"));
        let err = result.unwrap_err();
        assert!(matches!(err, ReadJsonError::Read { .. }));
        assert!(err.to_string().contains("does_not_exist.json"));
    }

    #[test]
    fn read_json_file_invalid_json_returns_parse_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "not valid json {{{").unwrap();

        let result = read_json_file::<serde_json::Value>(&path);
        let err = result.unwrap_err();
        assert!(matches!(err, ReadJsonError::Parse { .. }));
        assert!(err.to_string().contains("bad.json"));
    }

    #[test]
    fn read_json_file_valid_json_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("good.json");
        std::fs::write(&path, r#"{"key": "value"}"#).unwrap();

        let result: serde_json::Value = read_json_file(&path).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn read_json_error_display_includes_path() {
        let err = ReadJsonError::Read {
            path: "/tmp/test.json".to_string(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        };
        let msg = err.to_string();
        assert!(msg.contains("/tmp/test.json"));
        assert!(msg.contains("failed to read"));
    }
}
