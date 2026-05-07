//! Semantic no-panic family checker.
//!
//! Scans Rust source for panic-family constructs and matches each finding
//! against `policy/no-panic-allowlist.toml` by *path + family + selector*
//! (never by line/column).
//!
//! ## What this is, and what this isn't
//!
//! This is a *regex-shaped* scanner. It is not a full Rust AST parser. It
//! detects constructs like `.unwrap()` and `panic!(...)` lexically, with best-
//! effort string/comment skipping. The intent is fast, deterministic CI signal
//! at xtask cost. For high-fidelity enforcement (e.g. in a future release
//! where we promote the Clippy lints to deny), Clippy itself is the source of
//! truth — this checker carries the *receipts*.
//!
//! Identity:
//!
//! ```text
//! identity = path + family + selector
//! ```
//!
//! `last_seen.{line, column}` is advisory.

use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::common::{
    ensure_reports_dir, list_tracked_files, parse_iso_date, resolve_strict, today_utc, write_report,
};

const ALLOWLIST_PATH: &str = "policy/no-panic-allowlist.toml";
const REPORT_MD: &str = "no-panic.md";
const REPORT_JSON: &str = "no-panic.json";
const PROPOSAL_FILE: &str = "no-panic-proposed-allowlist.toml";

/// Family name vocabulary. Must match the docs in `policy/no-panic-allowlist.toml`.
pub const FAMILIES: &[&str] = &[
    "unwrap",
    "expect",
    "panic_macro",
    "todo",
    "unimplemented",
    "unreachable",
    "indexing",
    "string_slice",
    "get_unwrap",
    "unchecked_time_subtraction",
];

#[derive(Debug, Deserialize)]
struct AllowlistFile {
    schema_version: String,
    #[serde(default)]
    allow: Vec<AllowEntry>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(
    dead_code,
    reason = "schema fields are required for parsing/validation but not used downstream"
)]
struct AllowEntry {
    id: String,
    path: String,
    family: String,
    classification: String,
    owner: String,
    explanation: String,
    expires: String,
    selector: Selector,
    #[serde(default)]
    last_seen: Option<LastSeen>,
}

#[derive(Debug, Deserialize, Clone)]
struct Selector {
    kind: String,
    container: String,
    #[serde(default)]
    callee: Option<String>,
    #[serde(default)]
    receiver_fingerprint: Option<String>,
    #[serde(default)]
    macro_name: Option<String>,
    #[serde(default)]
    index_kind: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code, reason = "schema-only fields; advisory drift hints")]
struct LastSeen {
    line: u32,
    column: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct Finding {
    pub path: String,
    pub family: String,
    pub container: String,
    pub callee: Option<String>,
    pub macro_name: Option<String>,
    pub index_kind: Option<String>,
    pub receiver_fingerprint: Option<String>,
    pub selector_kind: String,
    pub line: u32,
    pub column: u32,
    pub snippet: String,
}

#[derive(Debug, Serialize)]
struct Report<'a> {
    schema_version: &'a str,
    summary: ReportSummary,
    unallowlisted: Vec<&'a Finding>,
    stale_entries: &'a [String],
    expired_entries: &'a [String],
}

#[derive(Debug, Serialize)]
struct ReportSummary {
    total_findings: usize,
    matched: usize,
    unallowlisted: usize,
    stale: usize,
    expired: usize,
}

