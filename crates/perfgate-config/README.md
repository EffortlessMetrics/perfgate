# perfgate-config

Configuration loading and merging logic for perfgate.

## Overview

`perfgate-config` resolves the effective perfgate configuration by merging
values from three sources in priority order:

1. **CLI flags** (highest priority)
2. **Environment variables** (`PERFGATE_SERVER_URL`, `PERFGATE_API_KEY`, `PERFGATE_PROJECT`)
3. **Config file** (`perfgate.toml` or `perfgate.json`)

The merged result is a `ResolvedServerConfig` that can create ready-to-use
`BaselineClient` or `FallbackClient` instances for talking to the centralized
Baseline Service.

## Key API

### Functions

- `load_config_file(path)` — reads a `perfgate.toml` or `perfgate.json` file
  and deserializes it into a `ConfigFile`. Returns the default config if the
  file does not exist.
- `resolve_server_config(flag_url, flag_key, flag_project, file_config)` —
  merges CLI flags with the `[baseline_server]` section of the config file,
  producing a `ResolvedServerConfig`.

### `ResolvedServerConfig`

| Method | Description |
|--------|-------------|
| `is_configured()` | `true` when a server URL is present |
| `create_client()` | builds a `BaselineClient` from the merged settings |
| `create_fallback_client(dir)` | wraps the client in a `FallbackClient` that falls back to local storage |
| `require_fallback_client(dir, msg)` | like above, but returns an error if the server is not configured |
| `resolve_project(override)` | resolves the project name from the override, config, or returns an error |

## Example

```rust
use std::path::Path;
use perfgate_config::{load_config_file, resolve_server_config};

let config = load_config_file(Path::new("perfgate.toml"))?;
let resolved = resolve_server_config(
    Some("https://baselines.example.com".into()),
    None,
    None,
    &config.baseline_server,
);

if resolved.is_configured() {
    let client = resolved.create_client()?;
    // use client to upload/download baselines...
}
```

## Workspace Role

`perfgate-config` bridges configuration sources and the client library:

`perfgate-types` + `perfgate-client` -> **`perfgate-config`** -> `perfgate-app` / `perfgate-cli`

## License

Licensed under either Apache-2.0 or MIT.
