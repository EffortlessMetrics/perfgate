# ADR 0014: AI-Generated Code Requires Separate AI Review Before Merge

## Status
Accepted

## Context

During a high-throughput development session (60+ agents, 40+ PRs, 26-crate Rust
workspace), 13 PRs were assigned dedicated review agents. Every single reviewed PR
had either a real bug or a meaningful improvement identified --- a 100% finding rate.

The bugs were not style nitpicks. They were runtime-breaking defects: precision
loss in numeric conversions, XSS vulnerabilities in HTML exports, silent failures
in database configuration, wrong field names in cross-language interfaces, and
platform-specific behaviors that would cause data corruption on specific CI
platforms.

The implementation agents and review agents were the same underlying AI model
(Claude). The only difference was the prompt framing: implementation agents received
"implement this feature" instructions, while review agents received "find bugs in
this diff" instructions.

This raises the question: should AI-generated code require a mandatory review pass
before merge, even when the reviewer is the same AI model?

## Decision

**All AI-generated code must receive a separate AI review pass before being merged
to main.** This applies regardless of the apparent simplicity of the change.

The review pass must:

1. **Use a separate agent instance** --- not the same conversation that produced the
   implementation. A fresh context with adversarial framing is essential.

2. **Run tests, not just read code** --- the review agent must execute `cargo clippy`
   and targeted tests (`cargo test -p <crate>`) to catch compilation errors and
   behavioral regressions invisible in a diff.

3. **Check the [Review Checklist](../REVIEW_CHECKLIST.md)** --- specifically the
   platform-specific, precision, and security categories, which have the highest
   hit rate for AI-generated code.

4. **Use adversarial framing** --- the review prompt must explicitly instruct the
   agent to find problems, not confirm correctness. Example framing: "You are
   reviewing this PR for bugs, security issues, and platform-specific failures.
   Assume there are problems and find them."

### When Full Review Can Be Skipped

A lightweight review (clippy + targeted tests only, no line-by-line diff review) is
acceptable for:

- Documentation-only PRs (no code changes)
- Dependency version bumps with passing CI
- Formatting-only changes produced by `cargo fmt`

These categories had the lowest finding rates during the session, though they were
not zero.

## Consequences

### Positive

- **Catches real bugs**: The 100% finding rate across 13 reviewed PRs demonstrates
  that the review pass is not overhead --- it is catching defects that would
  otherwise reach main.

- **Cost-effective**: Review agents consumed approximately 30% of the total session
  token budget. Given a 100% hit rate on real bugs, this is substantially cheaper
  than fixing bugs after merge (which requires bisection, diagnosis, and a new PR).

- **Same-model review works**: The counterintuitive finding that the same model
  catches bugs it introduced, when given adversarial framing, means review does not
  require a more capable model or a human reviewer to be effective. This makes the
  practice scalable.

- **Platform bugs are caught**: The hardest-to-detect bug category --- silent
  platform-specific failures --- was reliably caught by review agents with explicit
  "check platform behavior" instructions.

### Negative

- **Increased latency**: Every PR now requires an additional agent pass before
  merge. In a parallel session, this adds a full wave (approximately 30 minutes)
  to the development cycle.

- **Increased token cost**: Review adds approximately 30% to total session cost.
  For budget-constrained teams, this may reduce the number of PRs that can be
  produced per session.

- **False sense of security**: AI review catches many bugs but is not exhaustive.
  Teams must not treat a passing AI review as equivalent to a human review for
  security-critical or safety-critical code paths.

### Neutral

- **Does not replace human review**: This ADR establishes AI review as a mandatory
  pre-merge gate, not as a replacement for human review. Human reviewers may still
  be required by team policy, compliance requirements, or for changes to critical
  code paths. AI review is a quality floor, not a quality ceiling.

- **Review prompt engineering matters**: The effectiveness of review depends heavily
  on the framing of the review prompt. "Check this code" is less effective than
  "Find bugs in this code, paying special attention to platform-specific behavior,
  numeric precision, and security." Future sessions should invest in refining review
  prompts based on observed bug categories.

## References

- [Unconventional Findings](../UNCONVENTIONAL_FINDINGS.md) --- full analysis of the
  review paradox and 13/13 finding rate
- [Development Economics](../DEVELOPMENT_ECONOMICS.md) --- cost analysis of the
  three-wave development model
- [ADR 0013](0013-parallel-agent-development.md) --- the parallel agent development
  model that produces the code being reviewed
- [Review Checklist](../REVIEW_CHECKLIST.md) --- the concrete bug patterns that
  review agents should check
