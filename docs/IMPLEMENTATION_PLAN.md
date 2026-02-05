# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

---

## Status: PARITY COMPLETE | REAL-WORLD COMPATIBILITY IN PROGRESS

**Phases 1-54 complete. Full parity: 47/48 (97.9%).**

| Layer | Passing | Rate |
|-------|---------|------|
| Layer 1 (Inventory) | 48/48 | 100% |
| Layer 2 (Properties) | 48/48 | 100% |
| Layer 3 (SqlPackage) | 48/48 | 100% |
| Relationships | 47/48 | 97.9% |
| Layer 4 (Ordering) | 47/48 | 97.9% |
| Metadata | 48/48 | 100% |
| Layer 7 (Canonical XML) | 24/48 | 50.0% |

**Remaining Work:**
- Layer 7: element ordering differences between Rust and DotNet (24/48 failing)
- `body_dependencies_aliases`: 65 relationship ordering errors (not affecting functionality)

**Excluded Fixtures:** `external_reference`, `unresolved_reference` (DotNet fails to build with SQL71501)

---

## Known Issues

| Issue | Location | Status |
|-------|----------|--------|
| Relationship parity body_dependencies_aliases | body_deps.rs | 65 errors (ordering differences, not affecting functionality) |
| Layer 7 parity remaining | model_xml | 24/48 failing due to element ordering differences |

---

<details>
<summary>Completed Phases (1-54)</summary>

## Phase 54: Layer 7 Inline Constraint Ordering Fix (COMPLETE)

Improved Layer 7 canonical XML matching from 21/48 (43.8%) to 24/48 (50.0%).

**Root Cause:**
DotNet sorts inline constraints (unnamed, no Name attribute) by their DefiningTable reference in **descending** alphabetical order, while Rust was using ascending order.

**Changes:**
1. Modified `sort_elements()` in `builder.rs` to use `Reverse<String>` for secondary sort key of inline constraints
2. This ensures inline constraints sort by DefiningTable in descending order (Z→A) matching DotNet behavior

**Files Modified:**
- `src/model/builder.rs` - Updated sort_elements() to use descending order for inline constraint secondary keys

**Fixtures Now Passing Layer 7:**
- `constraints` (inline PK ordering fix)
- `index_naming` (element ordering fix)
- `reserved_keywords` (element ordering fix)

## Phase 53: Layer 7 XML Parity Improvements (COMPLETE)

Improved Layer 7 canonical XML matching from 19/48 (39.6%) to 21/48 (43.8%).

**Changes:**
1. **NUMERIC type preservation**: `NUMERIC` types now output `[numeric]` instead of normalizing to `[decimal]`
2. **Scale=0 omission**: `<Property Name="Scale" Value="0"/>` is now omitted (matches DotNet behavior)
3. **IsPadded property for PAD_INDEX=ON**: Added parsing and XML generation for `PAD_INDEX` option on indexes

**Files Modified:**
- `src/dacpac/model_xml/table_writer.rs` - Separate NUMERIC/DECIMAL handling, Scale=0 omission
- `src/dacpac/model_xml/programmability_writer.rs` - Scale=0 omission
- `src/dacpac/model_xml/mod.rs` - Scale=0 omission
- `src/dacpac/model_xml/other_writers.rs` - Write IsPadded property
- `src/parser/index_parser.rs` - Parse PAD_INDEX option, add is_padded field
- `src/parser/tsql_parser.rs` - Add is_padded to FallbackStatementType::Index
- `src/model/elements.rs` - Add is_padded field to IndexElement
- `src/model/builder.rs` - Pass is_padded through model building

**Fixtures Now Passing Layer 7:**
- `column_properties` (numeric type fix)
- `index_options` (IsPadded property fix)

## Phase 52: Procedure-Scoped Table Variable References (COMPLETE)

Fixed deployment errors for procedures that declare and use table variables with aliases in FROM/JOIN clauses.

