# Changelog

## 0.1.0 (Unreleased)

Initial release.

### Features

- **perfgate.toml language support**: Syntax highlighting via TextMate grammar, bracket matching, comment toggling, and folding for section headers.
- **Snippets**: Quick scaffolds for `[defaults]`, `[[bench]]`, `[bench.budgets.<metric>]`, `[baseline_server]`, full config, baseline patterns, and noise policies.
- **Task provider**: Automatically detects `perfgate.toml` and provides runnable tasks for `perfgate check`, `perfgate run`, and `perfgate compare`. Includes a cockpit-mode "check --all" task.
- **Problem matcher**: Parses `::error::` / `::warning::` annotations and `error: bench '...'` messages from perfgate CLI output into the VS Code Problems panel.
- **Diagnostics**: Watches `artifacts/perfgate/` for compare receipts and sensor reports, translates budget verdicts into inline diagnostics mapped to the originating `[[bench]]` entry in `perfgate.toml`.
- **Status bar**: Displays the last perfgate verdict (pass / warn / fail / error) in the VS Code status bar. Click to re-run check.
- **Commands**: `perfgate: Run Check`, `perfgate: Run Benchmark`, `perfgate: Compare Results`, `perfgate: View Last Report`, `perfgate: Open Dashboard`, `perfgate: Select Benchmark`.
- **Configuration**: `perfgate.binaryPath`, `perfgate.configPath`, `perfgate.serverUrl`, `perfgate.artifactDir`, `perfgate.autoRefreshDiagnostics`.
