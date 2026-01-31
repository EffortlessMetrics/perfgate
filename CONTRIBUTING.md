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
cargo run -p xtask -- mutants
```

## Fuzzing

See `fuzz/README.md`.
