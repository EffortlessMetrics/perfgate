//! Example demonstrating Binary Delta Blame.
//!
//! Run with: cargo run -p perfgate-domain --example blame

use perfgate_domain::compare_lockfiles;

fn main() {
    let old_lock = r#"
[[package]]
name = "serde"
version = "1.0.100"

[[package]]
name = "tokio"
version = "1.0.0"

[[package]]
name = "removed-pkg"
version = "0.1.0"
"#;

    let new_lock = r#"
[[package]]
name = "serde"
version = "1.0.101"

[[package]]
name = "tokio"
version = "1.0.0"

[[package]]
name = "added-pkg"
version = "2.0.0"
"#;

    println!("Analyzing Cargo.lock changes...");
    let blame = compare_lockfiles(old_lock, new_lock);

    if blame.changes.is_empty() {
        println!("No dependency changes detected.");
    } else {
        println!("Detected {} changes:", blame.changes.len());
        for change in blame.changes {
            match (change.old_version, change.new_version) {
                (Some(old), Some(new)) => {
                    println!("  [UPDATED] {} ({} -> {})", change.name, old, new);
                }
                (None, Some(new)) => {
                    println!("  [ADDED]   {} ({})", change.name, new);
                }
                (Some(old), None) => {
                    println!("  [REMOVED] {} ({})", change.name, old);
                }
                _ => unreachable!(),
            }
        }
    }
}
