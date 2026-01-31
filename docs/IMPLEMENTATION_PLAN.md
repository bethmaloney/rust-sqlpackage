# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Status: PARITY COMPLETE | REAL-WORLD COMPATIBILITY IN PROGRESS

**Phases 1-14 complete (146 tasks). Full parity achieved.**
**Phase 15 complete: Parser refactoring tasks finished (including whitespace-agnostic keyword matching).**
**Phase 16 in progress: Performance tuning and benchmarking.**
**Phase 17 in progress: Real-world SQL compatibility fixes.**
- Phase 15.1: ExtendedTsqlDialect infrastructure ✅
- Phase 15.2: Column definition token parsing (D1, D2, D3, E1, E2) ✅
- Phase 15.3: DDL object extraction (B1-B8) ✅
- Phase 15.4: Constraint parsing (C1-C4) ✅
- Phase 15.5: Statement detection (A1-A5) ✅
- Phase 15.6: Miscellaneous extraction (G1-G3) ✅
- Phase 15.7: SQL preprocessing (H1-H3) ✅
- Phase 15.8: Whitespace-agnostic keyword matching (J1-J7) ✅
- Phase 17.1: Comma-less constraint parsing (0/3) 🔄
- Phase 17.2: SQLCMD variable header format (0/2) 🔄
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

## Phase 15.8: Whitespace-Agnostic Keyword Matching

**Goal:** Replace space-only string matching patterns (e.g., `find(" AS ")`) with token-based parsing to correctly handle tabs, multiple spaces, and mixed whitespace around SQL keywords.

**Background:** Several locations use patterns like `find(" AS ")` or `contains(" FROM ")` which only match single spaces. SQL allows any whitespace (tabs, multiple spaces, newlines) between tokens. The fix uses sqlparser's tokenizer to find keywords regardless of surrounding whitespace.

**Pattern:** Use `Tokenizer::new(&MsSqlDialect{}, sql).tokenize()` and search for `Token::Word` with the appropriate `Keyword` enum value, tracking parenthesis depth when needed.

### Phase 15.8: Tasks (7/7) ✅

| ID | Task | File | Line | Current Pattern | Status |
|----|------|------|------|-----------------|--------|
| J1 | Fix AS alias in `parse_column_expression()` | `src/dacpac/model_xml.rs` | 1621+ | Token-based (fixed) | ✅ |
| J2 | Fix AS alias in TVF parameter references | `src/dacpac/model_xml.rs` | 1307 | Token-based (fixed) | ✅ |
| J3 | Fix AS keyword in `extract_view_query()` | `src/dacpac/model_xml.rs` | 1062-1092 | Token-based (fixed) | ✅ |
| J4 | Fix FOR keyword in trigger parsing | `src/dacpac/model_xml.rs` | 5050 | Token-based (fixed) | ✅ |
| J5 | Fix AS keyword in trigger body extraction | `src/dacpac/model_xml.rs` | 5054-5069 | Token-based (fixed) | ✅ |
| J6 | Fix FROM/AS TABLE in type detection | `src/parser/tsql_parser.rs` | 633 | Token-based (fixed) | ✅ |
| J7 | Fix FROM/NOT NULL in scalar type parsing | `src/parser/tsql_parser.rs` | 1029, 1038 | Token-based (fixed) | ✅ |

### Implementation Notes

**J1 (Complete):** Demonstrates the pattern - tokenize expression, track paren depth, find last `Keyword::AS` at depth 0, extract alias from subsequent tokens.

**J3 (Complete):** Uses token-based parsing to find the first AS keyword after VIEW keyword at paren depth 0. Returns everything after the AS as the query body. Unlike J1 which finds the last AS (for alias expressions), J3 finds the first AS (for view definitions).

**J4-J5 (Complete):** Combined implementation in `extract_trigger_body()`. Uses tokenization to find FOR/AFTER/INSTEAD keywords followed by AS at top level (paren depth 0). Removed the TRIGGER_AS_RE regex which is no longer needed.

**J6 (Complete):** Created `is_scalar_type_definition(sql: &str) -> Option<bool>` helper that uses tokenization to find FROM vs AS TABLE at paren depth 0. Returns `Some(true)` for scalar types (FROM keyword found), `Some(false)` for table types (AS TABLE found).

**J7 (Complete):** Rewrote `extract_scalar_type_info()` to use tokenization: finds FROM keyword at paren depth 0, extracts tokens after FROM, checks for NOT NULL using token matching instead of string contains.

### Lower Priority

These patterns are less likely to cause issues but must be addressed for consistency:

