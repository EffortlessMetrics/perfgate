# Baseline Service Notes

This file describes the current public baseline-service surface in
`perfgate 0.15.x`.

The older long-form "v2.0 design" proposal mixed shipped behavior with future
ideas. That made it too easy to treat speculative capabilities as current
product truth. This version keeps the source of truth narrow: what exists in
the repository today, how it is exposed, and what is still backlog.

For command snippets and flags, prefer checking current `--help` output before
running in production, especially when upgrading across releases.

## Current Implementation

The baseline service currently ships as these pieces:

- `perfgate-server`: dedicated Axum service for shared CI and team use
- `perfgate-client`: Rust client library for the REST API
- `perfgate` CLI integration:
  - global flags `--baseline-server`, `--api-key`, `--project`
  - `promote --to-server`
  - `compare --baseline @server:<bench>`
  - `baseline list|download|upload|delete|history|verdicts|submit-verdict|migrate`
  - config-driven use via `[baseline_server]` in `perfgate.toml`
- `perfgate serve`: local single-user dashboard/server wrapper around
  `perfgate-server` with local mode enabled

## Storage Backends

The current server binary supports:

| Backend | Status | Intended use |
|---------|--------|--------------|
| `memory` | shipped | tests and short-lived demos |
| `sqlite` | shipped | single-node deployments |
| `postgres` | shipped | multi-node or managed database deployments |

For local development, prefer `perfgate serve`. For a shared deployment,
prefer `perfgate-server` directly.

For local mode, `perfgate serve` runs with API auth disabled for single-user
workflows.

## Authentication

Current auth support is uneven by surface, so it is worth stating explicitly.

### Server binary

`perfgate-server` currently supports:

- API keys via `--api-keys role:key[:project[:benchmark_regex]]`
- JWT bearer-style token validation via `--jwt-secret`
- GitHub Actions OIDC mappings via `--github-oidc`
- GitLab CI OIDC mappings via `--gitlab-oidc`
- custom OIDC providers via `--oidc-provider`

### CLI

The CLI currently exposes API-key auth directly:

- `--api-key`
- `PERFGATE_API_KEY`

That means API keys are the documented end-to-end CLI path today. JWT and OIDC
are real server capabilities, but they are not yet first-class CLI flags.

### Local mode

`perfgate serve` runs the service in local mode:

- dashboard enabled
- auth disabled for API routes
- SQLite-backed local storage
- intended for one developer on one machine

Do not treat local mode as a shared or internet-facing deployment.

## Current REST Surface

Public routes currently exposed by the server are:

| Route | Purpose |
|-------|---------|
| `GET /health` | health check |
| `GET /info` | server info and local-mode flag |
| `GET /` | dashboard |
| `POST /api/v1/projects/{project}/baselines` | upload a baseline |
| `GET /api/v1/projects/{project}/baselines` | list baselines |
| `GET /api/v1/projects/{project}/baselines/{benchmark}/latest` | fetch latest baseline |
| `GET /api/v1/projects/{project}/baselines/{benchmark}/versions/{version}` | fetch a specific version |
| `DELETE /api/v1/projects/{project}/baselines/{benchmark}/versions/{version}` | delete a version |
| `POST /api/v1/projects/{project}/baselines/{benchmark}/promote` | promote a version |
| `GET /api/v1/projects/{project}/baselines/{benchmark}/trend` | fetch trend data |
| `POST /api/v1/projects/{project}/verdicts` | submit a verdict |
| `GET /api/v1/projects/{project}/verdicts` | list verdicts |
| `GET /api/v1/audit` | list audit events |
| `POST /api/v1/keys` | create an API key |
| `GET /api/v1/keys` | list API keys |
| `DELETE /api/v1/keys/{id}` | revoke an API key |
| `POST /api/v1/fleet/dependency-event` | record dependency events |
| `GET /api/v1/fleet/alerts` | list fleet alerts |
| `GET /api/v1/fleet/dependency/{dep_name}/impact` | query dependency impact |

## Current CLI Mapping

The main server-aware CLI workflows are:

| CLI command | Service behavior |
|-------------|------------------|
| `promote --to-server` | create a new baseline version from a run receipt |
| `compare --baseline @server:<bench>` | fetch the latest baseline for a benchmark |
| `compare --baseline @server:<bench> --baseline-project <project>` | fetch the latest baseline for a benchmark from another project |
| `baseline upload` | upload a run receipt directly |
| `baseline download` | fetch a baseline into a local file |
| `baseline list` | query project baselines |
| `baseline history` | inspect versions for one benchmark |
| `baseline verdicts` | inspect pass/warn/fail history |
| `baseline submit-verdict` | persist compare verdicts |
| `baseline migrate` | upload local baseline JSON files recursively |
| `fleet alerts` | list fleet-wide dependency regression alerts |
| `fleet impact` | inspect the project impact of a dependency |
| `fleet record-event` | record a dependency change event with performance delta |
| `serve` | run a local baseline server/dashboard in local mode |

Cross-project compare is currently a CLI-side lookup override for baseline
fetches. It does not change server-side auth or the project used by other
server-backed workflows.

## Recommended Deployment Shapes

### Local developer workflow

Use:

```bash
perfgate serve --no-open
```

Then point CLI commands at `http://127.0.0.1:8484/api/v1`.

### Shared CI workflow

Use:

```bash
perfgate-server \
  --storage-type sqlite \
  --database-url ./perfgate.db \
  --api-keys promoter:pg_live_<32+alnum>:my-project
```

Or PostgreSQL instead of SQLite when you need a shared database layer.

## What This Document No Longer Claims

This document intentionally does not treat the following as current product
truth unless they are visible in current code, help output, or route wiring:

- speculative migration phases and versioned rollout plans
- future-only admin UX
- unshipped CLI flags
- server capabilities mentioned only in old proposal prose

When in doubt, prefer:

- CLI `--help`
- `perfgate-server --help`
- `perfgate baseline --help`
- the route table in `crates/perfgate-server/src/server.rs`
- the current crate READMEs

## Near-Term Backlog

These remain reasonable follow-up items, but they should be treated as backlog,
not current guaranteed surface:

- expose non-API-key auth flows more directly in the CLI
- add stronger operator docs for key management and audit review
- tighten shared-server deployment guides and examples
- continue aligning crate READMEs, docs, and `--help` output from one source of
  truth

## Related Docs

- [Getting Started with Baseline Server](GETTING_STARTED_BASELINE_SERVER.md)
- [perfgate-server README](../crates/perfgate-server/README.md)
- [Configuration](CONFIG.md)
- [Architecture](ARCHITECTURE.md)
