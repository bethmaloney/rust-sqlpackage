# Body Dependencies Aliases Test Fixture

This fixture tests table alias resolution in SQL bodies (views, procedures, functions).

## Purpose

Verify that table aliases are correctly resolved to their actual table references in BodyDependencies, not treated as schema names or column names.

## Test Summary

| Status | Count | Description |
|--------|-------|-------------|
| ✓ PASSING | 10 | Tests that currently work correctly |
| ✗ FAILING | 11 | Tests that expose known bugs (marked `#[ignore]`) |

## Test Scenarios

### PASSING TESTS (10)

#### 1. Simple Alias Resolution
**File:** `Views/AccountSummary.sql`
**Test:** `test_simple_alias_resolution`

Simple flat JOINs with table aliases (A, T, AT).

#### 2. Procedure with Nested Aliases
**File:** `Procedures/GetAccountsWithTags.sql`
**Test:** `test_procedure_nested_alias_resolution`

Aliases in procedure bodies with STUFF and nested subqueries.

#### 3. Scalar Function with Nested Aliases
**File:** `Functions/GetInstrumentTagList.sql`
**Test:** `test_scalar_function_nested_alias_resolution`

STUFF with nested subquery in scalar function body.

#### 4. Table-Valued Function with Nested Aliases
**File:** `Functions/GetAccountTagsTable.sql`
**Test:** `test_tvf_nested_alias_resolution`

OUTER APPLY with aliases in TVF.

#### 5. Multiple CTEs in Procedure
**File:** `Procedures/GetAccountWithCte.sql`
**Test:** `test_multiple_ctes_in_procedure`

Multiple CTEs in sequence within a procedure.

#### 6. UNION with Different Aliases
**File:** `Views/AccountWithUnion.sql`
**Test:** `test_union_alias_resolution`

Each SELECT in UNION has different aliases.

#### 7. Window Functions with Aliases
**File:** `Views/AccountWithWindowFunction.sql`
**Test:** `test_window_function_alias_resolution`

Aliases used in window function OVER clauses.

#### 8. UPDATE with FROM Clause
**File:** `Procedures/UpdateAccountWithFrom.sql`
**Test:** `test_update_from_alias_resolution`

Aliases in UPDATE...FROM statements.

#### 9. DELETE with FROM Clause
**File:** `Procedures/DeleteAccountWithFrom.sql`
**Test:** `test_delete_from_alias_resolution`

Aliases in DELETE...FROM statements.

#### 10. INSERT...SELECT with Aliases
**File:** `Procedures/InsertSelectWithAliases.sql`
**Test:** `test_insert_select_alias_resolution`

Aliases in INSERT...SELECT with nested EXISTS.

---

### FAILING TESTS (11) - Known Bugs

#### 1. STUFF + Nested Subquery Aliases
**File:** `Views/InstrumentWithTags.sql`
**Test:** `test_stuff_nested_subquery_alias_resolution`

Aliases inside STUFF() function with nested SELECT.

**Bug:**
```xml
<References Name="[dbo].[Instrument].[ITTAG]"/>  <!-- WRONG -->
<References Name="[dbo].[Instrument].[IT2]"/>    <!-- WRONG -->
```

#### 2. Multiple Nested Subqueries
**File:** `Views/AccountWithNestedSubqueries.sql`
**Test:** `test_nested_subquery_alias_resolution`

Multiple levels of nesting with aliases at different depths.

**Bug:**
```xml
<References Name="[dbo].[Account].[AT2]"/>  <!-- WRONG -->
<References Name="[dbo].[Account].[T2]"/>   <!-- WRONG -->
```

#### 3. CROSS APPLY / OUTER APPLY
**File:** `Views/AccountWithApply.sql`
**Test:** `test_apply_clause_alias_resolution`

Aliases in APPLY clauses.

**Bug:**
```xml
<References Name="[dbo].[Account].[ATAG]"/>  <!-- WRONG -->
<References Name="[dbo].[Account].[ATA]"/>   <!-- WRONG -->
```

