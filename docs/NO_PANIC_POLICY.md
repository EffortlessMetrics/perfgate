# No-Panic Policy

perfgate currently has no enforced panic-family policy. The Rust 1.95 and
0.17.0 governance rollout adds one as a no-new-debt gate rather than a broad
cleanup commit.

This policy must start with exact identities. A scanner keyed only by file and
family can hide new callsites inside an already-allowed file.

## Governed Families

The first scanner should classify panic-family callsites such as:

- `panic!`
- `unreachable!`
- `todo!`
- `unimplemented!`
- `unwrap`
- `expect`

The policy should distinguish production, test, doc-test, generated, and
example surfaces instead of granting a blanket test carveout.

## Exact Identity

Each allowed or baselined callsite must include:

| Field | Purpose |
|-------|---------|
| `path` | Source path containing the callsite. |
| `family` | Panic-family group. |
| `selector_kind` | Macro, method, function, or other selector class. |
| `selector_callee` | Exact callee or macro selector. |
| `snippet` | Stable local source snippet for review. |
| `count` | Expected count for that identity. |
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
2. Add the generated no-new-debt baseline in the next PR.
3. Mark the baseline generated in `.gitattributes`.
4. Keep production and test policy choices explicit.
5. Treat broad carveouts as temporary debt with an owner and review date.

See [Rust 1.95 and 0.17.0 Governance Rollout](development/RUST_1_95_ROLLOUT.md).
