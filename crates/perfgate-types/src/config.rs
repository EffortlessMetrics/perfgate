//! Configuration file helpers and re-exports.
//!
//! This module keeps the stable `perfgate.toml` / `perfgate.json` contract
//! next to the receipt and schema types it configures.

use crate::read_json_file;
use std::fs;
use std::path::Path;
use thiserror::Error;
use toml_edit::{DocumentMut, Item, Table, Value};

pub use crate::{
    BaselineServerConfig, BenchConfigFile, ConfigFile, DefaultsConfig, RatchetChange,
    RatchetConfig, RatchetMode,
};

/// Error returned while loading a perfgate config file.
#[derive(Debug, Error)]
pub enum ConfigLoadError {
    /// A config file could not be read from disk.
    #[error("read {path}: {source}")]
    Read {
        /// Path being read.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A TOML config file could not be parsed.
    #[error("parse {path}: {source}")]
    TomlParse {
        /// Path being parsed.
        path: String,
        /// Underlying TOML parse error.
        #[source]
        source: toml::de::Error,
    },
    /// A JSON config file could not be loaded.
    #[error("load JSON config {path}: {source}")]
    Json {
        /// Path being loaded.
        path: String,
        /// Underlying JSON load error.
        #[source]
        source: crate::ReadJsonError,
    },
}

/// Error returned while applying ratchet edits to a TOML config file.
#[derive(Debug, Error)]
pub enum RatchetTomlEditError {
    /// The config file could not be read.
    #[error("read {path}: {source}")]
    Read {
        /// Path being read.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The config file could not be parsed as editable TOML.
    #[error("parse {path}: {source}")]
    Parse {
        /// Path being parsed.
        path: String,
        /// Underlying TOML parse error.
        #[source]
        source: toml_edit::TomlError,
    },
    /// The target TOML structure was malformed.
    #[error("{0}")]
    Malformed(String),
    /// The updated config file could not be written.
    #[error("write {path}: {source}")]
    Write {
        /// Path being written.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Loads a `perfgate.toml` or `perfgate.json` config file.
///
/// Returns [`ConfigFile::default`] when the path does not exist.
pub fn load_config_file(path: &Path) -> Result<ConfigFile, ConfigLoadError> {
    if !path.exists() {
        return Ok(ConfigFile::default());
    }

    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "json")
    {
        read_json_file(path).map_err(|source| ConfigLoadError::Json {
            path: path.display().to_string(),
            source,
        })
    } else {
        let content = fs::read_to_string(path).map_err(|source| ConfigLoadError::Read {
            path: path.display().to_string(),
            source,
        })?;
        toml::from_str::<ConfigFile>(&content).map_err(|source| ConfigLoadError::TomlParse {
            path: path.display().to_string(),
            source,
        })
    }
}

/// Preview ratchet edits as human-readable lines.
pub fn preview_ratchet_toml_changes(changes: &[RatchetChange]) -> Vec<String> {
    if changes.is_empty() {
        return vec!["No ratchet changes eligible.".to_string()];
    }
    let mut out = Vec::with_capacity(changes.len() + 1);
    out.push("Config updates (preview):".to_string());
    for c in changes {
        out.push(format!(
            "- bench.budgets.{}.{}: {:.4} -> {:.4}",
            c.metric.as_str(),
            c.field,
            c.old_value,
            c.new_value
        ));
    }
    out
}

/// Apply threshold ratchet changes to a bench section in TOML while preserving comments/order.
pub fn apply_ratchet_toml_changes(
    path: &Path,
    bench_name: &str,
    changes: &[RatchetChange],
) -> Result<bool, RatchetTomlEditError> {
    if changes.is_empty() {
        return Ok(false);
    }
    let raw = fs::read_to_string(path).map_err(|source| RatchetTomlEditError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let mut doc = raw
        .parse::<DocumentMut>()
        .map_err(|source| RatchetTomlEditError::Parse {
            path: path.display().to_string(),
            source,
        })?;

    let mut updated = false;
    let Some(benches) = doc.get_mut("bench").and_then(Item::as_array_of_tables_mut) else {
        return Ok(false);
    };

    for bench in benches.iter_mut() {
        let name_matches = bench
            .get("name")
            .and_then(Item::as_str)
            .is_some_and(|n| n == bench_name);
        if !name_matches {
            continue;
        }

        if bench.get("budgets").is_none() {
            bench.insert("budgets", Item::Table(Table::new()));
        }
        let budgets = bench
            .get_mut("budgets")
            .and_then(Item::as_table_like_mut)
            .ok_or_else(|| {
                RatchetTomlEditError::Malformed(
                    "bench.budgets is not a table-like object".to_string(),
                )
            })?;

        for c in changes {
            if c.field != "threshold" {
                continue;
            }
            let metric_key = c.metric.as_str();
            if !budgets.contains_key(metric_key) {
                budgets.insert(metric_key, Item::Table(Table::new()));
            }
            let metric_item = budgets.get_mut(metric_key).ok_or_else(|| {
                RatchetTomlEditError::Malformed(format!("missing budgets.{metric_key}"))
            })?;
            if !metric_item.is_table() {
                *metric_item = Item::Table(Table::new());
            }
            let metric_table = metric_item.as_table_mut().ok_or_else(|| {
                RatchetTomlEditError::Malformed(format!("budgets.{metric_key} is not a table"))
            })?;

            let current = metric_table
                .get("threshold")
                .and_then(Item::as_float)
                .unwrap_or(c.old_value);
            if c.new_value + f64::EPSILON < current {
                metric_table["threshold"] = Item::Value(Value::from(c.new_value));
                updated = true;
            }
        }
        break;
    }

    if updated {
        fs::write(path, doc.to_string()).map_err(|source| RatchetTomlEditError::Write {
            path: path.display().to_string(),
            source,
        })?;
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Metric;

    #[test]
    fn ratchet_toml_apply_preserves_comments() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("perfgate.toml");
        let src = r#"# top comment
[defaults]
threshold = 0.2

[[bench]]
# bench comment
name = "bench-a"
command = ["echo", "x"]
[bench.budgets.wall_ms]
threshold = 0.2 # inline comment
"#;
        std::fs::write(&path, src).expect("write");
        let changes = vec![RatchetChange {
            metric: Metric::WallMs,
            field: "threshold".to_string(),
            old_value: 0.2,
            new_value: 0.18,
            reason: "test".to_string(),
        }];

        let changed = apply_ratchet_toml_changes(&path, "bench-a", &changes).expect("apply");
        assert!(changed);
        let updated = std::fs::read_to_string(&path).expect("read");
        assert!(updated.contains("# top comment"));
        assert!(updated.contains("# bench comment"));
        assert!(updated.contains("threshold = 0.18"));
    }
}
