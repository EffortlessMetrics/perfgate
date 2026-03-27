# Development Economics: Parallel Agent Development

This document captures the economics, counterintuitive findings, and development
patterns observed during a high-throughput AI-assisted development session on the
perfgate workspace. It serves as institutional knowledge for planning future
development sprints that use parallel AI agents.

## Session Economics

### Raw Numbers

| Metric | Value |
|--------|-------|
| Plan consumed | ~5% of weekly Claude Code 20x Max |
| PRs produced | 33+ |
| Agents spawned | 60+ (implementation + review) |
| Workspace size | 26 Rust crates |
| Real bugs found by review agents | 13+ across 7 PRs |

### Cost Implications

- A weekly 20x Max plan supports **6--7 sessions of this scale**, roughly one
  per working day with headroom for ad-hoc work.
- The main cost driver is **review agents that build the entire workspace** per
  PR. A full `cargo test --all` is expensive both in tokens and wall-clock time.
- A lighter review pattern --- clippy plus targeted per-crate tests --- would cut
  review costs by approximately **60%** while still catching most issues.
- ROI is super-linear: agents parallelize work that would take a single developer
  weeks. The marginal cost of an additional PR in a session is far lower than the
  marginal cost of a developer context-switching to that task.

## Counterintuitive Findings

### 1. Review agents find bugs that implementation agents miss

The same AI model, given a review-focused prompt, catches precision loss, XSS
vectors, and silent failures that it did not catch when writing the code. The
adversarial framing of "find problems in this diff" activates different
reasoning paths than "implement this feature." This is analogous to why human
code review catches bugs the original author missed, even when the reviewer is
less experienced.

### 2. Documentation PRs are not "free"

README modernization across 26 crates required 8 agents and produced genuinely
different output than the originals. Applying a problem-first framing ("what
problem does this crate solve?") changed every README substantively. Treating
docs as a trivial task underestimates both the effort and the value.

### 3. Platform-specific bugs are the hardest to catch statically

Examples from this session:
- SQLite WAL mode on in-memory databases (silently ignored, causes test flakes).
- Bitbucket Pipelines missing cargo cache support (no built-in `cache: cargo`).
- CircleCI environment blocks using literal YAML syntax that differs from GitHub
  Actions.

These only surface when the agent (or developer) has deep knowledge of the
target platform's quirks. General-purpose code review misses them.

### 4. Merge order is a dependency graph

Not all PRs are independent. Observed dependencies:
- Cleanup PRs must land before feature PRs that touch the same files.
- Server PRs that modify handler signatures conflict with each other.
- CLI PRs that add new subcommands conflict on `main.rs` and help snapshot
  tests.

Treating PRs as a flat list leads to avoidable merge conflicts. Planning merge
order as a DAG saves rework.

### 5. Windows PDB contention is the main parallelism bottleneck

Multiple parallel `cargo build` invocations on Windows fight over PDB (Program
Database) file locks. On Linux this is not an issue. The fix is trivial
(`-j4` or lower parallelism per build) but non-obvious until you hit it.

### 6. Worktree agents work best at single-crate granularity

Cross-cutting changes (touching 3+ crates) take 3--5x longer and produce more
review findings than single-crate changes. The ideal unit of work for a parallel
agent is one crate or one tightly-scoped concern.

### 7. The "adoption surface" thesis validated

External analysis independently ranked the same priorities that emerged from the
session: **binaries** (pre-built releases) then **ingest** (easy data import)
then **PR bot** (GitHub integration). Reducing friction for new users
consistently outranks adding features for existing users.

## Development Patterns That Worked

### Three-Wave Approach

Development proceeds in three independently parallelizable waves:

1. **Build wave**: Implementation agents work in isolated worktrees, one per PR.
2. **Review wave**: Review agents check out each PR branch, run tests, read
   diffs, and file findings.
3. **Fix wave**: Fix agents address review findings, often in the same worktree.

Each wave can internally parallelize across all available agents. The waves
themselves are sequential: review cannot start until build completes, and fixes
cannot start until review completes.

### Issue-First Development

Creating GitHub issues before spawning implementation agents gives each agent
clear, bounded scope. In this session, 19 issues were created upfront. Agents
that received a well-scoped issue produced PRs that required fewer review
iterations.

### Merge Priority Ordering

PRs were merged in a deliberate order to minimize conflicts:

1. **Cleanup** --- remove dead code, fix warnings (no functional change)
2. **Documentation** --- READMEs, guides (no code change)
3. **Adapters** --- low-level crates with few dependents
4. **Server** --- isolated service with its own test suite
5. **Features** --- new CLI commands, new crate functionality
6. **Developer experience** --- CI config, tooling improvements

### Review Agents as QA

Review agents ran tests, not just read code. This is critical: static review
catches style and logic issues, but running `cargo clippy` and targeted tests
catches compilation errors, type mismatches, and behavioral regressions that
are invisible in a diff.

## Patterns That Did Not Work

### Too Many Worktrees on Windows

Symptoms:
- PDB file locks causing build failures.
- Disk space exhaustion from multiple `target/` directories.
- Git config file locks when multiple agents run `git` simultaneously.

Mitigation: Limit concurrent worktrees to 4--6 on Windows. Use shared
`target/` directories where possible, or accept sequential builds.

### Full `cargo test --all` in Reviews

Running the entire test suite for a single-crate PR is expensive and often
fails on unrelated snapshot tests or platform-specific tests. Targeted testing
(`cargo test -p <crate>` plus `cargo clippy --all`) is faster and more
informative.

### API 500 Errors Under Load

Two agents hit server errors when the Claude API was under heavy load from
parallel requests. There is no mitigation other than retry logic. Building
retry into agent orchestration is essential for reliability.

## Recommendations for Future Sessions

1. **Plan merge order before starting agents.** Sketch the dependency DAG and
   assign priority labels.
2. **Use single-crate scope for most agents.** Reserve cross-cutting work for
   a dedicated agent with extra context.
3. **Run lightweight reviews by default.** Full test suites only for PRs that
   touch core domain logic or public APIs.
4. **Limit Windows worktrees to 4--6.** Or develop on Linux where PDB
   contention does not exist.
5. **Create issues first.** The upfront cost of writing 19 issues is small
   compared to the rework cost of under-scoped agents.
6. **Budget for review and fix waves.** Implementation is roughly 40% of total
   cost. Review and fixes are the other 60%.
