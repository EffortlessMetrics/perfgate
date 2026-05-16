//! Baseline selector parsing for compare-style CLI commands.
//!
//! This module owns the policy for deciding whether a user-supplied baseline
//! argument names a local artifact path or a baseline-service benchmark.

use perfgate_client::ResolvedServerConfig;
use std::path::{Path, PathBuf};

pub(crate) enum BaselineSelector {
    Local(PathBuf),
    Server { benchmark: String, explicit: bool },
}

pub(crate) fn parse_baseline_selector(
    baseline: &str,
    server_config: &ResolvedServerConfig,
    not_configured_msg: &'static str,
) -> anyhow::Result<BaselineSelector> {
    if let Some(server_ref) = baseline.strip_prefix("@server:") {
        if server_ref.is_empty() {
            anyhow::bail!("--baseline requires a benchmark name after @server:");
        }

        if !server_config.is_configured() {
            return Err(anyhow::anyhow!(not_configured_msg));
        }

        return Ok(BaselineSelector::Server {
            benchmark: server_ref.to_string(),
            explicit: true,
        });
    }

    let path = Path::new(baseline);
    if !server_config.is_configured()
        || path.exists()
        || baseline.contains(std::path::MAIN_SEPARATOR)
        || baseline.contains('/')
        || baseline.contains('\\')
        || baseline.ends_with(".json")
    {
        return Ok(BaselineSelector::Local(path.to_path_buf()));
    }

    Ok(BaselineSelector::Server {
        benchmark: baseline.to_string(),
        explicit: false,
    })
}
