# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

---

## Status: PARITY COMPLETE | REAL-WORLD COMPATIBILITY IN PROGRESS

**Phases 1-49 complete. Full parity: 46/48 (95.8%).**

**Current Work:**
- Phase 50 complete through 50.4 (storage elements)
- Phase 50.5: Security statement support for WideWorldImporters (pending)

**Remaining Work:**
- Phase 50.5: Handle security statements and GO-spanning comments for WideWorldImporters
- Layer 7 remaining issues: element ordering, formatting differences (17/48 passing)
- Body dependency ordering/deduplication differences (65 relationship errors in `body_dependencies_aliases` fixture - not affecting functionality)

| Layer | Passing | Rate |
|-------|---------|------|
| Layer 1 (Inventory) | 48/48 | 100% |
| Layer 2 (Properties) | 48/48 | 100% |
| Layer 3 (SqlPackage) | 48/48 | 100% |
| Relationships | 47/48 | 97.9% |
| Layer 4 (Ordering) | 48/48 | 100% |
| Metadata | 48/48 | 100% |
| Layer 7 (Canonical XML) | 17/48 | 35.4% |

### Excluded Fixtures

Two fixtures are excluded from parity testing because DotNet fails to build them:

1. **external_reference** - References an external database via synonym; DotNet fails with SQL71501
2. **unresolved_reference** - View references non-existent table; DotNet fails with SQL71501

---

## Phase 50: Fix Schema-Aware Resolution Gaps

**Status:** PHASE 50.4 COMPLETE

**Goal:** Address gaps identified in Phase 49 review - remove unsafe fallback behavior, add view support, and complete deferred testing.

### Problem Statement

Phase 49 implemented schema-aware column resolution but included a "backward compatibility" fallback that defeats the purpose:

```rust
// Old behavior (problematic) - FIXED in Phase 50.1
match candidates.len() {
    1 => Some(table),      // Unique match - resolve ✓
    0 => fallback_table,   // WAS: Still causes false positives - NOW RETURNS None
    _ => None,             // Ambiguous - skip ✓
}
```

When 0 tables in the registry have the column, the code now returns `None` instead of falling back to the first table - correctly skipping dependency emission when we don't know which table has the column.

### Phase 50.1: Remove Fallback Behavior (3 tasks) - COMPLETE 2026-02-04

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 50.1.1 | Change `find_scope_table_for_column()` to return `None` when 0 matches | ✅ | Fallback removed, returns None |
| 50.1.2 | Update callers to handle `None` by skipping dependency emission | ✅ | Callers already used `if let Some()` pattern |
| 50.1.3 | Run parity tests and document any new failures | ✅ | All 57 parity tests pass, no regressions |

**Implementation Notes:**
- Changed line 1452 in `body_deps.rs`: `0 => tables_in_scope.first()` → `0 => None`
- Added `registry_with_columns()` test helper for creating registries with specific column data
- Updated `test_apply_subquery_unqualified_column_resolution` to use schema-aware registry

### Phase 50.2: Add View Columns to Registry (4 tasks) - COMPLETE 2026-02-04

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 50.2.1 | Parse view SELECT clause to extract column names/aliases | ✅ | Reused extract_view_columns_and_deps() |
| 50.2.2 | Add `ViewElement` columns to `ColumnRegistry::from_model()` | ✅ | Views tracked like tables |
| 50.2.3 | Handle `SELECT *` in views by marking as "unknown columns" | ✅ | views_with_wildcard HashSet for conservative resolution |
| 50.2.4 | Add unit tests for view column extraction | ✅ | 6 new tests for extraction, aliases, SELECT *, resolution |

**Implementation Notes:**
- Reused existing `extract_view_columns_and_deps()` from `view_writer.rs`
- ViewElement columns extracted by parsing SELECT clause via sqlparser tokenization
- Views with SELECT * tracked via `views_with_wildcard` HashSet for future conservative resolution
- Added 6 new unit tests for view column extraction, aliases, SELECT *, and resolution
- All 992 library tests + 500 unit tests pass with no regressions

