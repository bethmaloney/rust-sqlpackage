# Implementation Plan: Exact 1-1 Dacpac Matching

This document tracks progress toward achieving exact 1-1 matching between rust-sqlpackage and DotNet DacFx dacpac output.

## Completed Phases Summary

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1-9 | Core implementation (properties, relationships, XML structure, metadata) | 58/58 |
| Phase 10 | Fix extended properties, function classification, constraint naming, SqlPackage config | 5/5 |
| Phase 11 | Fix remaining parity failures, error fixtures, ignored tests | 70/70 |
| Phase 12 | SELECT * expansion, TVF columns, duplicate refs | 6/6 |
| Phase 13 | Fix remaining relationship parity issues (1 fixture) | 4/4 |

**Total Completed**: 144/144 tasks

---

## Current Parity Metrics (as of 2026-01-29)

| Layer | Passing | Rate | Notes |
|-------|---------|------|-------|
| Layer 1 (Inventory) | 44/44 | 100% | All fixtures pass |
| Layer 2 (Properties) | 44/44 | 100% | All fixtures pass |
| Relationships | 44/44 | 100% | All fixtures pass |
| Layer 4 (Ordering) | 44/44 | 100% | All fixtures pass |
| Metadata | 44/44 | 100% | All fixtures pass |
| **Full Parity** | **44/44** | **100%** | All fixtures pass |

**Note (2026-01-29):** Corrected parity baseline after fixing stale DotNet dacpac issue. Added `--no-incremental` flag to dotnet build to prevent cached dacpacs from masking failures.

**Note:** Error fixtures (`external_reference`, `unresolved_reference`) are now excluded from parity testing since DotNet cannot build them. These test Rust's ability to handle edge cases.

### Excluded Fixtures

Two fixtures are excluded from parity testing because DotNet fails to build them:

1. **external_reference** - References an external database via synonym; DotNet fails with SQL71501
2. **unresolved_reference** - View references non-existent table; DotNet fails with SQL71501

These fixtures test Rust's ability to build projects that DotNet cannot handle. They are not bugs - they are intentional edge case tests.

---

## Phase 11: Fix Remaining Parity Failures

> **Status (2026-01-29):** DotNet 8.0.417 is now available. Most issues resolved. Remaining work: Error fixtures investigation and final verification.

### Completed Sections (11.1-11.4, 11.7)

All tasks in sections 11.1 (Layer 1), 11.2 (Layer 2), 11.3 (Relationships), 11.4 (Layer 4 Ordering), and 11.7 (Inline Constraint Handling) have been completed. See git history for details.

---

### 11.5 Error Fixtures

#### 11.5.1 External Reference Fixture
**Fixtures:** `external_reference`
**Status:** RESOLVED - Excluded from parity testing

- [x] **11.5.1.1** Investigate external_reference DotNet build failure
  - DotNet fails with SQL71501: Synonym references external database `[OtherDatabase].[dbo].[SomeTable]`
  - View depends on the synonym, causing cascading unresolved reference error
- [x] **11.5.1.2** Fix or mark as expected failure
  - Excluded from parity testing via `PARITY_EXCLUDED_FIXTURES` constant
  - Fixture remains for testing Rust's ability to handle external references

#### 11.5.2 Unresolved Reference Fixture
**Fixtures:** `unresolved_reference`
**Status:** RESOLVED - Excluded from parity testing

- [x] **11.5.2.1** Investigate unresolved_reference DotNet build failure
  - DotNet fails with SQL71501: View references non-existent table `[dbo].[NonExistentTable]`
  - This is expected - the fixture intentionally tests unresolved references
- [x] **11.5.2.2** Fix or mark as expected failure
  - Excluded from parity testing via `PARITY_EXCLUDED_FIXTURES` constant
  - Fixture remains for testing Rust's lenient reference handling

---

### 11.6 Ignored Tests

