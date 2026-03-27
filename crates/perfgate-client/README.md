# perfgate-client

Rust client library for the perfgate baseline service REST API.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Problem

CI pipelines and local tools need a reliable way to talk to the centralized
baseline service -- uploading run receipts, fetching baselines for comparison,
promoting good builds, and recording verdicts. This crate wraps the REST API
in a type-safe, async client with retries and local-filesystem fallback.

## Key Operations

| Method | Description |
|--------|-------------|
| `upload_baseline` | Push a run receipt to the server |
| `get_latest_baseline` | Fetch the current baseline for a benchmark |
| `get_baseline_version` | Fetch a specific historical version |
| `list_baselines` | Query with filtering and pagination |
| `promote_baseline` | Promote a version to active baseline |
| `delete_baseline` | Remove a baseline version |
| `submit_verdict` | Record a pass/warn/fail verdict |
| `list_verdicts` | Query verdict history |
| `health_check` / `is_healthy` | Server liveness probe |

## Auth

- **API key** -- `config.with_api_key("pg_live_...")` sends `Bearer <key>`
- **JWT token** -- `config.with_token("ey...")` sends `Token <jwt>`

## Resilience

- **Retries** with exponential backoff on 429/5xx (configurable via `RetryConfig`)
- **Fallback** -- `FallbackClient` reads from local storage when the server is
  unreachable, so CI never hard-fails on a transient outage

## Quick Start

```rust
use perfgate_client::{BaselineClient, ClientConfig, ListBaselinesQuery};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ClientConfig::new("https://perfgate.example.com/api/v1")
        .with_api_key("pg_live_abcdefghijklmnopqrstuvwxyz123456");
    let client = BaselineClient::new(config)?;

    let query = ListBaselinesQuery::new().with_limit(10);
    let response = client.list_baselines("my-project", &query).await?;
    for b in &response.baselines {
        println!("{}: {}", b.benchmark, b.version);
    }
    Ok(())
}
```

## License

Licensed under either Apache-2.0 or MIT.
