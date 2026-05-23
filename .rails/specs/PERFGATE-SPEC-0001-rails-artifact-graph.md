# PERFGATE-SPEC-0001: Rails artifact graph contract

Status: accepted
Owner: docs-platform
Created: 2026-05-21
Linked proposal: PERFGATE-PROP-0001
Linked ADRs: PERFGATE-ADR-0001
Linked lane: rails-adoption
Linked issues:
Linked PRs:
Support-tier impact: none
Policy impact: references only

## Problem

Without an explicit graph contract, durable artifacts may drift, break links, or leak into external tool namespaces.

## Behavior

- Rails-owned artifacts must be indexed in `.rails/index.toml`.
- `.rails/index.toml` must use schema version `1.0`.
- `.rails/index.toml` must identify the repo as `perfgate`, framework as
  `rails`, and root as `.rails`.
- `.rails/index.toml` must preserve the registered ID-prefix conventions for
  proposals, specs, ADRs, and lanes.
- `.rails/index.toml` must preserve external namespace mappings for `.codex/`,
  `.spec/`, `.claude/`, and `.jules/`.
- Rails-owned artifact paths must live under `.rails/`.
- Rails-owned artifact paths must live under the directory that matches their
  registered kind: proposals, specs, ADRs, support, policy, closeouts, plans, or
  templates.
- ID-bearing Rails artifact filenames must start with their registered artifact
  ID. Support and policy artifacts are singleton registries and are exempt.
- Rails-owned artifacts under proposals, specs, ADRs, closeouts, plans, support,
  and policy directories must be registered.
- Rails support claim references and policy ledger paths must resolve.
- Rails support claim IDs must use the `PERFGATE-CLAIM-` prefix.
- Rails support claim IDs and policy ledger IDs must be unique inside their
  artifact.
- Rails support claim proof command entries must be non-empty.
- Rails links must resolve to the expected artifact kind: proposals to
  proposals, specs to specs, and ADRs to ADRs.
- External namespaces may be listed for awareness, but not owned.
- Specs define behavior contracts, not PR ordering.
- Lane trackers define focused implementation sequence.
- Lane tracker paths must be `.rails/lanes/<lane-id>/tracker.toml`.
- Lane tracker `schema_version` must be `1.0`.
- Lane tracker `id`, `name`, `status`, and `owner` must match the registry entry.
- Lane tracker `objective` must be non-empty, and lane tracker `end_state`
  must list non-empty done-state entries.
- Rails TOML artifacts must not contain unknown fields outside their documented
  schemas.
- Lane tracker work item IDs must be non-empty and unique within the lane tracker.
- Lane tracker work item statuses must use the allowed lane-work vocabulary:
  `planned`, `ready`, `active`, `blocked`, `implemented`, or `superseded`.
- Implemented lane trackers must not leave work items in `planned`, `ready`,
  `active`, or `blocked`; each work item must be `implemented` or `superseded`.
- Lane tracker work item `proposal` and `spec` references are required and must
  resolve to registered proposal and spec artifacts.
- Lane tracker work item `adr` references are optional, but when set must
  resolve to a registered ADR artifact.
- Lane tracker work item `implementation_plan` paths must resolve.
- Lane tracker work item `blocks` and `blocked_by` references must point to
  non-empty work item IDs inside the same lane tracker and must not point back
  to the same work item.
- Lane tracker work item proof command entries must be non-empty.
- Implemented lanes must have a registered implemented closeout artifact.

## Non-goals

Taking ownership of `.codex/`, `.spec/`, `.claude/`, or `.jules/`.

## Required evidence

- `git diff --check`

## Acceptance examples

A lane tracker is linked in `.rails/index.toml`, and all artifact paths resolve under `.rails/`.

## Test mapping

Future validator commands should parse and verify index and lane contracts.

## Implementation mapping

`.rails/index.toml`, `.rails/lanes/*/tracker.toml`, and templates.

## CI proof

`git diff --check` until dedicated validator commands land.

## Metrics / promotion rule

Promotion once index/lane validators are implemented and used in CI.

## Failure modes

Registry schema drift, project identity drift, prefix convention drift, external
namespace drift, missing linked artifacts, unregistered owned artifacts,
artifact kind-directory drift, artifact filename identity drift, wrong-kind
artifact links, unresolved support claim references, unresolved policy ledger
paths, duplicate support claim or policy ledger IDs, empty support proof
commands, support claim ID prefix drift, unknown Rails TOML fields, lane tracker
path drift, lane tracker schema drift, lane tracker identity drift, empty lane
tracker objective or end state entries, duplicate lane work item IDs, unknown
lane work item statuses,
unfinished work items in implemented lanes,
unknown or wrong-kind lane work item source links, unresolved lane work item
implementation plans, unknown or self-referential lane work item dependencies,
empty lane work item proof commands, missing closeouts for implemented lanes, or
paths under external namespaces must fail validation.
