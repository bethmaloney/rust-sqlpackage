# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Status: PARITY COMPLETE | PERFORMANCE TUNING IN PROGRESS

**Phases 1-14 complete (146 tasks). Full parity achieved.**
**Phase 15 complete: All parser refactoring tasks finished.**
**Phase 16 in progress: Performance tuning and benchmarking.**
- Phase 15.1: ExtendedTsqlDialect infrastructure ✅
- Phase 15.2: Column definition token parsing (D1, D2, D3, E1, E2) ✅
- Phase 15.3: DDL object extraction (B1-B8) ✅
- Phase 15.4: Constraint parsing (C1-C4) ✅
- Phase 15.5: Statement detection (A1-A5) ✅
- Phase 15.6: Miscellaneous extraction (G1-G3) ✅
- Phase 15.7: SQL preprocessing (H1-H3) ✅
- SQLCMD tasks I1-I2 remain regex-based by design (line-oriented preprocessing)

| Layer | Passing | Rate |
|-------|---------|------|
| Layer 1 (Inventory) | 44/44 | 100% |
| Layer 2 (Properties) | 44/44 | 100% |
| Layer 3 (SqlPackage) | 44/44 | 100% |
| Relationships | 44/44 | 100% |
| Layer 4 (Ordering) | 44/44 | 100% |
| Metadata | 44/44 | 100% |

### Excluded Fixtures

Two fixtures are excluded from parity testing because DotNet fails to build them:

1. **external_reference** - References an external database via synonym; DotNet fails with SQL71501
2. **unresolved_reference** - View references non-existent table; DotNet fails with SQL71501

These test Rust's ability to build projects that DotNet cannot handle.

---

## Verification Commands

```bash
just test                                    # Run all tests
cargo test --test e2e_tests test_parity_regression_check  # Check regressions
PARITY_UPDATE_BASELINE=1 cargo test --test e2e_tests test_parity_regression_check -- --nocapture  # Update baseline

# Test specific fixture
SQL_TEST_PROJECT=tests/fixtures/<name>/project.sqlproj cargo test --test e2e_tests test_layer1 -- --nocapture
SQL_TEST_PROJECT=tests/fixtures/<name>/project.sqlproj cargo test --test e2e_tests test_layer2 -- --nocapture
SQL_TEST_PROJECT=tests/fixtures/<name>/project.sqlproj cargo test --test e2e_tests test_relationship -- --nocapture
```

---

## Known Issues

### Deploy Test [nvarchar] Reference Error
**Test:** `test_e2e_deploy_comprehensive_with_post_deploy`
**Status:** Known issue - works in CI, fails locally without SQL Server

When deploying the e2e_comprehensive dacpac, SqlPackage may report "The reference to the element that has the name [nvarchar] could not be resolved". This is caused by type references (e.g., `[nvarchar]`) emitted in ExpressionDependencies for computed columns with CAST expressions.

This does not affect Layer 3 parity testing (which compares dacpacs, not deployments) and the test passes in CI where SQL Server is available via Docker.

---

## Phase 16: Performance Tuning

**Goal:** Establish benchmarking infrastructure and optimize build performance.

### Baseline Performance Metrics (Phase 16.1.7)

Benchmarks run on criterion 0.5 with 100 samples per measurement.

#### Full Pipeline (sqlproj → dacpac)

| Fixture | Files | Mean Time | Notes |
|---------|-------|-----------|-------|
| e2e_simple | minimal | **19.4 ms** | Minimal project baseline |
| e2e_comprehensive | 30 | **85.8 ms** | Production-realistic project |
| stress_test | 135 | **462.1 ms** | High file count stress test |

**Scaling:** ~3.4 ms/file for stress_test (135 files), ~2.9 ms/file for e2e_comprehensive (30 files).

#### Pipeline Stage Breakdown (e2e_comprehensive)

| Stage | Time | % of Total |
|-------|------|------------|
| sqlproj_parsing | 0.15 ms | 0.2% |
| sql_parsing (28 stmts) | 5.25 ms | 6.1% |
| model_building (34 stmts) | 8.18 ms | 9.5% |
| xml_generation (85 elements) | 70.6 ms | **82.2%** |
| dacpac_packaging | 73.5 ms | N/A (parallel) |

**Key Finding:** XML generation dominates at 82% of pipeline time. This is the primary optimization target.

#### Stress Test Stage Breakdown

| Stage | Time | vs e2e_comprehensive |
|-------|------|---------------------|
| sql_parsing (135 stmts) | 12.0 ms | 2.3x |
| model_building (135 stmts) | 38.2 ms | 4.7x |
| Full pipeline | 462.1 ms | 5.4x |

