# Unconventional Findings: Agent-Swarm Development

This document captures the surprising, counterintuitive, and non-obvious findings
from a development session that used 60+ parallel AI agents (Claude Code in
worktrees) to produce 40+ PRs across a 26-crate Rust workspace. These findings go
beyond the cost analysis in [DEVELOPMENT_ECONOMICS.md](DEVELOPMENT_ECONOMICS.md)
and focus on the things that would surprise someone who has not done agent-swarm
development before.

This is institutional knowledge. Every finding below was observed empirically, not
predicted in advance.

---

## The Review Paradox

The single most counterintuitive finding: **the same AI model that wrote the code,
when given a review-focused prompt, finds bugs it missed during implementation.**

You would expect the same model to have the same blind spots. It does not. The
explanation is that adversarial framing changes what the model attends to:

| Mode | Optimization target | Behavior |
|------|---------------------|----------|
| Implementation | "Make it work" | Optimizes for completion. Fills in reasonable defaults. Moves past edge cases. |
| Review | "Find what's wrong" | Optimizes for correctness. Questions every assumption. Checks platform behavior. |

This is not a minor effect. Real bugs found by review agents that were introduced
by implementation agents in the same session:

- **Precision loss**: Integer division before float cast in nanosecond-to-millisecond
  conversion, silently truncating sub-millisecond values to zero.
- **XSS vulnerabilities**: User-controlled benchmark names inserted into HTML via
  `innerHTML` without escaping.
- **Silent failures**: SQLite `PRAGMA journal_mode=WAL` on in-memory databases
  returns `"memory"` instead of `"wal"`, but the code never checked the return value.
- **Wrong field names**: JavaScript consumers referencing Rust struct field names
  instead of the actual serde-serialized JSON field names (which differ due to
  `#[serde(rename)]`).
- **Platform incompatibilities**: Chart.js receiving CSS `var()` custom properties
  that the Canvas 2D context cannot resolve, rendering all chart lines as black
  with no console errors.

**Implication**: Always run a separate review pass, even with the same model. The
cost is justified. See [ADR 0014](adrs/0014-ai-review-always-required.md) for the
formal decision.

---

## The 13/13 Finding Rate

Every single PR that received a dedicated review agent had either a real bug or a
meaningful improvement identified. Not 12 out of 13. Not "most." All 13.

These were not nitpicks or style suggestions. They were runtime-breaking defects:

- Struct field names that would cause `undefined` in JavaScript consumers
- Integer truncation that made sub-millisecond regressions invisible to budget checks
- Missing error handling that would swallow failures silently
- Security vulnerabilities exploitable via crafted benchmark names
- Platform-specific behavior that would cause silent data corruption

The denominator matters here. If even a handful of reviewed PRs had been clean, you
could argue that review is optional for "simple" changes. But 13/13 means the bug
density is high enough that **no AI-generated PR should be considered safe to merge
without a separate review pass**.

The cost of review was approximately 30% of the total session token budget. Given
that every reviewed PR had findings, this is not overhead --- it is quality
assurance with a 100% hit rate.

---

## Platform Knowledge Is the Hardest Thing

The bugs that were hardest to catch --- and most dangerous in production --- were
platform-specific behaviors that are not reliably encoded in any training data.

| Platform quirk | What goes wrong | Why it's hard to catch |
|----------------|-----------------|----------------------|
| SQLite in-memory WAL | `PRAGMA journal_mode=WAL` silently returns `"memory"` | No error, no warning, just degraded performance |
| Bitbucket cargo cache | No built-in `cache: cargo` type | Pipeline fails with a confusing "unknown cache" error |
| CircleCI environment blocks | `${VAR}` is treated as a literal string | Authentication silently fails with the wrong token |
| Chart.js + CSS variables | Canvas 2D cannot resolve `var()` | All lines render black, no console errors |
| Windows PDB file locks | Parallel builds hit `fatal error C1041` | Looks like a random flake, is actually deterministic |
| Bitbucket failed-step artifacts | Artifacts are not collected from failed steps | Reports lost exactly when you need them most |

These platform quirks share a common trait: **they fail silently**. The code
compiles, tests may pass locally, and the failure only manifests in production or
on a specific CI platform.

