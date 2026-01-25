# Dacpac Compatibility Test Fix Plan

This document tracks progress on fixing failing/ignored integration tests to achieve full compatibility with .NET DacFx.

## Summary

- **Total Ignored Tests**: 12
- **Fixed**: 12
- **Remaining**: 0

## Test Fixes

### 1. Named Default Constraints

- [x] **Test**: `test_build_with_named_default_constraints`
- **Status**: ✅ FIXED - Already implemented, test re-enabled
- **Notes**: The named inline default constraints were already being captured correctly.
  The parser in `src/parser/tsql_parser.rs` correctly extracts constraint names via
  `parse_column_definition()` regex patterns, the model builder creates `ConstraintElement`
  entries with the names, and the XML serializer outputs proper `SqlDefaultConstraint`
  elements with full names like `[dbo].[Entity].[DF_Entity_Version]`.
- **Fixture**: `tests/fixtures/default_constraints_named/`

---

### 2. Extended Properties

- [x] **Test**: `test_build_with_extended_properties`
- **Status**: ✅ FIXED - Extended properties now parsed and serialized correctly
- **Notes**: Extended properties from `sp_addextendedproperty` calls are now captured.
  The parser extracts property name, value, and target information (schema, table, column)
  from EXEC statements. The model builder creates `ExtendedPropertyElement` entries, and
  the XML serializer outputs proper `SqlExtendedProperty` elements with `Value` property
  (wrapped in N'...' for SQL string literal format) and `Host` relationship pointing to
  the target object.
- **Fixture**: `tests/fixtures/extended_properties/`

---

### 3. Full-Text Index

- [x] **Test**: `test_build_with_fulltext_index`
- **Status**: ✅ FIXED - Full-text indexes and catalogs now parsed and serialized correctly
- **Notes**: Full-text indexes and catalogs from `CREATE FULLTEXT INDEX` and `CREATE FULLTEXT CATALOG`
  statements are now captured. The parser extracts table, columns with language IDs, key index,
  catalog reference, and change tracking mode. The model builder creates `FullTextIndexElement` and
  `FullTextCatalogElement` entries, and the XML serializer outputs proper `SqlFullTextIndex` and
  `SqlFullTextCatalog` elements with `SqlFullTextIndexColumnSpecifier` entries for each column.
- **Fixture**: `tests/fixtures/fulltext_index/`

---

### 4. Table Types (Complete)

- [x] **Test**: `test_build_with_table_types`
- **Status**: ✅ FIXED - Table type columns and constraints now parsed and serialized correctly
- **Notes**: Table types are now fully supported with columns, PRIMARY KEY, UNIQUE, CHECK, and INDEX constraints.
  The parser in `src/parser/tsql_parser.rs` extracts columns and constraints via `extract_table_type_structure()`.
  The model builder creates `TableTypeColumnElement` and `TableTypeConstraint` entries. The XML serializer
  outputs proper `SqlTableTypeSimpleColumn`, `SqlTableTypePrimaryKeyConstraint`, `SqlTableTypeUniqueConstraint`,
  `SqlTableTypeCheckConstraint`, and `SqlTableTypeIndexedColumnSpecification` elements.
- **Fixture**: `tests/fixtures/table_types/`

---

### 5. Inline Constraints (Complete)

- [x] **Test**: `test_build_with_inline_constraints`
- **Status**: ✅ FIXED - Inline constraints now fully captured and serialized correctly
- **Notes**: Investigation revealed that modern .NET DacFx does NOT use `SqlInlineConstraintAnnotation`.
  Instead, inline constraints (DEFAULT, CHECK, UNIQUE, PRIMARY KEY) are converted to separate
  constraint elements with auto-generated names (e.g., `DF_TableName_ColumnName`, `CK_TableName_ColumnName`).
  The implementation now:
  - Captures inline DEFAULT constraints from column definitions (including bare literals like `0.00`, `1`,
    string literals like `'Active'`, and function calls like `GETDATE()`, `NEWID()`)
  - Captures inline CHECK constraints with auto-generated names if not explicitly named
  - Captures inline UNIQUE constraints on columns
  - Captures inline PRIMARY KEY constraints on columns
  - The parser regex was extended to handle DEFAULT values without parentheses