**Scaling Analysis:** Model building scales super-linearly (4.7x for 4.8x files), suggesting O(n log n) or relationship resolution overhead.

#### Comparison with .NET DacFx

| Metric | rust-sqlpackage | .NET DacFx | Speedup |
|--------|-----------------|------------|---------|
| e2e_comprehensive (30 files) | 85.8 ms | ~2.3s cold / ~800ms warm | **27x cold / 9x warm** |

### Phase 16.1: Benchmark Infrastructure (7/7) ✅

| ID | Task | Status | Blocked By |
|----|------|--------|------------|
| 16.1.1 | Add criterion benchmark infrastructure | ✅ | - |
| 16.1.2 | Create full pipeline benchmark | ✅ | 16.1.1 |
| 16.1.3 | Create SQL parsing benchmark | ✅ | 16.1.1 |
| 16.1.4 | Create model building benchmark | ✅ | 16.1.1 |
| 16.1.5 | Create XML generation benchmark | ✅ | 16.1.1 |
| 16.1.6 | Create stress_test fixture (100+ SQL files) | ✅ | - |
| 16.1.7 | Run initial profiling and document baseline | ✅ | 16.1.2-16.1.6 |

### Phase 16.2: Quick Wins (3/5)

| ID | Task | Status | Blocked By | Expected Gain | Actual Gain |
|----|------|--------|------------|---------------|-------------|
| 16.2.1 | Add once_cell dependency | ✅ | - | - | - |
| 16.2.2 | Cache regex compilations in model_xml.rs | ✅ | 16.1.7, 16.2.1 | 5-10% | **2-4% full pipeline** |
| 16.2.3 | Optimize string joining in preprocess_parser.rs | ✅ | 16.1.7 | 1-3% | **5-9% SQL parsing** |
| 16.2.4 | Cache uppercase SQL in fallback parsing | ⬜ | 16.1.7 | 1-2% | |
| 16.2.5 | Add capacity hints to vector allocations | ⬜ | 16.1.7 | <1% | |

#### 16.2.2 Implementation Notes

Replaced 30 `regex::Regex::new()` calls in `model_xml.rs` with static `LazyLock<Regex>` patterns that are compiled once and reused. Used `std::sync::LazyLock` (Rust 1.80+) instead of `once_cell::sync::Lazy`.

**Benchmark Results (vs baseline):**
- Full pipeline: 2-4% improvement (e2e_comprehensive: 18.7ms from 85.8ms baseline)
- XML generation: **99% improvement** (708µs from ~70ms) - this was the primary hotspot
- Model building: 5-6% improvement
- Dacpac packaging: 92% improvement

Note: The large improvements in xml_generation and dacpac_packaging are partially due to the benchmark measuring the cached regex benefit; the full pipeline shows more modest gains because other stages (SQL parsing, file I/O) dominate.

#### 16.2.3 Implementation Notes

Optimized string allocation patterns in `preprocess_parser.rs`:
- Replaced `Vec<String>.join("")` with single `String` buffer using `with_capacity()` for pre-allocated output
- Changed `token_to_string()` to return `Cow<'static, str>` for zero-allocation on static tokens (punctuation, operators)
- Added capacity hints to `parse_parenthesized_expression()` and whitespace collection

**Benchmark Results (vs baseline):**
- SQL parsing: **5-9% improvement** (e2e_comprehensive: 4.9ms, stress_test: 10.5ms)
- Full pipeline: No statistically significant change (within noise margin)

Note: The improvement is concentrated in SQL parsing where preprocessing occurs, but the full pipeline has higher variance due to I/O and other factors.

### Phase 16.3: Medium Effort Optimizations (0/3)

| ID | Task | Status | Blocked By | Expected Gain |
|----|------|--------|------------|---------------|
| 16.3.1 | Reduce cloning in model builder with Cow | ⬜ | 16.2.2-16.2.5 | 3-5% |
| 16.3.2 | Pre-compute sort keys for XML elements | ⬜ | 16.2.2-16.2.5 | 1-2% |
| 16.3.3 | Batch string formatting in XML generation | ⬜ | 16.2.2-16.2.5 | 2-5% |

### Phase 16.4: Parallelization (0/2)

| ID | Task | Status | Blocked By | Expected Gain |
|----|------|--------|------------|---------------|
| 16.4.1 | Add rayon dependency | ⬜ | - | - |
| 16.4.2 | Parallelize SQL file parsing | ⬜ | 16.1.6, 16.4.1 | 20-40% |

### Phase 16.5: Documentation (0/1)

| ID | Task | Status | Blocked By |
|----|------|--------|------------|
| 16.5.1 | Document performance improvements | ⬜ | 16.3.1-16.3.3, 16.4.2 |

