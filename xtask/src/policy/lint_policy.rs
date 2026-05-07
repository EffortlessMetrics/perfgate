//! Lint policy meta-checker.
//!
//! Validates that the workspace `[lints.*]` block matches
//! `policy/clippy-lints.toml`, that every soft-staged lint has a current
//! `policy/clippy-debt.toml` entry, and that planned lints are not active
//! before their listed MSRV.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use super::common::{parse_iso_date, resolve_strict, today_utc, write_report};

const LINTS_PATH: &str = "policy/clippy-lints.toml";
const DEBT_PATH: &str = "policy/clippy-debt.toml";
const ROOT_MANIFEST: &str = "Cargo.toml";

const REPORT_MD: &str = "lint-policy.md";
const REPORT_JSON: &str = "lint-policy.json";

#[derive(Debug, Deserialize)]
struct LintsFile {
    schema_version: String,
    msrv: String,
    #[serde(default)]
    active: Vec<ActiveLint>,
    #[serde(default)]
    planned: Vec<PlannedLint>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(
    dead_code,
    reason = "reason is parsed for validation but not propagated"
)]
struct ActiveLint {
    name: String,
    group: String, // "rust" | "clippy"
    level: String, // "deny" | "warn" | "allow"
    reason: String,
}

#[derive(Debug, Deserialize, Clone)]
struct PlannedLint {
    name: String,
    level: String,
    activate_when_msrv: String,
    #[serde(default)]
    reason: String,
}

#[derive(Debug, Deserialize)]
struct DebtFile {
    schema_version: String,
    #[serde(default)]
    debt: Vec<DebtEntry>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(
    dead_code,
    reason = "reason is parsed for validation but not propagated"
)]
struct DebtEntry {
    lint: String,
    current_level: String,
    target_level: String,
    owner: String,
    reason: String,
    expires: String,
}

#[derive(Debug, Serialize)]
struct Report<'a> {
    schema_version: &'a str,
    msrv: &'a str,
    summary: ReportSummary,
    workspace_mismatches: &'a [String],
    debt_violations: &'a [String],
    expired_debt: &'a [String],
    stale_debt: &'a [String],
    early_planned: &'a [String],
}

#[derive(Debug, Serialize)]
struct ReportSummary {
    active_lints: usize,
    planned_lints: usize,
    debt_entries: usize,
    workspace_mismatches: usize,
    debt_violations: usize,
    expired: usize,
    stale: usize,
    early_planned: usize,
}

pub fn run(strict_flag: bool) -> Result<()> {
    let strict = resolve_strict(strict_flag);
    let repo = Path::new(".");
    let lints = load_lints(repo)?;
    let debt = load_debt(repo)?;
    let workspace = load_workspace_lints(repo)?;

    let workspace_mismatches = compare_workspace(&lints.active, &debt, &workspace);
    let (debt_violations, stale_debt) = compare_debt(&lints.active, &debt, &workspace);
    let expired_debt = expired_debt(&debt)?;
    let early_planned = early_planned(&lints.planned, &lints.msrv);

    let summary = ReportSummary {
        active_lints: lints.active.len(),
        planned_lints: lints.planned.len(),
        debt_entries: debt.len(),
        workspace_mismatches: workspace_mismatches.len(),
        debt_violations: debt_violations.len(),
        expired: expired_debt.len(),
        stale: stale_debt.len(),
        early_planned: early_planned.len(),
    };

    let report = Report {
        schema_version: &lints.schema_version,
        msrv: &lints.msrv,
        summary: ReportSummary { ..summary },
        workspace_mismatches: &workspace_mismatches,
        debt_violations: &debt_violations,
        expired_debt: &expired_debt,
        stale_debt: &stale_debt,
        early_planned: &early_planned,
    };

    let json = serde_json::to_string_pretty(&report)?;
    write_report(REPORT_JSON, &json)?;
    let md = render_markdown(&report);
    write_report(REPORT_MD, &md)?;

    println!(
        "  ..  lint-policy: {} active, {} planned, {} debt ({} mismatches, {} debt-violations, {} expired, {} stale, {} early-planned)",
        lints.active.len(),
        lints.planned.len(),
        debt.len(),
        workspace_mismatches.len(),
        debt_violations.len(),
        expired_debt.len(),
        stale_debt.len(),
        early_planned.len()
    );

    let blocking = !workspace_mismatches.is_empty()
        || !debt_violations.is_empty()
        || !expired_debt.is_empty()
        || !stale_debt.is_empty()
        || !early_planned.is_empty();

    if strict && blocking {
        bail!("lint-policy violations detected; see target/perfgate/reports/lint-policy.md");
    }
    if !strict && blocking {
        println!(
            "  WARN lint-policy is advisory; rerun with --strict or PERFGATE_POLICY_STRICT=1 to enforce."
        );
    } else if !blocking {
        println!("  OK  lint-policy clean");
    }
    Ok(())
}

