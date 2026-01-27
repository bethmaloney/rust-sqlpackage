# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Completed Phases Summary

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1-9 | Core implementation (properties, relationships, XML structure, metadata) | 58/58 |
| Phase 10 | Fix extended properties, function classification, constraint naming, SqlPackage config | 5/5 |

**Total Completed**: 63/63 tasks

---

## Current Parity Metrics (as of 2026-01-27)

| Layer | Passing | Rate | Notes |
|-------|---------|------|-------|
| Layer 1 (Inventory) | 43/46 | 93.5% | 3 failing |
| Layer 2 (Properties) | 39/46 | 84.8% | 7 failing |
| Relationships | 31/46 | 67.4% | 15 failing |
| Layer 4 (Ordering) | 7/46 | 15.2% | 39 failing |
| Metadata | 44/46 | 95.7% | 2 ERROR fixtures |
| **Full Parity** | **6/46** | **13.0%** | collation, empty_project, indexes, only_schemas, procedure_parameters, views |

---

## Phase 11: Fix Remaining Parity Failures

### 11.1 Layer 1: Element Inventory Failures

#### 11.1.1 WITH NOCHECK Constraints Not Captured
**Fixtures:** `constraint_nocheck`
**Issue:** Foreign key and check constraints created `WITH NOCHECK` are not being captured.
- Missing: `SqlCheckConstraint` (2), `SqlForeignKeyConstraint` (2)
- Example: `[dbo].[CK_ChildNoCheck_Value]`, `[dbo].[FK_ChildNoCheck_Parent]`

- [x] **11.1.1.1** Parse `WITH NOCHECK` syntax in ALTER TABLE statements
- [x] **11.1.1.2** Emit constraints with `IsNotForReplication` or appropriate property

#### 11.1.2 Scalar Type Misclassification
**Fixtures:** `scalar_types`
**Issue:** `CREATE TYPE ... FROM` scalar types classified as `SqlTableType` instead of `SqlUserDefinedDataType`.
- Rust emits `SqlTableType` for: `[dbo].[Currency]`, `[dbo].[EmailAddress]`, `[dbo].[PhoneNumber]`, `[dbo].[SSN]`
- DotNet emits `SqlUserDefinedDataType` for these

- [x] **11.1.2.1** Distinguish scalar UDT (`CREATE TYPE x FROM basetype`) from table types (`CREATE TYPE x AS TABLE`)
- [x] **11.1.2.2** Emit `SqlUserDefinedDataType` for scalar types with correct properties

#### 11.1.3 Fulltext Index Naming
**Fixtures:** `fulltext_index`
**Issue:** Fulltext index element name includes `.FullTextIndex` suffix but DotNet uses table name only. Also missing PK constraint.
- Rust: `SqlFullTextIndex.[dbo].[Documents].[FullTextIndex]`
- DotNet: `SqlFullTextIndex.[dbo].[Documents]`

- [ ] **11.1.3.1** Fix fulltext index element naming to use table name only
- [ ] **11.1.3.2** Ensure PK constraint `[dbo].[PK_Documents]` is emitted

#### 11.1.4 Schema Authorization in Element Name
**Fixtures:** `only_schemas`
**Issue:** Schema element names include `AUTHORIZATION [owner]` but DotNet doesn't.
- Rust: `SqlSchema.[HR] AUTHORIZATION [dbo]`
- DotNet: `SqlSchema.[HR]`

- [x] **11.1.4.1** Remove AUTHORIZATION clause from schema element names
- [x] **11.1.4.2** Emit Authorizer as a relationship instead (see 11.3.1)

---

### 11.2 Layer 2: Property Failures

#### 11.2.1 Property Mismatches in Comprehensive Fixtures
**Fixtures:** `e2e_comprehensive` (3), `e2e_simple` (1), `element_types` (4), `index_options` (3), `scalar_types` (3)

- [ ] **11.2.1.1** Run detailed property comparison to identify specific mismatches
- [ ] **11.2.1.2** Fix identified property emission issues

---

### 11.3 Relationship Failures

#### 11.3.1 Schema Authorizer Relationship
**Fixtures:** `element_types` and others with custom schemas
**Issue:** `SqlSchema.[Sales] - Authorizer` relationship not emitted.

