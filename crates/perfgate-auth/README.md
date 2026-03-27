# perfgate-auth

Authentication and authorization types for the perfgate baseline service.

Part of the [perfgate](https://github.com/EffortlessMetrics/perfgate) workspace.

## Problem

The baseline service controls who can read, write, promote, and delete baselines
-- scoped to projects and optionally to benchmarks. This crate defines the
shared type vocabulary used by server, client, and CLI.

## Key Types

| Type | Purpose |
|------|---------|
| `ApiKey` | Authenticated key with project scope, role, and optional benchmark regex |
| `Role` | Viewer, Contributor, Promoter, Admin -- cumulative scopes |
| `Scope` | Granular permission: Read, Write, Promote, Delete, Admin |
| `JwtClaims` | JWT payload accepted by the server (subject, project, scopes, expiry) |

## API Key Format

- `pg_live_<32+ alphanumeric>` -- production keys
- `pg_test_<32+ alphanumeric>` -- test/sandbox keys

`validate_key_format()` enforces prefix and length; `generate_api_key(test)` mints new keys.

## Roles and Scopes

```text
Viewer       -> [Read]
Contributor  -> [Read, Write]
Promoter     -> [Read, Write, Promote]
Admin        -> [Read, Write, Promote, Delete, Admin]
```

`Role::from_scopes()` infers the closest built-in role from an arbitrary scope
set, useful when mapping JWT claims to role-based checks.

## Example

```rust
use perfgate_auth::{ApiKey, Role, Scope, validate_key_format};

let key = ApiKey::new(
    "key_1".into(),
    "CI writer".into(),
    "my-project".into(),
    Role::Contributor,
);
assert!(key.has_scope(Scope::Write));
assert!(!key.has_scope(Scope::Delete));

assert!(validate_key_format("pg_live_abcdefghijklmnopqrstuvwxyz123456").is_ok());
```

## License

Licensed under either Apache-2.0 or MIT.