Implementation agents miss these because they are optimizing for "make it work" and
the code does work --- on the happy path, on the developer's platform. Review agents
catch them because the review prompt explicitly says "check platform-specific
behavior" and "look for silent failures."

This suggests a general principle: **platform integration is the highest-value
target for review effort**, even more than business logic.

---

## The Real Cost Is Disk Space, Not API Tokens

The session consumed approximately 5% of a weekly Claude Code 20x Max plan for 40+
PRs. At first glance, this seems remarkably cheap.

But the actual bottleneck was not API cost. It was disk space.

Each worktree with its own `cargo build` target directory consumes 3--5 GB for a
26-crate workspace. With 20 concurrent worktrees, that is 60--100 GB of disk
consumed by build artifacts alone. On a developer machine with a 512 GB SSD, this
is 12--20% of total disk capacity for a single development session.

The naive fix --- cleaning `target/` directories between builds --- works but
introduces a different problem: every subsequent build in that worktree starts from
scratch, which is slow and increases API token consumption (agents waiting for
builds).

The actual optimal strategy is **fewer concurrent agents with more sequential work
per agent**. Instead of 20 agents each building once, use 6--8 agents that each
handle 2--3 PRs sequentially, reusing their warm build cache between tasks. This
trades wall-clock parallelism for disk efficiency, and the total time difference is
smaller than you would expect because build time dominates agent thinking time.

**Capacity planning rule of thumb**: budget 5 GB per concurrent worktree on a Rust
workspace of this size. Divide your available disk by 5 GB to find your maximum
useful parallelism.

---

## Merge Ordering Is a Dependency Graph Problem

After the build and review waves completed, we attempted to merge 33 PRs into main.
This immediately hit cascading merge conflicts.

The problem: each merge changes the state of `main`, which can invalidate the merge
status of other PRs. A PR that was conflict-free before the previous merge may now
have conflicts. Testing this serially means discovering conflicts one at a time,
rebasing, retesting, and re-merging --- an O(n^2) process for n PRs.

The solution is to treat merge ordering as a **topological sort by file overlap**.
PRs that touch disjoint files can merge in any order. PRs that touch overlapping
files must be serialized, and the order matters.

The ordering that worked in practice:

1. **Documentation PRs first** --- they touch `.md` files that no code PR modifies.
   Zero conflict potential. Get them out of the way.
2. **New crate PRs** --- adding a new crate (new directory, new entry in workspace
   `Cargo.toml`) has minimal overlap with existing crate modifications.
3. **Leaf crate PRs** --- crates with no downstream dependents (`perfgate-export`,
   `perfgate-render`) can merge independently.
4. **Server PRs** --- the server is relatively isolated but has handler signatures
   that multiple PRs might touch. Serialize these.
5. **CLI PRs last** --- `perfgate-cli` touches `main.rs`, argument parsing, and
   help text that almost every feature PR modifies. These must be serialized and
   merged last.

The mistake to avoid: treating PRs as a flat list and merging in creation order or
PR number order. This maximizes conflicts.

---

## The Three-Wave Pattern

The development session naturally fell into three waves, each independently
parallelizable but sequential with respect to each other:

```
Wave 1: Build          Wave 2: Review         Wave 3: Fix/Merge
 +---------+            +---------+            +---------+
 | Agent 1 |            | Review 1|            | Fix 1   |
 +---------+            +---------+            +---------+
 | Agent 2 |            | Review 2|            | Fix 2   |
 +---------+            +---------+            +---------+
 | Agent 3 |            | Review 3|            | Merge   |
 +---------+            +---------+            | (serial)|
 | ...     |            | ...     |            +---------+
 | Agent N |            | Review N|
 +---------+            +---------+
 ~45 min                ~30 min                ~45 min
```

Key constraints:
- **Within a wave**, all agents run concurrently (subject to disk/API limits).
- **Between waves**, there is a hard dependency: you cannot review a PR that has not
  been built, and you cannot fix what has not been reviewed.
- **Wave 3 (merge) is partially serial**: fixes can happen in parallel, but the
  actual merge-to-main step must be serialized per the dependency ordering above.

Total wall-clock time for 40+ PRs: approximately 2 hours. Most of that time is
agent runtime (builds, tests, thinking). The human's role is orchestration:
launching waves, assigning review targets, and managing merge order.