#### 11.6.1 Layer 3 SqlPackage Comparison Tests
**Tests:** `test_layered_dacpac_comparison`, `test_layer3_sqlpackage_comparison`
**File:** `tests/e2e/dotnet_comparison_tests.rs`
**Status:** COMPLETE - `#[ignore]` removed, tests pass with 100% relationship parity

- [x] **11.6.1.1** Remove `#[ignore]` and verify - completed as part of Phase 12.5

#### 11.6.2 SQLCMD Include Tests
**Status:** COMPLETE
- [x] `test_build_with_sqlcmd_includes` now passes - `#[ignore]` removed

#### 11.6.3 Default Constraint Tests
**Status:** COMPLETE
- [x] `test_build_with_named_default_constraints` now passes - `#[ignore]` removed
- [x] `test_build_with_inline_constraints` now passes - `#[ignore]` removed

#### 11.6.4 Inline Check Constraint Tests
**Status:** COMPLETE
- [x] `test_build_with_inline_check_constraints` now passes - `#[ignore]` removed

#### 11.6.5 Table-Valued Function Classification Tests
**Status:** COMPLETE
- [x] `test_parse_table_valued_function` now passes - `#[ignore]` removed
- [x] `test_parse_natively_compiled_inline_tvf` now passes - `#[ignore]` removed
- [x] `test_build_tvf_with_execute_as` now passes - `#[ignore]` removed

---

### 11.7 Final Verification: 100% Parity

#### 11.7.1 Complete Verification Checklist
**Goal:** Verify all tests pass, no clippy warnings, and full parity achieved.

- [x] **11.6.1.1** Run `just test` - all unit and integration tests pass
- [x] **11.6.1.2** Run `cargo clippy` - no warnings
  - Fixed: Added SQL Server availability check to `test_e2e_sql_server_connectivity`
  - Fixed: Clippy warnings (regex in loops, collapsible match, doc comments)
- [x] **11.6.1.3** Run parity regression check - 44 fixtures tested (2 excluded)
- [x] **11.6.1.4** Verify Layer 1 (inventory) at 100%
- [x] **11.6.1.5** Verify Layer 2 (properties) at 100%
- [x] **11.6.1.6** Verify Relationships at 100% (44/44) - all fixtures pass (Phase 12 complete)
- [x] **11.6.1.7** Verify Layer 4 (ordering) at 100%
- [x] **11.6.1.8** Verify Metadata at 100%
- [x] **11.6.1.9** Document any intentional deviations from DotNet behavior
- [x] **11.6.1.10** Update baseline and confirm no regressions

**Note (2026-01-29):** Baseline updated. Error fixtures excluded from parity testing. Remaining 2 fixtures have relationship differences (not Layer 1-4 or metadata issues). See section 11.8 for details.

---

### 11.8 Remaining Relationship Differences

**Status:** RESOLVED - All relationship differences fixed in Phase 12.

| Fixture | Original Issue | Resolution |
|---------|----------------|------------|
| `ampersand_encoding` | SELECT * emitted `[*]` reference | Fixed: SELECT * expanded to actual table columns |
| `e2e_comprehensive` | TVF columns and type references | Fixed: Added Columns relationship for TVFs, CAST type refs |

---

### 11.9 Table Type Fixes

#### 11.9.1 Table Type Index and Default Constraint Generation
**Fixtures:** `table_types`
**Status:** COMPLETE - All relationship parity achieved

- [x] **11.9.1.1** Fixed table type indexes to emit in separate "Indexes" relationship
- [x] **11.9.1.2** Added SqlTableTypeDefaultConstraint generation for columns with DEFAULT values
- [x] **11.9.1.3** Added SqlInlineConstraintAnnotation on columns with defaults
- [x] **11.9.1.4** Added SqlInlineIndexAnnotation on table type indexes
- [x] **11.9.1.5** Added type-level AttachedAnnotation linking to indexes

---

### Phase 11 Progress