- [x] **11.3.1.1** Parse schema authorization from `CREATE SCHEMA [name] AUTHORIZATION [owner]`
- [x] **11.3.1.2** Emit `Authorizer` relationship pointing to the owner

#### 11.3.2 Computed Column ExpressionDependencies
**Fixtures:** `computed_columns`
**Issue:** `SqlComputedColumn` missing `ExpressionDependencies` relationship.
- Example: `[dbo].[Employees].[YearsEmployed]`, `[dbo].[Employees].[TotalCompensation]`

- [ ] **11.3.2.1** Parse computed column expressions to extract column references
- [ ] **11.3.2.2** Emit `ExpressionDependencies` relationship with referenced columns

#### 11.3.3 Procedure/Function BodyDependencies Incomplete
**Fixtures:** `procedure_options`, `element_types`, `e2e_comprehensive`, `e2e_simple`
**Issue:** Procedure/function `BodyDependencies` missing table and column references.
- Rust captures parameter references but misses `[dbo].[Users]`, `[dbo].[Users].[Id]`, etc.

- [ ] **11.3.3.1** Parse procedure/function body to extract table references
- [ ] **11.3.3.2** Parse procedure/function body to extract column references
- [ ] **11.3.3.3** Add table/column references to `BodyDependencies` relationship

#### 11.3.4 Trigger BodyDependencies
**Fixtures:** `instead_of_triggers`
**Issue:** `SqlDmlTrigger` missing `BodyDependencies` relationship.
- Missing for: `[dbo].[TR_ProductsView_Delete]`, `[dbo].[TR_ProductsView_Insert]`, `[dbo].[TR_ProductsView_Update]`

- [ ] **11.3.4.1** Parse trigger body to extract table/column references
- [ ] **11.3.4.2** Emit `BodyDependencies` relationship for triggers

#### 11.3.5 View Columns/QueryDependencies for SCHEMABINDING Views
**Fixtures:** `instead_of_triggers`, `view_options`
**Issue:** Views with `SCHEMABINDING` or `WITH CHECK OPTION` should emit `Columns` and `QueryDependencies`.
- Missing for: `[dbo].[ProductsView]`, `[dbo].[ProductSummary]`

- [ ] **11.3.5.1** Detect SCHEMABINDING and WITH CHECK OPTION view options
- [ ] **11.3.5.2** Emit `Columns` relationship for bound views
- [ ] **11.3.5.3** Emit complete `QueryDependencies` with all referenced columns

#### 11.3.6 Table Type Index Relationships
**Fixtures:** `table_types`
**Issue:** `SqlTableTypeIndex` using wrong relationship name.
- Rust emits: `Columns`
- DotNet emits: `ColumnSpecifications`

- [ ] **11.3.6.1** Change `SqlTableTypeIndex` to emit `ColumnSpecifications` instead of `Columns`

#### 11.3.7 Table Type Constraints Relationship
**Fixtures:** `table_types`
**Issue:** `SqlTableType` missing `Constraints` relationship, has extra `PrimaryKey`/`CheckConstraints`.
- DotNet uses generic `Constraints` relationship for all constraint types

- [ ] **11.3.7.1** Emit `Constraints` relationship instead of `PrimaryKey`/`CheckConstraints`

#### 11.3.8 Sequence TypeSpecifier
**Fixtures:** `element_types`
**Issue:** `SqlSequence.[dbo].[OrderSequence]` missing `TypeSpecifier` relationship.

- [ ] **11.3.8.1** Emit `TypeSpecifier` relationship for sequences

#### 11.3.9 Inline TVF Columns and BodyDependencies
**Fixtures:** `element_types`
**Issue:** `SqlInlineTableValuedFunction` missing `Columns` and `BodyDependencies`.
- Missing for: `[dbo].[GetActiveUsers]`

- [ ] **11.3.9.1** Emit `Columns` relationship for inline TVFs
- [ ] **11.3.9.2** Emit `BodyDependencies` relationship for inline TVFs

#### 11.3.10 Multi-statement TVF Columns
**Fixtures:** `element_types`
**Issue:** `SqlMultiStatementTableValuedFunction` missing `Columns` relationship.
- Missing for: `[dbo].[GetUsersByName]`