fn load_lints(repo: &Path) -> Result<LintsFile> {
    let p = repo.join(LINTS_PATH);
    let raw = std::fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
    let parsed: LintsFile =
        toml::from_str(&raw).with_context(|| format!("parse {}", p.display()))?;
    if parsed.schema_version != "1.0" {
        bail!(
            "{}: schema_version must be \"1.0\" (got {:?})",
            p.display(),
            parsed.schema_version
        );
    }
    Ok(parsed)
}

fn load_debt(repo: &Path) -> Result<Vec<DebtEntry>> {
    let p = repo.join(DEBT_PATH);
    if !p.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
    let parsed: DebtFile =
        toml::from_str(&raw).with_context(|| format!("parse {}", p.display()))?;
    if parsed.schema_version != "1.0" {
        bail!(
            "{}: schema_version must be \"1.0\" (got {:?})",
            p.display(),
            parsed.schema_version
        );
    }
    Ok(parsed.debt)
}

/// Parsed `[workspace.lints.<group>.<name>] = level` table, flattened to
/// `"<group>::<name>" -> "<level>"`.
fn load_workspace_lints(repo: &Path) -> Result<BTreeMap<String, String>> {
    let p = repo.join(ROOT_MANIFEST);
    let raw = std::fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
    let v: toml::Value = toml::from_str(&raw)?;
    let mut out = BTreeMap::new();
    let Some(workspace) = v.get("workspace") else {
        return Ok(out);
    };
    let Some(lints) = workspace.get("lints") else {
        return Ok(out);
    };
    let table = match lints.as_table() {
        Some(t) => t,
        None => return Ok(out),
    };
    for (group, value) in table {
        let Some(group_table) = value.as_table() else {
            continue;
        };
        for (name, level_value) in group_table {
            let level = level_str(level_value);
            if level == "<unknown>" {
                continue;
            }
            // group "all = warn" expands into a category-priority shape; ignore
            // when the level entry is itself an inline table without a "level"
            // key (priority-only entries).
            out.insert(format!("{group}::{name}"), level);
        }
    }
    Ok(out)
}

fn level_str(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Table(t) => t
            .get("level")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "<unknown>".to_string()),
        _ => "<unknown>".to_string(),
    }
}

fn compare_workspace(
    active: &[ActiveLint],
    debt: &[DebtEntry],
    workspace: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut out = Vec::new();
    for lint in active {
        let key = format!("{}::{}", lint.group, lint.name);
        // If this lint has a debt entry, the workspace must match
        // `debt.current_level` (which may be softer than `policy.level`); the
        // debt entry itself is the receipt for that gap.
        let expected = debt
            .iter()
            .find(|d| debt_matches_active(d, lint))
            .map(|d| d.current_level.as_str())
            .unwrap_or(lint.level.as_str());
        match workspace.get(&key) {
            None => out.push(format!(
                "{key}: declared in policy/clippy-lints.toml but not in [workspace.lints.{}] of Cargo.toml",
                lint.group
            )),
            Some(wlevel) => {
                if !levels_at_least(wlevel, expected) {
                    out.push(format!(
                        "{key}: workspace level {:?} is softer than expected {:?}",
                        wlevel, expected
                    ));
                }
            }
        }
    }
    out
}

fn debt_matches_active(d: &DebtEntry, lint: &ActiveLint) -> bool {
    let want = strip_group_prefix(&d.lint);
    want == lint.name && (lint.group == "clippy" || !d.lint.starts_with("clippy::"))
}

/// Returns true iff `actual` is at least as strict as `target`.
fn levels_at_least(actual: &str, target: &str) -> bool {
    fn rank(s: &str) -> u8 {
        match s {
            "allow" => 0,
            "warn" => 1,
            "deny" => 2,
            "forbid" => 3,
            _ => 0,
        }
    }
    rank(actual) >= rank(target)
}

fn compare_debt(
    active: &[ActiveLint],
    debt: &[DebtEntry],
    workspace: &BTreeMap<String, String>,
) -> (Vec<String>, Vec<String>) {
    let mut violations = Vec::new();
    let mut stale = Vec::new();
    for entry in debt {
        let want_name = strip_group_prefix(&entry.lint);
        let active_match = active.iter().find(|a| {
            a.name == want_name && (a.group == "clippy" || entry.lint.starts_with("clippy::"))
        });
        let Some(active_lint) = active_match else {
            violations.push(format!(
                "{}: debt entry references lint not in policy/clippy-lints.toml",
                entry.lint
            ));
            continue;
        };
        // target_level must be at least as strict as current_level — debt
        // ratchets toward strictness, never away.
        if !levels_at_least(&entry.target_level, &entry.current_level) {
            violations.push(format!(
                "{}: debt target_level={:?} is softer than current_level={:?}",
                entry.lint, entry.target_level, entry.current_level
            ));
        }
        // target_level must reach the policy's declared active level — a debt
        // entry is the receipt for the gap, not a relaxation of the goal.
        if !levels_at_least(&entry.target_level, &active_lint.level) {
            violations.push(format!(
                "{}: debt target_level={:?} is softer than policy level {:?}",
                entry.lint, entry.target_level, active_lint.level
            ));
        }
        // If the workspace already ships at target_level (debt paid), the
        // entry is stale and should be removed.
        let key = format!("clippy::{}", strip_clippy(&entry.lint));
        if let Some(wlevel) = workspace.get(&key)
            && levels_at_least(wlevel, &entry.target_level)
            && wlevel != &entry.current_level
        {
            stale.push(format!(
                "{}: debt entry no longer needed (workspace already at {:?}, target {:?})",
                entry.lint, wlevel, entry.target_level
            ));
        }
    }
    (violations, stale)
}