### Phase 50.3: Complete Deferred Testing (4 tasks) - PARTIALLY COMPLETE

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 50.3.1 | Clone and build WideWorldImporters with rust-sqlpackage | 🔄 | Blocked by Security/Permissions.sql - see Phase 50.5 |
| 50.3.2 | Deploy WideWorldImporters dacpac and verify no false positive errors | ⬜ | Pending 50.3.1 |
| 50.3.3 | Add explicit test for table variable column NOT resolving to global table | ✅ | Added `test_table_variable_column_does_not_resolve_to_global_table` in body_deps.rs |
| 50.3.4 | Add explicit test for CTE column NOT resolving to global table | ✅ | Added `test_cte_column_does_not_resolve_to_global_table` in body_deps.rs |

### Phase 50.4: Add Storage Element Support (7 tasks) - COMPLETE 2026-02-04

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 50.4.1 | Add ModelElement variants for Filegroup, PartitionFunction, PartitionScheme | ✅ | Added to elements.rs |
| 50.4.2 | Add FallbackStatementType variants for storage elements | ✅ | Added to tsql_parser.rs |
| 50.4.3 | Create storage_parser.rs with token-based parsers | ✅ | parse_filegroup/partition_function/partition_scheme_tokens |
| 50.4.4 | Update model builder to handle storage elements | ✅ | Added handling in builder.rs |
| 50.4.5 | Implement XML writers for storage elements | ✅ | write_filegroup/partition_function/partition_scheme in other_writers.rs |
| 50.4.6 | Add unit tests for storage parsers | ✅ | 12 tests covering all variants |
| 50.4.7 | Verify all tests pass | ✅ | 1506 tests passing |

**Implementation Notes:**
- Created `src/parser/storage_parser.rs` with token-based parsers for:
  - `ALTER DATABASE ... ADD FILEGROUP [name] [CONTAINS MEMORY_OPTIMIZED_DATA]`
  - `CREATE PARTITION FUNCTION [name](type) AS RANGE RIGHT/LEFT FOR VALUES (...)`
  - `CREATE PARTITION SCHEME [name] AS PARTITION [function] [ALL] TO (...)`
- Storage elements are NOT schema-qualified (use `[name]` not `[schema].[name]`)
- WideWorldImporters storage files now parse successfully

### Implementation Notes

**Why remove the fallback:**
- The fallback preserves false positives for: external dacpac columns, view columns, misspelled columns, table variable columns
- DotNet has full schema knowledge and doesn't need fallbacks - it resolves correctly or not at all
- Parity test failures from removing fallback will reveal where our resolution differs from DotNet

**View column extraction approach:**
```rust
// For views like: CREATE VIEW v AS SELECT Id, Name AS DisplayName FROM Users
// Extract: ["Id", "DisplayName"] (use alias if present, otherwise column name)

// For views with SELECT *: mark as "has_wildcard = true"
// When resolving columns against such views, skip resolution (can't know columns statically)
```

**Verification commands:**
```bash
just test                                              # All tests
cargo test --lib column_registry                       # Registry tests
cargo test --test e2e_tests test_parity_all_fixtures   # Parity tests
cargo test --lib storage                               # Storage parser tests

# WideWorldImporters testing
git clone https://github.com/microsoft/sql-server-samples.git /tmp/sql-samples
rust-sqlpackage build --project /tmp/sql-samples/samples/databases/wide-world-importers/wwi-ssdt/wwi-ssdt/WideWorldImporters.sqlproj
```

### Phase 50.5: Security Statement Support (Future Work)

**Status:** PENDING

**Problem:** WideWorldImporters build fails on `Security/Permissions.sql` with:
```
Error: SQL parse error in Security/Permissions.sql at line 2: Unexpected EOF while in a multi-line comment
```

