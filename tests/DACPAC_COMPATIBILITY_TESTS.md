# Dacpac Compatibility Tests

Tests created to verify and track rust-sqlpackage compatibility with DotNet DacFx output.

## Test Counts

| Type | Total | Passing | Failing | Ignored |
|------|-------|---------|---------|---------|
| Unit Tests (dacpac_comparison) | 19 | 15 | 4 | 0 |
| Integration Tests (dacpac_compatibility) | 16 | 3 | 1 | 12 |
| E2E Tests (dotnet_comparison) | 7 | - | - | 7 |

- **Failing tests** = Bugs that need fixing
- **Ignored tests** = Missing features not yet implemented

## Failing Unit Tests (Bugs/Missing Features)

### 1. test_ampersand_in_procedure_name
**Status:** FAILING
**Issue:** Procedure names containing `&` are truncated
**Example:** `[IOLoansWithoutP&IConversionNotifications]` becomes `[IOLoansWithoutP]`

### 2. test_inline_check_constraint
**Status:** FAILING
**Issue:** Inline CHECK constraints are not captured as separate constraint elements

### 3. test_named_inline_default_constraint
**Status:** FAILING
**Issue:** Named inline default constraints (e.g., `CONSTRAINT [DF_Name] DEFAULT`) are not extracted

### 4. test_multiple_named_inline_defaults
**Status:** FAILING
**Issue:** Same as above - 0 named defaults captured instead of expected 3

## Missing Features (Documented by Integration Tests)

### Completely Missing (100%)

| Feature | Description |
|---------|-------------|
| SqlInlineConstraintAnnotation | Links columns to their inline constraints |
| SqlDatabaseOptions | Database-level settings (collation, ANSI, etc.) |
| Header section | AnsiNulls, QuotedIdentifier, CompatibilityMode |
| SqlCmdVariables | SQLCMD variable definitions in Header |
| SqlExtendedProperty | Column/table descriptions |
| SqlTableType columns | User-defined table type column structure |
| Table IsAnsiNullsOn | Table-level ANSI_NULLS property |
| Inline CHECK constraints | CHECK constraints defined inline with columns |

### Partially Missing

| Feature | Current | Expected | Coverage |
|---------|---------|----------|----------|
| SqlDefaultConstraint | 0 | ~14 | 0% |
| SqlSubroutineParameter | 14 | ~20 | 70% |

## Test Fixtures Created

| Fixture | Purpose |
|---------|---------|
| `extended_properties/` | Tests sp_addextendedproperty support |
| `fulltext_index/` | Tests FULLTEXT INDEX support |
| `table_types/` | Tests CREATE TYPE AS TABLE support |
| `ampersand_encoding/` | Tests & in identifiers |
| `index_naming/` | Tests index name format (double-bracket bug) |
| `default_constraints_named/` | Tests CONSTRAINT [name] DEFAULT |
| `inline_constraints/` | Tests inline UNIQUE/CHECK constraints |
| `procedure_parameters/` | Tests parameter capture (including OUTPUT) |
| `sqlcmd_variables/` | Tests SqlCmdVariable in project |
| `header_section/` | Tests Header generation with options |
| `database_options/` | Tests SqlDatabaseOptions element |

## Running Tests

```bash
# Run unit tests (quick feedback on bugs)
cargo test --test unit_tests dacpac_comparison

# Run integration tests (verify dacpac structure)
cargo test --test integration_tests dacpac_compatibility -- --nocapture

# Run e2e tests (requires dotnet SDK)
cargo test --test e2e_tests dotnet_comparison -- --ignored

# Run e2e tests with a custom project
SQL_TEST_PROJECT=/path/to/YourProject.sqlproj cargo test --test e2e_tests -- --ignored
```

## E2E Tests (Ignored by Default)

These tests require:
- DotNet SDK with Microsoft.Build.Sql
- (Optional) SqlPackage CLI for Layer 3 tests

By default, tests use the `tests/fixtures/e2e_comprehensive` fixture. You can specify a custom project via the `SQL_TEST_PROJECT` environment variable.

### Layered Comparison Approach

The E2E tests use a three-layer comparison approach for thorough validation:

| Layer | What it Tests | Error Messages |
|-------|---------------|----------------|
| **Layer 1** | Element inventory - all elements exist with correct names | "MISSING in Rust: SqlTable [dbo].[Users]" |
| **Layer 2** | Property comparison - element properties match | "PROPERTY MISMATCH: SqlSimpleColumn.[dbo].[Users].[Id] - IsNullable (Rust: None, DotNet: Some("False"))" |
| **Layer 3** | SqlPackage DeployReport - deployment equivalence | Shows actual DDL script of differences |

### Test Functions

| Test | Purpose |
|------|---------|
| test_layered_dacpac_comparison | Full 3-layer comparison (main test) |
| test_layer1_element_inventory | Layer 1 only with detailed grouping |
| test_layer2_property_comparison | Layer 2 only with property details |
| test_layer3_sqlpackage_comparison | Layer 3 only (requires SqlPackage) |
| test_print_element_summary | Print element counts by type |
| test_ampersand_encoding | Verify & in names is handled correctly |
| test_index_naming | Verify no double brackets in names |

## Priority Order for Fixes

### High Priority
1. **Ampersand truncation bug** - Data corruption issue
2. **Named default constraints** - 55% missing in real projects
3. **Header section** - Required for compatibility
4. **SqlDatabaseOptions** - Required for proper deployment

### Medium Priority
5. SqlInlineConstraintAnnotation
6. Inline CHECK constraints
7. SqlSubroutineParameter (30% missing)
8. Table IsAnsiNullsOn property

### Lower Priority
9. SqlExtendedProperty
10. SqlTableType columns
11. Full-text index support
12. SqlCmdVariables