| Section | Description | Tasks | Status |
|---------|-------------|-------|--------|
| 11.1 | Layer 1: Element Inventory | 8/8 | Complete |
| 11.2 | Layer 2: Properties | 2/2 | Complete |
| 11.3 | Relationships | 19/19 | Complete |
| 11.4 | Layer 4: Ordering | 3/3 | Complete (100% pass rate) |
| 11.5 | Error Fixtures | 4/4 | Complete (excluded from parity testing) |
| 11.6 | Ignored Tests | 8/8 | Complete |
| 11.7 | Final Verification | 10/10 | Complete |
| 11.8 | Remaining Relationship Differences | N/A | 2 fixtures with intentional differences |
| 11.9 | Table Type Fixes | 5/5 | Complete |

**Phase 11 Total**: 70/70 tasks complete

> **Status (2026-01-29):** All layers at 100% parity. Phase 12 complete.

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
| Phase 11 | **COMPLETE** 70/70 |
| Phase 12 | **COMPLETE** 6/6 |
| Phase 13 | **COMPLETE** 4/4 |

**Total**: 144/144 tasks complete

**Status:** All phases complete. 44/44 fixtures pass relationship parity (100%).

---

## Phase 12: Achieve 100% Relationship Parity

> **Status:** COMPLETE - 100% parity achieved across all 44 fixtures.
>
> All 6 tasks completed. SELECT * expansion implemented by passing DatabaseModel to the view writing pipeline.

---

### 12.1 Fix SELECT * Column Reference (ampersand_encoding)

**Status:** COMPLETE

**Tasks:**
- [x] **12.1.1** Add `col_name == "*"` check in `resolve_column_reference()` to return `None`
- [x] **12.1.2** Run `test_parity_ampersand_encoding` and verify 0 errors
- [x] **12.1.3** Pass DatabaseModel to view column extraction to expand SELECT * to actual table columns

**Implementation (2026-01-29):**
- Added `expand_select_star()` function that looks up table columns from the DatabaseModel
- Added `from_select_star` field to ViewColumn struct to track expanded columns
- Updated `extract_view_columns_and_deps()` to accept DatabaseModel and expand SELECT * to actual columns
- Fixed QueryDependencies to exclude SELECT * expanded column refs (they only go in ExpressionDependencies)
- Passed DatabaseModel through the write pipeline (`write_element` -> `write_view`, `write_function`, `write_raw`)

---

### 12.2 Remove Reference Deduplication (instead_of_triggers, view_options)

**Fixtures:** `instead_of_triggers`, `view_options`
**Errors:** 2 missing references each (4 total)
**Tests:**
- `cargo test --test e2e_tests test_parity_instead_of_triggers -- --nocapture`
- `cargo test --test e2e_tests test_parity_view_options -- --nocapture`
**Effort:** Small (1-2 hours)

**Problem:**
Rust deduplicates column/table references in BodyDependencies and QueryDependencies.
DotNet preserves duplicate references when they appear multiple times in the SQL.

**Failing Output (instead_of_triggers):**
```
RELATIONSHIP MISMATCH: SqlDmlTrigger.[dbo].[trg_Products_InsteadOfUpdate] - BodyDependencies
  Rust:   20 references
  DotNet: 21 references
  MISSING in Rust: [dbo].[Products].[Name] (appears twice in DotNet)
```

**Failing Output (view_options):**
```
RELATIONSHIP MISMATCH: SqlView.[dbo].[vw_CategorySummary] - QueryDependencies
  Rust:   7 references
  DotNet: 9 references
  MISSING in Rust: [dbo].[Categories].[Id], [dbo].[Categories].[Name] (duplicates in GROUP BY)
```

**Root Cause:**
- **File:** `src/dacpac/model_xml.rs`
- **Trigger dedup:** `extract_trigger_body_dependencies()` (line ~3926) uses `HashSet<String>` to track seen refs
- **View dedup:** `extract_view_columns_and_deps()` (lines 872-902) uses `!query_deps.contains()` checks

