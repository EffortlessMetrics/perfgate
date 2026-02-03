# perfgate Implementation Plan

This document serves as a maintenance plan for the perfgate codebase, describing evolution guidelines, schema versioning strategy, and future work.

The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT", "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted as described in RFC 2119.

## Contract Changes

### Schema Versioning Policy

**Breaking changes REQUIRE a v2 schema.**

A change is considered breaking if it:
- Removes a required field
- Changes the type of an existing field
- Changes the semantic meaning of an existing field
- Removes an enum variant
- Changes the default behavior in a way that invalidates existing receipts

**Additive changes MAY remain in the current version** if they:
- Add a new optional field with `#[serde(default)]`
- Add a new enum variant (consumers SHOULD handle unknown variants gracefully)
- Add new commands that don't affect existing artifacts

### Versioning Process

When creating a new schema version:

1. Create new type definitions (e.g., `RunReceiptV2`)
2. Define new schema constant (e.g., `RUN_SCHEMA_V2`)
3. Update CLI to write new version by default
4. Maintain backward-compatible reading of v1 schemas
5. Generate new JSON Schema file to `schemas/`
6. Update documentation to reflect changes

### Current Schema Versions

| Schema | Version | Status |
|--------|---------|--------|
| `perfgate.run.v1` | 1 | Current |
| `perfgate.compare.v1` | 1 | Current |
| `perfgate.report.v1` | 1 | Current |
| `perfgate.config.v1` | 1 | Current |

## Deterministic Ordering

### New Metrics Policy

**New metrics MUST include deterministic ordering.**

When adding a new metric type:

1. Add variant to `Metric` enum
2. Implement `Ord` for the variant (determines BTreeMap ordering)
3. Update `metric_to_string()` in all modules that use it
4. Add default direction via `default_direction()`
5. Add default warn factor via `default_warn_factor()`
6. Add display unit via `display_unit()`
7. Update export column ordering documentation

### Ordering Invariants

- `BTreeMap<Metric, _>` MUST be used for all metric collections
- Export functions MUST sort metrics alphabetically by string name
- Report findings MUST be ordered by metric (BTreeMap iteration order)
- These orderings MUST be verified by property tests

## Future Work

### Paired Mode

**Status:** Planned

Paired mode would interleave baseline and current executions to reduce environmental noise:

```
baseline-1, current-1, baseline-2, current-2, ...
```

**Considerations:**
- Requires both commands to be specified simultaneously
- May need new CLI interface (e.g., `perfgate paired --baseline-cmd ... --current-cmd ...`)
- Statistics would be computed per-pair, then aggregated
- Schema impact: new receipt type or extended run receipt

### Host Mismatch Policy

**Status:** Planned

Host mismatch detection would warn when comparing receipts from different machines:

**Detection criteria:**
- Different `os` or `arch`
- Significant difference in `cpu_count` (e.g., > 2x)
- Significant difference in `memory_bytes` (e.g., > 2x)
- Different `hostname_hash` (if both present)

**Policy options:**
1. `--host-mismatch=warn` (default): Warn but continue
2. `--host-mismatch=error`: Exit 1 on mismatch
3. `--host-mismatch=ignore`: Suppress warnings

**Schema impact:** None (uses existing `HostInfo` fields)

### Additional Metrics

**Status:** Considered

Potential future metrics:

1. **CPU time** (`cpu_ms`): User + system CPU time from `rusage`
   - Direction: Lower
   - Platform: Unix only

2. **Page faults** (`page_faults`): Major page faults from `rusage`
   - Direction: Lower
   - Platform: Unix only

3. **Context switches** (`ctx_switches`): Voluntary + involuntary from `rusage`
   - Direction: Lower
   - Platform: Unix only

4. **Binary size** (`binary_bytes`): Size of executable
   - Direction: Lower
   - Requires path to binary

**Adding a metric requires:**
- New `Metric` enum variant
- Type updates (Stats, Delta)
- Domain logic updates
- Adapter collection (if platform-specific)
- Schema version bump (if changes are breaking)

### Configuration Enhancements

**Status:** Considered

1. **Metric-specific budgets in config:**
   ```toml
   [[bench]]
   name = "my-bench"
   [bench.budgets.wall_ms]
   threshold = 0.10
   direction = "lower"
   ```

2. **Baseline auto-discovery:**
   ```toml
   [defaults]
   baseline_pattern = "baselines/{bench}.json"
   ```

3. **Multi-bench check:**
   ```bash
   perfgate check --config perfgate.toml --all
   ```

### CI Integration Improvements

**Status:** Considered

1. **GitHub Actions output:**
   ```yaml
   - run: perfgate check --output-github
   ```
   Would set outputs like `${{ steps.perfgate.outputs.verdict }}`

2. **Comment templates:**
   Allow customizing markdown output with templates

3. **Artifact upload helpers:**
   Integration with GitHub Actions artifacts

## Testing Requirements

### Property Test Coverage

When making changes, ensure property tests cover:

1. **Serialization round-trips**: All types MUST serialize/deserialize correctly
2. **Statistics ordering**: `min <= median <= max` MUST hold
3. **Warmup exclusion**: Warmup samples MUST NOT affect statistics
4. **Report determinism**: Same input MUST produce same output
5. **Export ordering**: Metrics MUST be sorted alphabetically

### Mutation Testing Targets

Minimum kill rates by crate:

| Crate | Target Kill Rate |
|-------|-----------------|
| perfgate-domain | 100% |
| perfgate-types | 95% |
| perfgate-app | 90% |
| perfgate-adapters | 80% |
| perfgate-cli | 70% |

### BDD Test Coverage

Feature files in `features/` MUST cover:

1. All happy-path command flows
2. Error conditions and exit codes
3. Baseline-missing scenarios
4. Platform-specific behavior (tagged `@unix`)

## Deprecation Policy

When deprecating functionality:

1. **Announce**: Add deprecation notice to CHANGELOG
2. **Warn**: Emit runtime warning for one minor version
3. **Remove**: Remove in next major version

For schema deprecation:
1. Continue reading deprecated version for two major versions
2. Stop writing deprecated version after one major version
3. Add migration guidance to documentation

## Code Style

### Error Handling

- Use `anyhow` for CLI-level errors
- Use `thiserror` for domain/adapter error types
- Domain errors MUST NOT leak implementation details
- Adapter errors SHOULD include platform context

### Documentation

- Public items MUST have doc comments
- Module-level docs MUST explain purpose and invariants
- Property tests MUST reference requirements they validate
- `/// **Validates: Requirements X.Y**` format for traceability

### Dependencies

- Minimize dependencies in inner crates (types, domain)
- Platform-specific code MUST use `#[cfg()]` attributes
- Optional features (e.g., `arbitrary`) for development-only deps
