//! Non-Rust file policy checker.

use anyhow::{Context, Result, bail};
use glob::{MatchOptions, Pattern};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

use super::common::{list_tracked_files, parse_iso_date, resolve_strict, today_utc, write_report};

const ALLOWLIST_PATH: &str = "policy/non-rust-allowlist.toml";
const REPORT_MD: &str = "file-policy.md";
const REPORT_JSON: &str = "file-policy.json";

/// File extensions implicitly classified as Rust (no allowlist required).
const IMPLICIT_RUST_EXT: &[&str] = &["rs"];

/// Filenames implicitly classified as Cargo metadata.
const CARGO_METADATA_FILES: &[&str] = &["Cargo.toml", "Cargo.lock"];

#[derive(Debug, Deserialize)]
struct AllowlistFile {
    schema_version: String,
    #[serde(default)]
    allow: Vec<AllowEntry>,
}

#[derive(Debug, Deserialize, Clone)]
struct AllowEntry {
    #[serde(default)]
    glob: Option<String>,
    #[serde(default)]
    path: Option<String>,
    kind: String,
    owner: String,
    surface: String,
    classification: String,
    reason: String,
    #[serde(default)]
    covered_by: Vec<String>,
    #[serde(default)]
    generated_by: Option<String>,
    #[serde(default)]
    expires: Option<String>,
    #[serde(default)]
    retired: bool,
}

#[derive(Debug, Serialize)]
struct Report<'a> {
    schema_version: &'a str,
    summary: ReportSummary,
    unallowlisted: &'a [String],
    stale_globs: &'a [String],
    expired_entries: &'a [String],
    missing_required: &'a [String],
}

#[derive(Debug, Serialize)]
struct ReportSummary {
    total_files: usize,
    rust_or_cargo: usize,
    matched: usize,
    unallowlisted: usize,
    stale_globs: usize,
    expired: usize,
    missing_required: usize,
}

pub fn run(strict_flag: bool) -> Result<()> {
    let strict = resolve_strict(strict_flag);
    let repo = Path::new(".");
    let entries = load_allowlist(repo)?;
    let files = list_tracked_files(repo)?;

    let mut total_files = 0usize;
    let mut rust_or_cargo = 0usize;
    let mut matched = 0usize;
    let mut unallowlisted: Vec<String> = Vec::new();
    let mut matched_globs: BTreeSet<usize> = BTreeSet::new();

    for f in &files {
        total_files += 1;
        if is_rust_or_cargo(f) {
            rust_or_cargo += 1;
            continue;
        }
        let mut hit = None;
        for (idx, e) in entries.iter().enumerate() {
            if entry_matches(e, f) {
                hit = Some(idx);
                break;
            }
        }
        match hit {
            Some(idx) => {
                matched += 1;
                matched_globs.insert(idx);
            }
            None => unallowlisted.push(f.display().to_string()),
        }
    }

    let stale_globs: Vec<String> = entries
        .iter()
        .enumerate()
        .filter(|(idx, e)| !matched_globs.contains(idx) && !e.retired)
        .map(|(_, e)| describe_entry(e))
        .collect();

    let expired_entries = expired_entries(&entries)?;
    let missing_required = missing_required_fields(&entries);

    let summary = ReportSummary {
        total_files,
        rust_or_cargo,
        matched,
        unallowlisted: unallowlisted.len(),
        stale_globs: stale_globs.len(),
        expired: expired_entries.len(),
        missing_required: missing_required.len(),
    };

    let report = Report {
        schema_version: "1.0",
        summary: ReportSummary { ..summary },
        unallowlisted: &unallowlisted,
        stale_globs: &stale_globs,
        expired_entries: &expired_entries,
        missing_required: &missing_required,
    };

    let json = serde_json::to_string_pretty(&report)?;
    write_report(REPORT_JSON, &json)?;
    let md = render_markdown(&report);
    write_report(REPORT_MD, &md)?;

    println!(
        "  ..  file-policy: {} files ({} Rust/Cargo, {} matched, {} unallowlisted, {} stale, {} expired)",
        total_files,
        rust_or_cargo,
        matched,
        unallowlisted.len(),
        stale_globs.len(),
        expired_entries.len()
    );

    let blocking = !unallowlisted.is_empty()
        || !stale_globs.is_empty()
        || !expired_entries.is_empty()
        || !missing_required.is_empty();
    if strict && blocking {
        bail!(
            "file-policy violations: {} unallowlisted, {} stale, {} expired, {} missing-fields",
            unallowlisted.len(),
            stale_globs.len(),
            expired_entries.len(),
            missing_required.len()
        );
    }
    if !strict && blocking {
        println!(
            "  WARN file-policy is advisory; rerun with --strict or PERFGATE_POLICY_STRICT=1 to enforce."
        );
    } else if !blocking {
        println!("  OK  file-policy clean");
    }
    Ok(())
}

