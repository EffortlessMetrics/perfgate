❌ perfgate: fail

**Bench:** `cli/check-single`

| metric | baseline (median) | current (median) | delta | budget | status |
|---|---:|---:|---:|---:|---|
| `binary_bytes` | 86016 bytes | 1446024 bytes | +1581.11% | 15.0% (lower) | ❌ |
| `cpu_ms` | 15 ms | 11 ms | -26.67% | 15.0% (lower) | ✅ |
| `max_rss_kb` | 8216 KB | 6788 KB | -17.38% | 15.0% (lower) | ✅ |
| `page_faults` | 2110 count | 0 count | -100.00% | 15.0% (lower) | ✅ |
| `wall_ms (p95)` | 520 ms | 11 ms | -97.89% | 20.0% (lower) | ✅ |

**Notes:**
- binary_bytes_fail: +1581.11% (fail > 15.00%)