### Identified Hotspots

Based on code analysis:

| Area | Location | Issue | Impact | Status |
|------|----------|-------|--------|--------|
| Regex compilation | `src/dacpac/model_xml.rs` | 32 uncached Regex::new() calls | HIGH | ✅ Fixed in 16.2.2 |
| String joining | `src/parser/preprocess_parser.rs` | Vec<String>.join() inefficiency | MEDIUM | ✅ Fixed in 16.2.3 |
| Cloning | `src/model/builder.rs` | 149 clone() calls | MEDIUM | ⬜ |
| String conversion | `src/parser/tsql_parser.rs` | Multiple .to_uppercase() on same SQL | LOW | ⬜ |
| Sequential I/O | `src/parser/tsql_parser.rs` | Sequential file parsing | HIGH (large projects) | ⬜ |

### Benchmark Commands

```bash
# Run all benchmarks
cargo bench

# Run specific benchmark
cargo bench --bench pipeline

# Compare against baseline
cargo bench -- --save-baseline before
# ... make changes ...
cargo bench -- --baseline before

# Generate flamegraph
cargo flamegraph --release -- build --project tests/fixtures/e2e_comprehensive/Database.sqlproj
```

---

<details>
<summary>Completed Phases Summary</summary>

### Phase Overview

| Phase | Description | Tasks |
|-------|-------------|-------|
| Phase 1-9 | Core implementation (properties, relationships, XML structure, metadata) | 58/58 |
| Phase 10 | Fix extended properties, function classification, constraint naming, SqlPackage config | 5/5 |
| Phase 11 | Fix remaining parity failures, error fixtures, ignored tests | 70/70 |
| Phase 12 | SELECT * expansion, TVF columns, duplicate refs | 6/6 |
| Phase 13 | Fix remaining relationship parity issues (TVP support) | 4/4 |
| Phase 14 | Layer 3 (SqlPackage) parity | 3/3 |
| Phase 15 | Parser refactoring: replace regex with token-based parsing | 34/34 |
| Phase 16 | Performance tuning: benchmarks, regex caching, parallelization | 9/18 |

### Key Implementation Details

#### Phase 11: Parity Failures & Error Fixtures
- Fixed Layer 1-4 and relationship parity across all fixtures
- Excluded `external_reference` and `unresolved_reference` from parity testing (DotNet cannot build them)
- Fixed table type indexes, default constraints, and inline annotations
- Removed all `#[ignore]` attributes from passing tests

#### Phase 12: Relationship Parity
- **SELECT * expansion**: Added `expand_select_star()` function to look up table columns from DatabaseModel
- **Duplicate references**: Removed deduplication in triggers and views to preserve duplicates in GROUP BY
- **CAST type references**: Added extraction of type references from CAST expressions in computed columns
- **TVF Columns**: Added `Columns` relationship for inline and multi-statement table-valued functions

#### Phase 13: TVP Support
- Full table-valued parameter (TVP) support for procedures
- DynamicObjects relationship with SqlDynamicColumnSource elements
- Parameter parsing for `[schema].[type]` format and READONLY keyword
- TVP column reference extraction for BodyDependencies

#### Phase 14: Layer 3 SqlPackage Parity
- Fixed DefaultFilegroup relationship in SqlDatabaseOptions
- Added missing database options properties (Collation, IsTornPageProtectionOn, DefaultLanguage, etc.)
- Changed IsFullTextEnabled default from False to True to match DotNet

#### Phase 15: Parser Refactoring
Replaced regex-based fallback parsing with token-based parsing using sqlparser-rs custom dialect:
- **15.1**: Created `ExtendedTsqlDialect` wrapper in `src/parser/tsql_dialect.rs`
- **15.2**: Token-based column parsing in `src/parser/column_parser.rs` (D1-D3, E1-E2)
- **15.3**: Token-based DDL object extraction (B1-B8) - procedures, functions, triggers, sequences, types, indexes, fulltext
- **15.4**: Token-based constraint parsing in `src/parser/constraint_parser.rs` (C1-C4)
- **15.5**: Token-based statement detection in `src/parser/statement_parser.rs` (A1-A5)
- **15.6**: Token-based extended property parsing in `src/parser/extended_property_parser.rs` (G1-G3)
- **15.7**: Token-based SQL preprocessing in `src/parser/preprocess_parser.rs` (H1-H3)
- SQLCMD (I1-I2) intentionally remain regex-based for line-oriented preprocessing

See [PARSER_REFACTORING_GUIDE.md](./PARSER_REFACTORING_GUIDE.md) for implementation details.

</details>
