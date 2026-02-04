# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

---

## Status: PARITY COMPLETE | REAL-WORLD COMPATIBILITY COMPLETE

**Phases 1-50.9 complete. Full parity: 47/48 (97.9%).**

| Layer | Passing | Rate |
|-------|---------|------|
| Layer 1 (Inventory) | 48/48 | 100% |
| Layer 2 (Properties) | 48/48 | 100% |
| Layer 3 (SqlPackage) | 48/48 | 100% |
| Relationships | 47/48 | 97.9% |
| Layer 4 (Ordering) | 47/48 | 97.9% |
| Metadata | 48/48 | 100% |
| Layer 7 (Canonical XML) | 20/48 | 41.7% |

**Remaining Work:**
- Layer 7: element ordering, formatting differences (28/48 failing)
- `body_dependencies_aliases`: 65 relationship ordering errors (not affecting functionality)

**Excluded Fixtures:** `external_reference`, `unresolved_reference` (DotNet fails to build with SQL71501)

---

## Phase 50.9: Decouple Column and Table Annotation Logic (4 tasks) - COMPLETE

| ID | Task | Status | Notes |
|----|------|--------|-------|
| 50.9.1 | Refactor Phase 4 to separate column and table annotation concerns | ✅ | Remove `if is_inline { } else { }` structure |
| 50.9.2 | Ensure inline constraints with `uses_annotation=false` add to `table_annotation` | ✅ | Fixes missing annotation errors |
| 50.9.3 | Add integration test for 2-constraint table with inline + table-level constraints | ✅ | Cover mixed constraint pattern |
| 50.9.4 | Verify real-world database deploys successfully | ✅ | Real-world validation |

**Problem Statement:**

Deployment fails with:
```
The AttachedAnnotation node is referencing an annotation that has the disambiguator N,
but no annotation exists with that disambiguator.
```

Affects tables with exactly 2 named constraints where one is inline (e.g., DEFAULT) and one is table-level (e.g., PRIMARY KEY).

**Root Cause:**

In `src/model/builder.rs` lines 1381-1416, the `if constraint.is_inline { } else { }` structure treats column and table annotations as mutually exclusive. Inline constraints never add to `table_annotation`, even when they have `uses_annotation=false`.

**Example:** Table with PK + inline DEFAULT constraint

```sql
CREATE TABLE [dbo].[Products] (
    [Id] UNIQUEIDENTIFIER NOT NULL,
    [Version] INT CONSTRAINT [DF_Products_Version] DEFAULT ((0)) NOT NULL,
    CONSTRAINT [PK_Products] PRIMARY KEY CLUSTERED ([Id])
);
```

| Constraint | is_inline | Disambiguator | Adds to table_annotation? |
|------------|-----------|---------------|---------------------------|
| `PK_Products` | false | 12 | ✅ Yes |
| `DF_Products_Version` | true | 13 | ❌ No (bug) |

Result: Table only has `<Annotation Disambiguator="12">`, missing annotation 13.

**Solution:** Decouple the two independent concerns:

```rust
// Concern 1: Column AttachedAnnotations (only for inline constraints)
if constraint.is_inline {
    if !is_single_named_inline {
        for col in &constraint.columns {
            column_annotations.entry(key).or_default().push(d);
        }
    }
}

// Concern 2: Table annotations (for ANY constraint using AttachedAnnotation)
// INDEPENDENT of is_inline - do NOT put in else branch
if let Some(d) = disambiguator {
    if constraint.uses_annotation {
        table_attached.entry(table_key).or_default().push((d, idx));
    } else {
        table_annotation.entry(table_key).or_default().push((d, idx));
    }
}
```

**Invariant:** Every constraint with `uses_annotation = false` must have its Annotation defined somewhere (table or column). The refactored code makes this invariant explicit.

**Files to Modify:** `src/model/builder.rs` (lines ~1381-1416)

---

## Known Issues

| Issue | Location | Status |
|-------|----------|--------|
| Relationship parity body_dependencies_aliases | body_deps.rs | 65 errors (ordering differences, not affecting functionality) |
| Layer 7 parity remaining | model_xml | 28/48 failing due to element ordering, formatting differences |

---

<details>
<summary>Completed Phases (1-50.8)</summary>

## Phase 50: Fix Schema-Aware Resolution Gaps (COMPLETE 2026-02-04)

| Sub-Phase | Description | Tasks |
|-----------|-------------|-------|
| 50.1 | Remove fallback behavior in column resolution | 3 |
| 50.2 | Add view columns to registry | 4 |
| 50.3 | Complete deferred testing (WideWorldImporters) | 4 |
| 50.4 | Add storage element support (Filegroup, PartitionFunction, PartitionScheme) | 7 |
| 50.5 | Security statement support (GRANT/DENY/REVOKE skipped) | 7 |
| 50.6 | Source-order disambiguator assignment for 2-constraint tables | 3 |
| 50.7 | Fix FullTextIndex and single named inline constraint annotation | 3 |
| 50.8 | Fix User-Defined Type (UDT) column resolution | 3 |

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
- **XML Parity (Phases 22-48):** Layer 7 improved from 0% to 41.7%

</details>
