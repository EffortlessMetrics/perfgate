//! Baseline path resolution logic.

use perfgate_types::ConfigFile;
use std::path::PathBuf;

/// Resolve the baseline path from CLI args or config defaults.
pub fn resolve_baseline_path(
    cli_baseline: &Option<PathBuf>,
    bench_name: &str,
    config: &ConfigFile,
) -> PathBuf {
    // 1. CLI takes precedence
    if let Some(path) = cli_baseline {
        return path.clone();
    }

    // 2. Fall back to baseline_pattern from config defaults.
    if let Some(pattern) = &config.defaults.baseline_pattern {
        return render_baseline_pattern(pattern, bench_name);
    }

    // 3. Fall back to baseline_dir from config defaults
    if let Some(baseline_dir) = &config.defaults.baseline_dir {
        if is_remote_storage_uri(baseline_dir) {
            return PathBuf::from(format!(
                "{}/{}.json",
                baseline_dir.trim_end_matches('/'),
                bench_name
            ));
        }
        return PathBuf::from(baseline_dir).join(format!("{}.json", bench_name));
    }

    // 4. Default convention: baselines/{bench_name}.json
    PathBuf::from("baselines").join(format!("{}.json", bench_name))
}

/// Render a baseline pattern by replacing {bench} placeholder.
pub fn render_baseline_pattern(pattern: &str, bench_name: &str) -> PathBuf {
    PathBuf::from(pattern.replace("{bench}", bench_name))
}

/// Check if a path is a remote storage URI (s3:// or gs://).
pub fn is_remote_storage_uri(path: &str) -> bool {
    path.starts_with("s3://") || path.starts_with("gs://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use perfgate_types::{BaselineServerConfig, ConfigFile, DefaultsConfig};

    #[test]
    fn test_resolve_baseline_path_uses_cli_over_config() {
        let config = ConfigFile {
            defaults: DefaultsConfig {
                noise_threshold: None,
                baseline_pattern: Some("pattern/{bench}.receipt.json".to_string()),
                baseline_dir: Some("bases".to_string()),
                ..Default::default()
            },
            baseline_server: BaselineServerConfig::default(),
            tradeoffs: Vec::new(),
            ratchet: None,
            benches: Vec::new(),
        };

        let cli = Some(PathBuf::from("cli.json"));
        assert_eq!(
            resolve_baseline_path(&cli, "bench", &config),
            PathBuf::from("cli.json")
        );

        let no_cli = None;
        assert_eq!(
            resolve_baseline_path(&no_cli, "bench", &config),
            PathBuf::from("pattern").join("bench.receipt.json")
        );
    }

    #[test]
    fn test_is_remote_storage_uri() {
        assert!(is_remote_storage_uri("s3://bucket/key"));
        assert!(is_remote_storage_uri("gs://bucket/key"));
        assert!(!is_remote_storage_uri("local/path"));
    }
}