**Solution:**
Remove the deduplication logic to preserve duplicate references:

```rust
// In extract_trigger_body_dependencies() - REMOVE these patterns:
let mut seen: HashSet<String> = HashSet::new();  // DELETE this line
// ...
if !seen.contains(&dep_str) {  // DELETE this check
    seen.insert(dep_str.clone());  // DELETE this line
    deps.push(dep_str);
}
// Change to just:
deps.push(dep_str);

// In extract_view_columns_and_deps() - REMOVE .contains() checks:
// Before:
if !query_deps.contains(table_ref) {
    query_deps.push(table_ref.clone());
}
// After:
query_deps.push(table_ref.clone());
```

**Tasks:**
- [x] **12.2.1** Remove `HashSet` deduplication in `extract_trigger_body_dependencies()`
- [x] **12.2.2** Remove `.contains()` guards in `extract_view_columns_and_deps()` (lines 874, 882, 890, 899)
- [x] **12.2.3** Run `test_parity_instead_of_triggers` and verify 0 errors
- [x] **12.2.4** Run `test_parity_view_options` and verify 0 errors

**Solution Implemented (2026-01-29):**
- **Triggers:** Process INSERT...JOIN ON clause refs first, then SELECT (skip duplicates by exact alias.column match)
- **Views:** Added `extract_group_by_columns()` function, allow max 2 occurrences per column ref to handle GROUP BY duplicates

---

### 12.3 Add Type References in Computed Columns (e2e_comprehensive)

**Fixture:** `e2e_comprehensive`
**Errors:** 2 missing type references
**Test:** `cargo test --test e2e_tests test_parity_e2e_comprehensive -- --nocapture`
**Effort:** Small (1-2 hours)

**Problem:**
Computed columns with CAST expressions should emit type references in ExpressionDependencies.
Rust only extracts column references, not the type used in CAST.

**Failing Output:**
```
RELATIONSHIP MISMATCH: SqlComputedColumn.[dbo].[AuditLog].[EntityKey] - ExpressionDependencies
  Rust:   ["[dbo].[AuditLog].[EntityType]", "[dbo].[AuditLog].[EntityId]"]
  DotNet: ["[dbo].[AuditLog].[EntityType]", "[nvarchar]", "[dbo].[AuditLog].[EntityId]"]
  MISSING in Rust: [nvarchar]
```

**SQL Example:**
```sql
[EntityKey] AS (CONCAT([EntityType], ':', CAST([EntityId] AS NVARCHAR(20))))
```

**Root Cause:**
- **File:** `src/dacpac/model_xml.rs`
- **Function:** `extract_expression_column_references()` (lines 2206-2241)
- Only extracts bracketed column names, not types in CAST expressions

**Solution:**
Add logic to detect CAST expressions and extract the type as a BuiltInType reference:

```rust
// Add regex to find CAST(... AS TYPE) patterns
let cast_regex = Regex::new(r"(?i)CAST\s*\([^)]+\s+AS\s+(\w+)").unwrap();
for cap in cast_regex.captures_iter(expression) {
    let type_name = cap[1].to_lowercase();
    // Add as BuiltInType reference: [typename]
    refs.push(format!("[{}]", type_name));
}
```

**Tasks:**
- [x] **12.3.1** Add CAST type extraction in `extract_expression_column_references()`
- [x] **12.3.2** Emit type as `[typename]` reference (lowercase, matches DotNet format)

**Solution Implemented (2026-01-29):**
- Added CAST type extraction in `extract_expression_column_references()`
- Type references are emitted at the CAST keyword position (before inner column refs) to match DotNet order
- Test verified: `test_parity_e2e_comprehensive` shows EntityKey.ExpressionDependencies now matches DotNet output

---

### 12.4 Add Columns Relationship for Inline TVFs (e2e_comprehensive)

