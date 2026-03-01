# perfgate-fake

Test utilities and fake implementations for perfgate testing.

Part of the [perfgate](https://github.com/nicholasgasior/perfgate) workspace.

## Overview

Deterministic, configurable test doubles for the perfgate adapter traits.
Use these in unit tests and integration tests to avoid I/O and ensure
reproducible test results.

## Key API

- `FakeProcessRunner` — configurable process runner returning pre-set results per command
- `FakeHostProbe` — configurable host probe returning pre-set system information
- `FakeClock` — configurable clock for deterministic time-based testing
- `MockProcessBuilder` — builder pattern for creating `RunResult` instances with sensible defaults

## Example

```rust
use perfgate_fake::{FakeProcessRunner, MockProcessBuilder, FakeClock};
use perfgate_adapters::{ProcessRunner, CommandSpec};
use std::time::Duration;

// Build a mock result with the fluent builder
let result = MockProcessBuilder::new()
    .exit_code(0)
    .wall_ms(100)
    .stdout(b"hello world\n".to_vec())
    .build();

// Configure the fake runner
let runner = FakeProcessRunner::new();
runner.set_result(&["echo", "hello"], result);

let spec = CommandSpec {
    argv: vec!["echo".to_string(), "hello".to_string()],
    cwd: None,
    env: vec![],
    timeout: None,
    output_cap_bytes: 1024,
};

let output = runner.run(&spec).unwrap();
assert_eq!(output.exit_code, 0);
assert_eq!(output.wall_ms, 100);

// Use the fake clock for deterministic timing
let clock = FakeClock::new().with_millis(1000);
clock.advance(Duration::from_millis(500));
assert_eq!(clock.now_millis(), 1500);
```

## License

Licensed under either Apache-2.0 or MIT.
