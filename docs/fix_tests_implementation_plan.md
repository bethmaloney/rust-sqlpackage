# Dacpac Compatibility Test Fix Plan

This document tracks progress on fixing failing/ignored integration tests to achieve full compatibility with .NET DacFx.

## Summary

- **Total Ignored Tests**: 12
- **Fixed**: 1
- **Remaining**: 11

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

- [ ] **Test**: `test_build_with_extended_properties`
- **Ignore Reason**: SqlExtendedProperty not yet implemented
- **Impact**: 174 missing elements (column/table descriptions)
- **Files to Modify**:
  - `src/parser/mod.rs` - Parse `sp_addextendedproperty` calls
  - `src/model/mod.rs` - Add ExtendedProperty variant to ModelElement
  - `src/dacpac/xml.rs` - Generate SqlExtendedProperty elements
- **Implementation Notes**:
  - Extended properties use `EXEC sp_addextendedproperty` syntax
  - Common property: `MS_Description` for documentation
  - Need to link to host element (column, table, etc.)
- **Fixture**: `tests/fixtures/extended_properties/`

---

### 3. Full-Text Index

- [ ] **Test**: `test_build_with_fulltext_index`
- **Ignore Reason**: SqlFullTextIndex not yet implemented
- **Impact**: 4 missing full-text indexes, 6 column specifiers
- **Files to Modify**:
  - `src/parser/index.rs` - Parse CREATE FULLTEXT INDEX statements
  - `src/model/mod.rs` - Add FullTextIndex variant to ModelElement
  - `src/dacpac/xml.rs` - Generate SqlFullTextIndex elements
- **Implementation Notes**:
  - Syntax: `CREATE FULLTEXT INDEX ON table (columns) KEY INDEX pk_name`
  - Properties: IsStopListOff, DoUseSystemStopList, LanguageId
  - Needs SqlFullTextIndexColumnSpecifier for each column
- **Fixture**: `tests/fixtures/fulltext_index/`

---

### 4. Table Types (Complete)

- [ ] **Test**: `test_build_with_table_types`
- **Ignore Reason**: SqlTableType column structure not yet implemented
- **Impact**: Missing table type columns, PK constraints, index specs
- **Files to Modify**:
  - `src/model/table_type.rs` - Add column definitions to TableType
  - `src/dacpac/xml.rs` - Generate SqlTableTypeSimpleColumn elements
  - `src/dacpac/xml.rs` - Generate SqlTableTypePrimaryKeyConstraint
- **Implementation Notes**:
  - Table types can have columns, PKs, and indexes
  - Need SqlTableTypeSimpleColumn, SqlTableTypePrimaryKeyConstraint
  - Currently only outputs basic SqlTableType element
- **Fixture**: `tests/fixtures/table_types/`

---

### 5. Inline Constraint Annotations

- [ ] **Test**: `test_build_with_inline_constraint_annotations`
- **Ignore Reason**: SqlInlineConstraintAnnotation not yet implemented
- **Impact**: Missing metadata linking columns to constraints
- **Files to Modify**:
  - `src/model/table.rs` - Track which constraints are inline
  - `src/dacpac/xml.rs` - Generate SqlInlineConstraintAnnotation elements
- **Implementation Notes**:
  - DacFx uses annotations to track inline vs standalone constraints
  - Annotations link column to its inline constraint
  - Format: `<Element Type="SqlInlineConstraintAnnotation" Name="...">`
- **Fixture**: `tests/fixtures/inline_constraints/`

---

### 6. Inline CHECK Constraints

- [ ] **Test**: `test_build_with_inline_check_constraints`
- **Ignore Reason**: Inline CHECK constraints not yet captured
- **Impact**: Missing CHECK constraints defined inline in column definitions
- **Files to Modify**:
  - `src/parser/table.rs` - Parse inline CHECK (expression) after column type
  - `src/model/table.rs` - Store inline check constraints
  - `src/dacpac/xml.rs` - Output SqlCheckConstraint elements
- **Implementation Notes**:
  - Pattern: `[Age] INT CHECK ([Age] >= 18)`
  - Different from standalone: `CONSTRAINT CK_Age CHECK ([Age] >= 18)`
  - Need to generate constraint name if not provided
