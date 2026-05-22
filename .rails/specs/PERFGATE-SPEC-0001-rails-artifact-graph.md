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
- Rails-owned artifact paths must live under `.rails/`.
- External namespaces may be listed for awareness, but not owned.
- Specs define behavior contracts, not PR ordering.
- Lane trackers define focused implementation sequence.
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

Missing linked artifacts, missing closeouts for implemented lanes, or paths
under external namespaces must fail validation.
