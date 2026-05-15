# Decision Ledger Operations Runbook

The decision ledger is optional team infrastructure. Local receipts remain the
source of correctness: `decision.md`, `decision.index.json`,
`decision-bundle.json`, `scenario.json`, `tradeoff.json`, probe receipts, and
compare receipts must be useful without a server.

Use the ledger when a team needs retained decision history, debt summaries,
audit exports, dashboard review, or shared API access.

## Operating Modes

### Local SQLite Mode

Use local mode for evaluation, demos, and small-team shared history on one
machine.

```bash
perfgate serve --doctor
perfgate serve --no-open
```

`serve --doctor` preflights the SQLite path, WAL setup, and dashboard port. The
SQLite backend is a single-node service mode; do not mount the same database
file behind multiple active server processes.

Check whether the current repo is ready to use the optional decision ledger:

```bash
perfgate ledger doctor
```

For most first-hour users, `ledger doctor` should say that server mode is not
needed yet. Local receipts and decision bundles remain the correctness
contract; the ledger is team history.

Back up local mode by copying the SQLite database while the service is stopped,
or by exporting ledger data before maintenance:

```bash
perfgate decision export --project default --days 90 --out artifacts/perfgate/decision-history.jsonl
perfgate audit export --project default --format jsonl --out artifacts/perfgate/audit.jsonl
```

### Team Server Mode

Use `perfgate-server` when CI or multiple users need a shared API endpoint.
Prefer a stable URL, persistent storage, TLS at the ingress layer, and scoped API
keys.

```bash
perfgate-server --storage sqlite --database-path /var/lib/perfgate/perfgate.db
```

Use PostgreSQL when operational policy requires managed backups, connection
pooling, or database-level availability controls:

```bash
perfgate-server --storage postgres --database-url "$DATABASE_URL"
```

## API Keys

For CLI and CI end-to-end usage, API keys are the supported path.

```bash
perfgate admin keys create --project my-project --role writer
perfgate admin keys list --project my-project
perfgate admin keys rotate --id key_123
perfgate admin keys revoke --id key_123
```

Store CI keys as repository or organization secrets. Use writer keys for
decision upload jobs and read-only keys for dashboards or audit export jobs.
Rotate keys when maintainers leave, secrets are exposed, or CI ownership
changes.

## Upload Path

Upload decision receipts after local decision evaluation has produced durable
artifacts:

```bash
perfgate decision evaluate --config perfgate.toml
perfgate decision upload --project my-project --file artifacts/perfgate/tradeoff.json --index artifacts/perfgate/decision.index.json
```

The upload stores a `perfgate.decision_record.v1` record. It should consume the
local receipts; it should not become a separate decision model.

## History And Debt

Use history for review and audit lookup:

```bash
perfgate decision history --project my-project --limit 20
perfgate decision history --project my-project --accepted true --rule memory_for_probe_speed
perfgate decision history --project my-project --review-required true
perfgate decision latest --project my-project
```

Use debt when accepted tradeoffs need ongoing visibility:

```bash
perfgate decision debt --project my-project --days 30
```

Debt output is review input. It should help teams decide whether accepted
tradeoffs are still cheap, still justified, or ready for follow-up work.

## Export And Backup

Export before releases, migrations, retention pruning, incident review, or
storage maintenance:

```bash
perfgate decision export --project my-project --days 90 --format jsonl --out artifacts/perfgate/decision-history.jsonl
perfgate audit export --project my-project --format jsonl --out artifacts/perfgate/audit.jsonl
```

For SQLite, combine database backups with JSONL exports. For PostgreSQL, use the
database platform backup policy and keep JSONL exports for portable audit
evidence.

## Pruning

Always preview retention changes first:

```bash
perfgate decision prune --project my-project --older-than 365d --dry-run
```

Only force prune after the export and audit window is complete:

```bash
perfgate decision export --project my-project --days 0 --format jsonl --out artifacts/perfgate/decision-history-before-prune.jsonl
perfgate audit export --project my-project --format jsonl --out artifacts/perfgate/audit-before-prune.jsonl
perfgate decision prune --project my-project --older-than 365d --force
```

Treat prune as a retention operation, not a cleanup habit. Keep the dry-run
output with the change record when pruning production history.

## CI Upload Behavior

CI should make the performance decision from local receipts first. Server upload
is a persistence step.

If upload fails:

- keep `decision.md`, `decision.index.json`, `tradeoff.json`, and related
  receipts as CI artifacts;
- rerun `perfgate decision upload` from the same artifacts after the server or
  credential issue is fixed;
- do not rerun benchmarks just to repair ledger persistence unless the receipts
  are missing or untrusted;
- fail the job only when the repository policy explicitly requires successful
  ledger persistence before merge.

For retry jobs, use the same `--file` and `--index` paths that were produced by
the original decision run.

## Dashboard Expectations

The dashboard is a review surface for retained evidence. Operators should expect
it to show recent decisions, audit events, debt summaries, and health state. It
is not the source of correctness for a decision; the linked receipts are.

Check service health and metrics before blaming perfgate verdicts:

```bash
curl -fsS http://localhost:8080/health
curl -fsS http://localhost:8080/metrics
```

## Failure Modes

| Symptom | Likely cause | Recovery |
|---------|--------------|----------|
| `401` or `403` from upload | Missing, expired, revoked, or under-scoped API key | Check CI secret, list keys, rotate if needed, rerun upload |
| SQLite busy or locked | Multiple writers or long-running local process | Stop extra server process, retry, consider Postgres for shared use |
| Upload fails but `decision.md` exists | Server persistence failed after local decision succeeded | Preserve artifacts and rerun `decision upload` |
| Prune removed too much | Forced prune without export or wrong retention window | Restore from database backup or JSONL export |
| Dashboard stale | Server reads old storage or upload failed | Check `/health`, audit events, and latest decision history |

## Proof Commands

```bash
cargo +1.95.0 run -p xtask -- docs-check
cargo +1.95.0 run -p xtask -- doc-test
cargo +1.95.0 run -p xtask -- docs-source-check
cargo +1.95.0 run -p xtask -- product-claims-check
```