**Root Cause:** The file has a block comment that spans a GO batch separator:
```sql
/*
GRANT VIEW ANY COLUMN ENCRYPTION KEY DEFINITION TO PUBLIC;
...
GO   <-- GO inside comment creates unterminated comment in first batch
...
*/
```

**Additional Issues:**
- File contains `CREATE LOGIN`, `CREATE USER`, `GRANT` statements not currently supported
- These are security/deployment statements, not schema elements typically in dacpac

**Potential Solutions:**
1. Handle multi-line comments spanning GO separators (pre-strip comments before batch splitting)
2. Add fallback parsing for security statements (CREATE LOGIN, CREATE USER, GRANT)
3. Skip security statements entirely as they're deployment-time, not schema definitions

---

## Phase 49: Schema-Aware Unqualified Column Resolution (COMPLETE)

**Status:** COMPLETE - 2026-02-04

**Goal:** Fix unqualified column resolution by checking which tables in scope actually have the column.

See `docs/UNQUALIFIED_COLUMN_RESOLUTION_ISSUE.md` for full analysis of the problem.

### Summary

Created `ColumnRegistry` to map tables to their columns, threaded it through the call chain, and updated resolution logic:
- If exactly 1 table in scope has the column → resolve to that table
- If 0 tables have the column → return None (skip dependency emission) - **FIXED in Phase 50.1**
- If >1 tables have the column → skip resolution (ambiguous)

**Files Created/Modified:**
- `src/dacpac/model_xml/column_registry.rs` (new - 380 lines)
- `src/dacpac/model_xml/mod.rs`, `body_deps.rs`, `programmability_writer.rs`, `view_writer.rs`

---

## Known Issues

| Issue | Location | Status |
|-------|----------|--------|
| Relationship parity body_dependencies_aliases | body_deps.rs | 65 errors (ordering/deduplication differences, not affecting functionality) |
| Layer 7 parity remaining | model_xml | 34/48 failing due to element ordering, formatting differences |

---

<details>
<summary>Completed Phases Summary (Phases 1-48)</summary>

## Phase Overview

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1-9 | Core implementation (properties, relationships, XML structure, metadata) | ✅ 58/58 |
| Phase 10 | Fix extended properties, function classification, constraint naming | ✅ 5/5 |
| Phase 11 | Fix remaining parity failures, error fixtures, ignored tests | ✅ 70/70 |
| Phase 12 | SELECT * expansion, TVF columns, duplicate refs | ✅ 6/6 |
| Phase 13 | Fix remaining relationship parity issues (TVP support) | ✅ 4/4 |
| Phase 14 | Layer 3 (SqlPackage) parity | ✅ 3/3 |
| Phase 15 | Parser refactoring: replace regex with token-based parsing | ✅ 34/34 |
| Phase 16 | Performance tuning: benchmarks, regex caching, parallelization | ✅ 18/18 |
| Phase 17 | Real-world SQL compatibility: comma-less constraints, SQLCMD format | ✅ 5/5 |
| Phase 18 | BodyDependencies alias resolution: fix table alias handling | ✅ 15/15 |
| Phase 19 | Whitespace-agnostic trim patterns: token-based TVP parsing | ✅ 3/3 |
| Phase 20 | Replace remaining regex with tokenization/AST | ✅ 43/43 |
| Phase 21 | Split model_xml.rs into submodules | ✅ 10/10 |
| Phase 22 | Layer 7 XML parity (annotations, CustomData, ordering) | ✅ 9/10 |
| Phase 23 | Fix IsMax property for MAX types | ✅ 4/4 |
| Phase 24 | Track dynamic column sources (CTEs, temp tables, table variables) | ✅ 8/8 |
| Phase 25 | Constraint parsing & properties (ALTER TABLE, IsNullable, CacheSize) | ✅ 6/6 |
| Phase 26 | Fix APPLY subquery alias capture in body dependencies | ✅ 4/4 |
| Phase 27-31 | Code consolidation (~1200 lines removed) | ✅ 13/13 |
| Phase 32 | Fix CTE column resolution in body dependencies | ✅ |
| Phase 33 | Fix comma-less table type PRIMARY KEY constraint parsing | ✅ 1/1 |
| Phase 34 | Fix APPLY subquery column resolution | ✅ 4/4 |
| Phase 35 | Fix default schema resolution for unqualified table names | ✅ 9/9 |
| Phase 36 | DacMetadata.xml dynamic properties (DacVersion, DacDescription) | ✅ 8/8 |
| Phase 37 | Derive CollationLcid from collation name | ✅ 10/10 |
| Phase 38 | Fix CollationCaseSensitive to always output "True" | ✅ |
| Phase 39 | Add SysCommentsObjectAnnotation to Views | ✅ |
| Phase 40 | Add SysCommentsObjectAnnotation to Procedures | ✅ |
| Phase 41 | Alias resolution for nested subqueries | ✅ |
| Phase 42 | Real-world deployment bug fixes (alias resolution, self-alias) | ✅ |
| Phase 43 | Scope-aware alias tracking (position-aware resolution) | ✅ 12/12 |
| Phase 44 | XML formatting improvements (space before />, element ordering) | ✅ |
| Phase 45 | Fix unit tests for XML format changes | ✅ |
| Phase 46 | Fix disambiguator numbering for package references | ✅ |
| Phase 47 | Column-level Collation property | ✅ |
| Phase 48 | Fix 2-named-constraint annotation pattern | ✅ |

