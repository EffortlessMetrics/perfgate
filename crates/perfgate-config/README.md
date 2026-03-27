# perfgate-config

Configuration loading with three-source priority merge: **CLI flags > environment variables > config file**.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Merge behavior

Each setting is resolved highest-priority-first; the first non-empty value wins:

| Setting  | CLI flag       | Env var                | File key (`perfgate.toml`)          |
|----------|----------------|------------------------|-------------------------------------|
| URL      | `--server-url` | `PERFGATE_SERVER_URL`  | `[baseline_server].url`             |
| API key  | `--api-key`    | `PERFGATE_API_KEY`     | `[baseline_server].api_key`         |
| Project  | `--project`    | `PERFGATE_PROJECT`     | `[baseline_server].project`         |

Config files are loaded from `perfgate.toml` (TOML) or `perfgate.json` (JSON).
If the file does not exist, defaults are used silently.

## Key types

- `ResolvedServerConfig` -- merged settings with helpers to create a `BaselineClient` or `FallbackClient`
- `load_config_file(path)` -- load a `ConfigFile` from TOML or JSON
- `resolve_server_config(flag_url, flag_key, flag_project, file_config)` -- merge all sources

## Example

```rust
use perfgate_config::{load_config_file, resolve_server_config};
use std::path::Path;

let file = load_config_file(Path::new("perfgate.toml")).unwrap();
let cfg = resolve_server_config(
    None,               // no CLI flag
    None,               // no CLI flag
    None,               // no CLI flag
    &file.baseline_server,
);

if cfg.is_configured() {
    let client = cfg.create_client().unwrap();
}
```

## License

Licensed under either Apache-2.0 or MIT.
