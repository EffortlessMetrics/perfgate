# perfgate-render

Compatibility wrapper for perfgate rendering APIs.

New code should use the facade path:

```rust
use perfgate::presentation::render::{github_annotations, render_markdown};
```

This package remains in the workspace only to preserve the 0.16 migration path.
It is not part of the target public package surface.

## License

Licensed under either Apache-2.0 or MIT.