/// Run the no-panic checker.
///
/// * `propose` — when true, emit a proposed allowlist file under
///   `target/perfgate/reports/no-panic-proposed-allowlist.toml`.
/// * `strict` — when true, exit non-zero on any unallowlisted/stale/expired finding.
pub fn run(propose: bool, strict_flag: bool) -> Result<()> {
    let strict = resolve_strict(strict_flag);
    let repo = Path::new(".");
    ensure_reports_dir()?;

    let allowlist = load_allowlist(repo)?;
    let findings = scan_repo(repo)?;

    let (matched, unallowlisted) = partition_findings(&findings, &allowlist);
    let stale = stale_entries(&allowlist, &findings);
    let expired = expired_entries(&allowlist)?;

    if propose {
        let proposal = build_proposal(&unallowlisted);
        let path = write_report(PROPOSAL_FILE, &proposal)?;
        println!(
            "  ..  no-panic proposal written to {} ({} entries)",
            path.display(),
            unallowlisted.len()
        );
    }

    let summary = ReportSummary {
        total_findings: findings.len(),
        matched: matched.len(),
        unallowlisted: unallowlisted.len(),
        stale: stale.len(),
        expired: expired.len(),
    };

    let unallowlisted_refs: Vec<&Finding> = unallowlisted.to_vec();
    let report = Report {
        schema_version: "0.3",
        summary: ReportSummary { ..summary },
        unallowlisted: unallowlisted_refs,
        stale_entries: &stale,
        expired_entries: &expired,
    };

    let json = serde_json::to_string_pretty(&report)?;
    write_report(REPORT_JSON, &json)?;
    let md = render_markdown(&report, &allowlist);
    write_report(REPORT_MD, &md)?;

    println!(
        "  ..  no-panic: {} findings ({} matched, {} unallowlisted, {} stale, {} expired)",
        findings.len(),
        matched.len(),
        unallowlisted.len(),
        stale.len(),
        expired.len(),
    );

    let blocking = !unallowlisted.is_empty() || !stale.is_empty() || !expired.is_empty();
    if strict && blocking {
        bail!(
            "no-panic policy violations: {} unallowlisted, {} stale, {} expired",
            unallowlisted.len(),
            stale.len(),
            expired.len()
        );
    }
    if !strict && blocking {
        println!(
            "  WARN no-panic policy is advisory; rerun with --strict or PERFGATE_POLICY_STRICT=1 to enforce."
        );
    } else if !blocking {
        println!("  OK  no-panic policy clean");
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

    if parsed.schema_version != "0.3" {
        bail!(
            "{} schema_version must be \"0.3\" (got {:?})",
            path.display(),
            parsed.schema_version
        );
    }
    let mut seen_ids = BTreeSet::new();
    for entry in &parsed.allow {
        if !FAMILIES.contains(&entry.family.as_str()) {
            bail!(
                "{}: entry {} has unknown family {:?}; expected one of {:?}",
                path.display(),
                entry.id,
                entry.family,
                FAMILIES
            );
        }
        if !seen_ids.insert(&entry.id) {
            bail!("{}: duplicate allowlist id {:?}", path.display(), entry.id);
        }
    }
    Ok(parsed.allow)
}

fn scan_repo(repo: &Path) -> Result<Vec<Finding>> {
    let files = list_tracked_files(repo)?;
    let mut out = Vec::new();
    for rel in files {
        if !rel.extension().map(|e| e == "rs").unwrap_or(false) {
            continue;
        }
        if super::common::is_build_or_vcs_dir(&rel) {
            continue;
        }
        let text = match std::fs::read_to_string(repo.join(&rel)) {
            Ok(s) => s,
            Err(_) => continue,
        };
        scan_file(&rel, &text, &mut out);
    }
    Ok(out)
}

fn scan_file(rel: &Path, text: &str, out: &mut Vec<Finding>) {
    let unwrap_re = Regex::new(r"\.unwrap\s*\(\s*\)").unwrap();
    let expect_re = Regex::new(r"\.expect\s*\(").unwrap();
    let panic_re = Regex::new(r"\bpanic\s*!\s*\(").unwrap();
    let todo_re = Regex::new(r"\btodo\s*!\s*[\(\[\{]").unwrap();
    let unimpl_re = Regex::new(r"\bunimplemented\s*!\s*[\(\[\{]").unwrap();
    let unreach_re = Regex::new(r"\bunreachable\s*!\s*[\(\[\{]").unwrap();
    // .get(...).unwrap() — overlap with unwrap_re is fine; we filter dupes below.
    let get_unwrap_re = Regex::new(r"\.get\s*\([^)]*\)\s*\.unwrap\s*\(\s*\)").unwrap();
    // We don't currently scan `indexing`, `string_slice`, or
    // `unchecked_time_subtraction` lexically — Clippy is more accurate for
    // those. The allowlist still understands them.

    let path_str = rel.display().to_string();

    // Track byte offsets per line for line/column lookup.
    let line_starts = build_line_starts(text);
    // Mask out string/line-comment content so we don't false-positive on text.
    let masked = mask_strings_and_comments(text);

    let emit = |family: &str,
                selector_kind: &str,
                callee: Option<String>,
                macro_name: Option<String>,
                receiver_fingerprint: Option<String>,
                byte_pos: usize,
                end: usize,
                out: &mut Vec<Finding>| {
        let (line, col) = line_col(byte_pos, &line_starts);
        let container = enclosing_container(text, byte_pos);
        let snippet = snippet_for(text, byte_pos, end);
        out.push(Finding {
            path: path_str.clone(),
            family: family.to_string(),
            container,
            callee,
            macro_name,
            index_kind: None,
            receiver_fingerprint,
            selector_kind: selector_kind.to_string(),
            line,
            column: col,
            snippet,
        });
    };

    let mut covered_unwrap_starts: BTreeSet<usize> = BTreeSet::new();

    for m in get_unwrap_re.find_iter(&masked) {
        let receiver_fp = receiver_fingerprint(text, m.start());
        emit(
            "get_unwrap",
            "method_call",
            Some("unwrap".into()),
            None,
            receiver_fp,
            m.start(),
            m.end(),
            out,
        );
        // Suppress the inner .unwrap() match for this site.
        let unwrap_offset = m.as_str().rfind(".unwrap").unwrap_or(0);
        covered_unwrap_starts.insert(m.start() + unwrap_offset);
    }

    for m in unwrap_re.find_iter(&masked) {
        if covered_unwrap_starts.contains(&m.start()) {
            continue;
        }
        let receiver_fp = receiver_fingerprint(text, m.start());
        emit(
            "unwrap",
            "method_call",
            Some("unwrap".into()),
            None,
            receiver_fp,
            m.start(),
            m.end(),
            out,
        );
    }

    for m in expect_re.find_iter(&masked) {
        let receiver_fp = receiver_fingerprint(text, m.start());
        emit(
            "expect",
            "method_call",
            Some("expect".into()),
            None,
            receiver_fp,
            m.start(),
            m.end(),
            out,
        );
    }

    for (re, family, name) in [
        (&panic_re, "panic_macro", "panic"),
        (&todo_re, "todo", "todo"),
        (&unimpl_re, "unimplemented", "unimplemented"),
        (&unreach_re, "unreachable", "unreachable"),
    ] {
        for m in re.find_iter(&masked) {
            emit(
                family,
                "macro_call",
                None,
                Some(name.to_string()),
                None,
                m.start(),
                m.end(),
                out,
            );
        }
    }
}

fn build_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn line_col(byte_pos: usize, starts: &[usize]) -> (u32, u32) {
    match starts.binary_search(&byte_pos) {
        Ok(i) => ((i as u32) + 1, 1),
        Err(i) => {
            let line_start = starts.get(i.saturating_sub(1)).copied().unwrap_or(0);
            ((i as u32), ((byte_pos - line_start) as u32) + 1)
        }
    }
}

fn snippet_for(text: &str, start: usize, end: usize) -> String {
    let line_start = text[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
    let line_end = text[end..]
        .find('\n')
        .map(|p| end + p)
        .unwrap_or(text.len());
    text[line_start..line_end].trim().to_string()
}

/// Walk back from `byte_pos` to find the enclosing `fn <name>` (or `impl ...`
/// when no fn is found). Best-effort; lexical only.
fn enclosing_container(text: &str, byte_pos: usize) -> String {
    let prefix = &text[..byte_pos];
    let fn_re = Regex::new(r#"(?m)^\s*(?:pub(?:\s*\([^)]*\))?\s+)?(?:async\s+)?(?:const\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)"#).unwrap();
    let last_fn = fn_re.captures_iter(prefix).last();
    if let Some(c) = last_fn {
        return c.get(1).unwrap().as_str().to_string();
    }
    let impl_re =
        Regex::new(r"(?m)^\s*impl(?:\s*<[^>]*>)?\s+([A-Za-z_][A-Za-z0-9_:<>, ]*)").unwrap();
    if let Some(c) = impl_re.captures_iter(prefix).last() {
        return format!("impl {}", c.get(1).unwrap().as_str().trim());
    }
    "<module>".to_string()
}

/// Approximate the receiver chain ending at `byte_pos`. Walks back at most
/// 64 bytes and trims to the last identifier-or-call segment. Used as a
/// stable fingerprint for selector matching, not as code.
fn receiver_fingerprint(text: &str, byte_pos: usize) -> Option<String> {
    let start = byte_pos.saturating_sub(64);
    let chunk = &text[start..byte_pos];
    let chunk = chunk.trim();
    if chunk.is_empty() {
        return None;
    }
    // Truncate at preceding `=`/`(`/`,`/`{`/`;`/whitespace boundary so we
    // don't leak unrelated syntax.
    let cut = chunk
        .rfind(['=', ';', '{', '}', ',', '\n'])
        .map(|i| i + 1)
        .unwrap_or(0);
    let s = chunk[cut..].trim();
    if s.is_empty() {
        return None;
    }
    Some(s.to_string())
}

fn partition_findings<'a>(
    findings: &'a [Finding],
    allow: &[AllowEntry],
) -> (Vec<&'a Finding>, Vec<&'a Finding>) {
    let mut matched = Vec::new();
    let mut unallowlisted = Vec::new();
    for f in findings {
        if allow.iter().any(|a| matches(a, f)) {
            matched.push(f);
        } else {
            unallowlisted.push(f);
        }
    }
    (matched, unallowlisted)
}

fn matches(a: &AllowEntry, f: &Finding) -> bool {
    if a.path != f.path {
        return false;
    }
    if a.family != f.family {
        return false;
    }
    match a.selector.kind.as_str() {
        "method_call" => {
            if a.selector.container != f.container {
                return false;
            }
            if let (Some(ac), Some(fc)) = (&a.selector.callee, &f.callee)
                && ac != fc
            {
                return false;
            }
            // receiver_fingerprint match is a "contains" so minor refactors
            // don't break the receipt.
            match (&a.selector.receiver_fingerprint, &f.receiver_fingerprint) {
                (Some(want), Some(got)) => got.contains(want.as_str()),
                (Some(_), None) => false,
                (None, _) => true,
            }
        }
        "macro_call" => {
            a.selector.container == f.container
                && a.selector.macro_name.as_deref() == f.macro_name.as_deref()
        }
        "index" => {
            a.selector.container == f.container
                && a.selector.index_kind.as_deref() == f.index_kind.as_deref()
        }
        _ => false,
    }
}

fn stale_entries(allow: &[AllowEntry], findings: &[Finding]) -> Vec<String> {
    allow
        .iter()
        .filter(|a| !findings.iter().any(|f| matches(a, f)))
        .map(|a| format!("{} ({} {})", a.id, a.path, a.family))
        .collect()
}

fn expired_entries(allow: &[AllowEntry]) -> Result<Vec<String>> {
    let today = today_utc();
    let mut out = Vec::new();
    for a in allow {
        let exp = parse_iso_date(&a.expires)
            .with_context(|| format!("entry {}: invalid expires", a.id))?;
        if exp < today {
            out.push(format!("{} (expired {})", a.id, a.expires));
        }
    }
    Ok(out)
}

#[expect(
    clippy::let_underscore_must_use,
    reason = "writeln! to a String is infallible; the Result must be observed but is unconditionally Ok"
)]
fn build_proposal(unallowlisted: &[&Finding]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(
        &mut s,
        "# Proposed no-panic allowlist entries.\n#\n# Generated by `cargo run -p xtask -- no-panic propose`.\n# Review every entry. Set `owner`, `classification`, `explanation`, and\n# `expires` before merging into `policy/no-panic-allowlist.toml`.\n\nschema_version = \"0.3\"\n"
    );
    let mut by_path: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
    for f in unallowlisted {
        by_path.entry(f.path.as_str()).or_default().push(f);
    }
    let mut counter = 1usize;
    for (path, group) in by_path {
        let _ = writeln!(&mut s, "\n# --- {path} ---");
        for f in group {
            let id = format!("panic-{:05}", counter);
            counter += 1;
            let _ = writeln!(&mut s, "\n[[allow]]");
            let _ = writeln!(&mut s, "id = {:?}", id);
            let _ = writeln!(&mut s, "path = {:?}", f.path);
            let _ = writeln!(&mut s, "family = {:?}", f.family);
            let _ = writeln!(&mut s, "classification = \"pending_burndown\"");
            let _ = writeln!(&mut s, "owner = \"TODO\"");
            let _ = writeln!(
                &mut s,
                "explanation = \"TODO: explain why panic-family is acceptable here.\""
            );
            let _ = writeln!(&mut s, "expires = \"2026-12-31\"");
            let _ = writeln!(&mut s, "\n[allow.selector]");
            let _ = writeln!(&mut s, "kind = {:?}", f.selector_kind);
            let _ = writeln!(&mut s, "container = {:?}", f.container);
            if let Some(c) = &f.callee {
                let _ = writeln!(&mut s, "callee = {:?}", c);
            }
            if let Some(m) = &f.macro_name {
                let _ = writeln!(&mut s, "macro_name = {:?}", m);
            }
            if let Some(rf) = &f.receiver_fingerprint {
                let _ = writeln!(&mut s, "receiver_fingerprint = {:?}", rf);
            }
            let _ = writeln!(&mut s, "\n[allow.last_seen]");
            let _ = writeln!(&mut s, "line = {}", f.line);
            let _ = writeln!(&mut s, "column = {}", f.column);
        }
    }
    s
}

