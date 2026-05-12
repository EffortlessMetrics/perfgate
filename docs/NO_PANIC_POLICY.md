# No-Panic Policy

perfgate has an exact panic-family scanner, an intentionally empty allowlist,
and a generated baseline. The Rust 1.95 and 0.17.0 governance rollout uses
these as a no-new-debt gate rather than a broad cleanup commit.

This policy must start with exact identities. A scanner keyed only by file and
family can hide new callsites inside an already-allowed file.

## Governed Families

The scanner classifies panic-family callsites such as:

- `panic!`
- `unreachable!`
- `todo!`
- `unimplemented!`
- `unwrap`
- `expect`

The policy should distinguish production, test, doc-test, generated, and
example surfaces instead of granting a blanket test carveout.

Run the current policy scanner with:

```bash
cargo run -p xtask -- policy check-no-panic-family
```

Refresh the generated baseline only after removing existing debt:

```bash
cargo run -p xtask -- policy check-no-panic-family --write-baseline
```

Baseline refreshes fail before writing if they would absorb a new
unallowlisted identity or a count increase. Intentional new debt belongs in
`policy/no-panic-allowlist.toml` with an owner, reason, and review date.

## Exact Identity

Each allowed or baselined identity must include:

| Field | Purpose |
|-------|---------|
| `path` | Source path containing the callsite. |
| `family` | Panic-family group. |
| `selector_kind` | Macro, method, function, or other selector class. |
| `selector_callee` | Exact callee or macro selector. |
| `snippet` | Stable local source snippet for review. |
| `count` | Expected count for that identity. |

Allowlist entries additionally require:

| Field | Purpose |
|-------|---------|
| `owner` | Owner responsible for review. |
| `reason` | Why the debt is intentional or temporarily accepted. |
| `review_after` | Date or release when it must be revisited. |

## Baseline Rule

The generated baseline may shrink when debt disappears. It must not silently
absorb new debt.

Allowed refresh behavior:

- remove identities that no longer exist,
- update generated ordering,
- fail when a new unallowed identity appears,
- fail when an existing identity count increases without an allowlist update.

## Rollout Rules

1. Add exact policy and scanner first.
2. Keep the generated no-new-debt baseline marked in `.gitattributes`.
3. Refresh the baseline only when debt shrinks or disappears.
4. Keep production and test policy choices explicit.
5. Treat broad carveouts as temporary debt with an owner and review date.

See [Rust 1.95 and 0.17.0 Governance Rollout](development/RUST_1_95_ROLLOUT.md).
