use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::path::PathBuf;
use std::process::Command;

use crate::Role;

fn default_project() -> String {
    "default".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialSource {
    Env { var: String },
    File { path: PathBuf },
    Command { command: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyPolicy {
    pub id: String,
    pub role: Role,
    pub project: String,
    pub benchmark_regex: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedCredential {
    pub policy: KeyPolicy,
    pub secret: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CredentialSourceError {
    #[error("missing environment variable {0}")]
    MissingEnvVar(String),

    #[error("failed to read key material from {source_name}: {message}")]
    ReadFailure {
        source_name: String,
        message: String,
    },

    #[error("command source failed (status {status})")]
    CommandFailure { status: i32 },

    #[error("key material document parse failed: {0}")]
    ParseFailure(String),

    #[error("invalid key policy document: {0}")]
    InvalidDocument(String),
}

#[derive(Debug, Clone, Deserialize)]
struct RawCredential {
    id: String,
    role: Role,
    #[serde(default = "default_project")]
    project: String,
    #[serde(default)]
    benchmark_regex: Option<String>,
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
    secret: String,
}

#[derive(Debug, Deserialize)]
struct JsonDocument {
    #[serde(default)]
    keys: Vec<RawCredential>,
    #[serde(default)]
    api_keys: Vec<RawCredential>,
}

#[derive(Debug, Deserialize)]
struct TomlDocument {
    #[serde(default)]
    keys: Vec<RawCredential>,
    #[serde(default)]
    api_keys: Vec<RawCredential>,
}

impl CredentialSource {
    pub fn load(&self) -> Result<Vec<LoadedCredential>, CredentialSourceError> {
        let content = match self {
            Self::Env { var } => {
                std::env::var(var).map_err(|_| CredentialSourceError::MissingEnvVar(var.clone()))?
            }
            Self::File { path } => {
                std::fs::read_to_string(path).map_err(|e| CredentialSourceError::ReadFailure {
                    source_name: format!("file {}", path.display()),
                    message: e.to_string(),
                })?
            }
            Self::Command { command } => run_command(command)?,
        };

        parse_credentials_document(&content)
    }
}

fn run_command(command: &str) -> Result<String, CredentialSourceError> {
    if command.trim().is_empty() {
        return Err(CredentialSourceError::ReadFailure {
            source_name: "command".to_string(),
            message: "command is empty".to_string(),
        });
    }

    let output =
        shell_command(command)
            .output()
            .map_err(|e| CredentialSourceError::ReadFailure {
                source_name: "command".to_string(),
                message: e.to_string(),
            })?;

    if !output.status.success() {
        let status = output.status.code().unwrap_or(-1);
        return Err(CredentialSourceError::CommandFailure { status });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(windows)]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("powershell");
    cmd.arg("-NoProfile").arg("-Command").arg(command);
    cmd
}

#[cfg(not(windows))]
fn shell_command(command: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command);
    cmd
}

pub fn parse_credentials_document(
    raw: &str,
) -> Result<Vec<LoadedCredential>, CredentialSourceError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let parsed = serde_json::from_str::<Vec<RawCredential>>(trimmed)
        .or_else(|_| {
            serde_json::from_str::<JsonDocument>(trimmed)
                .map(|doc| {
                    if !doc.keys.is_empty() {
                        doc.keys
                    } else {
                        doc.api_keys
                    }
                })
                .map_err(|e| serde_json::Error::io(std::io::Error::other(e.to_string())))
        })
        .or_else(|_| {
            toml::from_str::<Vec<RawCredential>>(trimmed)
                .map_err(|e| serde_json::Error::io(std::io::Error::other(e.to_string())))
        })
        .or_else(|_| {
            toml::from_str::<TomlDocument>(trimmed)
                .map(|doc| {
                    if !doc.keys.is_empty() {
                        doc.keys
                    } else {
                        doc.api_keys
                    }
                })
                .map_err(|e| serde_json::Error::io(std::io::Error::other(e.to_string())))
        })
        .map_err(|e| CredentialSourceError::ParseFailure(e.to_string()))?;

    let mut out = Vec::with_capacity(parsed.len());
    for entry in parsed {
        if entry.id.trim().is_empty() {
            return Err(CredentialSourceError::InvalidDocument(
                "entry id must not be empty".to_string(),
            ));
        }
        if entry.secret.trim().is_empty() {
            return Err(CredentialSourceError::InvalidDocument(format!(
                "entry '{}' has empty secret",
                entry.id
            )));
        }

        out.push(LoadedCredential {
            policy: KeyPolicy {
                id: entry.id,
                role: entry.role,
                project: entry.project,
                benchmark_regex: entry.benchmark_regex,
                expires_at: entry.expires_at,
            },
            secret: entry.secret,
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[cfg(windows)]
    fn read_file_command(path: &Path) -> String {
        let path = path.display().to_string().replace('\'', "''");
        format!("Get-Content -Raw -LiteralPath '{}'", path)
    }

    #[cfg(not(windows))]
    fn read_file_command(path: &Path) -> String {
        format!("cat \"{}\"", path.display())
    }

    #[cfg(windows)]
    fn failing_command() -> String {
        "Write-Error 'boom'; exit 9".to_string()
    }

    #[cfg(not(windows))]
    fn failing_command() -> String {
        "echo boom >&2; exit 9".to_string()
    }

    #[test]
    fn parse_json_credentials() {
        let doc = r#"[
          {
            "id":"ci-promoter",
            "role":"promoter",
            "project":"my-project",
            "benchmark_regex":".*",
            "secret":"pg_live_abcdefghijklmnopqrstuvwxyz123456"
          }
        ]"#;

        let creds = parse_credentials_document(doc).unwrap();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].policy.id, "ci-promoter");
        assert_eq!(creds[0].policy.role, Role::Promoter);
    }

    #[test]
    fn parse_toml_credentials_table() {
        let doc = r#"
          [[keys]]
          id = "dev"
          role = "admin"
          project = "default"
          secret = "pg_test_abcdefghijklmnopqrstuvwxyz123456"
        "#;

        let creds = parse_credentials_document(doc).unwrap();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].policy.role, Role::Admin);
    }

    #[test]
    fn parse_json_credentials_object_wrapper() {
        let doc = r#"{
          "keys": [
            {
              "id":"ci-promoter",
              "role":"promoter",
              "project":"my-project",
              "secret":"pg_live_abcdefghijklmnopqrstuvwxyz123456"
            }
          ]
        }"#;

        let creds = parse_credentials_document(doc).unwrap();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].policy.id, "ci-promoter");
        assert_eq!(creds[0].policy.role, Role::Promoter);
    }

    #[test]
    fn command_source_success() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("keys.json");
        std::fs::write(
            &path,
            "[{\"id\":\"k1\",\"role\":\"viewer\",\"project\":\"p\",\"secret\":\"pg_test_abcdefghijklmnopqrstuvwxyz123456\"}]",
        )
        .unwrap();
        let src = CredentialSource::Command {
            command: read_file_command(&path),
        };
        let creds = src.load().unwrap();
        assert_eq!(creds.len(), 1);
        assert_eq!(creds[0].policy.id, "k1");
    }

    #[test]
    fn command_source_failure_hides_stderr() {
        let src = CredentialSource::Command {
            command: failing_command(),
        };
        let err = src.load().unwrap_err().to_string();
        assert!(err.contains("status 9"));
        assert!(!err.contains("boom"));
    }

    #[test]
    fn file_source_loads_credentials() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            temp.path(),
            "[{\"id\":\"f1\",\"role\":\"viewer\",\"project\":\"x\",\"secret\":\"pg_test_abcdefghijklmnopqrstuvwxyz123456\"}]",
        )
        .unwrap();

        let src = CredentialSource::File {
            path: temp.path().to_path_buf(),
        };
        let creds = src.load().unwrap();
        assert_eq!(creds[0].policy.id, "f1");
    }

    #[test]
    fn env_source_loads_credentials() {
        let var = "PERFGATE_AUTH_TEST_KEYS";
        // SAFETY: test-only process-local environment mutation.
        unsafe {
            std::env::set_var(
                var,
                "[{\"id\":\"e1\",\"role\":\"viewer\",\"project\":\"x\",\"secret\":\"pg_test_abcdefghijklmnopqrstuvwxyz123456\"}]",
            );
        }

        let src = CredentialSource::Env {
            var: var.to_string(),
        };
        let creds = src.load().unwrap();
        assert_eq!(creds[0].policy.id, "e1");
    }
}
