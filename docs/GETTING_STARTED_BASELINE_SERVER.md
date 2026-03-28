# Getting Started with the Baseline Server

This guide documents the current, shipped baseline-server workflow in
`perfgate 0.15.x`. It intentionally focuses on commands and flags that exist
today.

## Pick a Mode

Use one of these two entry points:

- `perfgate serve`: local single-user sandbox with SQLite storage, dashboard,
  and auth disabled
- `perfgate-server`: shared service for CI and teams; use API keys from the
  CLI today

Use `perfgate serve` for local exploration and `perfgate-server` for a real
shared baseline service.

## Local Sandbox

Start a local server on `127.0.0.1:8484`:

```bash
cargo run -p perfgate-cli -- serve --no-open --port 8484
```

Point the CLI at the local API:

```bash
export PERFGATE_SERVER_URL=http://127.0.0.1:8484/api/v1
export PERFGATE_PROJECT=my-project
```

Create and promote a baseline:

```bash
perfgate run --name my-bench --out run.json -- ./my-benchmark

perfgate promote \
  --current run.json \
  --to-server \
  --benchmark my-bench \
  --version main-2026-03-28
```

Query and compare against the stored baseline:

```bash
perfgate baseline list
perfgate baseline history --benchmark my-bench

perfgate run --name my-bench --out current.json -- ./my-benchmark
perfgate compare \
  --baseline @server:my-bench \
  --current current.json \
  --out compare.json
```

`perfgate serve` runs in local mode and is intended for one developer on one
machine. Do not treat it as a shared or internet-facing deployment.

## Shared Server

Install or build the dedicated server:

```bash
cargo install perfgate-server
```

Start a shared SQLite-backed instance:

```bash
perfgate-server \
  --storage-type sqlite \
  --database-url ./perfgate.db \
  --api-keys promoter:pg_live_<32+alnum>:my-project \
  --api-keys viewer:pg_live_<32+alnum>:my-project
```

Then configure the CLI:

```bash
export PERFGATE_SERVER_URL=http://localhost:8080/api/v1
export PERFGATE_API_KEY=pg_live_<32+alnum>
export PERFGATE_PROJECT=my-project
```

From there, the workflow is the same:

```bash
perfgate run --name my-bench --out run.json -- ./my-benchmark

perfgate promote \
  --current run.json \
  --to-server \
  --benchmark my-bench \
  --version main-2026-03-28

perfgate compare \
  --baseline @server:my-bench \
  --current run.json \
  --out compare.json
```

## `perfgate.toml` Configuration

For config-driven workflows such as `perfgate check`, put server settings in
`perfgate.toml`:

```toml
[baseline_server]
url = "http://127.0.0.1:8484/api/v1"
api_key = "${PERFGATE_API_KEY}"
project = "my-project"
fallback_to_local = true
```

Then run:

```bash
perfgate check --config perfgate.toml --bench my-bench
```

`fallback_to_local = true` allows config-driven workflows to fall back to the
local `baselines/` directory when the server is unavailable.

## Supported CLI Workflows

The current CLI surfaces that talk to the baseline service are:

| Command | Current behavior |
|---------|------------------|
| `promote --to-server` | Upload a run receipt as a new baseline version |
| `compare --baseline @server:<bench>` | Fetch the latest server baseline for a benchmark |
| `baseline list` | List baselines for a project |
| `baseline download` | Download the latest or a specific version |
| `baseline upload` | Upload a run receipt directly |
| `baseline delete` | Delete a specific version |
| `baseline history` | Show version history |
| `baseline verdicts` | Query pass/warn/fail verdict history |
| `baseline submit-verdict` | Submit verdict data from a compare receipt |
| `baseline migrate` | Upload local baseline JSON files to the server |

## Server Endpoints

The server exposes these top-level surfaces:

- `GET /health`: health check
- `GET /`: web dashboard
- `GET /info`: server info, including local-mode status
- `POST /api/v1/projects/{project}/baselines`: upload a baseline
- `GET /api/v1/projects/{project}/baselines`: list baselines
- `GET /api/v1/projects/{project}/baselines/{benchmark}/latest`: fetch latest
- `GET /api/v1/projects/{project}/baselines/{benchmark}/versions/{version}`:
  fetch a specific version
- `DELETE /api/v1/projects/{project}/baselines/{benchmark}/versions/{version}`:
  delete a version
- `POST /api/v1/projects/{project}/baselines/{benchmark}/promote`: promote a
  version
- `GET /api/v1/projects/{project}/baselines/{benchmark}/trend`: fetch trend
  data
- `POST /api/v1/projects/{project}/verdicts`: submit a verdict
- `GET /api/v1/projects/{project}/verdicts`: list verdicts

Point the CLI at the versioned API root, for example
`http://localhost:8080/api/v1`, not just `http://localhost:8080`.

## Authentication Notes

Current auth surfaces are split like this:

- CLI today: `--api-key` or `PERFGATE_API_KEY`
- Server binary: API keys, JWT (`--jwt-secret`), GitHub OIDC
  (`--github-oidc`), GitLab OIDC (`--gitlab-oidc`), and custom OIDC
  (`--oidc-provider`)
- Local sandbox: `perfgate serve` disables auth for local use

If you are using the CLI end to end today, API keys are the supported path.

## Troubleshooting

`401 Missing authentication header`

You are talking to `perfgate-server` without an API key. Set
`PERFGATE_API_KEY` or pass `--api-key`.

`compare --baseline @server:my-bench` fails immediately

Set the global server configuration first:

```bash
export PERFGATE_SERVER_URL=http://localhost:8080/api/v1
export PERFGATE_PROJECT=my-project
```

Health checks work but baseline commands do not

`/health` lives at the server root. Data endpoints live under `/api/v1`.

Need a local dashboard without setting up shared auth

Use `perfgate serve`, not bare `perfgate-server`.

## Related Docs

- [Main README](../README.md)
- [perfgate-server README](../crates/perfgate-server/README.md)
- [Configuration](CONFIG.md)
- [Architecture](ARCHITECTURE.md)
