# Project Structure

## Architecture
Clean architecture with layered crates - dependencies flow inward only.

```
crates/
├── perfgate-types/     # Shared types, receipts, JSON schema support
├── perfgate-domain/    # Pure math/policy (I/O-free)
├── perfgate-adapters/  # Process runner, system metrics (I/O layer)
├── perfgate-app/       # Use-cases, rendering (coordinates domain + adapters)
└── perfgate-cli/       # CLI interface (clap), JSON read/write
```

## Crate Responsibilities

### perfgate-types
- Receipt structs (`RunReceipt`, `CompareReceipt`)
- Config file schema
- Metric/Budget/Verdict types
- JSON schema derivation via `schemars`

### perfgate-domain
- Statistics computation (median, min, max)
- Budget comparison logic
- No I/O - pure functions only

### perfgate-adapters
- `ProcessRunner` trait + `StdProcessRunner` impl
- Unix-specific: `wait4()` for rusage, timeout handling
- Portable fallback for non-Unix

### perfgate-app
- `RunBenchUseCase`, `CompareUseCase`
- `Clock` trait for testability
- Markdown and GitHub annotation rendering

### perfgate-cli
- Clap command definitions
- File I/O (atomic writes)
- Exit code handling

## Other Directories

```
xtask/          # Repo automation (ci, schema gen, mutants)
fuzz/           # Fuzz targets (excluded from workspace, requires nightly)
schemas/        # Generated JSON schemas
```

## Design Principles
- Versioned, explicit, boring types
- I/O at the edges only
- Testable via trait injection (Clock, ProcessRunner)
- BTreeMap for deterministic JSON key ordering
