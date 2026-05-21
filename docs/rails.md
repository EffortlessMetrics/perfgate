# Rails framework

This repository uses `.rails/` as its durable Rails knowledge base.

`docs/` explains Rails to humans; `.rails/` stores durable source-of-truth artifacts.

## Ownership boundaries

Rails owns and maintains:

- `.rails/` (proposals, specs, ADRs, lanes, templates, closeouts, support maps, policy references, receipts, and schemas)
- Human-facing guidance for this framework in `docs/rails.md` and `docs/contributing/rails.md`

Rails does **not** own, modify, migrate, or validate:

- `.codex/` (Codex execution state)
- `.spec/` (Spec Kit / speckit state)
- `.claude/` and `.jules/` (external agent/session state)

## Artifact graph contract

- Every Rails-owned artifact is linked through `.rails/index.toml`.
- No Rails-owned artifact path may live under `.codex/`, `.spec/`, `.claude/`, or `.jules/`.
- Lane sequencing lives in focused lane trackers under `.rails/lanes/`; do not create one global queue file.

Validate the registry with:

```bash
cargo run -p xtask -- rails check
```
