# Host Mismatch Detection

When comparing runs from different machines, perfgate can detect and warn about
hardware inconsistencies using host fingerprints embedded in run receipts.

## Usage

```bash
perfgate compare \
  --baseline baselines/bench.json \
  --current run.json \
  --host-mismatch warn \
  --out compare.json
```

## Modes

| Mode | Behavior |
|------|----------|
| `ignore` | Silently allow cross-host comparisons |
| `warn` | Emit a warning but continue |
| `error` | Treat mismatch as an error (exit code 1) |

## When This Matters

- Baselines generated on dedicated benchmark machines, CI runs on different hardware
- Fleet runners with heterogeneous specs
- Local development vs CI comparisons

The default is `ignore`. Use `warn` in CI to surface potential issues without
blocking, or `error` when you need strict hardware consistency.