fn load_allowlist(repo: &Path) -> Result<Vec<AllowEntry>> {
    let path = repo.join(ALLOWLIST_PATH);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let parsed: AllowlistFile =
        toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    if parsed.schema_version != "1.0" {
        bail!(
            "{} schema_version must be \"1.0\" (got {:?})",
            path.display(),
            parsed.schema_version
        );
    }
    for e in &parsed.allow {
        if e.glob.is_none() && e.path.is_none() {
            bail!(
                "{}: entry must specify either `glob` or `path` (kind={:?}, owner={:?})",
                path.display(),
                e.kind,
                e.owner
            );
        }
    }
    Ok(parsed.allow)
}

fn is_rust_or_cargo(p: &Path) -> bool {
    if let Some(ext) = p.extension().and_then(|s| s.to_str())
        && IMPLICIT_RUST_EXT.contains(&ext)
    {
        return true;
    }
    if let Some(name) = p.file_name().and_then(|s| s.to_str())
        && CARGO_METADATA_FILES.contains(&name)
    {
        return true;
    }
    false
}

fn entry_matches(e: &AllowEntry, file: &Path) -> bool {
    let s = file.to_string_lossy();
    if let Some(p) = &e.path
        && p.as_str() == s.as_ref()
    {
        return true;
    }
    if let Some(g) = &e.glob
        && let Ok(pat) = Pattern::new(g)
    {
        // Critical: `require_literal_separator = true` makes `*` and `?` stop
        // at `/`, which is what every other glob system does. The `glob`
        // crate's default of `false` would let `*.md` match
        // `crates/foo/README.md`, defeating the whole "narrow receipts" point
        // of this checker.
        let opts = MatchOptions {
            case_sensitive: true,
            require_literal_separator: true,
            require_literal_leading_dot: false,
        };
        return pat.matches_with(s.as_ref(), opts);
    }
    false
}

fn describe_entry(e: &AllowEntry) -> String {
    let key = e.glob.as_deref().or(e.path.as_deref()).unwrap_or("?");
    format!("{} ({}/{})", key, e.surface, e.kind)
}

fn expired_entries(entries: &[AllowEntry]) -> Result<Vec<String>> {
    let today = today_utc();
    let mut out = Vec::new();
    for e in entries {
        if let Some(d) = &e.expires {
            let parsed = parse_iso_date(d)
                .with_context(|| format!("entry {} has invalid `expires`", describe_entry(e)))?;
            if parsed < today {
                out.push(format!("{} (expired {})", describe_entry(e), d));
            }
        }
    }
    Ok(out)
}

fn missing_required_fields(entries: &[AllowEntry]) -> Vec<String> {
    let mut out = Vec::new();
    for e in entries {
        match e.classification.as_str() {
            "production" | "test" => {
                if e.covered_by.is_empty() {
                    out.push(format!(
                        "{}: classification={} requires non-empty covered_by",
                        describe_entry(e),
                        e.classification
                    ));
                }
            }
            "generated" => {
                if e.generated_by.is_none() {
                    out.push(format!(
                        "{}: classification=generated requires generated_by",
                        describe_entry(e)
                    ));
                }
            }
            _ => {}
        }
        if e.reason.trim().is_empty() {
            out.push(format!("{}: reason must not be empty", describe_entry(e)));
        }
    }
    out
}