#[expect(
    clippy::let_underscore_must_use,
    reason = "writeln! to a String is infallible; the Result must be observed but is unconditionally Ok"
)]
fn render_markdown(report: &Report<'_>, _allow: &[AllowEntry]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(&mut s, "# No-Panic Policy Report\n");
    let _ = writeln!(
        &mut s,
        "* Total findings: **{}**",
        report.summary.total_findings
    );
    let _ = writeln!(
        &mut s,
        "* Matched by allowlist: **{}**",
        report.summary.matched
    );
    let _ = writeln!(
        &mut s,
        "* Unallowlisted: **{}**",
        report.summary.unallowlisted
    );
    let _ = writeln!(&mut s, "* Stale entries: **{}**", report.summary.stale);
    let _ = writeln!(
        &mut s,
        "* Expired entries: **{}**\n",
        report.summary.expired
    );

    if !report.unallowlisted.is_empty() {
        let _ = writeln!(&mut s, "## Unallowlisted findings\n");
        let mut by_path: BTreeMap<&str, Vec<&Finding>> = BTreeMap::new();
        for f in &report.unallowlisted {
            by_path.entry(f.path.as_str()).or_default().push(*f);
        }
        for (path, group) in by_path {
            let _ = writeln!(&mut s, "### {path}\n");
            for f in group {
                let _ = writeln!(
                    &mut s,
                    "* `{}:{}` — {} in `{}` — `{}`",
                    f.path, f.line, f.family, f.container, f.snippet
                );
            }
            let _ = writeln!(&mut s);
        }
    }

    if !report.stale_entries.is_empty() {
        let _ = writeln!(&mut s, "## Stale allowlist entries\n");
        for e in report.stale_entries {
            let _ = writeln!(&mut s, "* {e}");
        }
        let _ = writeln!(&mut s);
    }

    if !report.expired_entries.is_empty() {
        let _ = writeln!(&mut s, "## Expired allowlist entries\n");
        for e in report.expired_entries {
            let _ = writeln!(&mut s, "* {e}");
        }
        let _ = writeln!(&mut s);
    }

    s
}

