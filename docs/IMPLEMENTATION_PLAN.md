# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Status: PARITY COMPLETE | PARSER REFACTORING IN PROGRESS

**Phases 1-14 complete (146 tasks). Full parity achieved.**
**Phase 15.1 complete: ExtendedTsqlDialect infrastructure created.**
**Phase 15.2 complete: Column definition token parsing (D1, D2, D3, E1, E2 all complete).**
**Phase 15.3 complete: DDL object extraction (B1 ✅, B2 ✅, B3 ✅, B4 ✅, B5 ✅, B6 ✅). Next: B7/B8 fulltext indexes, or Phase 15.4 constraints.**

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

## Phase 15: Parser Refactoring - Replace Regex Fallbacks with Custom sqlparser-rs Dialect

**Status:** IN PROGRESS (Phase 15.1 complete)

**Goal:** Replace brittle regex-based fallback parsing with proper token-based parsing using sqlparser-rs custom dialect extension. This improves maintainability, error messages, and handles edge cases better.

**Approach:** Create a custom `ExtendedTsqlDialect` that intercepts specific token sequences before delegating to the base MsSqlDialect.

**Documentation:** See **[PARSER_REFACTORING_GUIDE.md](./PARSER_REFACTORING_GUIDE.md)** for detailed implementation guidance, API reference, code examples, and migration path.

### Phase 15.1: Infrastructure ✅ COMPLETE

Created `ExtendedTsqlDialect` wrapper in `src/parser/tsql_dialect.rs`:

- Wraps `MsSqlDialect` and delegates all parsing to it
- Overrides `dialect()` method to return `MsSqlDialect`'s TypeId, ensuring `dialect_of!(self is MsSqlDialect)` checks pass
- This is critical because sqlparser uses these checks internally for T-SQL-specific parsing (e.g., IDENTITY columns)
- Added comprehensive tests for dialect behavior and MsSqlDialect equivalence
- Updated `parse_sql_file()` to use the new dialect
- All 250+ existing tests pass

### Phase 15.2: Critical Path (Column Definitions) 🔄 IN PROGRESS

Created token-based column definition parser in `src/parser/column_parser.rs`:

- New `ColumnTokenParser` struct for token-based column parsing
- `TokenParsedColumn` struct representing parsed column with all attributes (name, type, nullability, identity, default, collation, constraints)
- `parse_column_definition_tokens()` function that replaces 15+ regex patterns for column parsing
- Handles column name/type extraction, IDENTITY specifications, NULL/NOT NULL, COLLATE, DEFAULT constraints (named and unnamed), and inline PRIMARY KEY/UNIQUE constraints
- Updated `parse_column_definition()` in `tsql_parser.rs` to use the new token parser as primary method
- Added 22 unit tests covering various column definition patterns
- Tasks D1 (column name, type, options) and E1 (default constraint variants) completed

### Regex Inventory

Current fallback parsing uses **75+ regex patterns** across two files:
- `src/parser/tsql_parser.rs` - Main T-SQL parsing (70+ patterns)
- `src/parser/sqlcmd.rs` - SQLCMD directives (3 patterns)

### Tasks by Category

#### Category A: Statement Type Detection Fallbacks (5 tasks)
| # | Task | Regex Location | Priority |
|---|------|----------------|----------|
| A1 | DROP SYNONYM/TRIGGER/INDEX/PROC detection | `try_drop_fallback` L692-769 | Medium |
| A2 | CTE with DML (DELETE/UPDATE/INSERT/MERGE) | `try_cte_dml_fallback` L772-799 | High |
| A3 | MERGE with OUTPUT clause | `try_merge_output_fallback` L801-826 | Medium |
| A4 | UPDATE with XML methods (.MODIFY/.VALUE) | `try_xml_method_fallback` L828-855 | Low |
| A5 | Generic CREATE fallback | `try_generic_create_fallback` L1115-1171 | Low |

#### Category B: DDL Object Extraction (8 tasks)
| # | Task | Regex Location | Priority | Status |
|---|------|----------------|----------|--------|
| B1 | CREATE/ALTER PROCEDURE name | `extract_procedure_name`, `extract_alter_procedure_name` | High | ✅ |
| B2 | CREATE/ALTER FUNCTION name, params, return type | `extract_function_info` L1768-1899, `extract_alter_function_info` L1663-1682 | High | ✅ |
| B3 | CREATE TRIGGER (name, parent, events) | `extract_trigger_info` L1536-1614 | High | ✅ |
| B4 | CREATE/ALTER SEQUENCE (all options) | `extract_sequence_info` L1174-1260, `extract_alter_sequence_info` L1684-1766 | Medium | ✅ |
| B5 | CREATE TYPE AS TABLE | `extract_table_type_info` L1262-1535 | High | ✅ |
| B6 | CREATE INDEX (all options) | `extract_index_info` L1901-2018 | High | ✅ |
| B7 | CREATE FULLTEXT INDEX | `extract_fulltext_index_info` L2675-2789 | Low | |
| B8 | CREATE FULLTEXT CATALOG | `extract_fulltext_catalog_info` L2800-2815 | Low | |

