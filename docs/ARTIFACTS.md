# Artifact Layouts

perfgate writes artifacts in a predictable structure.

## Standard Mode

```
artifacts/perfgate/
├── run.json        # perfgate.run.v1 - raw measurement receipt
├── compare.json    # perfgate.compare.v1 - comparison result
├── report.json     # perfgate.report.v1 - cockpit ingestion format
└── comment.md      # PR comment markdown
```

When no baseline exists:
- `report.json` and `comment.md` are always written
- `compare.json` is omitted
- `report.json` uses verdict reason token `no_baseline`

## Cockpit Mode

See [COCKPIT_MODE.md](COCKPIT_MODE.md) for cockpit-specific layouts.

## Schemas

See [SCHEMAS.md](SCHEMAS.md) for receipt type documentation and validation.
