# ADR 0009: Paired Benchmarking

## Status
Accepted

## Context
CI runners share infrastructure with other jobs. System load, memory pressure, and thermal state vary between runs. When baseline and current measurements happen at different times (or on different machines), environmental noise can dominate the signal, making it impossible to detect real regressions smaller than ~10-15%.

## Decision
The `perfgate paired` command runs baseline and current commands in interleaved fashion: B₁, C₁, B₂, C₂, ..., Bₙ, Cₙ. Each pair is measured back-to-back to minimize environmental variance.

Key design choices:
1. **Interleaved execution** — each baseline run is immediately followed by a current run, so both experience the same system state.
2. **Separate commands** — baseline and current are specified as separate shell commands (`--baseline-cmd`, `--current-cmd`), allowing comparison of different binaries.
3. **Significance-based retries** — if `--max-retries` is set, paired mode will run additional pairs when the t-test doesn't reach significance, up to the retry limit.
4. **Standard output** — produces a `perfgate.compare.v1` receipt, compatible with all downstream commands (md, report, export).
5. **Lives in `perfgate-paired`** — separated from `perfgate-domain` because it involves I/O (process execution), while domain is intentionally I/O-free.

## Consequences
- Sub-5% regressions become detectable in noisy CI environments.
- Runtime roughly doubles compared to separate run+compare (two commands per pair).
- The interleaving strategy assumes environmental noise is correlated across adjacent time intervals — this holds for load and thermals but not for all noise sources.
