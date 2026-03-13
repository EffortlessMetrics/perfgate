# ADR 0003: Presentation Layer Split (Render, Export, CLI)

## Status
Accepted

## Context
The user-facing output logic (Markdown tables, JSON/CSV exports, and CLI interaction) was previously mingled in `perfgate-app` and `perfgate-cli`. This made it difficult to add new output formats without touching the core orchestration logic.

## Decision
We separated the presentation layer into three distinct crates:
- `perfgate-render`: Pure rendering logic for Markdown (using Handlebars) and terminal output.
- `perfgate-export`: Logic for transforming run/compare data into external formats (CSV, JSONL, HTML, Prometheus).
- `perfgate-cli`: Entry point responsible for argument parsing (clap) and wiring up the application.

## Consequences
- Rendering can be unit-tested by comparing string outputs without running the full application.
- Adding a new export format (e.g., OpenTelemetry) only requires changes to `perfgate-export`.
- `perfgate-cli` remains a thin wrapper around `perfgate-app`, focusing on the user interface and exit codes.