fn strip_clippy(s: &str) -> String {
    s.strip_prefix("clippy::").unwrap_or(s).to_string()
}

fn strip_group_prefix(s: &str) -> String {
    s.split("::").last().unwrap_or(s).to_string()
}

fn expired_debt(debt: &[DebtEntry]) -> Result<Vec<String>> {
    let today = today_utc();
    let mut out = Vec::new();
    for d in debt {
        let exp = parse_iso_date(&d.expires)
            .with_context(|| format!("debt {} has invalid expires", d.lint))?;
        if exp < today {
            out.push(format!(
                "{}: expired {} (owner: {})",
                d.lint, d.expires, d.owner
            ));
        }
    }
    Ok(out)
}

fn early_planned(planned: &[PlannedLint], msrv: &str) -> Vec<String> {
    let mut out = Vec::new();
    for p in planned {
        if msrv_ge(msrv, &p.activate_when_msrv) {
            out.push(format!(
                "{}: planned for MSRV {} but workspace MSRV is already {}",
                p.name, p.activate_when_msrv, msrv
            ));
        }
        let _ = &p.level;
        let _ = &p.reason;
    }
    out
}

fn msrv_ge(actual: &str, target: &str) -> bool {
    fn parts(s: &str) -> (u32, u32, u32) {
        let mut it = s.split('.');
        let a = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let b = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let c = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        (a, b, c)
    }
    parts(actual) >= parts(target)
}

#[expect(
    clippy::let_underscore_must_use,
    reason = "writeln! to a String is infallible; the Result must be observed but is unconditionally Ok"
)]
fn render_markdown(report: &Report<'_>) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(&mut s, "# Lint Policy Report\n");
    let _ = writeln!(&mut s, "* MSRV: **{}**", report.msrv);
    let _ = writeln!(
        &mut s,
        "* Active lints: **{}**",
        report.summary.active_lints
    );
    let _ = writeln!(
        &mut s,
        "* Planned lints: **{}**",
        report.summary.planned_lints
    );
    let _ = writeln!(
        &mut s,
        "* Debt entries: **{}**",
        report.summary.debt_entries
    );
    let _ = writeln!(
        &mut s,
        "* Workspace mismatches: **{}**",
        report.summary.workspace_mismatches
    );
    let _ = writeln!(
        &mut s,
        "* Debt schema violations: **{}**",
        report.summary.debt_violations
    );
    let _ = writeln!(&mut s, "* Expired debt: **{}**", report.summary.expired);
    let _ = writeln!(&mut s, "* Stale debt: **{}**", report.summary.stale);
    let _ = writeln!(
        &mut s,
        "* Planned lints active too early: **{}**\n",
        report.summary.early_planned
    );

    if !report.workspace_mismatches.is_empty() {
        let _ = writeln!(&mut s, "## Workspace mismatches\n");
        for m in report.workspace_mismatches {
            let _ = writeln!(&mut s, "* {m}");
        }
        let _ = writeln!(&mut s);
    }
    if !report.debt_violations.is_empty() {
        let _ = writeln!(&mut s, "## Debt schema violations\n");
        for m in report.debt_violations {
            let _ = writeln!(&mut s, "* {m}");
        }
        let _ = writeln!(&mut s);
    }
    if !report.expired_debt.is_empty() {
        let _ = writeln!(&mut s, "## Expired debt\n");
        for m in report.expired_debt {
            let _ = writeln!(&mut s, "* {m}");
        }
        let _ = writeln!(&mut s);
    }
    if !report.stale_debt.is_empty() {
        let _ = writeln!(&mut s, "## Stale debt\n");
        for m in report.stale_debt {
            let _ = writeln!(&mut s, "* {m}");
        }
        let _ = writeln!(&mut s);
    }
    if !report.early_planned.is_empty() {
        let _ = writeln!(&mut s, "## Planned lints active too early\n");
        for m in report.early_planned {
            let _ = writeln!(&mut s, "* {m}");
        }
        let _ = writeln!(&mut s);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_ordering() {
        assert!(levels_at_least("deny", "warn"));
        assert!(levels_at_least("warn", "warn"));
        assert!(!levels_at_least("warn", "deny"));
        assert!(levels_at_least("forbid", "deny"));
    }

    #[test]
    fn msrv_ordering() {
        assert!(msrv_ge("1.93", "1.93"));
        assert!(msrv_ge("1.93", "1.92"));
        assert!(!msrv_ge("1.92", "1.93"));
        assert!(msrv_ge("1.94.1", "1.94"));
    }
}
