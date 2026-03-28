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
use perfgate_types::{BaselineServerConfig, ConfigFile, Metric};
use std::fs;
use std::path::Path;

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

#[derive(Debug, Clone, PartialEq)]
pub struct ThresholdRatchetEdit {
    pub bench: String,
    pub metric: Metric,
    pub new_threshold: f64,
}

/// Apply threshold ratchet edits to a TOML config file while preserving formatting/comments.
pub fn apply_threshold_ratchets(path: &Path, edits: &[ThresholdRatchetEdit]) -> anyhow::Result<()> {
    if edits.is_empty() {
        return Ok(());
    }
    let content = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    for edit in edits {
        let mut in_bench = false;
        let mut bench_match = false;
        let mut in_metric = false;
        let mut updated = false;
        for i in 0..lines.len() {
            let trimmed = lines[i].trim();
            if trimmed == "[[bench]]" {
                in_bench = true;
                bench_match = false;
                in_metric = false;
                continue;
            }
            if in_bench
                && trimmed.starts_with("name")
                && trimmed.contains(&format!("\"{}\"", edit.bench))
            {
                bench_match = true;
                continue;
            }
            if in_bench
                && bench_match
                && trimmed == format!("[bench.budgets.{}]", edit.metric.as_str())
            {
                in_metric = true;
                continue;
            }
            if in_metric && trimmed.starts_with('[') {
                in_metric = false;
            }
            if in_metric && trimmed.starts_with("threshold") {
                lines[i] = format!("threshold = {:.6}", edit.new_threshold);
                updated = true;
                break;
            }
        }
        if !updated {
            anyhow::bail!(
                "could not find editable threshold for bench '{}' metric '{}'",
                edit.bench,
                edit.metric.as_str()
            );
        }
    }
    fs::write(path, format!("{}\n", lines.join("\n")))
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
