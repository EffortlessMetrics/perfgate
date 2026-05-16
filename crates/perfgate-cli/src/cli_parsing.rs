//! CLI argument parsing helpers.

use anyhow::Context;
use perfgate_types::{
    AggregateWeightMode, AggregationPolicy, FailIfNOfM, HostMismatchPolicy, MetricStatus,
    VerdictStatus,
};
use std::collections::BTreeMap;
use std::time::Duration;

pub fn parse_duration(s: &str) -> anyhow::Result<Duration> {
    let d = humantime::parse_duration(s).with_context(|| format!("invalid duration: {s}"))?;
    Ok(d)
}

pub fn parse_key_val_string(s: &str) -> Result<(String, String), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| "expected KEY=VALUE".to_string())?;
    Ok((k.to_string(), v.to_string()))
}

pub fn parse_key_val_f64(s: &str) -> Result<(String, f64), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| "expected KEY=VALUE".to_string())?;
    let f: f64 = v.parse().map_err(|_| format!("invalid float value: {v}"))?;
    Ok((k.to_string(), f))
}

pub fn parse_noise_policy(s: &str) -> Result<perfgate_types::NoisePolicy, String> {
    match s.to_lowercase().as_str() {
        "warn" => Ok(perfgate_types::NoisePolicy::Warn),
        "skip" => Ok(perfgate_types::NoisePolicy::Skip),
        "ignore" => Ok(perfgate_types::NoisePolicy::Ignore),
        _ => Err(format!(
            "invalid noise policy: {s} (expected warn|skip|ignore)"
        )),
    }
}

pub fn parse_flakiness_score(s: &str) -> Result<f64, String> {
    let score: f64 = s
        .parse()
        .map_err(|_| "flakiness score must be a number".to_string())?;
    if !score.is_finite() || !(0.0..=1.0).contains(&score) {
        return Err("flakiness score must be between 0.0 and 1.0".to_string());
    }
    Ok(score)
}

pub fn parse_verdict_status(s: &str) -> Result<VerdictStatus, String> {
    match s.to_lowercase().as_str() {
        "pass" => Ok(VerdictStatus::Pass),
        "warn" => Ok(VerdictStatus::Warn),
        "fail" => Ok(VerdictStatus::Fail),
        "skip" => Ok(VerdictStatus::Skip),
        _ => Err(format!(
            "invalid verdict status: {s} (expected pass|warn|fail|skip)"
        )),
    }
}

pub fn parse_metric_status(s: &str) -> Result<MetricStatus, String> {
    match s.to_lowercase().as_str() {
        "pass" => Ok(MetricStatus::Pass),
        "warn" => Ok(MetricStatus::Warn),
        "fail" => Ok(MetricStatus::Fail),
        "skip" => Ok(MetricStatus::Skip),
        _ => Err(format!(
            "invalid metric status: {s} (expected pass|warn|fail|skip)"
        )),
    }
}

pub fn parse_host_mismatch_policy(s: &str) -> Result<HostMismatchPolicy, String> {
    match s {
        "warn" => Ok(HostMismatchPolicy::Warn),
        "error" | "fail" => Ok(HostMismatchPolicy::Error),
        "ignore" => Ok(HostMismatchPolicy::Ignore),
        _ => Err(format!(
            "invalid host mismatch policy: {} (expected warn, error, or ignore)",
            s
        )),
    }
}

pub fn parse_aggregation_policy(s: &str) -> Result<AggregationPolicy, String> {
    match s {
        "all" => Ok(AggregationPolicy::All),
        "majority" => Ok(AggregationPolicy::Majority),
        "weighted" => Ok(AggregationPolicy::Weighted),
        "quorum" => Ok(AggregationPolicy::Quorum),
        "fail_if_n_of_m" => Ok(AggregationPolicy::FailIfNOfM),
        _ => Err(format!(
            "invalid aggregation policy: {s} (expected all|majority|weighted|quorum|fail_if_n_of_m)"
        )),
    }
}

pub fn parse_aggregate_weight_mode(s: &str) -> Result<AggregateWeightMode, String> {
    match s {
        "configured" => Ok(AggregateWeightMode::Configured),
        "inverse_variance" => Ok(AggregateWeightMode::InverseVariance),
        _ => Err(format!(
            "invalid aggregate weight mode: {s} (expected configured|inverse_variance)"
        )),
    }
}

