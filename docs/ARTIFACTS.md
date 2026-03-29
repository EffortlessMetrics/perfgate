# Artifact Layouts

perfgate writes artifacts in a predictable structure.

## Standard Mode

```
artifacts/perfgate/
├── run.json        # perfgate.run.v1 - raw measurement receipt
├── compare.json    # perfgate.compare.v1 - comparison result
├── report.json     # perfgate.report.v1 - cockpit ingestion format
└── comment.md      # Markdown artifact reused for sticky PR comments
```

When no baseline exists:
- `report.json` and `comment.md` are always written
- `compare.json` is omitted
- `report.json` uses verdict reason token `no_baseline`

To post the artifact as a sticky GitHub PR comment, use:

```bash
perfgate comment --body-file artifacts/perfgate/comment.md --repo owner/repo --pr 123
```

The hidden sticky marker is added at post time so `comment.md` remains plain
Markdown for local review and non-GitHub consumers.

## Cockpit Mode

See [COCKPIT_MODE.md](COCKPIT_MODE.md) for cockpit-specific layouts.

## Schemas

See [SCHEMAS.md](SCHEMAS.md) for receipt type documentation and validation.
