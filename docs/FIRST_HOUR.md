# First Hour With perfgate

This guide is for a cold user adding perfgate to an existing repository. It
keeps the path small: install, check the environment, initialize with
reviewable benchmark suggestions, run one local check, promote the first
baseline, prove the CI-equivalent gate locally, commit the durable files, and
let CI reproduce the same gate.

You do not need the server, probes, scenarios, or structured decisions for this
first hour.

## 1. Install

Use the binary installer path first:

```bash
cargo binstall perfgate-cli
```

If `cargo-binstall` is not available:

```bash
cargo install perfgate-cli
```

Check that the installed binary is usable:

```bash
perfgate --version
perfgate doctor --help
```

## 2. Initialize The Repo

Run this from the repository root:

```bash
perfgate init --ci github --profile standard --suggest-benches
```

Expected new files:

```text
perfgate.toml
.github/workflows/perfgate.yml
baselines/.gitkeep
.perfgate/README.md
```

Open `perfgate.toml` and replace the generated benchmark command with a real
command for your project. `--suggest-benches` appends commented candidates for
common repo shapes; they are suggestions, not policy. Keep the first benchmark
simple and deterministic.

Good first-hour benchmarks are usually fast, stable, and close to the workload
you want to protect. Avoid making a compile-heavy command the first required
gate until you have calibrated its noise.

## 3. Check The Setup

Run doctor against the generated config:

```bash
perfgate doctor --config perfgate.toml
```

A useful first result is either all pass, or a direct setup failure such as a
missing benchmark command. Fix setup failures before promoting a baseline.

## 4. Run The First Check

Run the local gate:

```bash
perfgate check --config perfgate.toml --all
```

Expected artifact directory:

```text
artifacts/perfgate/
```

Expected first-run behavior:

- perfgate runs each configured benchmark command;
- perfgate writes run receipts under the artifact directory;
- if no baseline exists yet, perfgate tells you to promote the first trusted
  local result instead of silently inventing a baseline.

Useful exit-code meanings:

```text
0  success, or warning without fail-on-warn
1  tool/runtime error
2  policy failure
3  warning treated as failure
```

## 5. Promote The First Baseline

After one local run looks representative, promote it:

```bash
perfgate baseline promote --config perfgate.toml --all
```

Expected durable baseline files:

```text
baselines/
```

These files are the comparison point for future local and CI checks.

## 6. Prove The CI-Equivalent Gate

After promotion, run the same baseline-required check that the generated
workflow will run:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

This is the point where a missing baseline becomes setup drift instead of a
normal first-run condition. Fix setup drift before pushing the branch.

## 7. Commit The Right Files

Commit the durable setup and baseline:

```bash
git add perfgate.toml .github/workflows/perfgate.yml baselines/ .perfgate/README.md
git commit -m "ci: add perfgate performance gate"
```

Usually commit:

- `perfgate.toml`
- `.github/workflows/perfgate.yml`
- `baselines/`
- `.perfgate/README.md`

Usually do not commit:

- `artifacts/perfgate/`
- temporary benchmark output
- local server databases
- one-off decision bundles unless they are attached to a release, audit, issue,
  or review record on purpose

## 8. Run CI

Push the branch and let the generated GitHub workflow run. The workflow uses
the repository action:

```yaml
- uses: EffortlessMetrics/perfgate@v0
  with:
    config: perfgate.toml
    all: "true"
    require_baseline: "true"
    upload_artifact: "true"
```

Use `@v0.18.0` for an exact patch pin, `@v0.18` for the current 0.18 line, or
`@v0` to follow the current compatible action tag.

## 9. Understand Pass And Fail

A passing check means the current benchmark receipts are within configured
budget policy relative to the committed baseline.

A failing check means one of these happened:

- the benchmark command could not run;
- perfgate could not read config, baselines, or artifact paths;
- a metric exceeded the configured budget;
- warning policy was configured to fail the gate.

For local reproduction, run the same command CI ran:

```bash
perfgate check --config perfgate.toml --all --require-baseline
```

Then inspect:

```text
artifacts/perfgate/
```

If the failure is a real intended performance change, rerun the benchmark
enough times to trust the new result, then promote the new baseline in a
separate, reviewable commit.

## 10. Grow Later

After the basic gate is stable:

- add GitHub Action decision mode when CI should explain structured tradeoffs;
- add probes when you need to show where work moved inside a benchmark;
- add the server ledger when a team needs shared decision history and debt
  summaries.

Start with [`PERFORMANCE_DECISIONS.md`](PERFORMANCE_DECISIONS.md) when you are
ready for structured decisions.
