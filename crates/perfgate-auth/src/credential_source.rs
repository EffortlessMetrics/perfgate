use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{ApiKey, Role};

/// Supported policy document encodings for API-key material.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyFormat {
    Json,
    Toml,
}

/// A single API-key policy entry loaded from env/file/command sources.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiKeyPolicyEntry {
    /// Stable principal or credential ID used for audit trails.
    pub id: String,
    /// Authorization role for this credential.
    pub role: Role,
    /// Project scope.
    #[serde(default = "default_project")]
    pub project: String,
    /// Optional benchmark regex scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benchmark_regex: Option<String>,
    /// Raw secret value (API key/token).
    pub secret: String,
    /// Optional expiry metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
}

fn default_project() -> String {
    "default".to_string()
}

impl ApiKeyPolicyEntry {
    /// Converts policy entry into runtime [`ApiKey`] metadata + raw secret value.
    pub fn into_runtime_parts(self) -> (ApiKey, String) {
        let mut api_key = ApiKey::new(
            self.id.clone(),
            format!("policy:{}", self.id),
            self.project,
            self.role,
        );
        api_key.benchmark_regex = self.benchmark_regex;
        api_key.expires_at = self.expires_at;
        (api_key, self.secret)
    }
}

/// Parses a policy document from JSON or TOML.
///
/// Accepts either an array of entries or an object wrapper `{ "keys": [...] }`.
pub fn parse_api_key_policy_document(
    content: &str,
    format: PolicyFormat,
) -> Result<Vec<ApiKeyPolicyEntry>, String> {
    match format {
        PolicyFormat::Json => parse_json(content),
        PolicyFormat::Toml => parse_toml(content),
    }
}

fn parse_json(content: &str) -> Result<Vec<ApiKeyPolicyEntry>, String> {
    if let Ok(entries) = serde_json::from_str::<Vec<ApiKeyPolicyEntry>>(content) {
        return Ok(entries);
    }

    #[derive(Deserialize)]
    struct Wrapper {
        keys: Vec<ApiKeyPolicyEntry>,
    }

    serde_json::from_str::<Wrapper>(content)
        .map(|w| w.keys)
        .map_err(|e| format!("failed to parse API key policy JSON: {}", e))
}

fn parse_toml(content: &str) -> Result<Vec<ApiKeyPolicyEntry>, String> {
    if let Ok(entries) = toml::from_str::<Vec<ApiKeyPolicyEntry>>(content) {
        return Ok(entries);
    }

    #[derive(Deserialize)]
    struct Wrapper {
        keys: Vec<ApiKeyPolicyEntry>,
    }

    toml::from_str::<Wrapper>(content)
        .map(|w| w.keys)
        .map_err(|e| format!("failed to parse API key policy TOML: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_json_array_policy() {
        let raw = r#"[
          {
            "id": "ci-promoter",
            "role": "promoter",
            "project": "my-project",
            "benchmark_regex": ".*",
            "secret": "pg_live_abcdefghijklmnopqrstuvwxyz123456"
          }
        ]"#;

        let entries = parse_api_key_policy_document(raw, PolicyFormat::Json).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "ci-promoter");
        assert_eq!(entries[0].role, Role::Promoter);
    }

    #[test]
    fn parse_toml_wrapper_policy() {
        let raw = r#"
[[keys]]
id = "ci-viewer"
role = "viewer"
project = "my-project"
secret = "pg_live_abcdefghijklmnopqrstuvwxyz123456"
"#;

        let entries = parse_api_key_policy_document(raw, PolicyFormat::Toml).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].project, "my-project");
    }
}
