# Codex Goals

Codex goal manifests record current agent execution state. They are
machine-readable pointers to the proposal, spec, plan, policy, proof commands,
allowed files, forbidden files, and completion criteria for the active lane.

Use `.codex/goals/active.toml` for the current goal and `.codex/goals/archive/`
for completed or superseded manifests. Read `docs/reference/SPEC_SYSTEM.md`
before selecting or changing an active work item.

Do not use `.perfgate/` for Codex goal state. `.perfgate/` is reserved for
product-generated user artifacts from `perfgate init` and related workflows.

## Boundaries

- Goal TOML may reference a proposal, spec, ADR, plan, policy file, or status
  doc.
- Goal TOML may constrain file scope and proof commands for Codex.
- Goal TOML must not define new product behavior.
- Goal TOML must not duplicate policy ledgers or release-readiness matrices.

## Suggested Shape

```toml
id = "perfgate-0-18-spec-driven-governance"
title = "perfgate 0.18.0 spec-driven governance"
status = "active"
owner = "codex"
created = "2026-05-13"

objective = """
Make perfgate's product claims, architecture decisions, policy ledgers,
release proof, and Codex execution state traceable through proposals, specs,
ADRs, implementation plans, and machine-readable active goals.
"""

linked_proposal = "docs/proposals/PERFGATE-PROP-0001-spec-driven-governance.md"
linked_spec = "docs/specs/PERFGATE-SPEC-0001-source-of-truth-stack.md"
linked_plan = "plans/0.18.0/implementation-plan.md"

allowed_files = [
  "docs/proposals/**",
  "docs/specs/**",
  "docs/adr/**",
  "docs/status/**",
  "docs/handoffs/**",
  "plans/0.18.0/**",
  ".codex/goals/**",
]

forbidden_files = [
  "Cargo.toml",
  "rust-toolchain.toml",
  ".github/**",
  "crates/**",
  "xtask/**",
  "policy/**",
  "schemas/**",
]

proof = [
  "cargo +1.95.0 run -p xtask -- docs-check",
  "cargo +1.95.0 run -p xtask -- doc-test",
  "git diff --check",
]
```
