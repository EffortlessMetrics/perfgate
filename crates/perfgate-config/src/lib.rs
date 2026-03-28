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
use perfgate_types::{BaselineServerConfig, ConfigFile, RatchetConfig, RatchetMode};
use std::fs;
use std::path::Path;
use toml_edit::{Array, DocumentMut, Item, Table, Value, value};

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

/// Upserts `[ratchet]` fields in a TOML config while preserving existing comments
/// and key ordering for untouched sections.
pub fn upsert_ratchet_config_toml(
    content: &str,
    ratchet: &RatchetConfig,
) -> anyhow::Result<String> {
    let mut doc = content
        .parse::<DocumentMut>()
        .with_context(|| "parse TOML document for ratchet update")?;

    let ratchet_item = doc.entry("ratchet").or_insert(Item::Table(Table::new()));
    if !ratchet_item.is_table() {
        return Err(anyhow::anyhow!(
            "cannot update [ratchet]: existing `ratchet` key is not a table"
        ));
    }

    let table = ratchet_item
        .as_table_like_mut()
        .ok_or_else(|| anyhow::anyhow!("internal error: ratchet table missing"))?;

    table.insert("enabled", value(ratchet.enabled));
    table.insert(
        "mode",
        value(match ratchet.mode {
            RatchetMode::Threshold => "threshold",
            RatchetMode::BaselineValue => "baseline_value",
        }),
    );
    table.insert("min_improvement", value(ratchet.min_improvement));
    table.insert("max_tightening", value(ratchet.max_tightening));
    table.insert("require_significance", value(ratchet.require_significance));

    let mut allow_metrics = Array::default();
    for metric in &ratchet.allow_metrics {
        allow_metrics.push(metric.to_string());
    }
    table.insert("allow_metrics", Item::Value(Value::Array(allow_metrics)));

    Ok(doc.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::Metric;

    #[test]
    fn upsert_ratchet_config_toml_preserves_existing_comments() {
        let input = r#"# file comment
[defaults]
# keep this comment
repeat = 5
"#;
        let ratchet = RatchetConfig {
            enabled: true,
            mode: RatchetMode::Threshold,
            min_improvement: 0.07,
            max_tightening: 0.1,
            require_significance: true,
            allow_metrics: vec![Metric::WallMs, Metric::CpuMs],
        };

        let output = upsert_ratchet_config_toml(input, &ratchet).expect("update should succeed");
        assert!(output.contains("# keep this comment"));
        assert!(output.contains("[ratchet]"));
        assert!(output.contains("enabled = true"));
        assert!(output.contains("mode = \"threshold\""));
        assert!(output.contains("allow_metrics = [\"wall_ms\", \"cpu_ms\"]"));
    }

    #[test]
    fn upsert_ratchet_config_toml_updates_existing_section() {
        let input = r#"[ratchet]
enabled = false
mode = "threshold"
"#;
        let ratchet = RatchetConfig {
            enabled: true,
            mode: RatchetMode::BaselineValue,
            min_improvement: 0.12,
            max_tightening: 0.2,
            require_significance: false,
            allow_metrics: vec![Metric::WallMs],
        };

        let output = upsert_ratchet_config_toml(input, &ratchet).expect("update should succeed");
        assert!(output.contains("enabled = true"));
        assert!(output.contains("mode = \"baseline_value\""));
        assert!(output.contains("min_improvement = 0.12"));
        assert!(output.contains("max_tightening = 0.2"));
        assert!(output.contains("require_significance = false"));
        assert!(output.contains("allow_metrics = [\"wall_ms\"]"));
    }
}