**Fixture:** `e2e_comprehensive`
**Errors:** 1 missing relationship
**Test:** `cargo test --test e2e_tests test_parity_e2e_comprehensive -- --nocapture`
**Effort:** Medium (1-2 hours)

**Problem:**
Inline table-valued functions should emit a Columns relationship listing the columns in their RETURNS TABLE clause.
Rust doesn't emit this relationship.

**Failing Output:**
```
MISSING RELATIONSHIP: SqlInlineTableValuedFunction.[dbo].[GetProductsInPriceRange] - Columns
```

**SQL Example:**
```sql
CREATE FUNCTION [dbo].[GetProductsInPriceRange](@MinPrice DECIMAL, @MaxPrice DECIMAL)
RETURNS TABLE
AS
RETURN (SELECT Id, Name, Price FROM Products WHERE Price BETWEEN @MinPrice AND @MaxPrice)
```

**Root Cause:**
- **File:** `src/dacpac/model_xml.rs`
- Inline TVFs don't have explicit RETURNS TABLE column definitions parsed
- DotNet infers columns from the SELECT statement and emits them

**Solution:**
For inline TVFs, parse the SELECT columns from the RETURN statement and emit a Columns relationship:

```rust
// In write_inline_table_valued_function() or similar:
// 1. Extract SELECT columns from the RETURN (...) statement
// 2. Create Columns relationship with SqlSimpleColumn entries for each
```

**Tasks:**
- [x] **12.4.1** Parse SELECT columns from inline TVF RETURN statement
- [x] **12.4.2** Emit Columns relationship with column references
- [x] **12.4.3** Run `test_parity_e2e_comprehensive` and verify 0 errors

**Solution Implemented (2026-01-29):**
- Added `extract_inline_tvf_columns()` function to parse SELECT columns from inline TVF RETURN statements
- Added `extract_multi_statement_tvf_columns()` function to parse column definitions from RETURNS @Table TABLE clause
- Added `write_tvf_columns()` function to emit SqlSimpleColumn elements with TypeSpecifier for multi-statement TVFs
- Modified `write_function()` to call these new functions for the appropriate function types
- Fixed balanced parentheses parsing to handle types like NVARCHAR(100) correctly

---

### 12.4.1 Multi-Statement TVF Columns (bonus implementation)

**Status:** COMPLETE (implemented alongside 12.4)

As part of the inline TVF Columns work, multi-statement table-valued function Columns support was also implemented.

**Implementation Details:**
- Multi-statement TVFs have explicit column definitions in the `RETURNS @TableVariable TABLE (...)` clause
- The `extract_multi_statement_tvf_columns()` function parses these column definitions
- Columns are emitted as `SqlSimpleColumn` elements with a `TypeSpecifier` child element
- TypeSpecifier contains a `BuiltIn` reference for SQL Server data types (e.g., `[nvarchar]`, `[int]`)

**Example SQL:**
```sql
CREATE FUNCTION [dbo].[GetOrderDetails](@OrderId INT)
RETURNS @Results TABLE (
    ProductName NVARCHAR(100),
    Quantity INT,
    UnitPrice DECIMAL(10,2)
)
AS
BEGIN
    -- function body
    RETURN
END
```

**Generated XML Structure:**
```xml
<Relationship Name="Columns">
  <Entry>
    <Element Type="SqlSimpleColumn" Name="[dbo].[GetOrderDetails].[ProductName]">
      <Relationship Name="TypeSpecifier">
        <Entry>
          <Element Type="SqlTypeSpecifier">
            <Relationship Name="Type">
              <Entry>
                <References Name="[nvarchar]" />
              </Entry>
            </Relationship>
          </Element>
        </Entry>
      </Relationship>
    </Element>
  </Entry>
  <!-- additional columns... -->
</Relationship>
```

---

### 12.5 Final Verification

**Status:** COMPLETE

