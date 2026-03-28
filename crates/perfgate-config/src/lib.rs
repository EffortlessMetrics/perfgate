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
use toml_edit::{DocumentMut, Item, Table, Value, value};

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

/// Apply ratchet changes to a TOML config while preserving comments/ordering.
pub fn apply_ratchet_changes_to_toml(
    config_path: &Path,
    bench_name: &str,
    changes: &[RatchetChange],
) -> anyhow::Result<()> {
    let content = fs::read_to_string(config_path)
        .with_context(|| format!("read {}", config_path.display()))?;
    let mut doc = content
        .parse::<DocumentMut>()
        .with_context(|| format!("parse {}", config_path.display()))?;

    let benches = doc["bench"]
        .as_array_of_tables_mut()
        .ok_or_else(|| anyhow::anyhow!("config has no [[bench]] entries"))?;

    let bench = benches
        .iter_mut()
        .find(|b| b["name"].as_str() == Some(bench_name))
        .ok_or_else(|| anyhow::anyhow!("bench '{}' not found in config", bench_name))?;

    for change in changes {
        let metric_key = change.metric.as_str();
        if !bench["budgets"].is_table() {
            bench["budgets"] = Item::Table(Table::new());
        }
        let metric_item = &mut bench["budgets"][metric_key];
        match metric_item {
            Item::Table(metric_table) => {
                metric_table["threshold"] = value(change.new_value);
            }
            Item::Value(v) if v.is_inline_table() => {
                if let Some(inline) = v.as_inline_table_mut() {
                    inline.insert("threshold", Value::from(change.new_value));
                }
            }
            _ => {
                let mut metric_table = Table::new();
                metric_table["threshold"] = value(change.new_value);
                *metric_item = Item::Table(metric_table);
            }
        }
    }

    fs::write(config_path, doc.to_string())
        .with_context(|| format!("write {}", config_path.display()))?;
    Ok(())
}