#### Category C: Constraint Parsing (4 tasks)
| # | Task | Regex Location | Priority |
|---|------|----------------|----------|
| C1 | ALTER TABLE ADD CONSTRAINT (FK) | `extract_alter_table_add_constraint` L887-992 | High |
| C2 | ALTER TABLE ADD CONSTRAINT (PK/UNIQUE) | `extract_alter_table_add_constraint` L993-1072 | High |
| C3 | Table constraint extraction | `extract_table_constraint` L2424-2540 | High |
| C4 | ALTER TABLE name extraction | `extract_alter_table_name` L857-876 | Medium |

#### Category D: Column Definition Parsing (3 tasks)
| # | Task | Regex Location | Priority | Status |
|---|------|----------------|----------|--------|
| D1 | Column name, type, and options | `extract_column_definition` L2181-2420 (15+ regex) | Critical | ✅ |
| D2 | Computed column detection | `extract_column_definition` L2187-2230 | High | ✅ |
| D3 | Table type column parsing | `parse_table_type_body` L1397-1535 | High | ✅ |

#### Category E: Inline Constraint Parsing (2 tasks)
| # | Task | Regex Location | Priority | Status |
|---|------|----------------|----------|--------|
| E1 | Default constraint variants (8 regex patterns) | `extract_column_definition` L2270-2380 | Critical | ✅ |
| E2 | Check constraint (named/unnamed) | `extract_column_definition` L2382-2420 | High | ✅ |

#### Category F: Index & Option Extraction (4 tasks)
| # | Task | Regex Location | Priority |
|---|------|----------------|----------|
| F1 | INCLUDE columns | `extract_include_columns` L1971-1979 | Medium |
| F2 | FILLFACTOR option | `extract_index_fill_factor` L1982-1989 | Medium |
| F3 | DATA_COMPRESSION option | `extract_index_data_compression` L1992-2001 | Medium |
| F4 | WHERE filter predicate | `extract_filter_predicate` L2003-2018 | Medium |

#### Category G: Miscellaneous Extraction (3 tasks)
| # | Task | Regex Location | Priority |
|---|------|----------------|----------|
| G1 | sp_addextendedproperty parsing | `extract_extended_property` L1073-1113 | Medium |
| G2 | Full-text index columns with LANGUAGE | `extract_fulltext_columns` L2745-2789 | Low |
| G3 | Data type parsing | `parse_data_type` L1314-1395 | Medium |

#### Category H: SQL Preprocessing (3 tasks)
| # | Task | Regex Location | Priority |
|---|------|----------------|----------|
| H1 | BINARY/VARBINARY(MAX) sentinel replacement | `preprocess_for_sqlparser` L2586-2590 | High |
| H2 | DEFAULT FOR constraint extraction | `preprocess_for_sqlparser` L2597-2615 | High |
| H3 | Trailing comma cleanup | `preprocess_for_sqlparser` L2615-2616 | Medium |

#### Category I: SQLCMD Preprocessing (2 tasks)
| # | Task | Regex Location | Priority |
|---|------|----------------|----------|
| I1 | :setvar directive parsing | `sqlcmd.rs` L71-82 | Low |
| I2 | :r include directive parsing | `sqlcmd.rs` L87-93 | Low |

### Implementation Strategy

1. **Phase 15.1: Infrastructure** ✅ - Created `ExtendedTsqlDialect` wrapper with MsSqlDialect delegation
2. **Phase 15.2: Critical Path** ✅ COMPLETE - D1, D2, D3, E1, E2 all complete (column definitions fully migrated to token-based parsing)
3. **Phase 15.3: DDL Objects** ✅ COMPLETE - B1 ✅, B2 ✅, B3 ✅, B4 ✅, B5 ✅, B6 ✅ (all high-priority DDL objects migrated)
4. **Phase 15.4: Constraints** - C1-C4, E2 (constraint parsing)
5. **Phase 15.5: Statement Detection** - A1-A5 (fallback statement types)
6. **Phase 15.6: Options & Misc** - F1-F4, G1-G3 (index options, extended properties)
7. **Phase 15.7: Preprocessing** - H1-H3, I1-I2 (SQL preprocessing, SQLCMD)

### Success Criteria

- [ ] All existing tests pass
- [ ] No regex patterns in hot parsing paths
- [ ] Improved error messages with line/column info
- [ ] Reduced parsing time for large SQL files (benchmark)

### Resources

- [sqlparser-rs custom dialect docs](https://github.com/apache/datafusion-sqlparser-rs/blob/main/docs/custom_sql_parser.md)
- [Databend custom parser blog](https://www.databend.com/blog/category-engineering/2025-09-10-query-parser/)
- [antlr4rust](https://github.com/rrevenantt/antlr4rust) (alternative approach)

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

</details>