/// Produce a copy of `text` where the contents of double-quoted string
/// literals and `//` line comments are replaced with spaces. Block comments
/// are best-effort masked too. Quote/comment delimiters and surrounding
/// structure are preserved so byte offsets line up with the original.
fn mask_strings_and_comments(text: &str) -> String {
    enum Mode {
        Code,
        LineComment,
        BlockComment(u32),
        DoubleString,
        ByteString,
        Char,
    }
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut mode = Mode::Code;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        match mode {
            Mode::Code => {
                if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    mode = Mode::LineComment;
                    out.push_str("//");
                    i += 2;
                    continue;
                }
                if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    mode = Mode::BlockComment(1);
                    out.push_str("/*");
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    mode = Mode::DoubleString;
                    out.push('"');
                    i += 1;
                    continue;
                }
                if c == b'b' && i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                    mode = Mode::ByteString;
                    out.push_str("b\"");
                    i += 2;
                    continue;
                }
                if c == b'\'' {
                    // crude: treat as character literal until the next single quote
                    mode = Mode::Char;
                    out.push('\'');
                    i += 1;
                    continue;
                }
                out.push(c as char);
                i += 1;
            }
            Mode::LineComment => {
                if c == b'\n' {
                    mode = Mode::Code;
                    out.push('\n');
                } else {
                    out.push(' ');
                }
                i += 1;
            }
            Mode::BlockComment(depth) => {
                if c == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                    if depth == 1 {
                        mode = Mode::Code;
                    } else {
                        mode = Mode::BlockComment(depth - 1);
                    }
                    out.push_str("*/");
                    i += 2;
                    continue;
                }
                if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
                    mode = Mode::BlockComment(depth + 1);
                    out.push_str("/*");
                    i += 2;
                    continue;
                }
                out.push(if c == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
            Mode::DoubleString | Mode::ByteString => {
                if c == b'\\' && i + 1 < bytes.len() {
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    mode = Mode::Code;
                    out.push('"');
                    i += 1;
                    continue;
                }
                out.push(if c == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
            Mode::Char => {
                if c == b'\\' && i + 1 < bytes.len() {
                    out.push(' ');
                    out.push(' ');
                    i += 2;
                    continue;
                }
                if c == b'\'' {
                    mode = Mode::Code;
                    out.push('\'');
                    i += 1;
                    continue;
                }
                out.push(if c == b'\n' { '\n' } else { ' ' });
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_unwrap() {
        let mut out = Vec::new();
        scan_file(
            Path::new("test.rs"),
            "fn foo() -> i32 {\n    let x = bar().unwrap();\n    x\n}\n",
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].family, "unwrap");
        assert_eq!(out[0].container, "foo");
    }

    #[test]
    fn detects_get_unwrap_and_suppresses_inner() {
        let mut out = Vec::new();
        scan_file(
            Path::new("test.rs"),
            "fn foo() {\n    let x = v.get(0).unwrap();\n}\n",
            &mut out,
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].family, "get_unwrap");
    }

    #[test]
    fn detects_macros() {
        let mut out = Vec::new();
        scan_file(
            Path::new("test.rs"),
            "fn foo() {\n    panic!(\"x\");\n    todo!();\n    unreachable!();\n}\n",
            &mut out,
        );
        let families: Vec<&str> = out.iter().map(|f| f.family.as_str()).collect();
        assert!(families.contains(&"panic_macro"));
        assert!(families.contains(&"todo"));
        assert!(families.contains(&"unreachable"));
    }

    #[test]
    fn ignores_unwrap_in_strings_and_comments() {
        let mut out = Vec::new();
        scan_file(
            Path::new("test.rs"),
            "fn foo() {\n    let s = \"x.unwrap()\";\n    // .unwrap()\n}\n",
            &mut out,
        );
        assert!(out.is_empty(), "unexpected findings: {out:?}");
    }

    #[test]
    fn enclosing_container_basics() {
        let src = "fn alpha() {}\nfn beta() {\n    .unwrap()\n}\n";
        let pos = src.find(".unwrap()").unwrap();
        assert_eq!(enclosing_container(src, pos), "beta");
    }

    #[test]
    fn parse_iso_date_works() {
        let d = parse_iso_date("2026-09-30").unwrap();
        assert_eq!(d.year(), 2026);
    }
}
