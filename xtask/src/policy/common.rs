//! Shared helpers for the policy checkers.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use time::Date;
use time::macros::format_description;

/// Output directory for all policy reports.
pub const REPORTS_DIR: &str = "target/perfgate/reports";

/// Returns true if policy checks should fail the process on any finding.
///
/// Resolution order:
///   1. explicit `--strict` flag passed by the caller
///   2. `PERFGATE_POLICY_STRICT=1` environment variable
///   3. otherwise advisory (warn only)
pub fn resolve_strict(flag: bool) -> bool {
    flag || std::env::var("PERFGATE_POLICY_STRICT")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

/// Parse an ISO-8601 date in `YYYY-MM-DD` form.
pub fn parse_iso_date(s: &str) -> Result<Date> {
    let fmt = format_description!("[year]-[month]-[day]");
    Date::parse(s, &fmt).with_context(|| format!("invalid date: {s} (expected YYYY-MM-DD)"))
}

/// Today's UTC date, as understood by the policy checkers.
pub fn today_utc() -> Date {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let days = (now.as_secs() / 86_400) as i64;
    // Date::from_julian_day uses Julian Day Number where 1970-01-01 = 2440588.
    Date::from_julian_day(2_440_588_i32 + days as i32).unwrap_or(Date::MIN)
}

/// Ensure the report directory exists, return its path.
pub fn ensure_reports_dir() -> Result<PathBuf> {
    let path = PathBuf::from(REPORTS_DIR);
    std::fs::create_dir_all(&path)
        .with_context(|| format!("create report directory {}", path.display()))?;
    Ok(path)
}

/// Write a report file under the standard reports directory.
pub fn write_report(name: &str, contents: &str) -> Result<PathBuf> {
    let dir = ensure_reports_dir()?;
    let path = dir.join(name);
    std::fs::write(&path, contents).with_context(|| format!("write report {}", path.display()))?;
    Ok(path)
}

/// List all git-tracked files in the repo. Falls back to a recursive walk if
/// `git` is not available (so the checker still works in tarballs).
pub fn list_tracked_files(repo: &Path) -> Result<Vec<PathBuf>> {
    let output = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(repo)
        .output();

    if let Ok(output) = output
        && output.status.success()
    {
        let stdout = output.stdout;
        let mut files = Vec::new();
        for chunk in stdout.split(|&b| b == 0) {
            if chunk.is_empty() {
                continue;
            }
            let s = match std::str::from_utf8(chunk) {
                Ok(s) => s.to_string(),
                Err(_) => continue,
            };
            files.push(PathBuf::from(s));
        }
        files.sort();
        return Ok(files);
    }

    let mut out = Vec::new();
    walk_files(repo, repo, &mut out)?;
    out.sort();
    Ok(out)
}

fn walk_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') && name != "." && name != ".." {
            // Skip dotfiles/dotdirs (.git, .target, .cargo, ...).
            // These are still listed by `git ls-files`; the fallback path is
            // best-effort only.
            if name == ".github"
                || name == ".cargo"
                || name == ".ci"
                || name == ".gemini"
                || name == ".kiro"
                || name == ".gitignore"
            {
                // Fall through.
            } else {
                continue;
            }
        }
        if path.is_dir() {
            if name == "target" || name == "node_modules" {
                continue;
            }
            walk_files(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_path_buf());
        }
    }
    Ok(())
}

/// Whether a path's *first segment* points at a generated/build dir we never
/// want to scan.
pub fn is_build_or_vcs_dir(path: &Path) -> bool {
    let first = path.iter().next().and_then(|s| s.to_str()).unwrap_or("");
    matches!(first, "target" | "node_modules" | ".git")
}