- **Fixture**: `tests/fixtures/inline_constraints/`

---

### 6. Inline CHECK Constraints

- [x] **Test**: `test_build_with_inline_check_constraints`
- **Status**: ✅ FIXED - Covered by inline constraints fix above
- **Notes**: Inline CHECK constraints are now parsed via sqlparser-rs AST and serialized as
  `SqlCheckConstraint` elements with auto-generated names if no explicit constraint name is provided.
- **Fixture**: `tests/fixtures/inline_constraints/`

---

### 7. OUTPUT Parameters

- [x] **Test**: `test_build_with_output_parameters`
- **Status**: ✅ FIXED - Already implemented, test re-enabled
- **Notes**: OUTPUT parameter support was already fully implemented:
  - Parser in `src/dacpac/model_xml.rs` extracts OUTPUT/OUT keywords via `extract_procedure_parameters()`
    using regex pattern `(?:\s+(OUTPUT|OUT))?` in the parameter extraction regex
  - `ProcedureParameter` struct already had `is_output: bool` field
  - XML serializer already writes `<Property Name="IsOutput" Value="True"/>` when `param.is_output` is true
- **Fixture**: `tests/fixtures/procedure_parameters/`

---

### 8. Header Section

- [x] **Test**: `test_build_with_header_section`
- **Status**: ✅ FIXED - Header section now generated correctly
- **Notes**: The Header section is now generated before the Model element in model.xml.
  The implementation:
  - Parses `AnsiNulls` and `QuotedIdentifier` settings from sqlproj PropertyGroup (defaults: true)
  - Generates `<Header>` element with `<CustomData>` entries for:
    - `AnsiNulls` - from project settings
    - `QuotedIdentifier` - from project settings
    - `CompatibilityMode` - derived from target platform (130, 140, 150, 160)
  - Added `ansi_nulls` and `quoted_identifier` fields to `SqlProject` struct
  - Added `compatibility_mode()` method to `SqlServerVersion` enum
- **Fixture**: `tests/fixtures/header_section/`

---

### 9. Package References in Header

- [x] **Test**: `test_build_with_package_references`
- **Status**: ✅ FIXED - Package references now parsed and serialized correctly
- **Notes**: Package references from `<PackageReference>` items in sqlproj are now captured.
  The parser in `src/project/sqlproj_parser.rs` extracts package name and version via
  `find_package_references()`. Added `PackageReference` struct and `package_references` field
  to `SqlProject`. The XML serializer in `src/dacpac/model_xml.rs` outputs proper
  `<CustomData Category="Reference" Type="SqlSchema">` elements with `FileName`, `LogicalName`,
  and `SuppressMissingDependenciesErrors` metadata. Package names like
  `Microsoft.SqlServer.Dacpacs.Master` are converted to `master.dacpac`.
- **Fixture**: `tests/fixtures/header_section/`

---

### 10. Database Options

- [x] **Test**: `test_build_with_database_options`
- **Status**: ✅ FIXED - SqlDatabaseOptions element now generated correctly
- **Notes**: Database options from sqlproj PropertyGroup are now parsed and serialized.
  The implementation:
  - Added `DatabaseOptions` struct to `src/project/sqlproj_parser.rs` to capture settings:
    - `collation` - DefaultCollation from sqlproj
    - `page_verify` - PageVerify mode (CHECKSUM, TORN_PAGE_DETECTION, NONE)
    - `ansi_null_default_on`, `ansi_nulls_on`, `ansi_warnings_on`, `arith_abort_on`
    - `concat_null_yields_null_on`, `full_text_enabled`
  - `parse_database_options()` function extracts these settings from sqlproj XML
  - `write_database_options()` in `src/dacpac/model_xml.rs` generates `SqlDatabaseOptions` element
  - Properties include: Collation, IsAnsiNullDefaultOn, IsAnsiNullsOn, IsAnsiWarningsOn,
    IsArithAbortOn, IsConcatNullYieldsNullOn, IsFullTextEnabled, PageVerifyMode
  - PageVerify strings are converted to numeric values (NONE=0, TORN_PAGE_DETECTION=1, CHECKSUM=3)
