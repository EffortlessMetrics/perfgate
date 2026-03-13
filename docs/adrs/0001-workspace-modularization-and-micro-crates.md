# ADR 0001: Workspace Modularization and Micro-crates

## Status
Accepted

## Context
As `perfgate` evolved, the core logic became increasingly complex. Maintaining a monolithic or few-crate structure led to:
- Long compilation times due to large crate boundaries.
- Tight coupling between unrelated components (e.g., statistical math and CLI formatting).
- Difficulty in reusing specific parts of the system (like the measurement model) in other tools.

## Decision
We decided to decompose the workspace into 19 specialized "micro-crates". Each crate has a single, narrow responsibility and a minimal dependency footprint.

The workspace is now organized into:
- **Core logic**: `perfgate-types`, `perfgate-domain`, `perfgate-stats`, `perfgate-budget`.
- **Infrastructure**: `perfgate-adapters`, `perfgate-host-detect`, `perfgate-sha256`.
- **Use cases**: `perfgate-app`, `perfgate-paired`.
- **Presentation**: `perfgate-render`, `perfgate-export`, `perfgate-cli`, `perfgate-sensor`.
- **Ecosystem**: `perfgate-server`, `perfgate-client`.
- **Cross-cutting**: `perfgate-error`, `perfgate-validation`, `perfgate-fake`.

## Consequences
- **Improved Build Performance**: Incremental builds are faster as changes are localized to small crates.
- **Enforced Boundaries**: Circular dependencies are impossible, and internal details are hidden behind public APIs of small crates.
- **Testability**: Crates can be tested in isolation with minimal mocking.
- **Complexity**: The number of `Cargo.toml` files increased, requiring more workspace management overhead.
