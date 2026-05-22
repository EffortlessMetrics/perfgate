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
- Rails-owned artifacts under proposals, specs, ADRs, closeouts, support, and policy directories must be registered.
- Rails support claim references and policy ledger paths must resolve.
- External namespaces may be listed for awareness, but not owned.
- Specs define behavior contracts, not PR ordering.
- Lane trackers define focused implementation sequence.
- Lane tracker `id`, `status`, and `owner` must match the registry entry.
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
unresolved support claim references, unresolved policy ledger paths, lane
tracker drift, missing closeouts for implemented lanes, or paths under external
namespaces must fail validation.
