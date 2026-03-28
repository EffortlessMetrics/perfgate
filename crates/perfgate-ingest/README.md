# perfgate-ingest

**Import external benchmark outputs into `perfgate.run.v1` receipts.**

`perfgate-ingest` is the library behind the `perfgate ingest` CLI flow. It
parses common benchmark formats and converts them into perfgate's native run
receipt so the rest of the pipeline can treat imported data the same as locally
measured runs.

Supported inputs:
- Criterion `estimates.json`
- hyperfine `--export-json`
- Go benchmark text output from `go test -bench . -benchmem`
- pytest-benchmark JSON

## Library Usage

```rust
use perfgate_ingest::{ingest, IngestFormat, IngestRequest};

let request = IngestRequest {
    format: IngestFormat::Hyperfine,
    input: std::fs::read_to_string("hyperfine.json")?,
    name: Some("cli-bench".to_string()),
};

let receipt = ingest(&request)?;
println!("{}", receipt.bench.name);
# Ok::<(), anyhow::Error>(())
```

If you just want the command-line workflow, use the `perfgate ingest` command
from the `perfgate-cli` crate instead of depending on this crate directly.

## More

- Workspace overview: [README.md](../../README.md)
- CLI usage: [crates/perfgate-cli/README.md](../perfgate-cli/README.md)
- API docs: [docs.rs/perfgate-ingest](https://docs.rs/perfgate-ingest)

## License

Licensed under either of:
- Apache License, Version 2.0
- MIT license
