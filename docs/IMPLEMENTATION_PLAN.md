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

Two tests are currently failing:
- `test_layered_dacpac_comparison` - Element inventory, property, and SqlPackage issues
- `test_layer3_sqlpackage_comparison` - SqlPackage configuration issue

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

### 10.3 View IsAnsiNullsOn Property

**Goal:** Emit `IsAnsiNullsOn` property for views.

**Issue:** Views are missing the `IsAnsiNullsOn` property (Rust: `None`, DotNet: `Some("True")`).

- [ ] **10.3.1 Add IsAnsiNullsOn to all views**
  - Files: `src/model/elements.rs`, `src/dacpac/model_xml.rs`
  - Currently `IsAnsiNullsOn` is only emitted for views with options (schema-bound, etc.)
  - DotNet emits `IsAnsiNullsOn="True"` for all views
  - Change to always emit this property for views
  - Expected impact: Fix 3 property mismatches in Layer 2

### 10.4 Extra Default Constraints

**Goal:** Suppress emission of default constraints that DotNet doesn't emit.

**Issue:** Rust generates 5 extra `SqlDefaultConstraint` elements for Settings table that DotNet doesn't produce.

- [ ] **10.4.1 Investigate Settings table default constraint differences**
  - Files: `tests/fixtures/e2e_comprehensive/`, `src/model/builder.rs`
  - Examine the Settings table definition in the e2e_comprehensive fixture
  - Determine why DotNet doesn't emit these constraints (inline vs named, syntax differences)
  - Likely cause: Inline defaults without explicit constraint names may be handled differently
  - Expected impact: Fix 5 EXTRA items in Layer 1

### 10.5 SqlPackage Test Configuration

**Goal:** Fix Layer 3 SqlPackage tests to provide required parameters.

**Issue:** `test_layer3_sqlpackage_comparison` fails with "Operation Script requires a target database name".

- [ ] **10.5.1 Add target database parameter to SqlPackage tests**
  - File: `tests/e2e/dotnet_comparison_tests.rs` or `tests/e2e/parity/layer3_sqlpackage.rs`
  - SqlPackage DeployReport action requires `/TargetDatabaseName:` parameter
  - Add a dummy database name for comparison purposes
  - Expected impact: Fix both failing tests' Layer 3 errors

---

### Phase 10 Progress

| Section | Status | Completion |
|---------|--------|------------|
| 10.1 Extended Property Key Format | PENDING | 0/1 |
| 10.2 Function Type Classification | PENDING | 0/1 |
| 10.3 View IsAnsiNullsOn Property | PENDING | 0/1 |
| 10.4 Extra Default Constraints | PENDING | 0/1 |
| 10.5 SqlPackage Test Configuration | PENDING | 0/1 |

**Phase 10 Overall**: 0/5 tasks

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
| Phase 10 | **IN PROGRESS** 0/5 |

**Total**: 58/63 tasks complete

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