pub fn parse_weight_map(weights: &[String]) -> anyhow::Result<BTreeMap<String, f64>> {
    let mut map = BTreeMap::new();
    for raw in weights {
        let (label, weight_raw) = raw
            .split_once('=')
            .ok_or_else(|| anyhow::anyhow!("invalid --weight '{raw}', expected label=value"))?;
        if label.trim().is_empty() {
            anyhow::bail!("invalid --weight '{raw}': label cannot be empty");
        }
        let weight: f64 = weight_raw
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid --weight '{raw}': weight must be a number"))?;
        if !weight.is_finite() || weight < 0.0 {
            anyhow::bail!("invalid --weight '{raw}': weight must be a non-negative finite number");
        }
        map.insert(label.trim().to_string(), weight);
    }
    Ok(map)
}

pub fn validate_aggregate_options(
    policy: AggregationPolicy,
    weight_mode: AggregateWeightMode,
    quorum: Option<f64>,
    fail_n: Option<u32>,
    fail_m: Option<u32>,
    variance_floor: Option<f64>,
) -> anyhow::Result<(Option<f64>, Option<FailIfNOfM>, Option<f64>)> {
    if let Some(quorum) = quorum {
        if !quorum.is_finite() || !(0.0..=1.0).contains(&quorum) {
            anyhow::bail!("--quorum must be between 0.0 and 1.0, got {quorum}");
        }
        if !matches!(
            policy,
            AggregationPolicy::Weighted | AggregationPolicy::Quorum
        ) {
            anyhow::bail!("--quorum requires --policy weighted or quorum");
        }
    }

    if matches!(weight_mode, AggregateWeightMode::InverseVariance)
        && !matches!(policy, AggregationPolicy::Weighted)
    {
        anyhow::bail!("--weight-mode inverse_variance requires --policy weighted");
    }

    if let Some(variance_floor) = variance_floor {
        if !variance_floor.is_finite() || variance_floor <= 0.0 {
            anyhow::bail!(
                "--variance-floor must be a positive finite number, got {variance_floor}"
            );
        }
        if !matches!(weight_mode, AggregateWeightMode::InverseVariance) {
            anyhow::bail!("--variance-floor requires --weight-mode inverse_variance");
        }
    }

    match policy {
        AggregationPolicy::FailIfNOfM => {
            let n = fail_n
                .ok_or_else(|| anyhow::anyhow!("--policy fail_if_n_of_m requires --fail-n"))?;
            if n == 0 {
                anyhow::bail!("--fail-n must be at least 1");
            }
            if let Some(m) = fail_m {
                if m == 0 {
                    anyhow::bail!("--fail-m must be at least 1");
                }
                if m < n {
                    anyhow::bail!("--fail-m must be greater than or equal to --fail-n");
                }
            }
            Ok((quorum, Some(FailIfNOfM { n, m: fail_m }), variance_floor))
        }
        _ => {
            if fail_n.is_some() || fail_m.is_some() {
                anyhow::bail!("--fail-n and --fail-m require --policy fail_if_n_of_m");
            }
            Ok((quorum, None, variance_floor))
        }
    }
}

pub fn parse_significance_alpha(s: &str) -> Result<f64, String> {
    let alpha: f64 = s.parse().map_err(|_| format!("invalid float value: {s}"))?;
    if !(0.0..=1.0).contains(&alpha) {
        return Err(format!(
            "significance alpha must be between 0.0 and 1.0, got {alpha}"
        ));
    }
    Ok(alpha)
}

pub fn normalize_paired_cli_command(
    args: Vec<String>,
    flag_name: &str,
) -> anyhow::Result<Vec<String>> {
    if args.is_empty() {
        anyhow::bail!("{} requires at least one argument", flag_name);
    }

    if args.len() == 1 && args[0].chars().any(char::is_whitespace) {
        let raw = &args[0];
        let parsed = shell_words::split(raw)
            .with_context(|| format!("failed to parse {} shell string: {}", flag_name, raw))?;
        if parsed.is_empty() {
            anyhow::bail!("{} parsed to an empty command", flag_name);
        }
        return Ok(parsed);
    }

    Ok(args)
}
