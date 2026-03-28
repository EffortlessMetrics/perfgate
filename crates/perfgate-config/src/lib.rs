//! Configuration loading and merging logic for perfgate.
//!
//! Loads TOML configuration files, merges environment variables and CLI overrides,
//! and resolves baseline server settings for perfgate workflows.
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.
//!
//! # Example
//!
//! ```no_run
//! use perfgate_config::load_config_file;
//! use std::path::Path;
//!
//! let config = load_config_file(Path::new("perfgate.toml")).unwrap();
//! println!("Benches: {}", config.benches.len());
//! ```

use anyhow::Context;
use perfgate_client::{BaselineClient, ClientConfig, FallbackClient, FallbackStorage};
use perfgate_types::{BaselineServerConfig, ConfigFile, RatchetChange};
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item, Table, Value};

/// Resolved server configuration with all sources merged.
#[derive(Debug, Clone, Default)]
pub struct ResolvedServerConfig {
    pub url: Option<String>,
    pub api_key: Option<String>,
    pub project: Option<String>,
    pub fallback_to_local: bool,
}

impl ResolvedServerConfig {
    /// Returns true if server is configured (has a URL).
    pub fn is_configured(&self) -> bool {
        self.url.as_ref().is_some_and(|u| !u.is_empty())
    }

    /// Creates a BaselineClient from this configuration.
    pub fn create_client(&self) -> anyhow::Result<Option<BaselineClient>> {
        if !self.is_configured() {
            return Ok(None);
        }

        let url = self.url.as_ref().unwrap();
        let mut config = ClientConfig::new(url);

        if let Some(api_key) = &self.api_key {
            config = config.with_api_key(api_key);
        }

        let client = BaselineClient::new(config)
            .with_context(|| format!("Failed to create baseline client for {}", url))?;

        Ok(Some(client))
    }

    /// Creates a FallbackClient if fallback is enabled and server is configured.
    pub fn create_fallback_client(
        &self,
        fallback_dir: Option<&Path>,
    ) -> anyhow::Result<Option<FallbackClient>> {
        let client = match self.create_client()? {
            Some(c) => c,
            None => return Ok(None),
        };

        let fallback = if self.fallback_to_local {
            fallback_dir.map(|dir| FallbackStorage::local(dir.to_path_buf()))
        } else {
            None
        };

        Ok(Some(FallbackClient::new(client, fallback)))
    }

    /// Returns a baseline client for server operations, or an error if not configured.
    pub fn require_fallback_client(
        &self,
        fallback_dir: Option<&Path>,
        error_msg: &str,
    ) -> anyhow::Result<FallbackClient> {
        self.create_fallback_client(fallback_dir)?
            .ok_or_else(|| anyhow::anyhow!(error_msg.to_string()))
    }

    /// Resolve a project for server operations.
    pub fn resolve_project(&self, project: Option<String>) -> anyhow::Result<String> {
        project.or_else(|| self.project.clone()).ok_or_else(|| {
            anyhow::anyhow!(
                "--project is required (or set --project flag, PERFGATE_PROJECT, or [baseline_server].project in perfgate.toml)"
            )
        })
    }
}

/// Loads the perfgate.toml or perfgate.json config file.
pub fn load_config_file(path: &Path) -> anyhow::Result<ConfigFile> {
    if !path.exists() {
        return Ok(ConfigFile::default());
    }

    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;

    if path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "json")
    {
        serde_json::from_str::<ConfigFile>(&content)
            .with_context(|| format!("parse {}", path.display()))
    } else {
        toml::from_str::<ConfigFile>(&content).with_context(|| format!("parse {}", path.display()))
    }
}

/// Resolves server configuration from multiple sources.
pub fn resolve_server_config(
    flag_url: Option<String>,
    flag_key: Option<String>,
    flag_project: Option<String>,
    file_config: &BaselineServerConfig,
) -> ResolvedServerConfig {
    ResolvedServerConfig {
        url: flag_url.or_else(|| file_config.resolved_url()),
        api_key: flag_key.or_else(|| file_config.resolved_api_key()),
        project: flag_project.or_else(|| file_config.resolved_project()),
        fallback_to_local: file_config.fallback_to_local,
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
) -> anyhow::Result<bool> {
    if changes.is_empty() {
        return Ok(false);
    }
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut doc = raw
        .parse::<DocumentMut>()
        .with_context(|| format!("parse {}", path.display()))?;

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
            .ok_or_else(|| anyhow::anyhow!("bench.budgets is not a table-like object"))?;

        for c in changes {
            if c.field != "threshold" {
                continue;
            }
            let metric_key = c.metric.as_str();
            if !budgets.contains_key(metric_key) {
                budgets.insert(metric_key, Item::Table(Table::new()));
            }
            let metric_item = budgets
                .get_mut(metric_key)
                .ok_or_else(|| anyhow::anyhow!("missing budgets.{}", metric_key))?;
            if !metric_item.is_table() {
                *metric_item = Item::Table(Table::new());
            }
            let metric_table = metric_item
                .as_table_mut()
                .ok_or_else(|| anyhow::anyhow!("budgets.{} is not a table", metric_key))?;

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
        fs::write(path, doc.to_string()).with_context(|| format!("write {}", path.display()))?;
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::Metric;

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
