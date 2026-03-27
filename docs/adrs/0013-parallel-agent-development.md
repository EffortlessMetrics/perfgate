# ADR 0013: Parallel Agent Development with Worktrees

## Status
Accepted

## Context

The perfgate workspace contains 26 crates with a broad surface area of
improvements needed: documentation updates, bug fixes, new features, server
hardening, and CI improvements. A single developer working sequentially would
need weeks to address the backlog. Meanwhile, AI coding agents (Claude Code)
can operate in parallel if given isolated working directories.

Git worktrees provide exactly this isolation: each worktree is a separate
checkout of the repository with its own working directory and branch, sharing
the same `.git` object store. Combined with Claude Code's ability to spawn
sub-agents, this enables a model where many agents work simultaneously on
independent PRs.

The key question is how to organize this parallel work to maximize throughput
while minimizing merge conflicts, wasted effort, and review burden.

## Decision

We adopt a **worktree-per-agent model** with a **three-wave development cycle**
for high-throughput development sessions.

### Worktree-per-Agent Model

Each implementation agent operates in its own git worktree:

```
.claude/worktrees/
  agent-aaa/     # Branch: fix/server-error-handling
  agent-bbb/     # Branch: feat/html-export-xss
  agent-ccc/     # Branch: docs/readme-modernization
  ...
```

Agents are scoped to **single-crate granularity** whenever possible. A PR that
modifies only `perfgate-export` is strongly preferred over a PR that touches
`perfgate-export`, `perfgate-render`, and `perfgate-cli` simultaneously.
Cross-cutting changes are reserved for cases where the change is inherently
atomic (e.g., renaming a type used across crates).

### Three-Wave Cycle

Development proceeds in three sequential waves, each internally parallel:

**Wave 1 -- Build:** Implementation agents receive a GitHub issue or task
description and produce a PR. All agents in this wave run concurrently.

**Wave 2 -- Review:** Review agents check out each PR branch and perform:
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test -p <affected-crate>`
- Diff review for logic errors, security issues, and style problems

Review agents use an adversarial framing ("find bugs in this code") which
empirically catches issues that the implementation agent missed, even though
both are the same underlying model.

**Wave 3 -- Fix:** Fix agents address review findings. If the fixes are
trivial, they push to the existing PR branch. If the findings require
rethinking the approach, a new PR may be created.

### Merge Priority Ordering

PRs are merged in dependency order to minimize conflicts:

1. Cleanup (no functional change)
2. Documentation (no code change)
3. Leaf crates (few or no dependents)
4. Server (isolated service)
5. Features (new functionality)
6. Developer experience (CI, tooling)

### Issue-First Planning

Before spawning agents, the full set of tasks is captured as GitHub issues with
clear scope boundaries. This prevents agents from overlapping on the same files
and provides a natural tracking mechanism for the session.

## Consequences

### Positive

- **High throughput**: A single session produced 33+ PRs across 26 crates,
  work that would take a solo developer weeks.
- **Cost-effective**: The session consumed approximately 5% of a weekly Claude
  Code 20x Max plan, allowing 6--7 sessions of this scale per week.
- **Quality assurance**: Review agents found 13+ real bugs across 7 PRs,
  including precision loss, XSS vectors, and silent error swallowing.
- **Deterministic merge order**: Planning the dependency DAG upfront eliminated
  most merge conflicts.

### Negative

- **Windows PDB contention**: Multiple parallel `cargo build` invocations on
  Windows fight over Program Database file locks. This is the primary
  parallelism bottleneck on Windows and does not occur on Linux. Mitigation:
  limit concurrent worktrees to 4--6 or reduce per-build parallelism with
  `-j4`.
- **Disk space**: Each worktree with its own `target/` directory consumes
  significant disk space. A 26-crate workspace with 6 worktrees can easily
  exceed 20 GB.
- **Git config locks**: Concurrent git operations across worktrees occasionally
  contend on shared lock files. This is transient and resolves on retry.
- **Cross-cutting changes are expensive**: PRs that span 3+ crates take 3--5x
  longer and produce more review findings. The model works best when work can
  be decomposed into single-crate units.
- **API rate limits**: Under heavy parallel load, API calls may return 500
  errors. Retry logic is essential in any orchestration layer.

### Neutral

- **Review cost dominates**: Implementation is roughly 40% of total token cost.
  Review and fix waves account for the remaining 60%. Using targeted tests
  instead of full-workspace builds in reviews could reduce this by
  approximately 60%.
- **Same model, different results**: The finding that review agents catch bugs
  that implementation agents miss --- despite being the same model --- suggests
  that prompt framing (adversarial vs. constructive) is a significant factor
  in output quality. This has implications for how we structure agent prompts
  in future sessions.
