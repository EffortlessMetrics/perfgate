# perfgate Documentation Source of Truth

perfgate uses a linked documentation stack so contributors and agents can move
from intent to implementation without rediscovering the architecture each time.
The stack separates why, what, how, what now, and what proves it.

Existing topic docs remain valid. New governance and spec work should use these
homes so product claims, policy gates, public surface rules, release proof, and
Codex execution state stay reviewable.

| Artifact | Owns | Location |
|----------|------|----------|
| Proposal | Why a lane exists, who benefits, alternatives, and success criteria | [`proposals/`](proposals/) |
| Spec | What behavior or proof contract must be true | [`specs/`](specs/) |
| ADR | Durable architecture decisions | [`adr/`](adr/) |
| Plan | How work lands PR by PR | [`../plans/`](../plans/) |
| Goal TOML | Current Codex execution state | [`../.codex/goals/`](../.codex/goals/) |
| Policy ledger | Machine-readable exceptions, gates, and governed surfaces | [`../policy/`](../policy/) |
| Status docs | Product claim support tiers and proof map | [`status/`](status/) |
| Handoff | Closeout notes, remaining work, and operator context | [`handoffs/`](handoffs/) |

## Source-of-truth Rules

- Proposals explain why a lane exists. They do not contain the full PR queue.
- Specs define behavior, evidence, and proof. They do not duplicate release
  tables, policy ledgers, or implementation diaries.
- ADRs record architectural decisions expected to survive individual releases.
- Plans sequence file changes, proof commands, rollback, blockers, and PR order.
- Goal TOML files record active agent state. They do not define new behavior.
- Policy files own concrete exceptions and governed surfaces.
- Status docs map user-facing claims to support tiers and proof commands.
- Handoffs record what changed, what remains, and what the next operator needs.

## Existing Anchors

- Public crate surface: [`CRATE_SEAMS.md`](CRATE_SEAMS.md),
  [`policy/public_crates.txt`](../policy/public_crates.txt), and
  [`policy/absorbed_crates.txt`](../policy/absorbed_crates.txt)
- Release proof: [`RELEASE_READINESS.md`](RELEASE_READINESS.md) and
  [`audits/release-0.17.0-publish-readiness.md`](audits/release-0.17.0-publish-readiness.md)
- Verification badges and PR evidence:
  [`VERIFICATION.md`](VERIFICATION.md)
- First-hour adoption path: [`FIRST_HOUR.md`](FIRST_HOUR.md)
- Adoption levels: [`ADOPTION_LEVELS.md`](ADOPTION_LEVELS.md)
- Performance decisions: [`PERFORMANCE_DECISIONS.md`](PERFORMANCE_DECISIONS.md)
- Decision outcome gallery:
  [`examples/decision-outcomes.md`](examples/decision-outcomes.md)
- Probe instrumentation: [`PROBE_QUICKSTART.md`](PROBE_QUICKSTART.md)
- Probe design patterns:
  [`PROBE_DESIGN_PATTERNS.md`](PROBE_DESIGN_PATTERNS.md)
- Signal calibration: [`SIGNAL_CALIBRATION.md`](SIGNAL_CALIBRATION.md)
- Decision ledger operations:
  [`DECISION_LEDGER_RUNBOOK.md`](DECISION_LEDGER_RUNBOOK.md)
- Workspace inventory: [`WORKSPACE.md`](WORKSPACE.md)
- GitHub Actions setup:
  [`GETTING_STARTED_GITHUB_ACTIONS.md`](GETTING_STARTED_GITHUB_ACTIONS.md)
- Architecture: [`ARCHITECTURE.md`](ARCHITECTURE.md) and the existing
  [`adrs/`](adrs/) archive
- Policy allowlists: [`POLICY_ALLOWLISTS.md`](POLICY_ALLOWLISTS.md)

## Header Standard

Every proposal, spec, ADR, and plan should start with a short metadata block.
Each subdirectory README defines the fields for that artifact type.

Status values are intentionally small:

```text
proposed
accepted
implemented
superseded
```
