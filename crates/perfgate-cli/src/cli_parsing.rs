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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Display;

    type TestResult<T = ()> = anyhow::Result<T>;

    fn ok_value<T, E: Display>(result: Result<T, E>) -> TestResult<T> {
        result.map_err(|err| anyhow::anyhow!("{err}"))
    }

    fn err_string<T, E: Display>(result: Result<T, E>) -> TestResult<String> {
        match result {
            Ok(_) => anyhow::bail!("expected error"),
            Err(err) => Ok(err.to_string()),
        }
    }

    fn some_value<T>(value: Option<T>, context: &'static str) -> TestResult<T> {
        value.ok_or_else(|| anyhow::anyhow!(context))
    }

    #[test]
    fn parse_duration_accepts_humantime_value() -> TestResult {
        let d = ok_value(parse_duration("1500ms"))?;
        assert_eq!(d, Duration::from_millis(1500));
        let d = ok_value(parse_duration("2s"))?;
        assert_eq!(d, Duration::from_secs(2));
        Ok(())
    }

    #[test]
    fn parse_duration_rejects_garbage() -> TestResult {
        let err = err_string(parse_duration("not-a-duration"))?;
        assert!(err.contains("invalid duration"), "got: {err}");
        Ok(())
    }

    #[test]
    fn parse_key_val_string_splits_on_first_equal() -> TestResult {
        let (k, v) = ok_value(parse_key_val_string("FOO=bar=baz"))?;
        assert_eq!(k, "FOO");
        assert_eq!(v, "bar=baz");
        Ok(())
    }

    #[test]
    fn parse_key_val_string_requires_equal_sign() -> TestResult {
        let err = err_string(parse_key_val_string("no-equals"))?;
        assert_eq!(err, "expected KEY=VALUE");
        Ok(())
    }

    #[test]
    fn parse_key_val_f64_parses_value_as_float() -> TestResult {
        let (k, v) = ok_value(parse_key_val_f64("p99=12.5"))?;
        assert_eq!(k, "p99");
        assert!((v - 12.5).abs() < f64::EPSILON);
        Ok(())
    }

    #[test]
    fn parse_key_val_f64_rejects_non_numeric_value() -> TestResult {
        let err = err_string(parse_key_val_f64("p99=abc"))?;
        assert!(err.contains("invalid float value"), "got: {err}");
        Ok(())
    }

    #[test]
    fn parse_key_val_f64_requires_equal_sign() -> TestResult {
        let err = err_string(parse_key_val_f64("p99"))?;
        assert_eq!(err, "expected KEY=VALUE");
        Ok(())
    }

    #[test]
    fn parse_noise_policy_round_trip_variants() -> TestResult {
        assert!(matches!(
            ok_value(parse_noise_policy("warn"))?,
            perfgate_types::NoisePolicy::Warn
        ));
        assert!(matches!(
            ok_value(parse_noise_policy("SKIP"))?,
            perfgate_types::NoisePolicy::Skip
        ));
        assert!(matches!(
            ok_value(parse_noise_policy("Ignore"))?,
            perfgate_types::NoisePolicy::Ignore
        ));
        Ok(())
    }

    #[test]
    fn parse_noise_policy_rejects_unknown_value() -> TestResult {
        let err = err_string(parse_noise_policy("loud"))?;
        assert!(err.contains("invalid noise policy"), "got: {err}");
        Ok(())
    }

    #[test]
    fn parse_flakiness_score_accepts_in_range_values() -> TestResult {
        assert_eq!(ok_value(parse_flakiness_score("0"))?, 0.0);
        assert_eq!(ok_value(parse_flakiness_score("0.5"))?, 0.5);
        assert_eq!(ok_value(parse_flakiness_score("1"))?, 1.0);
        Ok(())
    }

    #[test]
    fn parse_flakiness_score_rejects_out_of_range() {
        assert!(parse_flakiness_score("-0.1").is_err());
        assert!(parse_flakiness_score("1.1").is_err());
    }

    #[test]
    fn parse_flakiness_score_rejects_non_finite() {
        assert!(parse_flakiness_score("NaN").is_err());
        assert!(parse_flakiness_score("inf").is_err());
    }

    #[test]
    fn parse_flakiness_score_rejects_non_numeric() -> TestResult {
        let err = err_string(parse_flakiness_score("noisy"))?;
        assert!(err.contains("must be a number"), "got: {err}");
        Ok(())
    }

    #[test]
    fn parse_verdict_status_handles_all_variants_case_insensitively() {
        assert!(matches!(
            parse_verdict_status("pass"),
            Ok(VerdictStatus::Pass)
        ));
        assert!(matches!(
            parse_verdict_status("WARN"),
            Ok(VerdictStatus::Warn)
        ));
        assert!(matches!(
            parse_verdict_status("Fail"),
            Ok(VerdictStatus::Fail)
        ));
        assert!(matches!(
            parse_verdict_status("skip"),
            Ok(VerdictStatus::Skip)
        ));
        assert!(parse_verdict_status("blocked").is_err());
    }

    #[test]
    fn parse_metric_status_handles_all_variants_case_insensitively() {
        assert!(matches!(
            parse_metric_status("pass"),
            Ok(MetricStatus::Pass)
        ));
        assert!(matches!(
            parse_metric_status("WARN"),
            Ok(MetricStatus::Warn)
        ));
        assert!(matches!(
            parse_metric_status("Fail"),
            Ok(MetricStatus::Fail)
        ));
        assert!(matches!(
            parse_metric_status("skip"),
            Ok(MetricStatus::Skip)
        ));
        assert!(parse_metric_status("unknown").is_err());
    }

    #[test]
    fn parse_host_mismatch_policy_aliases_fail_to_error() {
        assert!(matches!(
            parse_host_mismatch_policy("warn"),
            Ok(HostMismatchPolicy::Warn)
        ));
        assert!(matches!(
            parse_host_mismatch_policy("error"),
            Ok(HostMismatchPolicy::Error)
        ));
        assert!(matches!(
            parse_host_mismatch_policy("fail"),
            Ok(HostMismatchPolicy::Error)
        ));
        assert!(matches!(
            parse_host_mismatch_policy("ignore"),
            Ok(HostMismatchPolicy::Ignore)
        ));
        assert!(
            parse_host_mismatch_policy("WARN").is_err(),
            "policy is case-sensitive"
        );
        assert!(parse_host_mismatch_policy("bogus").is_err());
    }

    #[test]
    fn parse_aggregation_policy_covers_all_variants() {
        assert!(matches!(
            parse_aggregation_policy("all"),
            Ok(AggregationPolicy::All)
        ));
        assert!(matches!(
            parse_aggregation_policy("majority"),
            Ok(AggregationPolicy::Majority)
        ));
        assert!(matches!(
            parse_aggregation_policy("weighted"),
            Ok(AggregationPolicy::Weighted)
        ));
        assert!(matches!(
            parse_aggregation_policy("quorum"),
            Ok(AggregationPolicy::Quorum)
        ));
        assert!(matches!(
            parse_aggregation_policy("fail_if_n_of_m"),
            Ok(AggregationPolicy::FailIfNOfM)
        ));
        assert!(parse_aggregation_policy("nope").is_err());
    }

    #[test]
    fn parse_aggregate_weight_mode_covers_variants() {
        assert!(matches!(
            parse_aggregate_weight_mode("configured"),
            Ok(AggregateWeightMode::Configured)
        ));
        assert!(matches!(
            parse_aggregate_weight_mode("inverse_variance"),
            Ok(AggregateWeightMode::InverseVariance)
        ));
        assert!(parse_aggregate_weight_mode("other").is_err());
    }

    #[test]
    fn parse_weight_map_handles_multiple_entries_and_trims_labels() -> TestResult {
        let input = vec!["foo=1.0".into(), " bar =2.5".into()];
        let map = ok_value(parse_weight_map(&input))?;
        assert_eq!(map.len(), 2);
        assert_eq!(map["foo"], 1.0);
        assert_eq!(map["bar"], 2.5);
        Ok(())
    }

    #[test]
    fn parse_weight_map_rejects_missing_equals() -> TestResult {
        let err = err_string(parse_weight_map(&["foo".into()]))?;
        assert!(err.contains("expected label=value"), "got: {err}");
        Ok(())
    }

    #[test]
    fn parse_weight_map_rejects_empty_label() -> TestResult {
        let err = err_string(parse_weight_map(&["=1.0".into()]))?;
        assert!(err.contains("label cannot be empty"), "got: {err}");
        let err = err_string(parse_weight_map(&["   =1.0".into()]))?;
        assert!(err.contains("label cannot be empty"), "got: {err}");
        Ok(())
    }

    #[test]
    fn parse_weight_map_rejects_non_numeric_weight() -> TestResult {
        let err = err_string(parse_weight_map(&["foo=bad".into()]))?;
        assert!(err.contains("weight must be a number"), "got: {err}");
        Ok(())
    }

    #[test]
    fn parse_weight_map_rejects_negative_and_nonfinite() -> TestResult {
        let err = err_string(parse_weight_map(&["foo=-1".into()]))?;
        assert!(err.contains("non-negative"), "got: {err}");
        let err = err_string(parse_weight_map(&["foo=NaN".into()]))?;
        assert!(err.contains("non-negative"), "got: {err}");
        Ok(())
    }

    #[test]
    fn parse_weight_map_empty_input_returns_empty_map() -> TestResult {
        let map = ok_value(parse_weight_map(&[]))?;
        assert!(map.is_empty());
        Ok(())
    }

    #[test]
    fn validate_aggregate_options_accepts_default_all_policy() -> TestResult {
        let (q, fnm, vf) = validate_aggregate_options(
            AggregationPolicy::All,
            AggregateWeightMode::Configured,
            None,
            None,
            None,
            None,
        )
        .map_err(|err| anyhow::anyhow!("{err}"))?;
        assert!(q.is_none());
        assert!(fnm.is_none());
        assert!(vf.is_none());
        Ok(())
    }

    #[test]
    fn validate_aggregate_options_rejects_quorum_out_of_range() -> TestResult {
        let err = err_string(validate_aggregate_options(
            AggregationPolicy::Weighted,
            AggregateWeightMode::Configured,
            Some(1.5),
            None,
            None,
            None,
        ))?;
        assert!(err.contains("--quorum must be between"), "got: {err}");

        let err = err_string(validate_aggregate_options(
            AggregationPolicy::Weighted,
            AggregateWeightMode::Configured,
            Some(f64::NAN),
            None,
            None,
            None,
        ))?;
        assert!(err.contains("--quorum must be between"), "got: {err}");
        Ok(())
    }

    #[test]
    fn validate_aggregate_options_quorum_requires_weighted_or_quorum_policy() -> TestResult {
        let err = err_string(validate_aggregate_options(
            AggregationPolicy::All,
            AggregateWeightMode::Configured,
            Some(0.5),
            None,
            None,
            None,
        ))?;
        assert!(err.contains("--quorum requires"), "got: {err}");
        Ok(())
    }

    #[test]
    fn validate_aggregate_options_inverse_variance_requires_weighted() -> TestResult {
        let err = err_string(validate_aggregate_options(
            AggregationPolicy::Majority,
            AggregateWeightMode::InverseVariance,
            None,
            None,
            None,
            None,
        ))?;
        assert!(err.contains("inverse_variance requires"), "got: {err}");
        Ok(())
    }

    #[test]
    fn validate_aggregate_options_variance_floor_rules() -> TestResult {
        let err = err_string(validate_aggregate_options(
            AggregationPolicy::Weighted,
            AggregateWeightMode::InverseVariance,
            None,
            None,
            None,
            Some(0.0),
        ))?;
        assert!(err.contains("variance-floor"), "got: {err}");

        let err = err_string(validate_aggregate_options(
            AggregationPolicy::Weighted,
            AggregateWeightMode::Configured,
            None,
            None,
            None,
            Some(1.0),
        ))?;
        assert!(err.contains("inverse_variance"), "got: {err}");
        Ok(())
    }

    #[test]
    fn validate_aggregate_options_fail_if_n_of_m_requires_fail_n() -> TestResult {
        let err = err_string(validate_aggregate_options(
            AggregationPolicy::FailIfNOfM,
            AggregateWeightMode::Configured,
            None,
            None,
            None,
            None,
        ))?;
        assert!(err.contains("--fail-n"), "got: {err}");
        Ok(())
    }

    #[test]
    fn validate_aggregate_options_fail_if_n_of_m_rejects_zero_or_inverted() -> TestResult {
        let err = err_string(validate_aggregate_options(
            AggregationPolicy::FailIfNOfM,
            AggregateWeightMode::Configured,
            None,
            Some(0),
            None,
            None,
        ))?;
        assert!(err.contains("--fail-n must be at least 1"), "got: {err}");

        let err = err_string(validate_aggregate_options(
            AggregationPolicy::FailIfNOfM,
            AggregateWeightMode::Configured,
            None,
            Some(3),
            Some(0),
            None,
        ))?;
        assert!(err.contains("--fail-m must be at least 1"), "got: {err}");

        let err = err_string(validate_aggregate_options(
            AggregationPolicy::FailIfNOfM,
            AggregateWeightMode::Configured,
            None,
            Some(5),
            Some(3),
            None,
        ))?;
        assert!(err.contains("must be greater than or equal"), "got: {err}");
        Ok(())
    }

    #[test]
    fn validate_aggregate_options_fail_if_n_of_m_success_path() -> TestResult {
        let (q, fnm, vf) = validate_aggregate_options(
            AggregationPolicy::FailIfNOfM,
            AggregateWeightMode::Configured,
            None,
            Some(2),
            Some(5),
            None,
        )
        .map_err(|err| anyhow::anyhow!("{err}"))?;
        assert!(q.is_none());
        let fnm = some_value(fnm, "missing FailIfNOfM result")?;
        assert_eq!(fnm.n, 2);
        assert_eq!(fnm.m, Some(5));
        assert!(vf.is_none());
        Ok(())
    }

    #[test]
    fn validate_aggregate_options_fail_n_or_m_outside_correct_policy_errors() -> TestResult {
        let err = err_string(validate_aggregate_options(
            AggregationPolicy::All,
            AggregateWeightMode::Configured,
            None,
            Some(1),
            None,
            None,
        ))?;
        assert!(err.contains("fail_if_n_of_m"), "got: {err}");
        Ok(())
    }

    #[test]
    fn validate_aggregate_options_inverse_variance_success_passes_floor_through() -> TestResult {
        let (q, fnm, vf) = validate_aggregate_options(
            AggregationPolicy::Weighted,
            AggregateWeightMode::InverseVariance,
            Some(0.6),
            None,
            None,
            Some(2.5),
        )
        .map_err(|err| anyhow::anyhow!("{err}"))?;
        assert_eq!(q, Some(0.6));
        assert!(fnm.is_none());
        assert_eq!(vf, Some(2.5));
        Ok(())
    }

    #[test]
    fn parse_significance_alpha_accepts_in_range() -> TestResult {
        assert_eq!(ok_value(parse_significance_alpha("0.05"))?, 0.05);
        assert_eq!(ok_value(parse_significance_alpha("0"))?, 0.0);
        assert_eq!(ok_value(parse_significance_alpha("1"))?, 1.0);
        Ok(())
    }

    #[test]
    fn parse_significance_alpha_rejects_invalid_values() {
        assert!(parse_significance_alpha("abc").is_err());
        assert!(parse_significance_alpha("1.5").is_err());
        assert!(parse_significance_alpha("-0.1").is_err());
    }

    #[test]
    fn normalize_paired_cli_command_requires_nonempty_input() -> TestResult {
        let err = err_string(normalize_paired_cli_command(vec![], "--current-cmd"))?;
        assert!(err.contains("--current-cmd"), "got: {err}");
        assert!(err.contains("at least one argument"), "got: {err}");
        Ok(())
    }

    #[test]
    fn normalize_paired_cli_command_splits_quoted_single_arg() -> TestResult {
        let out =
            normalize_paired_cli_command(vec!["echo \"hello world\"".into()], "--current-cmd")
                .map_err(|err| anyhow::anyhow!("{err}"))?;
        assert_eq!(out, vec!["echo".to_string(), "hello world".to_string()]);
        Ok(())
    }

    #[test]
    fn normalize_paired_cli_command_passes_through_multi_arg_form() -> TestResult {
        let out = normalize_paired_cli_command(
            vec!["echo".into(), "hello world".into()],
            "--baseline-cmd",
        )
        .map_err(|err| anyhow::anyhow!("{err}"))?;
        assert_eq!(out, vec!["echo".to_string(), "hello world".to_string()]);
        Ok(())
    }

    #[test]
    fn normalize_paired_cli_command_single_arg_without_whitespace_returns_as_is() -> TestResult {
        let out = normalize_paired_cli_command(vec!["./bench".into()], "--baseline-cmd")
            .map_err(|err| anyhow::anyhow!("{err}"))?;
        assert_eq!(out, vec!["./bench".to_string()]);
        Ok(())
    }

    #[test]
    fn normalize_paired_cli_command_empty_quoted_string_is_an_error() -> TestResult {
        // shell_words::split("   ") yields an empty Vec, which must trigger the empty-parse error
        let err = err_string(normalize_paired_cli_command(
            vec!["   ".into()],
            "--current-cmd",
        ))?;
        assert!(err.contains("parsed to an empty command"), "got: {err}");
        Ok(())
    }

    #[test]
    fn normalize_paired_cli_command_reports_invalid_shell_string() -> TestResult {
        let err = err_string(normalize_paired_cli_command(
            vec!["echo \"unterminated".into()],
            "--current-cmd",
        ))?;
        assert!(err.contains("failed to parse"), "got: {err}");
        Ok(())
    }
}