- **Fixture**: `tests/fixtures/database_options/`

---

### 11. SQLCMD Variables

- [x] **Test**: `test_build_with_sqlcmd_variables`
- **Status**: ✅ FIXED - SQLCMD variables now parsed and serialized correctly
- **Notes**: SQLCMD variables from `<SqlCmdVariable>` items in sqlproj are now captured.
  The implementation:
  - Added `SqlCmdVariable` struct to `src/project/sqlproj_parser.rs` to capture name, value, and default_value
  - `find_sqlcmd_variables()` function extracts variables from sqlproj XML
  - Added `sqlcmd_variables` field to `SqlProject` struct
  - `write_sqlcmd_variable()` in `src/dacpac/model_xml.rs` generates `CustomData` elements in Header
  - Format: `<CustomData Category="SqlCmdVariable"><Metadata Name="SqlCmdVariable" Value="..."/><Metadata Name="DefaultValue" Value="..."/></CustomData>`
- **Fixture**: `tests/fixtures/sqlcmd_variables/`

---

### 12. Table IsAnsiNullsOn Property

- [x] **Test**: `test_table_has_ansi_nulls_property`
- **Status**: ✅ FIXED - IsAnsiNullsOn property now added to all SqlTable elements
- **Notes**: The `IsAnsiNullsOn` property is now written to all table elements in model.xml.
  The implementation adds `<Property Name="IsAnsiNullsOn" Value="True"/>` to each SqlTable
  element in `src/dacpac/model_xml.rs` via the `write_table()` function. ANSI_NULLS ON is
  the default behavior in SQL Server, so all tables have this property set to True.
- **Fixture**: `tests/fixtures/simple_table/` (existing)

---

## Additional Issues Found (Not Yet Tests)

### Missing Tables (11 tables failed to parse)

The following tables exist in SQL files but are not in the Rust dacpac:

- `[History].[InstrumentNote]`
- `[History].[PropertyTitle]`
- `[dbo].[AccountSplitDetail]`
- `[dbo].[AssociatedAddress]`
- `[dbo].[Card]`
- `[dbo].[Dependant]`
- `[dbo].[PartyCategoryType]`
- `[dbo].[PartyPaymentLimit]`
- `[dbo].[Profile]`
- `[dbo].[TitleType]`
- `[dbo].[Watcher]`

**Likely Cause**: Some SQL files use `GO;` (with semicolon) instead of `GO`. The Watcher.sql also has duplicate constraint names which may cause parsing issues.

---

## Priority Order (Recommended)

1. **Named Default Constraints** - High impact (1,739 elements), straightforward fix
2. **Table IsAnsiNullsOn Property** - Simple property addition
3. **OUTPUT Parameters** - Small change with clear benefit
4. **Inline CHECK Constraints** - Completes constraint coverage
5. **Extended Properties** - Documentation support
6. **Header Section** - Foundation for other header features
7. **Database Options** - Depends on header
8. **SQLCMD Variables** - Depends on header
9. **Package References** - Depends on header
10. **Full-Text Index** - New parser needed
11. **Table Types Complete** - Complex nested structure
12. **Inline Constraint Annotations** - Metadata only, low priority

---

## Verification

After fixing each test:

1. Remove `#[ignore]` attribute from test
2. Run `just test` to verify test passes
3. Run `just test-e2e` to verify no regressions
4. Update this document to mark as complete

---

*Last updated: 2026-01-25 (Table IsAnsiNullsOn property implemented - ALL TESTS COMPLETE)*
