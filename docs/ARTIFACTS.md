# Artifact Layouts

perfgate writes artifacts in a predictable structure.

## Standard `check --bench`

```text
artifacts/perfgate/
  run.json        # perfgate.run.v1 - raw measurement receipt
  compare.json    # perfgate.compare.v1 - comparison result
  report.json     # perfgate.report.v1 - verdict summary
  comment.md      # PR comment markdown
```

When no baseline exists:
- `report.json` and `comment.md` are always written
- `compare.json` is omitted
- `report.json` uses verdict reason token `no_baseline`

## Standard `check --all`

`check --all` writes each benchmark under the configured artifact directory,
even when the config contains only one benchmark:

```text
artifacts/perfgate/<bench>/
  run.json
  compare.json    # when a baseline exists
  report.json
  comment.md
```

## Cockpit Mode

See [COCKPIT_MODE.md](COCKPIT_MODE.md) for cockpit-specific layouts.

## Schemas

See [SCHEMAS.md](SCHEMAS.md) for receipt type documentation and validation.