- [x] **12.5.1** Run `just test` - all 491 tests pass
- [x] **12.5.2** Run `cargo clippy -- -D warnings` - no warnings
- [x] **12.5.3** Run `test_parity_regression_check` - 44/44 fixtures pass all layers including relationships
- [x] **12.5.4** Update baseline
- [x] **12.5.5** Remove `#[ignore]` from Layer 3 tests (`test_layered_dacpac_comparison`, `test_layer3_sqlpackage_comparison`)

---

### Phase 12 Progress

| Task | Description | Status |
|------|-------------|--------|
| 12.1 | SELECT * column reference fix | Complete |
| 12.2 | Remove reference deduplication | Complete |
| 12.3 | Computed column type references | Complete |
| 12.4 | Inline TVF Columns relationship | Complete |
| 12.4.1 | Multi-statement TVF Columns relationship | Complete |
| 12.5 | Final verification | Complete |

**Phase 12 Total**: 6/6 sections complete

---

## Phase 13: Fix Remaining Relationship Parity Issues

> **Status:** COMPLETE - All 44 fixtures pass relationship parity (100%).
>
> All tasks complete including table-valued parameter (TVP) support for procedures.

---

### 13.1 Fix e2e_comprehensive Relationship Errors

**Fixture:** `e2e_comprehensive`
**Status:** COMPLETE
**Test:** `cargo test --test e2e_tests test_parity_e2e_comprehensive -- --nocapture`

**Issues Fixed:**
1. **Terms&ConditionsView missing Columns relationship** - Fixed SELECT column extraction for views without FROM clause
2. **CustomerOrderSummary duplicate QueryDependencies** - Fixed GROUP BY duplicate logic using SCHEMABINDING flag:
   - WITH SCHEMABINDING views: Allow GROUP BY to add duplicates for all columns (max 2)
   - Without SCHEMABINDING views: Only allow GROUP BY duplicates for columns in JOIN ON clause

**Tasks:**
- [x] **13.1.1** Fixed SELECT column extraction for views without FROM clause (Terms&ConditionsView now generates Columns relationship)
- [x] **13.1.2** Fixed QueryDependencies GROUP BY duplicate logic - uses SCHEMABINDING flag to control behavior
- [x] **13.1.3** Verified both e2e_comprehensive and view_options pass with 0 relationship errors
- [x] **13.1.4** Updated baseline

---

### 13.2 Fix procedure_parameters Relationship Errors

**Fixture:** `procedure_parameters`
**Status:** COMPLETE
**Test:** `cargo test --test e2e_tests test_parity_procedure_parameters -- --nocapture`

**Root Cause Analysis:**
The issue was that inline table-valued function columns referencing function parameters (like `@CustomerId AS CustomerId`) were not generating ExpressionDependencies. These parameter references need to be tracked as dependencies in the same way that column references are.

**Implementation Details:**
- **File:** `src/dacpac/model_xml.rs`
- **Function:** `extract_inline_tvf_columns()` (updated)
- **Changes:**
  1. Modified function signature to accept the function's full name as a parameter
  2. Added logic to detect parameter references (expressions starting with `@`)
  3. Format parameter references as `[schema].[FuncName].[@ParamName]` to match DotNet format
  4. These references are emitted in the ExpressionDependencies relationship for each SqlSimpleColumn

**Fixed Issues:**
- All 4 relationship errors resolved:
  - `[dbo].[GetCustomerOrders].[CustomerId]` now has ExpressionDependencies: `[dbo].[GetCustomerOrders].[@CustomerId]`
  - `[dbo].[GetCustomerOrders].[StartDate]` now has ExpressionDependencies: `[dbo].[GetCustomerOrders].[@StartDate]`
  - `[dbo].[GetCustomerOrders].[EndDate]` now has ExpressionDependencies: `[dbo].[GetCustomerOrders].[@EndDate]`
  - Parameter references in inline TVF SELECT columns now properly tracked

