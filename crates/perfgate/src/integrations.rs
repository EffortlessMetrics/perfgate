//! Integrations for importing external benchmark data and CI platform output.

#[cfg(feature = "github")]
pub mod github;
pub mod ingest;