| Pattern | File | Line | Notes |
|---------|------|------|-------|
| `trim_end_matches(" READONLY")` | `src/dacpac/model_xml.rs` | 2728 | End-of-string, less likely tab-affected |
| `trim_end_matches(" NULL")` | `src/dacpac/model_xml.rs` | 2729 | End-of-string, less likely tab-affected |
| `trim_end_matches(" NOT")` | `src/dacpac/model_xml.rs` | 2730 | End-of-string, less likely tab-affected |

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

### Phase 16.2: Quick Wins (5/5) ✅

| ID | Task | Status | Blocked By | Expected Gain | Actual Gain |
|----|------|--------|------------|---------------|-------------|
| 16.2.1 | Add once_cell dependency | ✅ | - | - | - |
| 16.2.2 | Cache regex compilations in model_xml.rs | ✅ | 16.1.7, 16.2.1 | 5-10% | **2-4% full pipeline** |
| 16.2.3 | Optimize string joining in preprocess_parser.rs | ✅ | 16.1.7 | 1-3% | **5-9% SQL parsing** |
| 16.2.4 | Cache uppercase SQL in fallback parsing | ✅ | 16.1.7 | 1-2% | **~1.5% SQL parsing** |
| 16.2.5 | Add capacity hints to vector allocations | ✅ | 16.1.7 | <1% | **<1% full pipeline** |

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

#### 16.2.4 Implementation Notes

Modified `extract_scalar_type_info()` and `extract_table_structure()` functions in `src/parser/tsql_parser.rs` to accept a pre-computed uppercase SQL string parameter instead of calling `.to_uppercase()` internally. Updated call sites in `try_fallback_parse()` to pass the already-computed `sql_upper` variable, eliminating redundant uppercase conversions.

**Benchmark Results (vs baseline):**
- Full pipeline: No statistically significant change (within noise margin)
- SQL parsing: ~1.5% improvement on e2e_comprehensive (within noise)

Note: This optimization targets a narrow code path (fallback parsing for CREATE TYPE scalar and CREATE TABLE edge cases), so the measurable impact is minimal. The change eliminates unnecessary allocations but the affected code paths are infrequently executed.

#### 16.2.5 Implementation Notes

Added capacity hints to 9 key vector allocations in hot paths across `src/parser/tsql_parser.rs` and `src/dacpac/model_xml.rs`:

**Parser hot paths (tsql_parser.rs):**
- `parse_sql_files()`: `Vec::with_capacity(files.len() * 2)` - estimate 2 statements per file
- `parse_sql_file()`: `Vec::with_capacity(batches.len())` - one statement per batch
- `parse_table_body()`: `Vec::with_capacity(parts.len())` for columns, capacity 4 for constraints
- `split_by_top_level_comma()`: `Vec::with_capacity(s.len() / 30)` - estimate column definition length
- `split_batches()`: `Vec::with_capacity(line_count / 20)` - estimate GO separator frequency

**Model XML generation (model_xml.rs):**
- `expand_select_star()`: `Vec::with_capacity(table_aliases.len() * 5)` - estimate 5 columns per table
- `extract_view_columns_and_deps()`: Pre-allocate based on `select_columns.len()` and `table_aliases`
- `extract_multi_statement_tvf_columns()`: `Vec::with_capacity(col_defs.len())` - known size
- `extract_body_dependencies()`: `Vec::with_capacity(10)` for deps and HashSet, `Vec::with_capacity(5)` for table_refs

**Benchmark Results (vs baseline):**
- Full pipeline: <1% improvement (within noise margin)

Note: These are micro-optimizations that reduce allocations and potential reallocation overhead but don't significantly impact overall performance. The full pipeline is dominated by XML generation and I/O, so allocation efficiency has minimal measurable effect.

### Phase 16.3: Medium Effort Optimizations (2/3)

| ID | Task | Status | Blocked By | Expected Gain | Actual Gain |
|----|------|--------|------------|---------------|-------------|
| 16.3.1 | Reduce cloning in model builder with Cow | ✅ | 16.2.2-16.2.5 | 3-5% | ~2-3% SQL parsing |
| 16.3.2 | Pre-compute sort keys for XML elements | ✅ | 16.2.2-16.2.5 | 1-2% | ~2-4% full pipeline |
| 16.3.3 | Batch string formatting in XML generation | ⬜ | 16.2.2-16.2.5 | 2-5% | |

#### 16.3.1 Implementation Notes

Introduced `track_schema()` helper function using `Cow<'static, str>` to reduce redundant cloning in schema tracking. Key changes:

- Schema tracking now uses `BTreeSet<Cow<'static, str>>` instead of `BTreeSet<String>`
- Static "dbo" schema uses borrowed reference (`Cow::Borrowed(DBO_SCHEMA)`) avoiding allocation
- New `track_schema()` function checks if schema already exists before cloning, reducing duplicate allocations
- All 14 `schemas.insert()` call sites updated to use the optimized pattern

