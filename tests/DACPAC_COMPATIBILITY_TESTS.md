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

# Run e2e tests (requires dotnet SDK and Capital.DatabaseCore)
cargo test --test e2e_tests dotnet_comparison -- --ignored
```

## E2E Tests (Ignored by Default)

These tests require:
- DotNet SDK with Microsoft.Build.Sql
- Access to Capital.DatabaseCore project

| Test | Purpose |
|------|---------|
| test_compare_capital_database_dacpacs | Full comparison report |
| test_missing_header_section | Verify Header is generated |
| test_missing_database_options | Verify SqlDatabaseOptions |
| test_ampersand_encoding_bug | Verify & not truncated |
| test_index_double_bracket_bug | Verify [[ not in names |
| test_missing_inline_constraint_annotations | Verify annotations |
| test_default_constraint_coverage | Verify >90% coverage |

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