#[expect(
    clippy::let_underscore_must_use,
    reason = "writeln! to a String is infallible; the Result must be observed but is unconditionally Ok"
)]
fn render_markdown(report: &Report<'_>) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(&mut s, "# File Policy Report\n");
    let _ = writeln!(
        &mut s,
        "* Total tracked files: **{}**",
        report.summary.total_files
    );
    let _ = writeln!(
        &mut s,
        "* Rust/Cargo (auto-allowed): **{}**",
        report.summary.rust_or_cargo
    );
    let _ = writeln!(
        &mut s,
        "* Matched by allowlist: **{}**",
        report.summary.matched
    );
    let _ = writeln!(
        &mut s,
        "* Unallowlisted non-Rust files: **{}**",
        report.summary.unallowlisted
    );
    let _ = writeln!(
        &mut s,
        "* Stale glob/path entries: **{}**",
        report.summary.stale_globs
    );
    let _ = writeln!(&mut s, "* Expired entries: **{}**", report.summary.expired);
    let _ = writeln!(
        &mut s,
        "* Entries missing required fields: **{}**\n",
        report.summary.missing_required
    );

    if !report.unallowlisted.is_empty() {
        let _ = writeln!(&mut s, "## Unallowlisted files\n");
        for f in report.unallowlisted {
            let _ = writeln!(&mut s, "* `{f}`");
        }
        let _ = writeln!(&mut s);
    }
    if !report.stale_globs.is_empty() {
        let _ = writeln!(&mut s, "## Stale entries (matched no files)\n");
        for f in report.stale_globs {
            let _ = writeln!(&mut s, "* {f}");
        }
        let _ = writeln!(&mut s);
    }
    if !report.expired_entries.is_empty() {
        let _ = writeln!(&mut s, "## Expired entries\n");
        for f in report.expired_entries {
            let _ = writeln!(&mut s, "* {f}");
        }
        let _ = writeln!(&mut s);
    }
    if !report.missing_required.is_empty() {
        let _ = writeln!(&mut s, "## Entries missing required fields\n");
        for f in report.missing_required {
            let _ = writeln!(&mut s, "* {f}");
        }
        let _ = writeln!(&mut s);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(glob: &str, classification: &str) -> AllowEntry {
        AllowEntry {
            glob: Some(glob.into()),
            path: None,
            kind: "test".into(),
            owner: "test".into(),
            surface: "fixtures".into(),
            classification: classification.into(),
            reason: "test".into(),
            covered_by: Vec::new(),
            generated_by: None,
            expires: None,
            retired: false,
        }
    }

    #[test]
    fn rust_files_are_implicit() {
        assert!(is_rust_or_cargo(&PathBuf::from("crates/foo/src/lib.rs")));
        assert!(is_rust_or_cargo(&PathBuf::from("Cargo.toml")));
        assert!(!is_rust_or_cargo(&PathBuf::from("docs/INTRO.md")));
    }

    #[test]
    fn glob_matches() {
        let e = entry("docs/**/*.md", "docs");
        assert!(entry_matches(&e, &PathBuf::from("docs/INTRO.md")));
        assert!(entry_matches(&e, &PathBuf::from("docs/sub/INTRO.md")));
        assert!(!entry_matches(&e, &PathBuf::from("README.md")));
    }

    #[test]
    fn star_does_not_cross_slashes() {
        // Without `require_literal_separator = true` this would silently match
        // every .md file in the tree. We explicitly enforce strict behavior.
        let top = entry("*.md", "docs");
        assert!(entry_matches(&top, &PathBuf::from("README.md")));
        assert!(!entry_matches(&top, &PathBuf::from("crates/foo/README.md")));
    }

    #[test]
    fn one_level_star_path() {
        let e = entry("crates/*/CLAUDE.md", "docs");
        assert!(entry_matches(
            &e,
            &PathBuf::from("crates/perfgate-app/CLAUDE.md")
        ));
        assert!(!entry_matches(
            &e,
            &PathBuf::from("crates/perfgate-app/sub/CLAUDE.md")
        ));
    }

    #[test]
    fn missing_required_for_production() {
        let mut e = entry("contracts/**", "production");
        e.covered_by.clear();
        let m = missing_required_fields(std::slice::from_ref(&e));
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn missing_required_for_generated() {
        let e = entry("schemas/**", "generated");
        let m = missing_required_fields(std::slice::from_ref(&e));
        assert_eq!(m.len(), 1);
    }
}
