❌ perfgate: fail

**Bench:** `cli/check-no-baseline`

| metric | baseline (median) | current (median) | delta | budget | status |
|---|---:|---:|---:|---:|---|
| `binary_bytes` | 86016 bytes | 1446024 bytes | +1581.11% | 15.0% (lower) | ❌ |
| `cpu_ms` | 31 ms | 11 ms | -64.52% | 15.0% (lower) | ✅ |
| `max_rss_kb` | 8224 KB | 6788 KB | -17.46% | 15.0% (lower) | ✅ |
| `page_faults` | 2115 count | 0 count | -100.00% | 15.0% (lower) | ✅ |
| `wall_ms (p95)` | 722 ms | 12 ms | -98.34% | 20.0% (lower) | ✅ |

**Notes:**
- binary_bytes_fail: +1581.11% (fail > 15.00%)
