# ADR 0004: Baseline Service API and Centralized Storage

## Status

Accepted

## Context

Prior to v2.0, perfgate relied on local file-based storage or cloud object storage (S3/GCS) for performance baselines. While effective for individual projects or small teams, this approach lacked:

1.  **Centralized Management**: No single source of truth for fleet-scale monitoring.
2.  **Version History**: Difficulty in tracking how baselines evolved over time or rolling back regressions in the baseline itself.
3.  **Multi-tenancy**: No native support for isolating different projects or teams within the same storage backend.
4.  **Rich Metadata**: Limited ability to filter baselines by git tags, branches, or custom metadata without complex path-based conventions.
5.  **Access Control**: Relying on IAM or file permissions which are often too coarse-grained for performance data.

## Decision

We decided to implement a dedicated Baseline Service (v2.0) that provides a centralized REST API for baseline management.

### Key Architectural Components

1.  **`perfgate-server`**: A new crate providing a high-performance REST API built with Axum. It supports multiple storage backends:
    *   **In-Memory**: For testing and ephemeral environments.
    *   **SQLite**: For single-server deployments with zero external dependencies.
    *   **PostgreSQL**: For production-grade, highly available deployments.
2.  **`perfgate-client`**: A Rust client library that encapsulates the API interactions, retry logic, and fallback mechanisms.
3.  **Multi-tenancy**: The API is organized around "projects", providing isolation between different organizational units.
4.  **Versioning**: Every baseline upload creates a new version, allowing for historical tracking and atomic "promotion" of baselines.
5.  **Authentication**: Support for API Keys (static) and JWT (short-lived) for CI/CD integration.

### Integration Strategy

*   The `check`, `compare`, and `promote` commands were extended to support the `--baseline-server` flag (and corresponding configuration).
*   A new `baseline` command group was added for manual management (list, download, delete, history, rollback).
*   **Graceful Degradation**: The client is designed to fall back to local or cloud storage if the Baseline Server is unavailable, ensuring CI pipelines remain resilient.

## Consequences

### Positive

*   **Fleet-wide Visibility**: Easier to compare performance across different microservices or components.
*   **Auditability**: All baseline changes (uploads, promotions, rollbacks) are logged in a centralized audit trail.
*   **Operational Simplicity**: Teams no longer need to manage S3 buckets or local directories for baselines; they just point to the central server.
*   **Atomic Promotions**: Reducing the risk of corrupted baselines during concurrent CI runs.

### Negative

*   **Increased Complexity**: Introducing a new service to deploy and maintain.
*   **Network Dependency**: Baseline resolution now depends on server availability (mitigated by fallback storage).
*   **Migration Effort**: Existing baselines need to be migrated to the new service using the provided migration tools.
