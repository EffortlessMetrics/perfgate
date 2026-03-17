❌ perfgate: fail

**Bench:** `render/report`

| metric | baseline (median) | current (median) | delta | budget | status |
|---|---:|---:|---:|---:|---|
| `binary_bytes` | 86016 bytes | 1446024 bytes | +1581.11% | 15.0% (lower) | ❌ |
| `cpu_ms` | 15 ms | 6 ms | -60.00% | 15.0% (lower) | ✅ |
| `max_rss_kb` | 8220 KB | 6788 KB | -17.42% | 15.0% (lower) | ✅ |
| `page_faults` | 2113 count | 0 count | -100.00% | 15.0% (lower) | ✅ |
| `wall_ms` | 131 ms | 7 ms | -94.66% | 15.0% (lower) | ✅ |

**Notes:**
- binary_bytes_fail: +1581.11% (fail > 15.00%)
