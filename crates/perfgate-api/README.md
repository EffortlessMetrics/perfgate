# perfgate-api

Workspace-only compatibility wrapper for baseline service API contracts.

The baseline service wire types now live in `perfgate-types`:

```rust
use perfgate_types::baseline_service::{ListBaselinesQuery, UploadBaselineRequest};
use perfgate_types::baseline_service::auth::{Role, Scope};
```

`perfgate-api` remains in the workspace with `publish = false` so existing
internal imports can migrate gradually. New code should depend on
`perfgate-types` for request/response/auth contract types.

Runtime credential-source loading for the server moved to
`perfgate_server::CredentialSource`; it is intentionally not part of
`perfgate-types`.

## License

Licensed under either Apache-2.0 or MIT.
