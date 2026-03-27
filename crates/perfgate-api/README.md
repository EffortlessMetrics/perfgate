# perfgate-api

Common API types and models for the perfgate baseline service.

## Overview

`perfgate-api` defines the shared request/response types and data models used by
both `perfgate-server` (the centralized Baseline Service) and `perfgate-client`
(the client library). If you are building tooling that talks to the perfgate
baseline service, this crate gives you the canonical wire types.

## Key Types

### Storage models

- `BaselineRecord` — primary storage model for a baseline snapshot, including the
  full `RunReceipt`, git metadata, tags, content hash, and soft-delete flag.
- `BaselineVersion` — lightweight version history entry (no receipt payload).
- `BaselineSummary` — compact listing entry with optional receipt inclusion.
- `VerdictRecord` — recorded outcome of a benchmark execution (pass/warn/fail/skip).
- `Project` — multi-tenancy namespace with `RetentionPolicy` and `VersioningStrategy`.

### Request / response pairs

| Operation | Request | Response |
|-----------|---------|----------|
| Upload baseline | `UploadBaselineRequest` | `UploadBaselineResponse` |
| Promote baseline | `PromoteBaselineRequest` | `PromoteBaselineResponse` |
| Delete baseline | -- | `DeleteBaselineResponse` |
| List baselines | `ListBaselinesQuery` | `ListBaselinesResponse` |
| Submit verdict | `SubmitVerdictRequest` | -- |
| List verdicts | `ListVerdictsQuery` | `ListVerdictsResponse` |
| Health check | -- | `HealthResponse` |

### Supporting types

- `BaselineSource` — how a baseline was created (`Upload`, `Promote`, `Migrate`, `Rollback`).
- `RetentionPolicy` — configurable limits for version count, age, and preserved tags.
- `VersioningStrategy` — auto-versioning mode (`RunId`, `Timestamp`, `GitSha`, `Manual`).
- `PaginationInfo` — offset/limit pagination metadata for list endpoints.
- `ApiError` — structured error response with code, message, and optional details.

### Schema identifiers

```text
perfgate.baseline.v1
perfgate.project.v1
perfgate.verdict.v1
```

## Feature Flags

- `server` — enables `axum::response::IntoResponse` impl for `ApiError`, so
  the server crate can return API errors directly as HTTP responses.

## Example

```rust
use perfgate_api::{ListBaselinesQuery, UploadBaselineRequest};

// Build a filtered query with the builder API
let query = ListBaselinesQuery::new()
    .with_benchmark("my-bench")
    .with_limit(10)
    .with_receipts();

// Convert to HTTP query parameters
let params = query.to_query_params();
```

## Workspace Role

`perfgate-api` sits between the core types and the network layer:

`perfgate-types` -> **`perfgate-api`** -> `perfgate-server` / `perfgate-client`

## License

Licensed under either Apache-2.0 or MIT.
