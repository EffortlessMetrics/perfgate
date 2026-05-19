//! Advisory baseline maturity reporting.

use anyhow::Context;
use chrono::{DateTime, Utc};
use perfgate::app::baseline_resolve::{is_remote_storage_uri, resolve_baseline_path};
use perfgate_types::config::load_config_file;
use perfgate_types::error::ConfigValidationError;
use perfgate_types::{ConfigFile, RunReceipt};
use std::path::Path;

use crate::doctor::plural;
use crate::storage::read_json_from_location;

const NEW_SAMPLE_LIMIT: usize = 3;
const MATURE_SAMPLE_LIMIT: usize = 7;
const HIGH_NOISE_CV: f64 = 0.10;
const STALE_BASELINE_DAYS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BaselineMaturity {
    Missing,
    New,
    Immature,
    Mature,
    Stale,
    HostMismatched,
    HighNoise,
    Remote,
}

impl BaselineMaturity {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::New => "new",
            Self::Immature => "immature",
            Self::Mature => "mature",
            Self::Stale => "stale",
            Self::HostMismatched => "host_mismatched",
            Self::HighNoise => "high_noise",
            Self::Remote => "remote",
        }
    }

    pub(crate) fn recommendation(self) -> &'static str {
        match self {
            Self::Missing => "run a local check and promote only after reviewing the workload",
            Self::New => "keep advisory until more measured samples exist",
            Self::Immature => "increase repeat count or collect more runs before blocking PRs",
            Self::Mature => "safe to use as a gate if the workload is still representative",
            Self::Stale => "refresh and review the baseline before relying on it for blocking CI",
            Self::HostMismatched => "refresh on the same runner class before comparing or gating",
            Self::HighNoise => "keep advisory; calibrate or use paired mode before blocking PRs",
            Self::Remote => "remote baseline configured; inspect server history before gating",
        }
    }
}

pub(crate) struct BaselineDoctorRow {
    pub(crate) bench: String,
    pub(crate) path: String,
    pub(crate) maturity: BaselineMaturity,
    pub(crate) samples: Option<usize>,
    pub(crate) cv: Option<f64>,
    pub(crate) host: Option<String>,
    pub(crate) age_days: Option<i64>,
}

pub(crate) fn execute_baseline_doctor(
    config_path: &Path,
    bench: Option<&str>,
) -> anyhow::Result<()> {
    let config = load_validated_baseline_config(config_path)?;
    let benches = configured_benches(&config, bench)?;

    println!("Baseline doctor ({})", config_path.display());
    if benches.is_empty() {
        println!("No benchmarks are configured.");
        println!();
        println!("Next:");
        println!("  perfgate init --ci github --profile standard --suggest-benches");
        return Ok(());
    }

    let mut counts = BaselineDoctorCounts::default();
    for bench_name in &benches {
        let row = inspect_baseline(&config, bench_name)?;
        counts.record(row.maturity);
        print_row(&row);
    }

    println!();
    println!(
        "Summary: {} mature, {} immature, {} new, {} missing, {} stale, {} host-mismatched, {} high-noise, {} remote",
        counts.mature,
        counts.immature,
        counts.new,
        counts.missing,
        counts.stale,
        counts.host_mismatched,
        counts.high_noise,
        counts.remote
    );
    println!();
    println!("Next:");
    if counts.missing > 0 {
        println!("  perfgate check --config {} --all", config_path.display());
        println!(
            "  perfgate baseline promote --config {} --all",
            config_path.display()
        );
    } else if counts.high_noise > 0 {
        println!(
            "  perfgate calibrate --config {} --bench <bench>",
            config_path.display()
        );
        println!(
            "  perfgate paired --name <bench> --baseline-cmd \"<baseline-cmd>\" --current-cmd \"<current-cmd>\" --repeat 10 --out artifacts/perfgate/<bench>/paired.json"
        );
    } else if counts.stale > 0 || counts.host_mismatched > 0 {
        println!(
            "  perfgate check --config {} --all --require-baseline",
            config_path.display()
        );
        println!("  refresh stale or host-mismatched baselines after review");
    } else if counts.immature > 0 || counts.new > 0 {
        println!("  collect more measured samples before making these benchmarks blocking");
        println!(
            "  perfgate calibrate --config {} --bench <bench>",
            config_path.display()
        );
    } else {
        println!(
            "  perfgate check --config {} --all --require-baseline",
            config_path.display()
        );
    }
    println!();
    println!("Do not:");
    println!(
        "  promote baselines blindly or loosen thresholds to make maturity warnings disappear"
    );

    Ok(())
}

#[derive(Default)]
struct BaselineDoctorCounts {
    missing: usize,
    new: usize,
    immature: usize,
    mature: usize,
    stale: usize,
    host_mismatched: usize,
    high_noise: usize,
    remote: usize,
}

impl BaselineDoctorCounts {
    fn record(&mut self, maturity: BaselineMaturity) {
        match maturity {
            BaselineMaturity::Missing => self.missing += 1,
            BaselineMaturity::New => self.new += 1,
            BaselineMaturity::Immature => self.immature += 1,
            BaselineMaturity::Mature => self.mature += 1,
            BaselineMaturity::Stale => self.stale += 1,
            BaselineMaturity::HostMismatched => self.host_mismatched += 1,
            BaselineMaturity::HighNoise => self.high_noise += 1,
            BaselineMaturity::Remote => self.remote += 1,
        }
    }
}

