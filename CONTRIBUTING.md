# Contributing

## Local workflow

```bash
cargo run -p xtask -- ci
```

## Schemas

```bash
cargo run -p xtask -- schema
```

## Mutation testing

Install:

```bash
cargo install cargo-mutants
```

Run:

```bash
# Run on all crates
cargo run -p xtask -- mutants

# Run on specific crate with summary
cargo run -p xtask -- mutants --crate perfgate-domain --summary
```

See [docs/MUTATION_TESTING.md](docs/MUTATION_TESTING.md) for detailed documentation including target kill rates and troubleshooting.

## Fuzzing

See `fuzz/README.md`.