**Changes:**
- Fixed `SqlDynamicColumnSource` element naming: `[schema].[proc].[@var]` instead of `[schema].[proc].[TableVariable1].[@var]`
- Added `full_name` context to `TableAliasTokenParser` for procedure-scoped references
- Updated `extract_table_reference_after_from_join` to detect `@` prefix and create procedure-scoped alias mappings
- Added `table_variable_refs` test fixture covering DECLARE TABLE, FROM @var, JOIN @var patterns

**Files Modified:**
- `src/dacpac/model_xml/programmability_writer.rs` - Element naming fix
- `src/dacpac/model_xml/body_deps.rs` - Added `full_name` to parser, procedure-scoped table variable alias resolution
- `tests/fixtures/table_variable_refs/` - New test fixture

## Phase 51: Layer 7 Canonical Comparison Test Fix (COMPLETE)

Updated `test_canonical_comparison_all_fixtures` to use prebuilt DotNet dacpac cache. Result: 19/48 exact match (39.6%) - all fixtures now tested.

## Phase 50.9: Decouple Column and Table Annotation Logic (COMPLETE)

Fixed deployment error for tables with mixed inline + table-level constraints. Decoupled column and table annotation concerns in `builder.rs`.

## Phase 50: Fix Schema-Aware Resolution Gaps (COMPLETE)

| Sub-Phase | Description | Tasks |
|-----------|-------------|-------|
| 50.1-50.8 | Column resolution, view registry, storage elements, security statements, UDT columns | 34 |

**Key Changes:**
- `ColumnRegistry` returns `None` when 0 tables match (no fallback)
- Views tracked in registry; SELECT * views marked as wildcard
- Storage elements parsed via token-based parsers
- Security statements silently skipped
- FullTextIndex disambiguators assigned before constraints
- UDT columns reference actual type instead of `[sql_variant]`

## Phase 49: Schema-Aware Unqualified Column Resolution (COMPLETE)

Created `ColumnRegistry` to map tables to columns. Resolution: 1 match → resolve, 0 matches → None, >1 matches → skip (ambiguous).

## Phases 1-48 Overview

| Phase | Description | Status |
|-------|-------------|--------|
| 1-9 | Core implementation (properties, relationships, XML structure, metadata) | ✅ 58/58 |
| 10 | Extended properties, function classification, constraint naming | ✅ 5/5 |
| 11 | Remaining parity failures, error fixtures, ignored tests | ✅ 70/70 |
| 12-13 | SELECT * expansion, TVF columns, TVP support | ✅ 10/10 |
| 14 | Layer 3 (SqlPackage) parity | ✅ 3/3 |
| 15, 20 | Parser refactoring: replace regex with token-based parsing | ✅ 77/77 |
| 16 | Performance: 116x faster than DotNet cold, 42x faster warm | ✅ 18/18 |
| 17-19 | Real-world compatibility: comma-less constraints, SQLCMD, TVP parsing | ✅ 11/11 |
| 21 | Split model_xml.rs into submodules | ✅ 10/10 |
| 22-25 | Layer 7 XML parity, IsMax, dynamic column sources, constraint properties | ✅ 27/28 |
| 26, 32, 34, 41-43 | Body dependency resolution (APPLY, CTE, nested subqueries, scope-aware) | ✅ |
| 27-31 | Code consolidation (~1200 lines removed) | ✅ 13/13 |
| 35 | Default schema resolution for unqualified table names | ✅ 9/9 |
| 36 | DacMetadata.xml dynamic properties | ✅ 8/8 |
| 37-38 | Collation handling (LCID map, CollationCaseSensitive) | ✅ |
| 39-40 | SysCommentsObjectAnnotation for views/procedures | ✅ |
| 44-45 | XML formatting (space before />, element ordering) | ✅ |
| 46 | Disambiguator numbering for package references | ✅ |
| 47 | Column-level Collation property | ✅ |
| 48 | 2-named-constraint annotation pattern | ✅ |

## Key Milestones

- **Parity Achievement (Phase 14):** L1-L3 100%, Relationships 97.9%
- **Performance (Phase 16):** 116x/42x faster than DotNet cold/warm
- **Parser Modernization (Phases 15, 20):** All regex replaced with token-based parsing
- **XML Parity (Phases 22-54):** Layer 7 improved from 0% to 50.0%

</details>
