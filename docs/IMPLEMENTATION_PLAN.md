# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Completed Phases Summary

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1 | Fix Known High-Priority Issues (ampersand truncation, constraints, etc.) | ✓ 9/9 |
| Phase 2 | Expand Property Comparison (strict mode, all properties) | ✓ 4/4 |
| Phase 3 | Add Relationship Comparison (references, entries) | ✓ 4/4 |
| Phase 4 | Add XML Structure Comparison (element ordering) | ✓ 4/4 |
| Phase 5 | Add Metadata Files Comparison (Content_Types, DacMetadata, Origin, scripts) | ✓ 5/5 |
| Phase 6 | Per-Feature Parity Tests (all 46 fixtures) | ✓ 5/5 |
| Phase 7 | Canonical XML Comparison (byte-level matching) | ✓ 4/4 |
| Phase 8 | Test Infrastructure (modular parity layers, CI metrics, regression detection) | ✓ 4/4 |
| Phase 9 | Achieve 100% Parity (ordering, properties, relationships, metadata, edge cases) | ✓ 19/19 |

**Phases 1-9 Complete**: 58/58 tasks

---

## Current Parity Metrics (as of 2026-01-27)

| Layer | Passing | Rate | Notes |
|-------|---------|------|-------|
| Layer 1 (Inventory) | 34/46 | 73.9% | Extended property key format differences |
| Layer 2 (Properties) | 37/46 | 80.4% | View IsAnsiNullsOn missing |
| Layer 3 (Relationships) | 30/46 | 65.2% | |
| Layer 4 (Structure) | 6/46 | 13.0% | |
| Layer 5 (Metadata) | 44/46 | 95.7% | 2 are ERROR (DotNet build failures) |

---

## Phase 10: Fix Remaining Test Failures

Tests with remaining issues:
- `test_layered_dacpac_comparison` - Element inventory issues (extended properties, function types, default constraints)
- `test_layer3_sqlpackage_comparison` - ~~SqlPackage configuration issue~~ (FIXED: added /TargetDatabaseName parameter)

### 10.1 Extended Property Key Format

**Goal:** Fix extended property element naming to include parent type prefix.

**Issue:** Rust emits `[schema].[table].[MS_Description]` but DotNet emits `[SqlTableBase].[schema].[table].[MS_Description]` (includes parent element type like `SqlColumn`, `SqlTableBase`).

- [ ] **10.1.1 Add parent type to extended property keys**
  - File: `src/dacpac/model_xml.rs`
  - Extended properties on tables need `[SqlTableBase]` prefix
  - Extended properties on columns need `[SqlColumn]` prefix
  - Format: `[ParentType].[schema].[object].[property_name]` or `[ParentType].[schema].[table].[column].[property_name]`
  - Expected impact: Fix 7 MISSING and 7 EXTRA items in Layer 1

### 10.2 Function Type Classification

**Goal:** Correctly classify inline table-valued functions vs multi-statement TVFs.

**Issue:** `GetProductsInPriceRange` is classified as `SqlMultiStatementTableValuedFunction` but DotNet identifies it as `SqlInlineTableValuedFunction`.

- [ ] **10.2.1 Distinguish inline vs multi-statement TVFs**
  - Files: `src/parser/tsql_parser.rs`, `src/model/builder.rs`
  - Inline TVF: Single `RETURN SELECT ...` statement, no `BEGIN/END` block
  - Multi-statement TVF: Declares a table variable, has `BEGIN/END` block with `INSERT` statements
  - Parse the function body to detect which pattern is used
  - Expected impact: Fix COUNT MISMATCH for SqlInlineTableValuedFunction/SqlMultiStatementTableValuedFunction

### ~~10.3 View IsAnsiNullsOn Property~~ (INVALID - removed)

**Status:** REMOVED - Original analysis was incorrect.

**Finding:** The current implementation is correct. DotNet only emits `IsAnsiNullsOn` for views with options (SCHEMABINDING, CHECK OPTION, or VIEW_METADATA). Simple views without any options do NOT get this property. The `views` fixture confirmed this - DotNet's dacpac for `[dbo].[ActiveItems]` (a simple view) does not include `IsAnsiNullsOn`, while `view_options` fixture views with SCHEMABINDING do include it.

