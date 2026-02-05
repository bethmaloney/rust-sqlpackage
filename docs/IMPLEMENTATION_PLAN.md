# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

---

## Status: PARITY COMPLETE | REAL-WORLD COMPATIBILITY IN PROGRESS

**Phases 1-51 complete. Full parity: 47/48 (97.9%). Phase 52 pending (table variable scoping).**

| Layer | Passing | Rate |
|-------|---------|------|
| Layer 1 (Inventory) | 48/48 | 100% |
| Layer 2 (Properties) | 48/48 | 100% |
| Layer 3 (SqlPackage) | 48/48 | 100% |
| Relationships | 47/48 | 97.9% |
| Layer 4 (Ordering) | 47/48 | 97.9% |
| Metadata | 48/48 | 100% |
| Layer 7 (Canonical XML) | 19/48 | 39.6% |

**Remaining Work:**
- Phase 52: Table variable references use incorrect scope (blocks deployment of procedures with table variables)
- Layer 7: element ordering differences between Rust and DotNet (29/48 failing)
- `body_dependencies_aliases`: 65 relationship ordering errors (not affecting functionality)

**Excluded Fixtures:** `external_reference`, `unresolved_reference` (DotNet fails to build with SQL71501)

---

## Phase 52: Procedure-Scoped Table Variable References (6 tasks) - PENDING

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 52.1 | Add test fixture with table variable usage patterns | ⬜ | Cover DECLARE TABLE, FROM @var, JOIN @var with aliases |
| 52.2 | Fix element naming: remove `[TableVariable1]` intermediate from SqlDynamicColumnSource | ⬜ | Element should be `[schema].[ProcName].[@VarName]` |
| 52.3 | Thread `full_name` through table alias extraction in body_deps.rs | ⬜ | Required for procedure-scoped references |
| 52.4 | Update `extract_table_reference_after_from_join` to handle `@` prefixed table variables | ⬜ | Create procedure-scoped alias mapping |
| 52.5 | Add unit tests for table variable alias resolution | ⬜ | Test column references resolve to `[schema].[proc].[@var].[col]` |
| 52.6 | Verify deployment succeeds with table variable procedures | ⬜ | End-to-end validation |

**Problem Statement:**

Deployment fails with:
```
The reference to the element that has the name [dbo].[@OrderItems] could not be resolved
because no element with that name exists.
```

Affects stored procedures that declare and use table variables with aliases in FROM/JOIN clauses.

**Root Cause:**

Two related issues in `src/dacpac/model_xml/body_deps.rs`:

1. **Element naming**: `SqlDynamicColumnSource` elements include an extra `[TableVariable1]` component
2. **Reference path**: Table variable aliases resolve to `[dbo].[@VarName]` instead of `[dbo].[ProcName].[@VarName]`

**Example:** Procedure with table variable

```sql
CREATE PROCEDURE [dbo].[GetOrdersByStatus]
    @Status INT
AS
BEGIN
    DECLARE @FilteredOrders TABLE (
        [OrderId] UNIQUEIDENTIFIER NOT NULL,
        [CustomerId] UNIQUEIDENTIFIER NOT NULL
    )

    INSERT INTO @FilteredOrders
    SELECT [OrderId], [CustomerId] FROM [dbo].[Orders] WHERE [Status] = @Status

    SELECT
        [o].[OrderId],
        [c].[CustomerName]
    FROM
        @FilteredOrders [o]
        INNER JOIN [dbo].[Customers] [c] ON [o].[CustomerId] = [c].[Id]
END
```

| Aspect | Current (Wrong) | Expected (DacFx) |
|--------|-----------------|------------------|
| Element Name | `[dbo].[GetOrdersByStatus].[TableVariable1].[@FilteredOrders]` | `[dbo].[GetOrdersByStatus].[@FilteredOrders]` |
| Reference | `[dbo].[@FilteredOrders]` | `[dbo].[GetOrdersByStatus].[@FilteredOrders]` |
| Column Ref | `[dbo].[@FilteredOrders].[OrderId]` | `[dbo].[GetOrdersByStatus].[@FilteredOrders].[OrderId]` |

**Technical Details:**

In `body_deps.rs`, `extract_table_reference_after_from_join()` (lines 2112-2176):
- `parse_table_name()` returns `("dbo", "@FilteredOrders")` for `FROM @FilteredOrders`
- Creates `table_ref = "[dbo].[@FilteredOrders]"` without procedure scope
- Stores incorrect reference in alias map

**Solution:**

1. Pass `full_name` (e.g., `[dbo].[GetOrdersByStatus]`) to `TableAliasTokenParser`
2. In `extract_table_reference_after_from_join`, detect `@` prefix on table name
3. For table variables, create alias mapping: `alias → [schema].[procName].[@varName]`
4. Fix `SqlDynamicColumnSource` element naming in `programmability_writer.rs` to remove `[TableVariable1]`

**Files to Modify:**
- `src/dacpac/model_xml/body_deps.rs` (alias extraction, reference generation)
- `src/dacpac/model_xml/programmability_writer.rs` (element naming)
- `tests/fixtures/` (new test fixture)

---

## Known Issues

| Issue | Location | Status |
|-------|----------|--------|
| Table variable references not procedure-scoped | body_deps.rs, programmability_writer.rs | Causes deployment failure (Phase 52) |
| Relationship parity body_dependencies_aliases | body_deps.rs | 65 errors (ordering differences, not affecting functionality) |
| Layer 7 parity remaining | model_xml | 29/48 failing due to element ordering differences |

---

<details>
<summary>Completed Phases (1-51)</summary>

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
- **XML Parity (Phases 22-48):** Layer 7 improved from 0% to 39.6%

</details>
