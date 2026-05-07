# perfgate-adapters

Workspace-only compatibility wrapper for `perfgate::runtime`.

## Build and Test

```bash
cargo test -p perfgate-adapters
cargo test -p perfgate --all-targets --all-features app::runtime
```

## Design Rule

Do not add new implementation code here. Runtime process execution, host
probing, and platform-specific metrics live in `perfgate::runtime` and are
exposed publicly through `perfgate::runtime`.