### 10.4 Extra Default Constraints

**Goal:** Suppress emission of default constraints that DotNet doesn't emit.

**Issue:** Rust generates 5 extra `SqlDefaultConstraint` elements for Settings table that DotNet doesn't produce.

- [ ] **10.4.1 Investigate Settings table default constraint differences**
  - Files: `tests/fixtures/e2e_comprehensive/`, `src/model/builder.rs`
  - Examine the Settings table definition in the e2e_comprehensive fixture
  - Determine why DotNet doesn't emit these constraints (inline vs named, syntax differences)
  - Likely cause: Inline defaults without explicit constraint names may be handled differently
  - Expected impact: Fix 5 EXTRA items in Layer 1

### 10.5 SqlPackage Test Configuration ✓

**Goal:** Fix Layer 3 SqlPackage tests to provide required parameters.

**Issue:** `test_layer3_sqlpackage_comparison` fails with "Operation Script requires a target database name".

- [x] **10.5.1 Add target database parameter to SqlPackage tests**
  - File: `tests/e2e/parity/layer3_sqlpackage.rs`
  - Added `/TargetDatabaseName:ParityTestDb` to SqlPackage command
  - Layer 3 now properly runs and reports schema differences (instead of configuration error)

---

### Phase 10 Progress

| Section | Status | Completion |
|---------|--------|------------|
| 10.1 Extended Property Key Format | PENDING | 0/1 |
| 10.2 Function Type Classification | PENDING | 0/1 |
| 10.3 View IsAnsiNullsOn Property | ~~REMOVED~~ | N/A |
| 10.4 Extra Default Constraints | PENDING | 0/1 |
| 10.5 SqlPackage Test Configuration | COMPLETE | 1/1 |

**Phase 10 Overall**: 1/4 tasks (task 10.3 removed - was invalid)

---

## Verification Commands

```bash
just test                                    # Run all tests
cargo test --test e2e_tests test_parity_regression_check  # Check regressions
PARITY_UPDATE_BASELINE=1 cargo test --test e2e_tests test_parity_regression_check -- --nocapture  # Update baseline
cargo test --test e2e_tests test_parity_metrics_collection -- --nocapture  # Check metrics
cargo test --test e2e_tests test_layered_dacpac_comparison -- --nocapture  # Run failing test with output
```

---

## Overall Progress

| Phase | Status |
|-------|--------|
| Phases 1-9 | **COMPLETE** ✓ 58/58 |
| Phase 10 | **IN PROGRESS** 1/4 |

**Total**: 59/62 tasks complete (task 10.3 removed - was invalid)

---

## Archived: Phase 9 Details

<details>
<summary>Click to expand Phase 9 completed tasks</summary>

### 9.1 Deterministic Element Ordering (2/2) ✓

- [x] **9.1.1 Replace HashSet with BTreeSet for schemas**
- [x] **9.1.2 Sort elements by type then name**

### 9.2 Property Value Fixes (6/6) ✓

- [x] **9.2.0 IsNullable property emission**
- [x] **9.2.1 Type specifier properties**
- [x] **9.2.2 Script content normalization**
- [x] **9.2.3 Boolean property consistency**
- [x] **9.2.4 Constraint expression properties**
- [x] **9.2.5 IsNullable emission fix**

### 9.3 Relationship Completeness (4/4) ✓

- [x] **9.3.1 Procedure/function dependencies**
- [x] **9.3.2 Parameter relationships**
- [x] **9.3.3 Foreign key relationship ordering**
- [x] **9.3.4 CheckExpressionDependencies relationship**

### 9.4 Metadata File Alignment (4/4) ✓

- [x] **9.4.1 Origin.xml adjustments**
- [x] **9.4.2 Update comparison tolerance**
- [x] **9.4.3 DacMetadata.xml alignment**
- [x] **9.4.4 [Content_Types].xml fixes**

### 9.5 Edge Cases and Polishing (3/3) ✓

- [x] **9.5.1 View columns**
- [x] **9.5.2 Inline constraint annotation disambiguator**
- [x] **9.5.3 Trigger support verification**

</details>