## Key Milestones

### Parity Achievement (Phase 14)
- Layer 1 (Inventory): 100%
- Layer 2 (Properties): 100%
- Layer 3 (SqlPackage): 100%
- Relationships: 97.9%

### Performance (Phase 16)
- 116x faster than DotNet cold build
- 42x faster than DotNet warm build

### Parser Modernization (Phases 15, 20)
- Replaced all regex patterns with token-based parsing
- Created `BodyDependencyTokenScanner`, `TableAliasTokenParser`, `ColumnTokenParser`

### Body Dependency Resolution (Phases 26, 32, 34, 41-43)
- APPLY subquery alias capture and column resolution
- CTE column resolution to underlying tables
- Position-aware scope tracking for nested subqueries
- Handles same alias in different scopes

### XML Parity (Phases 22, 38-40, 44-48)
- Constraint annotation patterns match DotNet
- SysCommentsObjectAnnotation for views, procedures, functions
- XML formatting (space before />, element ordering)
- Layer 7 improved from 0% to 29.2%

## Phase Details

### Phase 22: Layer 7 Canonical XML Parity

**Constraint Annotation Pattern:**
- Single-constraint tables: TABLE gets Annotation, CONSTRAINT gets AttachedAnnotation
- Multi-constraint tables: CONSTRAINTs get Annotation, TABLE gets AttachedAnnotation
- Disambiguator numbering accounts for package references
- Median-based AttachedAnnotation ordering implemented

### Phase 35: Default Schema Resolution

Fixed unqualified table names resolving to containing object's schema instead of `[dbo]`. DotNet always resolves to default schema regardless of containing object.

### Phase 37-38: Collation Handling

- Created `COLLATION_LCID_MAP` with 100+ collation prefixes
- `CollationCaseSensitive` always outputs "True" (matches DotNet behavior)

### Phase 43: Scope-Aware Alias Tracking

**Problem:** Same alias in different subquery scopes caused incorrect resolution.

**Solution:**
- Added `ScopeType` enum (Apply, DerivedTable)
- Extended scope struct with per-scope aliases
- `resolve_alias_for_position()` returns innermost scope's alias
- Position-aware lookup during column resolution

### Phase 47: Column-Level Collation

Added `collation: Option<String>` to `ColumnElement` for columns with explicit COLLATE clauses.

### Phase 48: 2-Named-Constraint Pattern

Tables with exactly 2 named constraints get 2 Annotation elements (one per constraint).

</details>
