# Platform Metric Support

perfgate receipts use optional metric fields. A missing platform metric does
not invalidate the gate; it means that metric was not available for that run and
will not be compared unless both baseline and current receipts contain it.

Use this matrix to decide which metrics can be required on a platform and which
should stay advisory.

## Legend

| Status | Meaning |
|--------|---------|
| supported | Populated by the standard runner and suitable for same-platform gates. |
| advisory | Populated or useful, but semantics are platform-specific or best-effort. |
| unavailable | Not collected by the standard runner on this platform. |

## Metric Matrix

| Surface | Linux | macOS | Windows | Notes |
|---------|-------|-------|---------|-------|
| `wall_ms` | supported | supported | supported | Wall-clock timing is the primary cross-platform metric. |
| timeout enforcement | supported | supported | supported | Unix and Windows runners poll child process completion and kill on timeout. |
| `cpu_ms` | supported | supported | unavailable | Unix uses child `rusage`; Windows receipts omit this field. |
| `max_rss_kb` | supported | supported | supported | Unix uses `rusage`; Windows uses peak working set. Compare within the same host class. |
| `page_faults` | supported | supported | advisory | Unix records major faults; Windows records total page faults from process memory counters. |
| `ctx_switches` | supported | supported | unavailable | Unix records voluntary plus involuntary context switches; Windows receipts omit this field. |
| `io_read_bytes` / `io_write_bytes` | unavailable | unavailable | advisory | Windows uses process IO counters; Unix receipts currently omit these fields. |
| `binary_bytes` | advisory | advisory | advisory | Best-effort executable path metadata; wrapper scripts and shell commands may omit it. |
| `energy_uj` | unavailable | unavailable | unavailable | Schema field exists, but the standard runner does not currently collect it. |
| `network_packets` | unavailable | unavailable | unavailable | Schema field exists, but the standard runner does not currently collect it. |

## Required Gates

For required branch protection:

- prefer `wall_ms` first;
- use `cpu_ms`, `max_rss_kb`, `page_faults`, or `ctx_switches` only on a host
  class where the field is consistently present;
- keep Windows `page_faults`, Windows IO counters, and `binary_bytes`
  advisory unless the team has reviewed runner-specific behavior;
- do not compare baselines across materially different host classes;
- use [`HOST_MISMATCH.md`](../HOST_MISMATCH.md) when CI should warn or fail on
  different runner fingerprints.

## Missing Metrics

Metric fields are optional in `perfgate.run.v1`. If a field is missing from
either side of a comparison, perfgate cannot evaluate that metric's budget for
that comparison.

That means:

- a missing optional metric should not make `wall_ms` unusable;
- a budget for a platform-unavailable metric will not provide protection on
  that platform;
- required metric policies should be paired with a stable runner class and
  reviewed artifact examples;
- release claims should say which platform owns the proof.

## Calibration

Use platform support together with signal calibration:

- Linux/macOS CI can gate on `wall_ms`, `cpu_ms`, and selected process metrics
  when the runner class is stable.
- Windows CI can gate on `wall_ms` and use memory/page-fault/IO fields as
  advisory evidence unless the team explicitly calibrates those counters.
- Cross-platform checks should usually report metrics separately rather than
  sharing one baseline namespace.

For threshold and noise guidance, see
[`../SIGNAL_CALIBRATION.md`](../SIGNAL_CALIBRATION.md).
