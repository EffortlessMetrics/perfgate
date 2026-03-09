//! Client configuration types.
//!
//! This module defines configuration options for the baseline client,
//! including authentication, timeouts, and retry behavior.

use std::path::PathBuf;
use std::time::Duration;

/// Authentication method for the client.
#[derive(Debug, Clone, Default)]
pub enum AuthMethod {
    /// No authentication.
    #[default]
    None,
    /// API key authentication (Bearer token).
    ApiKey(String),
    /// JWT token authentication (Token header).
    Token(String),
}

impl AuthMethod {
    /// Returns the Authorization header value for this auth method.
    pub fn header_value(&self) -> Option<String> {
        match self {
            AuthMethod::None => None,
            AuthMethod::ApiKey(key) => Some(format!("Bearer {}", key)),
            AuthMethod::Token(token) => Some(format!("Token {}", token)),
        }
    }
}

/// Retry configuration for transient failures.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts.
    pub max_retries: u32,
    /// Base delay between retries (exponential backoff).
    pub base_delay: Duration,
    /// Maximum delay between retries.
    pub max_delay: Duration,
    /// HTTP status codes that should trigger a retry.
    pub retry_status_codes: Vec<u16>,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            retry_status_codes: vec![429, 500, 502, 503, 504],
        }
    }
}

impl RetryConfig {
    /// Creates a new retry configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the maximum number of retries.
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Sets the base delay for exponential backoff.
    pub fn with_base_delay(mut self, base_delay: Duration) -> Self {
        self.base_delay = base_delay;
        self
    }

    /// Sets the maximum delay between retries.
    pub fn with_max_delay(mut self, max_delay: Duration) -> Self {
        self.max_delay = max_delay;
        self
    }

    /// Calculates the delay for a given retry attempt using exponential backoff.
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let multiplier = 2u32.pow(attempt);
        let delay = self.base_delay.saturating_mul(multiplier);
        delay.min(self.max_delay)
    }
}

/// Fallback storage configuration.
#[derive(Debug, Clone)]
pub enum FallbackStorage {
    /// Local filesystem storage.
    Local {
        /// Directory for storing baseline files.
        dir: PathBuf,
    },
}

impl FallbackStorage {
    /// Creates a local fallback storage.
    pub fn local(dir: impl Into<PathBuf>) -> Self {
        FallbackStorage::Local { dir: dir.into() }
    }
}

/// Client configuration.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Base URL of the server (e.g., "https://perfgate.example.com/api/v1").
    pub server_url: String,
    /// Authentication method.
    pub auth: AuthMethod,
    /// Request timeout.
    pub timeout: Duration,
    /// Retry configuration.
    pub retry: RetryConfig,
    /// Fallback storage when server is unavailable.
    pub fallback: Option<FallbackStorage>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_url: String::new(),
            auth: AuthMethod::None,
            timeout: Duration::from_secs(30),
            retry: RetryConfig::default(),
            fallback: None,
        }
    }
}

impl ClientConfig {
    /// Creates a new client configuration with the specified server URL.
    pub fn new(server_url: impl Into<String>) -> Self {
        Self {
            server_url: server_url.into(),
            ..Self::default()
        }
    }

    /// Sets the API key for authentication.
    pub fn with_api_key(mut self, api_key: impl Into<String>) -> Self {
        self.auth = AuthMethod::ApiKey(api_key.into());
        self
    }

    /// Sets the JWT token for authentication.
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.auth = AuthMethod::Token(token.into());
        self
    }

    /// Sets the request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the retry configuration.
    pub fn with_retry(mut self, retry: RetryConfig) -> Self {
        self.retry = retry;
        self
    }

    /// Sets the fallback storage.
    pub fn with_fallback(mut self, fallback: FallbackStorage) -> Self {
        self.fallback = Some(fallback);
        self
    }

    /// Validates the configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.server_url.is_empty() {
            return Err("server_url is required".to_string());
        }

        // Validate URL format
        if let Err(e) = url::Url::parse(&self.server_url) {
            return Err(format!("Invalid server_url: {}", e));
        }

        if self.timeout.is_zero() {
            return Err("timeout must be greater than zero".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_method_header_value() {
        assert_eq!(AuthMethod::None.header_value(), None);
        assert_eq!(
            AuthMethod::ApiKey("secret".to_string()).header_value(),
            Some("Bearer secret".to_string())
        );
        assert_eq!(
            AuthMethod::Token("jwt-token".to_string()).header_value(),
            Some("Token jwt-token".to_string())
        );
    }

    #[test]
    fn test_retry_config_delay() {
        let config = RetryConfig {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            retry_status_codes: vec![],
        };

        // Exponential backoff: 100ms, 200ms, 400ms
        assert_eq!(config.delay_for_attempt(0), Duration::from_millis(100));
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(200));
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(400));
    }

    #[test]
    fn test_retry_config_delay_capped() {
        let config = RetryConfig {
            max_retries: 10,
            base_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(5),
            retry_status_codes: vec![],
        };

        // Should cap at max_delay
        assert_eq!(config.delay_for_attempt(10), Duration::from_secs(5));
    }

    #[test]
    fn test_client_config_validation() {
        let config = ClientConfig::new("https://example.com/api/v1");
        assert!(config.validate().is_ok());

        let empty_config = ClientConfig {
            server_url: String::new(),
            ..Default::default()
        };
        assert!(empty_config.validate().is_err());

        let invalid_url = ClientConfig::new("not a url");
        assert!(invalid_url.validate().is_err());

        let zero_timeout = ClientConfig {
            server_url: "https://example.com".to_string(),
            timeout: Duration::ZERO,
            ..Default::default()
        };
        assert!(zero_timeout.validate().is_err());
    }

    #[test]
    fn test_client_config_builder() {
        let config = ClientConfig::new("https://example.com/api/v1")
            .with_api_key("my-key")
            .with_timeout(Duration::from_secs(60))
            .with_fallback(FallbackStorage::local("/tmp/baselines"));

        assert_eq!(config.server_url, "https://example.com/api/v1");
        assert!(matches!(config.auth, AuthMethod::ApiKey(_)));
        assert_eq!(config.timeout, Duration::from_secs(60));
        assert!(config.fallback.is_some());
    }
}
