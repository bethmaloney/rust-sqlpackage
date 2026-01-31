# Performance Documentation

This document details the performance characteristics of rust-sqlpackage, including benchmark methodology, optimization history, and comparison with .NET DacFx.

## Quick Summary

| Metric | rust-sqlpackage | .NET DacFx | Speedup |
|--------|-----------------|------------|---------|
| 30-file project | 86ms | ~2.3s cold / ~800ms warm | **27x cold / 9x warm** |
| 135-file stress test | 462ms | N/A | - |

## Benchmark Infrastructure

Benchmarks use [criterion](https://crates.io/crates/criterion) 0.5 with 100 samples per measurement.

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark group
cargo bench --bench pipeline

# Compare against a baseline
cargo bench -- --save-baseline before
# ... make changes ...
cargo bench -- --baseline before

# Generate flamegraph for profiling
cargo flamegraph --release -- build --project tests/fixtures/e2e_comprehensive/Database.sqlproj
```

### Test Fixtures

| Fixture | SQL Files | Description |
|---------|-----------|-------------|
| e2e_simple | minimal | Minimal project baseline |
| e2e_comprehensive | 30 | Production-realistic project |
| stress_test | 135 | High file count stress test |

## Pipeline Stage Breakdown

The build pipeline consists of several stages. Here's the time distribution for a 30-file project (e2e_comprehensive):

| Stage | Time | % of Total | Description |
|-------|------|------------|-------------|
| sqlproj_parsing | 0.15 ms | 0.2% | Parse .sqlproj XML |
| sql_parsing | 5.25 ms | 6.1% | Parse SQL files to AST |
| model_building | 8.18 ms | 9.5% | Build DatabaseModel from AST |
| xml_generation | 70.6 ms | **82.2%** | Generate model.xml |
| dacpac_packaging | 73.5 ms | N/A | ZIP packaging (parallel) |

**Key Finding:** XML generation dominates at 82% of pipeline time.

### Scaling Behavior

| Stage | 30 files | 135 files | Scaling Factor |
|-------|----------|-----------|----------------|
| sql_parsing | 5.25 ms | 12.0 ms | 2.3x |
| model_building | 8.18 ms | 38.2 ms | 4.7x |
| Full pipeline | 85.8 ms | 462.1 ms | 5.4x |

Model building scales super-linearly (4.7x for 4.8x files), suggesting O(n log n) complexity from relationship resolution overhead.

## Optimization History

### Phase 16.2: Quick Wins

#### Regex Caching (16.2.2)
Replaced 30 `Regex::new()` calls with static `LazyLock<Regex>` patterns that compile once and reuse.

| Measurement | Improvement |
|-------------|-------------|
| Full pipeline | 2-4% |
| XML generation | 99% (from cached regex benefit) |
| Model building | 5-6% |

#### String Joining Optimization (16.2.3)
Optimized `preprocess_parser.rs` by replacing `Vec<String>.join("")` with pre-allocated `String` buffer and using `Cow<'static, str>` for zero-allocation on static tokens.

| Measurement | Improvement |
|-------------|-------------|
| SQL parsing | 5-9% |
| Full pipeline | Within noise margin |

#### Cached Uppercase SQL (16.2.4)
Eliminated redundant `.to_uppercase()` calls by passing pre-computed uppercase SQL to helper functions.

| Measurement | Improvement |
|-------------|-------------|
| SQL parsing | ~1.5% |
| Full pipeline | Within noise margin |

#### Vector Capacity Hints (16.2.5)
Added `with_capacity()` hints to 9 key vector allocations in hot paths.

| Measurement | Improvement |
|-------------|-------------|
| Full pipeline | <1% |

### Phase 16.3: Medium Effort Optimizations

#### Cow for Schema Tracking (16.3.1)
Introduced `track_schema()` helper using `Cow<'static, str>` to reduce redundant cloning. Static "dbo" schema uses borrowed reference.

| Measurement | Improvement |
|-------------|-------------|
| SQL parsing | 2-3% |
| Dacpac packaging | 6.4% |

#### Pre-computed Sort Keys (16.3.2)
Used `sort_by_cached_key` to compute sort keys once per element instead of on each comparison. Optimized `db_options_sort_key` to use `&str` instead of owned `String`.

| Measurement | Improvement |
|-------------|-------------|
| Full pipeline | 2-4% |
| Model building (stress_test) | 2.7% |

#### Batched XML Attributes (16.3.3)
Converted 125 sequential `push_attribute()` calls to batched `with_attributes()` calls throughout `model_xml.rs`.

| Measurement | Improvement |
|-------------|-------------|
| XML generation | 5.6% |
| Full pipeline (simple) | 6.5% |
| Full pipeline (comprehensive) | 3.7% |
| Full pipeline (stress_test) | 4.8% |

### Phase 16.4: Parallelization

#### Parallel SQL Parsing (16.4.2)
Added rayon for parallel SQL file parsing with adaptive threshold:
- Files >= 8: Uses `par_iter()` for parallel processing
- Files < 8: Uses sequential processing to avoid rayon overhead

| Measurement | Improvement |
|-------------|-------------|
| SQL parsing (28 files) | **43%** |
| SQL parsing (135 files) | **67%** |
| Full pipeline | Within noise (other stages dominate) |

## Comparison with .NET DacFx

Benchmarked on e2e_comprehensive fixture (30 SQL files):

| Build Type | Time | Notes |
|------------|------|-------|
| rust-sqlpackage | 85.8 ms | Consistent across runs |
| .NET DacFx (cold) | ~2.3 s | After cleaning bin/obj |
| .NET DacFx (warm) | ~800 ms | Incremental, no changes |

**Speedup: 27x cold / 9x warm**

### Why rust-sqlpackage is Faster

1. **No JIT warmup**: Compiled native code runs immediately at full speed
2. **Parallel parsing**: SQL files parsed concurrently with rayon
3. **Cached regex**: Static regex patterns compiled once at startup
4. **Minimal allocations**: Strategic use of `Cow`, capacity hints, and batching
5. **Focused scope**: Only builds dacpac, no code analysis or incremental tracking

## Identified Hotspots

| Area | Status | Impact |
|------|--------|--------|
| Regex compilation in model_xml.rs | Fixed | HIGH |
| String joining in preprocess_parser.rs | Fixed | MEDIUM |
| Sequential SQL file parsing | Fixed | HIGH (large projects) |
| Clone calls in model/builder.rs | Partially addressed | MEDIUM |
| Uppercase SQL conversions | Fixed | LOW |

## Future Optimization Opportunities

1. **Incremental builds**: Cache parsed SQL AST, only rebuild changed files
2. **Memory-mapped file I/O**: For very large SQL files
3. **SIMD text processing**: For tokenization hotspots
4. **Custom XML writer**: Avoid quick-xml overhead for known schema
