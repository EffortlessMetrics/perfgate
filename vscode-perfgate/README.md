# perfgate - Performance Budgets for VS Code

Performance regression detection and budget management, integrated directly into your editor.

[perfgate](https://github.com/EffortlessMetrics/perfgate) is a Rust CLI tool for performance budgets and baseline diffs in CI. This extension brings that workflow into VS Code with syntax support, inline diagnostics, and one-click benchmark execution.

## Features

### perfgate.toml Language Support

Full editing support for `perfgate.toml` configuration files:

- Syntax highlighting for sections (`[defaults]`, `[[bench]]`, `[bench.budgets.*]`, `[baseline_server]`), keys, values, comments, and metric names
- Bracket matching and auto-closing
- Comment toggling with `Ctrl+/`
- Section folding

<!-- Screenshot: perfgate.toml with syntax highlighting -->

### Snippets

Quickly scaffold configuration with built-in snippets:

| Prefix | Description |
|--------|-------------|
| `defaults` | `[defaults]` block with repeat, warmup, threshold, warn_factor, baseline_dir |
| `bench` | `[[bench]]` entry with name, command, and optional work |
| `bench-budget` | `[[bench]]` entry with inline budget override |
| `budget` | `[bench.budgets.<metric>]` section for fine-grained control |
| `server` | `[baseline_server]` block for centralized baselines |
| `perfgate-full` | Complete configuration scaffold |
| `baseline-pattern` | `baseline_pattern` with `{bench}` placeholder |
| `noise` | Noise threshold and policy settings |

### Task Provider

Automatically detects `perfgate.toml` and provides runnable tasks (Terminal > Run Task):

- **perfgate: check --all** - Run all benchmarks in cockpit mode
- **perfgate: check: \<name\>** - Run a specific benchmark check
- **perfgate: run: \<name\>** - Execute a benchmark and capture results
- **perfgate: compare** - Compare baseline vs current results

All tasks use the `$perfgate` problem matcher so warnings and errors appear in the Problems panel.

<!-- Screenshot: task list showing auto-detected perfgate tasks -->

### Diagnostics / Problems Panel

Budget violations from perfgate output are shown as VS Code diagnostics:

- **Errors** for `fail` verdicts (budget violated)
- **Warnings** for `warn` verdicts (approaching budget)
- Diagnostics are mapped to the corresponding `[[bench]]` entry in `perfgate.toml`
- Automatically refreshes when `artifacts/perfgate/compare.json` or sensor reports change

Supports both standard mode (`compare.json`) and cockpit mode (`sensor.report.v1`) output.

<!-- Screenshot: Problems panel showing perfgate budget violations -->

### Status Bar

A status bar item at the bottom of VS Code shows the last perfgate verdict:

- **pass** - All benchmarks within budget
- **warn** - Some metrics approaching budget threshold
- **fail** - Budget violated
- **running** - Check in progress (animated spinner)

Click the status bar item to run a new check.

<!-- Screenshot: status bar showing verdict states -->

### Commands

Available from the Command Palette (`Ctrl+Shift+P`):

| Command | Description |
|---------|-------------|
| `perfgate: Run Check` | Select a benchmark and run `perfgate check` |
| `perfgate: Run Benchmark` | Execute a benchmark with custom command |
| `perfgate: Compare Results` | Compare two JSON receipt files |
| `perfgate: View Last Report` | Open the most recent report/compare JSON |
| `perfgate: Open Dashboard` | Open the Baseline Service dashboard in browser |
| `perfgate: Select Benchmark` | Pick a benchmark from perfgate.toml |

### Problem Matchers

Two problem matchers are included for parsing perfgate CLI output:

- `$perfgate` - Matches `::error::` and `::warning::` annotations (GitHub Actions format)
- `$perfgate-check` - Matches `error: bench '<name>': <message>` lines

## Installation

### From OpenVSIX / VS Code Marketplace

Search for **perfgate** in the Extensions view (`Ctrl+Shift+X`).

### From VSIX

1. Download the `.vsix` file from the [releases page](https://github.com/EffortlessMetrics/perfgate/releases)
2. Install via `code --install-extension perfgate-0.1.0.vsix`

### Prerequisites

The [perfgate CLI](https://github.com/EffortlessMetrics/perfgate) must be installed and available on your `$PATH`, or configure `perfgate.binaryPath` to point to the binary.

```bash
cargo install perfgate-cli
```

## Configuration

All settings are under the `perfgate.*` namespace:

| Setting | Default | Description |
|---------|---------|-------------|
| `perfgate.binaryPath` | `"perfgate"` | Path to the perfgate CLI binary |
| `perfgate.configPath` | `"perfgate.toml"` | Config file path relative to workspace root |
| `perfgate.serverUrl` | `""` | Baseline Service URL for dashboard link |
| `perfgate.artifactDir` | `"artifacts/perfgate"` | Directory for perfgate output artifacts |
| `perfgate.autoRefreshDiagnostics` | `true` | Auto-refresh diagnostics on output file changes |

## How It Works

1. The extension activates when it finds a `perfgate.toml` file in your workspace
2. It parses the config to discover `[[bench]]` entries and provide tasks/snippets
3. When you run a check, it executes the perfgate CLI as a VS Code task with a problem matcher
4. After the task completes, it reads the JSON output artifacts and translates budget verdicts into VS Code diagnostics
5. The status bar updates to reflect the latest verdict

## Exit Codes

perfgate CLI uses specific exit codes that the extension interprets:

| Code | Meaning | Status Bar |
|------|---------|------------|
| `0` | Success | pass |
| `1` | Runtime error | error |
| `2` | Budget violated | fail |
| `3` | Warn (with `--fail-on-warn`) | warn |

## Development

```bash
cd vscode-perfgate
npm install
npm run compile
# Press F5 in VS Code to launch Extension Development Host
```

## License

MIT - see [LICENSE](LICENSE).