fn print_row(row: &BaselineDoctorRow) {
    println!();
    println!("bench: {}", row.bench);
    println!("status: {}", row.maturity.as_str());
    println!("path: {}", row.path);
    if let Some(samples) = row.samples {
        println!("samples: {samples} measured sample{}", plural(samples));
    } else {
        println!("samples: unavailable");
    }
    println!(
        "cv: {}",
        row.cv
            .map(format_percent)
            .unwrap_or_else(|| "unavailable".to_string())
    );
    println!(
        "host: {}",
        row.host.clone().unwrap_or_else(|| "unknown".to_string())
    );
    println!(
        "age: {}",
        row.age_days
            .map(|days| format!("{days} day{}", plural(days as usize)))
            .unwrap_or_else(|| "unknown".to_string())
    );
    println!("recommendation: {}", row.maturity.recommendation());
}

pub(crate) fn inspect_baseline(
    config: &ConfigFile,
    bench_name: &str,
) -> anyhow::Result<BaselineDoctorRow> {
    let path = resolve_baseline_path(&None, bench_name, config);
    let path_text = path.to_string_lossy().to_string();
    if is_remote_storage_uri(&path_text) {
        return Ok(BaselineDoctorRow {
            bench: bench_name.to_string(),
            path: path_text,
            maturity: BaselineMaturity::Remote,
            samples: None,
            cv: None,
            host: None,
            age_days: None,
        });
    }

    if !path.exists() {
        return Ok(BaselineDoctorRow {
            bench: bench_name.to_string(),
            path: path.display().to_string(),
            maturity: BaselineMaturity::Missing,
            samples: None,
            cv: None,
            host: None,
            age_days: None,
        });
    }

    let receipt: RunReceipt = read_json_from_location(&path)
        .with_context(|| format!("failed to read baseline receipt from {}", path.display()))?;
    let samples = measured_sample_count(&receipt);
    let cv = receipt.stats.wall_ms.cv();
    let host = host_class(&receipt);
    let age_days = baseline_age_days(&receipt);
    let maturity = classify_baseline(samples, cv, &host, age_days);

    Ok(BaselineDoctorRow {
        bench: bench_name.to_string(),
        path: path.display().to_string(),
        maturity,
        samples: Some(samples),
        cv,
        host: Some(host),
        age_days,
    })
}

fn classify_baseline(
    samples: usize,
    cv: Option<f64>,
    host: &str,
    age_days: Option<i64>,
) -> BaselineMaturity {
    if host != current_host_class() {
        return BaselineMaturity::HostMismatched;
    }
    if cv.is_some_and(|cv| cv > HIGH_NOISE_CV) {
        return BaselineMaturity::HighNoise;
    }
    if age_days.is_some_and(|days| days > STALE_BASELINE_DAYS) {
        return BaselineMaturity::Stale;
    }
    if samples < NEW_SAMPLE_LIMIT {
        return BaselineMaturity::New;
    }
    if samples < MATURE_SAMPLE_LIMIT || cv.is_none() {
        return BaselineMaturity::Immature;
    }
    BaselineMaturity::Mature
}

fn load_validated_baseline_config(config_path: &Path) -> anyhow::Result<ConfigFile> {
    let config = load_config_file(config_path)
        .with_context(|| format!("failed to load {}", config_path.display()))?;
    config
        .validate()
        .map_err(|error| anyhow::anyhow!("{} is invalid: {error}", config_path.display()))?;
    Ok(config)
}

pub(crate) fn configured_benches(
    config: &ConfigFile,
    bench: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    if let Some(bench) = bench {
        if config
            .benches
            .iter()
            .any(|candidate| candidate.name == bench)
        {
            return Ok(vec![bench.to_string()]);
        }

        return Err(ConfigValidationError::BenchName(format!(
            "benchmark '{}' is not defined in the config file",
            bench
        ))
        .into());
    }

    Ok(config
        .benches
        .iter()
        .map(|bench| bench.name.clone())
        .collect())
}

fn measured_sample_count(receipt: &RunReceipt) -> usize {
    receipt
        .samples
        .iter()
        .filter(|sample| !sample.warmup)
        .count()
}

fn host_class(receipt: &RunReceipt) -> String {
    format!("{}-{}", receipt.run.host.os, receipt.run.host.arch)
}

fn current_host_class() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn baseline_age_days(receipt: &RunReceipt) -> Option<i64> {
    let started_at = DateTime::parse_from_rfc3339(&receipt.run.started_at).ok()?;
    let age = Utc::now().signed_duration_since(started_at.with_timezone(&Utc));
    Some(age.num_days().max(0))
}

fn format_percent(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_baseline_prefers_host_mismatch_before_noise() {
        assert_eq!(
            classify_baseline(10, Some(0.50), "other-os-x86_64", Some(0)),
            BaselineMaturity::HostMismatched
        );
    }

    #[test]
    fn classify_baseline_distinguishes_core_maturity_states() {
        let host = current_host_class();
        assert_eq!(
            classify_baseline(10, Some(0.11), &host, Some(0)),
            BaselineMaturity::HighNoise
        );
        assert_eq!(
            classify_baseline(10, Some(0.03), &host, Some(31)),
            BaselineMaturity::Stale
        );
        assert_eq!(
            classify_baseline(2, Some(0.03), &host, Some(0)),
            BaselineMaturity::New
        );
        assert_eq!(
            classify_baseline(5, Some(0.03), &host, Some(0)),
            BaselineMaturity::Immature
        );
        assert_eq!(
            classify_baseline(7, None, &host, Some(0)),
            BaselineMaturity::Immature
        );
        assert_eq!(
            classify_baseline(7, Some(0.03), &host, Some(0)),
            BaselineMaturity::Mature
        );
    }
}
