# Proof Freshness

Proof freshness describes how recently a claim's evidence was exercised. It is
not a support tier. Support tiers say how strong the product promise is;
freshness says whether the linked proof is current enough to rely on.

Use freshness labels in product claims, canary records, release notes, and
handoffs when stale evidence could otherwise sound stronger than it is.

## Freshness States

| State | Meaning | Use |
|-------|---------|-----|
| `current` | Proof applies to the current public release or current active lane boundary. | Use for release smoke, focused tests, and docs/source checks run after the latest relevant change. |
| `recent` | Proof is still relevant, but was not rerun from every current public artifact or every current surface. | Use for external canaries or examples that still describe the behavior but were not rerun for the newest release. |
| `stale` | Proof may be useful history, but should not support a new or promoted claim by itself. | Use when behavior, action wiring, public artifacts, or platform assumptions changed after the proof. |
| `superseded` | Newer proof or an explicit closeout replaced this evidence. | Keep as history only; cite the newer proof for active claims. |
| `unproven` | No durable proof artifact exists yet for this claim shape. | Keep visible when a lane intentionally leaves a gap, such as a probe-backed external canary. |

## Product Claim Use

Each product claim can cite a mix of proof freshness states. The claim should
use the weakest relevant state in its language:

- `current` proof can support current behavior claims.
- `recent` proof can support bounded claims with explicit limits.
- `stale` proof needs rerun or corroboration before claim promotion.
- `superseded` proof should point to the replacement.
- `unproven` gaps should stay in "known limits" or canary matrices, not in
  positive claim text.

## Claim Promotion Discipline

Policy rollout claims should carry an explicit `Proof freshness:` field before
they are promoted or used as support for team policy. The product-claims check
accepts only these freshness states:

```text
current
recent
stale
superseded
unproven
```

`stable` and `supported` claims cannot use `stale`, `superseded`, or
`unproven` as their proof freshness. Refresh the proof, lower the claim
language, or keep the gap in "Known limits" instead.

Freshness still does not make advisory guidance blocking. A current maturity or
policy-rollout claim can support a recommendation, but required gates still
need a deliberate policy patch and reviewer approval.

## Non-Inferences

- Fresh proof for one repo shape does not prove every repo shape.
- Fresh public install smoke does not prove hosted external CI.
- Fresh in-repo server tests do not prove production database operations.
- Fresh repair-context fixtures do not make agents policy authorities.
- Fresh maturity guidance does not make advisory checks blocking gates.