The anti-pattern is trying to pipeline waves (start reviewing PR 1 while PR 2 is
still being built). This works in theory but in practice causes agents to work on
stale branches, producing reviews of code that has already changed.

---

## Documentation Is a Product Decision

README modernization across 26 crates required 8 dedicated agents and produced
output that was substantively different from the originals --- not just reformatted,
but genuinely rewritten.

The key change was switching from **feature-first framing** to **problem-first
framing**:

**Before (feature-first)**:
> `perfgate-budget` provides budget evaluation and verdict logic for performance
> metrics.

**After (problem-first)**:
> When a benchmark runs 10% slower than yesterday, should your CI pipeline fail?
> `perfgate-budget` answers that question by evaluating performance metrics against
> configurable thresholds and producing pass/warn/fail verdicts.

Every README was different after rewrite. The problem-first framing forced the agent
to articulate the crate's value proposition, which in several cases revealed that
the crate's public API did not match its stated purpose (the README described
capabilities that the code did not actually expose).

This finding challenges the assumption that documentation is a low-skill task that
can be batched cheaply. **Writing good documentation requires understanding the
product deeply enough to articulate why someone should care.** That is a product
decision, not a formatting exercise, and it benefits from the same review rigor as
code.

---

## Agent Specialization and Scope

Not all agent tasks are equal. The data from this session shows a clear relationship
between task scope and output quality:

| Scope | Typical duration | Review findings | Success pattern |
|-------|-----------------|-----------------|-----------------|
| Single crate, single concern | 10--20 min | 0--1 findings | Clean, focused diffs |
| Single crate, multiple concerns | 20--40 min | 1--2 findings | Some scope creep |
| Cross-cutting (3+ crates) | 40--90 min | 3--5 findings | Complex diffs, more bugs |
| Cross-cutting (5+ files, 3+ crates) | 60--120 min | 4--8 findings | Highest bug density |

The relationship is roughly linear: **agents that modify 5+ files across 3+ crates
take 3--5x longer and produce approximately 3x more review findings** than agents
scoped to a single crate.

This is not surprising in isolation --- larger changes are harder. What is
surprising is the magnitude. The bug density per line of change is higher for
cross-cutting changes, not just the absolute count. This suggests that AI agents
lose coherence when they must hold too many files and crate boundaries in context
simultaneously.

**Optimal strategy**: decompose work into single-crate units whenever possible.
Reserve cross-cutting agents for changes that are inherently atomic (e.g., renaming
a type that is used across 5 crates). If a feature requires changes to 3 crates,
consider whether it can be split into 3 PRs that each modify one crate, merged in
dependency order.

---

## Practical Recommendations

Based on these findings, the following practices are recommended for future
agent-swarm development sessions:

1. **Always run a separate review wave.** The 100% finding rate justifies the cost.
   See [ADR 0014](adrs/0014-ai-review-always-required.md).

2. **Scope agents to single crates.** Cross-cutting work produces more bugs per
   line of change. Decompose into single-crate PRs when possible.

3. **Plan merge order before starting agents.** Sketch the file-overlap DAG.
   Documentation first, leaf crates next, CLI last.

4. **Budget disk space, not just API tokens.** 5 GB per concurrent worktree on a
   Rust workspace. Prefer fewer agents with warm caches over many agents with cold
   builds.

5. **Use adversarial framing for reviews.** The prompt "find bugs in this code"
   activates different reasoning than "implement this feature." Make the review
   prompt explicitly adversarial.

6. **Treat documentation as product work.** Problem-first READMEs require genuine
   understanding, not just reformatting. Budget time and review accordingly.

7. **Watch for platform-specific silent failures.** These are the highest-value
   review targets. Implementation agents almost never catch them.

---

## Related Documents

- [DEVELOPMENT_ECONOMICS.md](DEVELOPMENT_ECONOMICS.md) --- cost analysis and ROI
- [ADR 0013: Parallel Agent Development](adrs/0013-parallel-agent-development.md) --- the worktree model
- [ADR 0014: AI Review Always Required](adrs/0014-ai-review-always-required.md) --- the review decision
- [REVIEW_CHECKLIST.md](REVIEW_CHECKLIST.md) --- concrete bug patterns to check for
