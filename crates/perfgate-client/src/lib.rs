//! Client library for the perfgate baseline service.
//!
//! This crate provides a client for interacting with the perfgate baseline
//! service API, including:
//!
//! - Uploading and downloading baselines
//! - Listing baselines with filtering
//! - Promoting and deleting baselines
//! - Health checking
//! - Automatic fallback to local storage when the server is unavailable
//!
//! Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use perfgate_client::{BaselineClient, ClientConfig, ListBaselinesQuery};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Create a client
//!     let config = ClientConfig::new("https://perfgate.example.com/api/v1")
//!         .with_api_key("your-api-key");
//!     
//!     let client = BaselineClient::new(config)?;
//!     
//!     // Check server health
//!     let health = client.health_check().await?;
//!     println!("Server status: {}", health.status);
//!     
//!     // List baselines
//!     let query = ListBaselinesQuery::new().with_limit(10);
//!     let response = client.list_baselines("my-project", &query).await?;
//!     
//!     for baseline in &response.baselines {
//!         println!("{}: {}", baseline.benchmark, baseline.version);
//!     }
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Fallback Storage
//!
//! When the server is unavailable, the client can fall back to local file storage:
//!
//! ```rust,no_run
//! use perfgate_client::{BaselineClient, ClientConfig, FallbackClient, FallbackStorage};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = ClientConfig::new("https://perfgate.example.com/api/v1")
//!         .with_api_key("your-api-key")
//!         .with_fallback(FallbackStorage::local("./baselines"));
//!     
//!     let client = BaselineClient::new(config)?;
//!     let fallback_client = FallbackClient::new(
//!         client,
//!         Some(FallbackStorage::local("./baselines")),
//!     );
//!     
//!     // This will fall back to local storage if the server is unavailable
//!     let baseline = fallback_client
//!         .get_latest_baseline("my-project", "my-bench")
//!         .await?;
//!     
//!     Ok(())
//! }
//! ```
//!
//! ## Error Handling
//!
//! The client provides detailed error types for different failure scenarios:
//!
//! ```rust,no_run
//! use perfgate_client::{BaselineClient, ClientConfig, ClientError};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = ClientConfig::new("https://perfgate.example.com/api/v1");
//!     let client = BaselineClient::new(config).unwrap();
//!     
//!     match client.get_latest_baseline("my-project", "my-bench").await {
//!         Ok(baseline) => println!("Got baseline: {}", baseline.id),
//!         Err(ClientError::NotFoundError(msg)) => {
//!             eprintln!("Baseline not found: {}", msg);
//!         }
//!         Err(ClientError::AuthError(msg)) => {
//!             eprintln!("Authentication failed: {}", msg);
//!         }
//!         Err(ClientError::ConnectionError(msg)) => {
//!             eprintln!("Server unavailable: {}", msg);
//!         }
//!         Err(e) => eprintln!("Error: {}", e),
//!     }
//! }
//! ```

pub mod client;
pub mod config;
pub mod error;
pub mod fallback;
pub mod types;

// Re-export main types at the crate root for convenience
pub use client::BaselineClient;
pub use config::{AuthMethod, ClientConfig, FallbackStorage, RetryConfig};
pub use error::ClientError;
pub use fallback::FallbackClient;
pub use types::{
    AffectedProject, BaselineRecord, BaselineSource, BaselineSummary, DeleteBaselineResponse,
    DependencyChange, DependencyEvent, DependencyImpactQuery, DependencyImpactResponse, FleetAlert,
    HealthResponse, ListBaselinesQuery, ListBaselinesResponse, ListFleetAlertsQuery,
    ListFleetAlertsResponse, ListVerdictsQuery, ListVerdictsResponse, PaginationInfo,
    PromoteBaselineRequest, PromoteBaselineResponse, RecordDependencyEventRequest,
    RecordDependencyEventResponse, StorageHealth, SubmitVerdictRequest, UploadBaselineRequest,
    UploadBaselineResponse, VerdictRecord,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reexports() {
        // Ensure all re-exports are accessible
        let _config = ClientConfig::new("https://example.com");
        let _query = ListBaselinesQuery::new();
    }
}