**Benchmark Results (vs baseline):**
- Full pipeline: -7.3% improvement on first run (within noise on subsequent runs)
- SQL parsing: -2.8% to -3.0% improvement
- Dacpac packaging: -6.4% improvement
- Model building: No statistically significant change (within noise margin)

Note: The optimization reduces allocations for schema tracking but doesn't affect the model building stage significantly because struct fields still require owned Strings. The primary benefit is reduced memory churn from duplicate schema name allocations.

#### 16.3.2 Implementation Notes

Used `sort_by_cached_key` in `sort_elements()` function (`src/model/builder.rs`) to pre-compute sort keys once per element instead of recomputing on each comparison. Also optimized the `db_options_sort_key` in `generate_model_xml()` (`src/dacpac/model_xml.rs`) to use static string slices instead of owned Strings.

**Changes:**
- Replaced `sort_by()` with `sort_by_cached_key()` which computes `(xml_name_attr().to_lowercase(), type_name().to_lowercase())` once per element
- Changed `db_options_sort_key` from `(String, String)` to `(&str, &str)` to avoid allocation

**Benchmark Results (vs baseline):**
- Full pipeline: **-4.5%** improvement (e2e_comprehensive: 18.0ms, stress_test: 57.1ms)
- Model building: -2.7% improvement on stress_test
- XML generation: -2.4% improvement

Note: The improvement is more pronounced on larger datasets (stress_test) where sorting overhead becomes more significant.

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
| String conversion | `src/parser/tsql_parser.rs` | Multiple .to_uppercase() on same SQL | LOW | ✅ Fixed in 16.2.4 |
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

## Phase 17: Real-World SQL Compatibility

**Goal:** Fix parsing and format issues discovered when testing against real-world databases that use relaxed SQL syntax not covered by test fixtures.

**Background:** Real-world SQL databases often use syntactic patterns that SQL Server accepts but are not strictly standard. These include:
- Constraints without comma separators between column definitions and constraints
- Different SQLCMD variable header formats

### Phase 17.1: Comma-less Constraint Parsing (0/3)

**Problem:** SQL Server accepts constraints without comma separators:
```sql
CREATE TABLE [dbo].[Example] (
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL
    PRIMARY KEY ([Id])  -- No comma before PRIMARY KEY
);
```

sqlparser-rs doesn't parse these constraints, causing them to be silently ignored.

**Test:** `test_parity_commaless_constraints` in `tests/e2e/dotnet_comparison_tests.rs`
**Fixture:** `tests/fixtures/commaless_constraints/`

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 17.1.1 | Investigate sqlparser-rs constraint parsing behavior | ⬜ | Determine if fixable in dialect or needs post-processing |
| 17.1.2 | Implement comma-less constraint detection and parsing | ⬜ | Either extend dialect or add fallback parser |
| 17.1.3 | Verify all constraint types work (PK, FK, CHECK, DEFAULT) | ⬜ | Update fixture to cover all cases |

### Phase 17.2: SQLCMD Variable Header Format (0/2)

**Problem:** SQLCMD variables in model.xml Header use different format than .NET DacFx.

**Current Rust format:**
```xml
<CustomData Category="SqlCmdVariable">
  <Metadata Name="SqlCmdVariable" Value="Environment"/>
  <Metadata Name="DefaultValue" Value="Development"/>
</CustomData>
<!-- Repeated for each variable -->
```

**Expected .NET format:**
```xml
<CustomData Category="SqlCmdVariables" Type="SqlCmdVariable">
  <Metadata Name="Environment" Value="" />
  <Metadata Name="ServerName" Value="" />
</CustomData>
<!-- Single element with all variables -->
```

**Test:** `test_sqlcmd_variables_header_format` in `tests/e2e/dotnet_comparison_tests.rs`
**Fixture:** `tests/fixtures/sqlcmd_variables/`

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 17.2.1 | Update Header CustomData format for SQLCMD variables | ⬜ | Change Category to plural, add Type attribute |
| 17.2.2 | Use variable name as Metadata Name attribute | ⬜ | Match .NET format exactly |

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
| Phase 16 | Performance tuning: benchmarks, regex caching, parallelization | 12/18 |
| Phase 17 | Real-world SQL compatibility: comma-less constraints, SQLCMD format | 0/5 |

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
- **15.8**: Whitespace-agnostic keyword matching (J1-J7) - replace space-only patterns with tokenizer
- SQLCMD (I1-I2) intentionally remain regex-based for line-oriented preprocessing

See [PARSER_REFACTORING_GUIDE.md](./PARSER_REFACTORING_GUIDE.md) for implementation details.

</details>