#### 4. CTE Alias Recognition
**File:** `Views/AccountWithCTE.sql`
**Test:** `test_cte_alias_recognition`

CTEs should not appear in dependencies.

**Bug:**
```xml
<References Name="[dbo].[TaggedAccounts]"/>  <!-- CTE, should not appear -->
```

#### 5. EXISTS/NOT EXISTS Subqueries
**File:** `Views/AccountWithExistsSubquery.sql`
**Test:** `test_exists_subquery_alias_resolution`

Aliases inside EXISTS and NOT EXISTS subqueries.

**Bug:**
```xml
<References Name="[dbo].[Account].[AT1]"/>  <!-- WRONG -->
<References Name="[dbo].[Account].[T1]"/>   <!-- WRONG -->
```

#### 6. IN Clause Subqueries
**File:** `Views/AccountWithInSubquery.sql`
**Test:** `test_in_subquery_alias_resolution`

Aliases inside IN clause subqueries, including nested IN.

#### 7. Correlated Subqueries in SELECT
**File:** `Views/AccountWithCorrelatedSubquery.sql`
**Test:** `test_correlated_subquery_alias_resolution`

Aliases in correlated subqueries within the SELECT list.

#### 8. CASE Expression Subqueries
**File:** `Views/AccountWithCaseSubquery.sql`
**Test:** `test_case_subquery_alias_resolution`

Subqueries with aliases inside CASE WHEN expressions.

#### 9. Derived Table Chain
**File:** `Views/AccountWithDerivedTableChain.sql`
**Test:** `test_derived_table_chain_alias_resolution`

Multiple levels of nested derived tables.

#### 10. Recursive CTE
**File:** `Views/AccountWithRecursiveCTE.sql`
**Test:** `test_recursive_cte_alias_resolution`

Recursive CTE with self-reference - the CTE name should not appear as a dependency.

**Bug:**
```xml
<References Name="[dbo].[TagHierarchy]"/>  <!-- CTE, should not appear -->
```

#### 11. MERGE Statement
**File:** `Procedures/MergeAccountTags.sql`
**Test:** `test_merge_alias_resolution`

TARGET and SOURCE aliases in MERGE statements.

**Bug:**
```xml
<References Name="[TARGET].[AccountId]"/>       <!-- WRONG -->
<References Name="[dbo].[AccountTag].[MERGE]"/> <!-- WRONG - keyword parsed as column -->
```

## Running Tests

```bash
# Run all passing alias resolution tests
cargo test --test integration_tests alias_resolution

# Run ignored tests to see bugs
cargo test --test integration_tests alias_resolution -- --ignored

# Run with output to see dependency lists
cargo test --test integration_tests alias_resolution -- --nocapture

# Run a specific test
cargo test --test integration_tests test_exists_subquery_alias_resolution -- --ignored --nocapture
```

## Root Cause

The alias resolution logic in `src/dacpac/model_xml.rs` does not properly capture aliases defined within:
- Deeply nested subqueries (especially in STUFF functions)
- APPLY clauses in views
- Subqueries used as derived tables in FROM clauses
- EXISTS/IN clause subqueries
- Correlated subqueries in SELECT list
- CASE expression subqueries
- CTEs (single CTE in views, recursive CTEs)
- MERGE statements

When an alias is not found in the `table_aliases` map, it's incorrectly treated as a column of a previously referenced table instead of being recognized as an unresolved reference.

## Expected Fix

The parser should:
1. Properly extract ALL table aliases regardless of nesting depth
2. Handle aliases in all contexts (STUFF, APPLY, derived tables, CTEs, EXISTS, IN, CASE, etc.)
3. Not emit references to unresolved aliases - either resolve them or emit a warning
4. Not treat CTE names as table references
5. Handle MERGE TARGET/SOURCE as special alias cases
6. Track derived table aliases separately from table aliases