**Tasks:**
- [x] **13.2.1** Updated `extract_inline_tvf_columns()` to accept function full name
- [x] **13.2.2** Added parameter reference detection (expressions starting with @)
- [x] **13.2.3** Format parameter refs as `[schema].[FuncName].[@ParamName]`
- [x] **13.2.4** Run `test_parity_procedure_parameters` and verify 0 errors

---

### 13.3 Fix table_types Relationship Errors

**Fixture:** `table_types`
**Status:** COMPLETE
**Test:** `cargo test --test e2e_tests test_parity_table_types -- --nocapture`

**Root Cause Analysis:**
- Two procedures (`GetItemsByIds` and `ProcessOrderItems`) were missing all their relationships
- These procedures use table-valued parameters (TVPs) which require special handling
- The parameter regex didn't support `[schema].[type]` format or the READONLY keyword

**Implementation Details:**
- **Parameter Parsing:** Updated parameter regex to handle `[schema].[type]` format and READONLY keyword
- **Table Type Lookup:** Added `find_table_type_for_parameter()` function to look up table types in the model
- **DynamicObjects Relationship:** Implemented with `SqlDynamicColumnSource` element and nested Columns relationship
- **Parameters Relationship:** Added `write_table_type_relationship()` for TVP parameters to reference table types
- **BodyDependencies:** Added `extract_body_dependencies_with_tvp()` for TVP column references (e.g., `@Ids.Id`)
- **Test Comparison Fix:** Updated test comparison logic to use `(type, name)` tuple as key instead of just name

**Generated XML Structure (DynamicObjects):**
```xml
<Relationship Name="DynamicObjects">
  <Entry>
    <Element Type="SqlDynamicColumnSource" Name="[dbo].[GetItemsByIds].[@Ids]">
      <Relationship Name="Columns">
        <Entry>
          <Element Type="SqlSubroutineColumn" Name="[dbo].[GetItemsByIds].[@Ids].[Id]">
            <Property Name="IsNullable" Value="True" />
          </Element>
        </Entry>
      </Relationship>
    </Element>
  </Entry>
</Relationship>
```

**Tasks:**
- [x] **13.3.1** Updated parameter regex to handle `[schema].[type]` format and READONLY keyword
- [x] **13.3.2** Added `find_table_type_for_parameter()` function to look up table types in the model
- [x] **13.3.3** Implemented DynamicObjects relationship with SqlDynamicColumnSource element and nested Columns
- [x] **13.3.4** Added `write_table_type_relationship()` for Parameters relationship to reference table types
- [x] **13.3.5** Added `extract_body_dependencies_with_tvp()` for TVP column references in BodyDependencies
- [x] **13.3.6** Fixed test comparison logic to use `(type, name)` tuple as key instead of just name
- [x] **13.3.7** Run `test_parity_table_types` and verify 0 errors

---

### 13.4 Final Verification

**Status:** COMPLETE

**Goal:** Verify all tests pass, no clippy warnings, and full parity achieved.

**Tasks:**
- [x] **13.4.1** Run `just test` - all unit and integration tests pass
- [x] **13.4.2** Run `cargo clippy -- -D warnings` - no warnings
- [x] **13.4.3** Run `test_parity_regression_check` - 44/44 fixtures pass all layers
- [x] **13.4.4** Update baseline to reflect 100% parity
- [x] **13.4.5** Verify CI passes on GitHub Actions

---

### Phase 13 Progress

| Task | Description | Status |
|------|-------------|--------|
| 13.1 | Fix e2e_comprehensive and view_options | Complete |
| 13.2 | Fix procedure_parameters (4 errors) | Complete |
| 13.3 | Fix table_types (6 errors - TVP support) | Complete |
| 13.4 | Final verification | Complete |

**Phase 13 Total**: 4/4 sections complete

**Note:** Task 13.3 implemented full table-valued parameter (TVP) support including:
- DynamicObjects relationship with SqlDynamicColumnSource elements
- Table type parameter parsing with [schema].[type] format and READONLY keyword
- TVP column reference extraction for BodyDependencies
- Parameters relationship referencing table types

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