- **Fixture**: `tests/fixtures/inline_constraints/`

---

### 7. OUTPUT Parameters

- [ ] **Test**: `test_build_with_output_parameters`
- **Ignore Reason**: OUTPUT parameter mode not yet captured
- **Impact**: Missing IsOutput property on procedure parameters
- **Files to Modify**:
  - `src/parser/procedure.rs` - Parse OUTPUT keyword on parameters
  - `src/model/procedure.rs` - Add is_output field to Parameter
  - `src/dacpac/xml.rs` - Add IsOutput property when true
- **Implementation Notes**:
  - Pattern: `@Result INT OUTPUT` or `@Result INT OUT`
  - Also `@Param INT = NULL OUTPUT` with default
  - Need Property element: `<Property Name="IsOutput" Value="True"/>`
- **Fixture**: `tests/fixtures/procedure_parameters/`

---

### 8. Header Section

- [ ] **Test**: `test_build_with_header_section`
- **Ignore Reason**: Header section not yet implemented
- **Impact**: Missing model.xml Header with settings
- **Files to Modify**:
  - `src/dacpac/xml.rs` - Add Header element before Model
  - `src/project/mod.rs` - Extract settings from sqlproj
- **Implementation Notes**:
  - Header contains: AnsiNulls, QuotedIdentifier, CompatibilityMode
  - Format: `<Header><... /></Header>` before `<Model>`
  - Settings come from sqlproj PropertyGroup
- **Fixture**: `tests/fixtures/header_section/`

---

### 9. Package References in Header

- [ ] **Test**: `test_build_with_package_references`
- **Ignore Reason**: Package references in Header not yet implemented
- **Impact**: Missing dacpac references (master.dacpac, etc.)
- **Files to Modify**:
  - `src/project/mod.rs` - Parse PackageReference items
  - `src/dacpac/xml.rs` - Add CustomData section to Header
- **Implementation Notes**:
  - References like `Microsoft.SqlServer.Dacpacs.Master`
  - Goes in Header/CustomData section
  - Format: `<CustomData Category="Reference" Type="...">`
- **Fixture**: `tests/fixtures/header_section/`

---

### 10. Database Options

- [ ] **Test**: `test_build_with_database_options`
- **Ignore Reason**: SqlDatabaseOptions not yet implemented
- **Impact**: 1 missing SqlDatabaseOptions element
- **Files to Modify**:
  - `src/project/mod.rs` - Extract database options from sqlproj
  - `src/model/mod.rs` - Add DatabaseOptions struct
  - `src/dacpac/xml.rs` - Generate SqlDatabaseOptions element
- **Implementation Notes**:
  - Properties: IsAnsiNullsOn, PageVerifyMode, Collation, etc.
  - Comes from sqlproj PropertyGroup settings
  - Single element per database
- **Fixture**: `tests/fixtures/database_options/`

---

### 11. SQLCMD Variables

- [ ] **Test**: `test_build_with_sqlcmd_variables`
- **Ignore Reason**: SqlCmdVariables in Header not yet implemented
- **Impact**: Missing SQLCMD variable definitions
- **Files to Modify**:
  - `src/project/mod.rs` - Parse SqlCmdVariable items
  - `src/dacpac/xml.rs` - Add to Header/CustomData section
- **Implementation Notes**:
  - Pattern in sqlproj: `<SqlCmdVariable Include="Environment">`
  - Goes in Header/CustomData section
  - Variables have Value and DefaultValue
- **Fixture**: `tests/fixtures/sqlcmd_variables/`

---

### 12. Table IsAnsiNullsOn Property

- [ ] **Test**: `test_table_has_ansi_nulls_property`
- **Ignore Reason**: Table IsAnsiNullsOn property not yet implemented
- **Impact**: Missing property on all table elements
- **Files to Modify**:
  - `src/dacpac/xml.rs` - Add IsAnsiNullsOn property to SqlTable
- **Implementation Notes**:
  - Simple addition: `<Property Name="IsAnsiNullsOn" Value="True"/>`
  - All tables should have this property
  - Value typically True (ANSI_NULLS ON is default)
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

*Last updated: 2025-01-25*
