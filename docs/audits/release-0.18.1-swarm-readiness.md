# v0.18.1 Swarm Readiness Packet

Date: 2026-06-11

Source branch: `chore/0.18.1-release-readiness-truth-refresh` on `perfgate-swarm`

Main SHA: `f8304daf3f1a8a22be2d7e7be6f4f9f1e8a0f1a84`

Purpose:

This packet is the swarm-side readiness handoff for the v0.18.1 patch-release hardening
lane. It is a narrow trust-hardening snapshot intended for the canonical publish lane
transition into `EffortlessMetrics/perfgate`.

Scope:

- first-use guidance clarity
- first-run artifact-layout and bootstrap repair guidance
- no-panic governance truthing
- action wiring and proof references

This packet does not perform publish, tag, alias movement, or hosted release-canary
actions.

## What landed in 0.18.1 patch source

Recent merged PRs included in this batch:

| PR | Title | Merged |
| --- | ----- | ------ |
| #248 | chore: refresh github action pins | 2026-06-05 |
| #249 | fix: explain missing first-run compare artifacts | 2026-06-05 |
| #250 | fix: remove production no-panic callsites | 2026-06-05 |
| #251 | docs: align public install and cli readme path | 2026-06-05 |
| #252 | fix: surface first-run cli guidance | 2026-06-05 |
| #253 | docs: archive stale release status wording | 2026-06-05 |
| #254 | docs: guard baseline promotion examples | 2026-06-05 |
| #255 | fix: fail baseline commands on missing config | 2026-06-05 |
| #256 | Scope baseline bootstrap guidance to selected bench | 2026-06-05 |
| #257 | Fix action summary test fixture | 2026-06-05 |
| #258 | docs: clarify check artifact layouts | 2026-06-05 |

## Readiness status

The batch is intended as a patch-release hardening layer. It improves first-run
recovery and release-trust wording and keeps this lane free of broad new-product
surfaces.

### Included user-facing improvements

- Missing compare path and missing baseline guidance now prints clearer recovery
  instructions.
- First-run outputs now expose artifact/layout expectations and guidance.
- First-run bootstrap and baseline workflow docs explicitly handle edge states.
- Baseline promotion examples and artifact checks are constrained to safe user paths.
- Action summary fixture and action-install examples are refreshed.
- Release status wording and pin guidance are aligned with patch intent.

### Known non-goals / explicit deferrals

- No new `review-explain`, benchmark passport, policy promote-plan,
  hosted canary expansion, or scheduler/adapter additions.
- No public release artifacts or tags are generated in this packet.
- No automatic gate-promotion flow has been performed.
- No PR/issue queue is open for this lane.

## Release-trust truth for v0.18.1

- `cargo +1.95.0 run -p xtask -- policy check-no-panic-family` currently reports
  `213 no-panic policy issue(s) found`.
- This condition is intentionally treated as **advisory/deferred** for the v0.18.1
  patch proof.
- `docs/status/PRODUCT_CLAIMS.md` explicitly keeps the no-panic-related claim at
  advisory until debt is removed or explicitly reviewed.

## Proof evidence

### From the `perfgate-swarm` source-repo

| Command | Result | Evidence |
| --- | --- | --- |
| `cargo run -p xtask -- docs-source-check` | Pass | Source-of-truth doc metadata and ID checks valid. |
| `cargo run -p xtask -- docs-check` | Pass | Documentation drift check and link surface checks passed. |
| `cargo run -p xtask -- product-claims-check` | Pass | Product claims map checks passed. |
| `cargo +1.95.0 run -p xtask -- policy check-no-panic-family` | Fail | `213 no-panic policy issue(s) found`; debt remains deferred. |
| `cargo run -p xtask -- action-check` | Pass | GitHub Action install wiring checks passed. |
| `cargo run -p xtask -- publish-check --package-list` | Pass | Five public publishable crates are listed. |

### Current status split

- **Source-built swarm proof**: captured in this packet.
- **Public release proof**: not yet executed in this swarm packet.

## Promotion path from swarm to release authority

1. Merge the coherent source batch into `EffortlessMetrics/perfgate:main` using a
   normal merge commit (`--no-rebase --no-squash`) per `development/SWARM_PROMOTION.md`.
2. Verify publish readiness in `perfgate` before any release actions:
   `xtask ci`, `action-check`, `docs-check`, `docs-source-check`,
   `product-claims-check`.
3. Continue to the v0.18.1 publish-prep and public smoke steps in `perfgate`.

## Packet completion criteria

- No stale claim or source state is represented as published proof.
- v0.18.1 proof is framed as trust-hardening only; no-panic remains an
  advisory/deferred condition.
- The canonical publishing queue remains `perfgate`; `perfgate-swarm` stays the
  development lane.