- [ ] **11.3.10.1** Emit `Columns` relationship for multi-statement TVFs

---

### 11.4 Layer 4: Element Ordering

#### 11.4.1 Fix XML Element Ordering
**Fixtures:** 39 fixtures fail Layer 4
**Issue:** XML elements not in same order as DotNet output.

- [ ] **11.4.1.1** Analyze DotNet element ordering algorithm
- [ ] **11.4.1.2** Implement matching sort order for model elements
- [ ] **11.4.1.3** Verify ordering matches for all fixtures

---

### 11.5 Error Fixtures

#### 11.5.1 External Reference Fixture
**Fixtures:** `external_reference`
**Status:** ERROR - DotNet build likely fails due to missing referenced dacpac

- [ ] **11.5.1.1** Investigate external_reference DotNet build failure
- [ ] **11.5.1.2** Fix or mark as expected failure

#### 11.5.2 Unresolved Reference Fixture
**Fixtures:** `unresolved_reference`
**Status:** ERROR - DotNet build likely fails due to unresolved references

- [ ] **11.5.2.1** Investigate unresolved_reference DotNet build failure
- [ ] **11.5.2.2** Fix or mark as expected failure

---

### 11.6 Final Verification: 100% Parity

#### 11.6.1 Complete Verification Checklist
**Goal:** Verify all tests pass, no clippy warnings, and full parity achieved.

- [ ] **11.6.1.1** Run `just test` - all unit and integration tests pass
- [ ] **11.6.1.2** Run `cargo clippy` - no warnings
- [ ] **11.6.1.3** Run parity regression check - all 46 fixtures at full parity
- [ ] **11.6.1.4** Verify Layer 1 (inventory) at 100%
- [ ] **11.6.1.5** Verify Layer 2 (properties) at 100%
- [ ] **11.6.1.6** Verify Relationships at 100%
- [ ] **11.6.1.7** Verify Layer 4 (ordering) at 100%
- [ ] **11.6.1.8** Verify Metadata at 100%
- [ ] **11.6.1.9** Document any intentional deviations from DotNet behavior
- [ ] **11.6.1.10** Update baseline and confirm no regressions

---

### Phase 11 Progress

| Section | Description | Tasks |
|---------|-------------|-------|
| 11.1 | Layer 1: Element Inventory | 6/8 |
| 11.2 | Layer 2: Properties | 0/2 |
| 11.3 | Relationships | 2/16 |
| 11.4 | Layer 4: Ordering | 0/3 |
| 11.5 | Error Fixtures | 0/4 |
| 11.6 | Final Verification | 0/10 |

**Phase 11 Total**: 8/43 tasks

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

## Overall Progress

| Phase | Status |
|-------|--------|
| Phases 1-10 | **COMPLETE** 63/63 |
| Phase 11 | **IN PROGRESS** 8/43 |

**Total**: 71/106 tasks complete

---

<details>
<summary>Archived: Phases 1-10 Details</summary>

### Phase 1-9 Summary (58 tasks)
- Phase 1: Fix Known High-Priority Issues (ampersand truncation, constraints, etc.)
- Phase 2: Expand Property Comparison (strict mode, all properties)
- Phase 3: Add Relationship Comparison (references, entries)
- Phase 4: Add XML Structure Comparison (element ordering)
- Phase 5: Add Metadata Files Comparison (Content_Types, DacMetadata, Origin, scripts)
- Phase 6: Per-Feature Parity Tests (all 46 fixtures)
- Phase 7: Canonical XML Comparison (byte-level matching)
- Phase 8: Test Infrastructure (modular parity layers, CI metrics, regression detection)
- Phase 9: Achieve 100% Parity (ordering, properties, relationships, metadata, edge cases)

### Phase 10 Summary (5 tasks)
- 10.1 Extended Property Key Format - Add parent type prefix
- 10.2 Function Type Classification - Distinguish inline vs multi-statement TVFs
- 10.3 View IsAnsiNullsOn Property - RESOLVED (was incorrect assumption)
- 10.4 Default Constraint Naming Fix - Fix inline constraint name handling
- 10.5 SqlPackage Test Configuration - Add TargetDatabaseName parameter
- 10.6 View Relationship Parity - RESOLVED (implementation was correct)

</details>
