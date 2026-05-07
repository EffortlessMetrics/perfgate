//! Binary delta blame logic for perfgate.
//!
//! This module provides functions to analyze changes in Cargo.lock
//! and map them to observed changes in binary_bytes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

/// Information about a dependency change.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DependencyChange {
    pub name: String,
    pub old_version: Option<String>,
    pub new_version: Option<String>,
    pub change_type: DependencyChangeType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DependencyChangeType {
    Added,
    Removed,
    Updated,
}

/// Result of a binary blame analysis.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq, Default)]
pub struct BinaryBlame {
    pub changes: Vec<DependencyChange>,
}

/// Parses a Cargo.lock string and returns a map of package name to version.
pub fn parse_lockfile(content: &str) -> BTreeMap<String, String> {
    let mut packages = BTreeMap::new();
    let mut current_package = None;

    for line in content.lines() {
        let line = line.trim();
        if line == "[[package]]" {
            current_package = None;
        } else if line.starts_with("name = ") {
            current_package = line
                .strip_prefix("name = ")
                .map(|s| s.trim_matches('"').to_string());
        } else if line.starts_with("version = ")
            && let (Some(name), Some(version)) = (
                current_package.as_ref(),
                line.strip_prefix("version = ").map(|s| s.trim_matches('"')),
            )
        {
            packages.insert(name.clone(), version.to_string());
        }
    }
    packages
}

/// Compares two lockfiles and returns the differences.
pub fn compare_lockfiles(old_lock: &str, new_lock: &str) -> BinaryBlame {
    let old_pkgs = parse_lockfile(old_lock);
    let new_pkgs = parse_lockfile(new_lock);

    let mut changes = Vec::new();
    let all_names: HashSet<_> = old_pkgs.keys().chain(new_pkgs.keys()).collect();

    for name in all_names {
        match (old_pkgs.get(name), new_pkgs.get(name)) {
            (Some(old_v), Some(new_v)) if old_v != new_v => {
                changes.push(DependencyChange {
                    name: name.clone(),
                    old_version: Some(old_v.clone()),
                    new_version: Some(new_v.clone()),
                    change_type: DependencyChangeType::Updated,
                });
            }
            (None, Some(new_v)) => {
                changes.push(DependencyChange {
                    name: name.clone(),
                    old_version: None,
                    new_version: Some(new_v.clone()),
                    change_type: DependencyChangeType::Added,
                });
            }
            (Some(old_v), None) => {
                changes.push(DependencyChange {
                    name: name.clone(),
                    old_version: Some(old_v.clone()),
                    new_version: None,
                    change_type: DependencyChangeType::Removed,
                });
            }
            _ => {}
        }
    }

    // Sort by name for deterministic output
    changes.sort_by(|a, b| a.name.cmp(&b.name));

    BinaryBlame { changes }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lockfile() {
        let lock = r#"
[[package]]
name = "pkg1"
version = "1.0.0"

[[package]]
name = "pkg2"
version = "2.1.0"
"#;
        let pkgs = parse_lockfile(lock);
        assert_eq!(pkgs.len(), 2);
        assert_eq!(pkgs["pkg1"], "1.0.0");
        assert_eq!(pkgs["pkg2"], "2.1.0");
    }

    #[test]
    fn test_compare_lockfiles() {
        let old = r#"
[[package]]
name = "stay"
version = "1.0.0"
[[package]]
name = "update"
version = "1.0.0"
[[package]]
name = "remove"
version = "1.0.0"
"#;
        let new = r#"
[[package]]
name = "stay"
version = "1.0.0"
[[package]]
name = "update"
version = "1.1.0"
[[package]]
name = "add"
version = "1.0.0"
"#;
        let blame = compare_lockfiles(old, new);
        assert_eq!(blame.changes.len(), 3);

        assert_eq!(blame.changes[0].name, "add");
        assert_eq!(blame.changes[0].change_type, DependencyChangeType::Added);

        assert_eq!(blame.changes[1].name, "remove");
        assert_eq!(blame.changes[1].change_type, DependencyChangeType::Removed);

        assert_eq!(blame.changes[2].name, "update");
        assert_eq!(blame.changes[2].change_type, DependencyChangeType::Updated);
    }
}
