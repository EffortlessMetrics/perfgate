# Contributing

## Local workflow

```bash
cargo run -p xtask -- ci
```

## Changelog

When adding features or fixing bugs, update [CHANGELOG.md](CHANGELOG.md) under the `[Unreleased]` section following the [Keep a Changelog](https://keepachangelog.com/) format.

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
