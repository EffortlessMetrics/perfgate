# Coverage

Codecov coverage is execution-surface evidence.

It answers:

> Did tests execute this Rust surface?

It does not answer:

- whether performance-regression decisions are statistically valid,
- whether benchmark samples are stable,
- whether baselines are trustworthy,
- whether host mismatch detection is correct,
- whether the baseline server is production-ready,
- whether mutation adequacy is strong,
- whether publish readiness is proven.

Those are separate proof lanes.

## Workflow

The Coverage workflow runs on:

- push to `main`,
- `workflow_dispatch` (manual trigger),
- PRs labeled `coverage` or `full-ci`.

## Evidence Artifacts

Coverage workflow emits:

- `coverage.json` -- JSON summary of coverage by file and function
- `coverage.txt` -- Text report of line/branch/function coverage
- `lcov.info` -- Standard LCOV format for tool integration
- GitHub Actions artifact `coverage-report` (14-day retention)
- Codecov dashboard

## Configuration

See `codecov.yml` for Codecov status and reporting settings.
Current configuration uses informational (non-blocking) checks while real data accumulates on `main`.

## Ratcheting

Once stable main-branch data exists, thresholds will be ratcheted incrementally
to match actual coverage levels, then improved over time.
