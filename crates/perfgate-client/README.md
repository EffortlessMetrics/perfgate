# perfgate-client

A Rust client library for the perfgate baseline service.

[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](../../LICENSE-MIT)

## Overview

perfgate-client provides a type-safe, async client for interacting with the perfgate baseline server. It handles:

- API communication with automatic retries
- Authentication via API keys
- Fallback to local storage when the server is unavailable
- Integration with the perfgate CLI

**Documentation:** [Getting Started with Baseline Server](../../docs/GETTING_STARTED_BASELINE_SERVER.md)

## Features

- **Full API Support**: Upload, download, list, promote, and delete baselines
- **Automatic Retries**: Configurable retry logic with exponential backoff
- **Fallback Storage**: Automatic fallback to local storage when server is unavailable
- **Type-Safe**: Strongly typed request and response models
- **Async/Await**: Built on Tokio for async operations

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
perfgate-client = { path = "path/to/perfgate-client" }
```

## Quick Start

```rust
use perfgate_client::{BaselineClient, ClientConfig, ListBaselinesQuery};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a client
    let config = ClientConfig::new("https://perfgate.example.com/api/v1")
        .with_api_key("your-api-key");
    
    let client = BaselineClient::new(config)?;
    
    // Check server health
    let health = client.health_check().await?;
    println!("Server status: {}", health.status);
    
    // List baselines
    let query = ListBaselinesQuery::new().with_limit(10);
    let response = client.list_baselines("my-project", &query).await?;
    
    for baseline in &response.baselines {
        println!("{}: {}", baseline.benchmark, baseline.version);
    }
    
    Ok(())
}
```

## Fallback Storage

When the server is unavailable, the client can fall back to local file storage:

```rust
use perfgate_client::{BaselineClient, ClientConfig, FallbackClient, FallbackStorage};

let config = ClientConfig::new("https://perfgate.example.com/api/v1")
    .with_api_key("your-api-key")
    .with_fallback(FallbackStorage::local("./baselines"));

let client = BaselineClient::new(config)?;
let fallback_client = FallbackClient::new(
    client,
    Some(FallbackStorage::local("./baselines")),
);

// This will fall back to local storage if the server is unavailable
let baseline = fallback_client
    .get_latest_baseline("my-project", "my-bench")
    .await?;
```

## Error Handling

```rust
use perfgate_client::{BaselineClient, ClientConfig, ClientError};

let config = ClientConfig::new("https://perfgate.example.com/api/v1");
let client = BaselineClient::new(config).unwrap();

match client.get_latest_baseline("my-project", "my-bench").await {
    Ok(baseline) => println!("Got baseline: {}", baseline.id),
    Err(ClientError::NotFoundError(msg)) => {
        eprintln!("Baseline not found: {}", msg);
    }
    Err(ClientError::AuthError(msg)) => {
        eprintln!("Authentication failed: {}", msg);
    }
    Err(ClientError::ConnectionError(msg)) => {
        eprintln!("Server unavailable: {}", msg);
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

## API Reference

### `BaselineClient`

The main client for interacting with the baseline service.

| Method | Description |
|--------|-------------|
| `new(config)` | Create a new client with configuration |
| `upload_baseline(project, request)` | Upload a new baseline |
| `get_latest_baseline(project, benchmark)` | Get the latest baseline for a benchmark |
| `get_baseline_version(project, benchmark, version)` | Get a specific baseline version |
| `list_baselines(project, query)` | List baselines with filtering |
| `delete_baseline(project, benchmark, version)` | Delete a baseline version |
| `promote_baseline(project, benchmark, request)` | Promote a baseline version |
| `health_check()` | Check server health |
| `is_healthy()` | Returns true if server is healthy |

### `FallbackClient`

A client wrapper with automatic fallback to local storage.

| Method | Description |
|--------|-------------|
| `new(client, fallback)` | Create a new fallback client |
| `get_latest_baseline(project, benchmark)` | Get latest with fallback |
| `get_baseline_version(project, benchmark, version)` | Get version with fallback |
| `upload_baseline(project, request)` | Upload with fallback |
| `has_fallback()` | Check if fallback is configured |

### Configuration

| Type | Description |
|------|-------------|
| `ClientConfig` | Main client configuration |
| `RetryConfig` | Retry behavior configuration |
| `AuthMethod` | Authentication method (None, ApiKey, Token) |
| `FallbackStorage` | Fallback storage type (Local) |

## License

MIT OR Apache-2.0
