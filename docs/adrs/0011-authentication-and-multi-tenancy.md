# ADR 0011: Authentication and Multi-tenancy

## Status
Accepted

## Context
The baseline server needs to support multiple projects sharing a single instance, with access control that prevents one team from modifying another's baselines. CI runners need to authenticate without human interaction, and GitHub Actions runners need keyless authentication via OIDC tokens.

## Decision
We implement a layered auth system in `perfgate-auth` and `perfgate-server`:

### API Keys
- Keys follow the format `pg_live_<32+ alphanumeric chars>` or `pg_test_<prefix>`.
- Each key is scoped to a role, project, and optional benchmark regex pattern.
- Roles: `viewer` (read), `contributor` (read + write), `promoter` (+ promote), `admin` (full).
- Keys are passed via `--api-key` flag or `PERFGATE_API_KEY` env var.
- Server accepts keys via `--api-keys "role:key:project:benchmark_regex"` at startup.

### Project Isolation
- All baselines are namespaced by project (`--project` flag or `PERFGATE_PROJECT` env var).
- Non-admin keys are restricted to their assigned project.
- The `*` project scope grants access to all projects (admin only).

### OIDC (GitHub Actions)
- The server can validate GitHub Actions OIDC tokens via `--github-oidc "org/repo:project_id:role"`.
- Repository claims are mapped directly to project IDs and roles.
- This eliminates the need to store API keys in GitHub Secrets for Actions workflows.

## Consequences
- Single server instance can serve multiple teams/projects safely.
- API key format validation prevents accidental use of weak keys.
- OIDC support is currently GitHub-specific; GitLab/Okta support requires additional work.
- No CLI for key management yet — keys are configured at server startup via flags.
- Benchmark regex patterns must use regex syntax (`.*`), not glob syntax (`*`). This is a known friction point (#58).
