# perfgate-adapters

Workspace-only compatibility wrapper for `perfgate_app::runtime`.

## Build and Test

```bash
cargo test -p perfgate-adapters
cargo test -p perfgate-app runtime --all-targets
```

## Design Rule

Do not add new implementation code here. Runtime process execution, host
probing, and platform-specific metrics live in `perfgate_app::runtime` and are
exposed publicly through `perfgate::runtime`.
