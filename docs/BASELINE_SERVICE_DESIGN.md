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
  - `admin keys create|list|revoke|rotate`
  - config-driven use via `[baseline_server]` in `perfgate.toml`
- verdict history with wall-time CV and historical flakiness scores
- `perfgate serve`: local single-user dashboard/server wrapper around
  `perfgate-server` with local mode enabled
- `/health` and `/metrics` for basic production observability

## Storage Backends

The current server binary supports:

| Backend | Status | Intended use |
|---------|--------|--------------|
| `memory` | shipped | tests and short-lived demos |
| `sqlite` | shipped | single-node deployments |
| `postgres` | shipped | multi-node or managed database deployments |

For local development, prefer `perfgate serve`. For a shared deployment,
prefer `perfgate-server` directly.
Run `perfgate serve --doctor` before starting the local sandbox when you want
to verify the SQLite path, WAL setup, and dashboard port without keeping a
server process running.

The SQLite backend is intended for one server process. File-backed SQLite
connections are configured with WAL mode and a 5 second busy timeout so normal
dashboard reads and CI writes can proceed without immediate lock failures.
In-memory SQLite is still supported for tests and local sandboxing, but WAL is
not applicable there.

The PostgreSQL backend is intended for multi-node or managed database
deployments. The server binary exposes pool-size, idle-timeout,
connection-lifetime, acquire-timeout, and statement-timeout flags. The storage
layer pings pooled connections before reuse, retries transient connection
failures, and reports pool metrics from `/health`.

Artifact retention is implemented for deployments with an object artifact
store. The binary exposes `--retention-days` and
`--cleanup-interval-hours`, and embedded deployments can attach an object store
through `ServerConfig::artifacts_url`. If retention is configured without an
artifact store, the server logs that cleanup is skipped. Provider-side lifecycle
rules should still be used for managed stores such as S3, GCS, or Azure Blob
Storage.

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
- baseline, verdict, flakiness, decision-ledger, and audit-event views
- auth disabled for API routes
- SQLite-backed local storage
- intended for one developer on one machine

Do not treat local mode as a shared or internet-facing deployment.

## Current REST Surface

Public routes currently exposed by the server are:

| Route | Purpose |
|-------|---------|
| `GET /health` | health check |
| `GET /metrics` | Prometheus metrics |
| `GET /` | dashboard |
| `GET /api/v1/info` | server info and local-mode flag |
| `POST /api/v1/projects/{project}/baselines` | upload a baseline |
| `GET /api/v1/projects/{project}/baselines` | list baselines |
| `GET /api/v1/projects/{project}/baselines/{benchmark}/latest` | fetch latest baseline |
| `GET /api/v1/projects/{project}/baselines/{benchmark}/versions/{version}` | fetch a specific version |
| `DELETE /api/v1/projects/{project}/baselines/{benchmark}/versions/{version}` | delete a version |
| `POST /api/v1/projects/{project}/baselines/{benchmark}/promote` | promote a version |
| `GET /api/v1/projects/{project}/baselines/{benchmark}/trend` | fetch trend data |
| `POST /api/v1/projects/{project}/verdicts` | submit a verdict |
| `GET /api/v1/projects/{project}/verdicts` | list verdicts |
| `POST /api/v1/projects/{project}/decisions` | upload a decision receipt |
| `GET /api/v1/projects/{project}/decisions` | list decision receipts |
| `GET /api/v1/projects/{project}/decisions/latest` | fetch latest decision |
| `GET /api/v1/audit` | list audit events |
| `POST /api/v1/keys` | create an API key |
| `GET /api/v1/keys` | list API keys |
| `DELETE /api/v1/keys/{id}` | revoke an API key |
| `DELETE /api/v1/admin/cleanup` | run artifact cleanup |
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
| `baseline flaky` | inspect benchmarks with elevated historical noise |
| `baseline submit-verdict` | persist compare verdicts |
| `baseline migrate` | upload local baseline JSON files recursively |
| `decision upload` | store a structured performance decision receipt |
| `decision history` | list stored performance decisions with scenario/status/verdict/review/rule filters |
| `decision export` | export decision ledger records as JSONL or JSON |
| `decision debt` | summarize accepted tradeoff debt, cap usage, and accepted deltas by scenario |
| `decision prune --dry-run` / `--force` | preview or delete old decision ledger records |
| `audit list` | inspect append-only audit events |
| `audit export --format jsonl` | export audit events for operators or compliance review |
| `fleet alerts` | list fleet-wide dependency regression alerts |
| `fleet impact` | inspect the project impact of a dependency |
| `fleet record-event` | record a dependency change event with performance delta |
| `admin keys create` | create a scoped API key |
| `admin keys list` | list scoped API keys |
| `admin keys revoke` | revoke an API key |
| `admin keys rotate` | create a replacement key and revoke the old key |
| `serve` | run a local baseline server/dashboard in local mode |
| `serve --doctor` | preflight the local SQLite path, WAL setup, and dashboard port |

Cross-project compare is currently a CLI-side lookup override for baseline
fetches. It does not change server-side auth or the project used by other
server-backed workflows.

## Baseline Resolution Contract

The CLI keeps local and remote baseline semantics deliberately explicit:

| Input | Server configured? | Behavior |
|-------|:------------------:|----------|
| `--baseline ./baseline.json` and file exists | yes/no | use the local file |
| `--baseline @server:bench` | yes | fetch the server baseline |
| `--baseline @server:bench` | no | hard error |
| `--baseline bench` and no local file/path is selected | yes | fetch the server baseline |
| `--baseline bench` and no local file/path is selected | no | use the existing local read error |
| `run --upload` upload fails | yes | preserve the local run receipt and exit nonzero |
| `promote --to-server` server fails | yes | hard error with no local fallback write |

In short: explicit local paths win, explicit remote operations hard-fail, and
fallback is only allowed for implicit remote behavior.

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

For PostgreSQL, start with a small bounded pool and raise it only when CI
parallelism requires it:

```bash
perfgate-server \
  --storage-type postgres \
  --database-url postgresql://perfgate:secret@db.example.com/perfgate \
  --pg-max-connections 20 \
  --pg-min-connections 4 \
  --pg-acquire-timeout 10 \
  --pg-statement-timeout 30 \
  --api-keys promoter:pg_live_<32+alnum>:my-project
```

Use `/health` to verify storage readiness and PostgreSQL pool occupancy before
making the server a required CI dependency. Unhealthy storage responses include
a coarse non-secret `storage.detail` code; the raw storage error is kept in
server logs. Use `/metrics` for Prometheus scraping once the service is shared
by more than one workflow. The operational series include:

```text
perfgate_server_requests_total
perfgate_server_request_duration_seconds
perfgate_baselines_total
perfgate_verdicts_total
perfgate_upload_failures_total
perfgate_auth_failures_total
perfgate_storage_errors_total
```

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
- tighten shared-server deployment guides and examples
- continue aligning crate READMEs, docs, and `--help` output from one source of
  truth

## Related Docs

- [Getting Started with Baseline Server](GETTING_STARTED_BASELINE_SERVER.md)
- [Flakiness History](FLAKINESS.md)
- [perfgate-server README](../crates/perfgate-server/README.md)
- [Configuration](CONFIG.md)
- [Architecture](ARCHITECTURE.md)
