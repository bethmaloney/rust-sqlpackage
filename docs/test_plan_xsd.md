# XSD-Based Test Plan for dacpac_tests.rs

This document tracks additional tests that can be added based on the official Microsoft XSD schema and existing functionality.

## DataSchemaModel Root Element Tests

- [x] **test_model_xml_has_file_format_version** - Verify `FileFormatVersion` attribute exists and is valid decimal
- [x] **test_model_xml_has_schema_version** - Verify `SchemaVersion` attribute exists and is valid decimal
- [x] **test_model_xml_has_collation_lcid** - Verify `CollationLcid` attribute exists (unsigned short)
- [x] **test_model_xml_has_collation_case_sensitive** - Verify `CollationCaseSensitive` attribute exists

## Element Type Coverage Tests

- [x] **test_model_contains_procedures** - Test stored procedure elements (SqlProcedure)
- [x] **test_model_contains_scalar_functions** - Test scalar functions (SqlScalarFunction)
- [x] **test_model_contains_table_valued_functions** - Test TVFs (SqlTableValuedFunction, SqlInlineTableValuedFunction)
- [x] **test_model_contains_sequences** - Test sequences (SqlSequence)
- [x] **test_model_contains_user_defined_types** - Test UDTs (SqlUserDefinedTableType)
- [x] **test_model_contains_triggers** - Test DML triggers (SqlDmlTrigger) via RawElement
- [x] **test_model_contains_schemas** - Verify custom schemas are captured

## Column Property Tests

- [x] **test_column_nullable_property** - Verify IsNullable property is set correctly
- [ ] **test_column_identity_property** - Verify IsIdentity property for identity columns *(FAILING: builder doesn't extract IDENTITY)*
- [x] **test_column_type_specifier** - Verify TypeSpecifier relationship with correct type reference
- [x] **test_column_length_property** - Verify Length property for varchar/char types
- [x] **test_column_precision_scale_properties** - Verify Precision/Scale for decimal types
- [x] **test_column_max_property** - Verify IsMax property for varchar(max)/nvarchar(max)
- [ ] **test_column_varbinary_max_property** - Verify IsMax for varbinary(max) *(FAILING: parser falls back for VARBINARY)*

## Index Property Tests

- [x] **test_index_is_unique_property** - Verify IsUnique property for unique indexes
- [x] **test_index_is_clustered_property** - Verify IsClustered property
- [x] **test_index_column_specifications** - Verify ColumnSpecifications relationship
- [x] **test_index_include_columns** - Verify IncludedColumns relationship

## Constraint Tests

- [x] **test_primary_key_constraint** - Verify SqlPrimaryKeyConstraint with DefiningTable relationship
- [x] **test_foreign_key_constraint_with_referenced_table** - Verify FK has ForeignTable relationship
- [x] **test_check_constraint_with_definition** - Verify check constraint has script annotation
- [x] **test_default_constraint** - Verify SqlDefaultConstraint element

## Relationship Structure Tests

- [x] **test_table_has_schema_relationship** - Verify table→schema relationship
- [x] **test_table_has_columns_relationship** - Verify table→columns relationship
- [x] **test_view_has_schema_relationship** - Verify view→schema relationship
- [x] **test_type_references_have_external_source** - Verify built-in types use `ExternalSource="BuiltIns"`

## Origin.xml Tests

- [x] **test_origin_xml_has_package_properties** - Verify PackageProperties element exists
- [x] **test_origin_xml_has_version** - Verify Version element in PackageProperties
- [x] **test_origin_xml_has_contains_exported_data** - Verify ContainsExportedData element

## DacMetadata.xml Tests

- [x] **test_metadata_xml_has_name** - Verify Name element present
- [x] **test_metadata_xml_has_version** - Verify Version element present

## Content_Types.xml Tests

- [x] **test_content_types_has_correct_mime_types** - Verify XML content types defined

## Edge Case Tests

- [x] **test_empty_project** - Project with no SQL objects
- [x] **test_project_with_only_schemas** - Only schema definitions
- [x] **test_reserved_keyword_identifiers** - Tables/columns using SQL reserved words
- [x] **test_unicode_identifiers** - Non-ASCII table/column names
- [x] **test_large_table_many_columns** - Table with many columns (stress test)
- [ ] **test_all_constraint_types_combined** - Single table with PK, FK, UQ, CK, DF *(FAILING: complex inline constraints cause parser fallback)*
- [x] **test_multiple_indexes_same_table** - Multiple indexes on one table
- [x] **test_self_referencing_foreign_key** - FK referencing same table

## Cross-Fixture Tests

- [x] **test_pre_post_deploy_scripts_excluded_from_model** - Deploy scripts don't appear as elements
- [x] **test_sdk_style_exclusions_work** - build_with_exclude fixture respects exclusions
- [x] **test_sqlcmd_includes_resolved** - sqlcmd_includes fixture processes includes

## XML Well-formedness Tests

- [x] **test_model_xml_is_well_formed** - Parse with XML parser without errors
- [x] **test_model_xml_has_xml_declaration** - Verify XML declaration with UTF-8 encoding
- [x] **test_model_xml_has_dataschemamodel_root** - Verify DataSchemaModel root element
- [x] **test_model_xml_has_model_element** - Verify Model element exists
- [x] **test_special_characters_escaped_in_definitions** - Verify `<`, `>`, `&` properly escaped in definitions

---

## Test Summary

**Total Tests**: 49
**Passing**: 46
**Failing**: 3

### Test Results by Category

| Category | Passing | Failing |
|----------|---------|---------|
| DataSchemaModel Root Element | 4 | 0 |
| Element Type Coverage | 7 | 0 |
| Column Property | 5 | 2 |
| Index Property | 4 | 0 |
| Constraint | 4 | 0 |
| Relationship Structure | 4 | 0 |
| Origin.xml | 3 | 0 |
| DacMetadata.xml | 2 | 0 |
| Content_Types.xml | 1 | 0 |
| Edge Case | 7 | 1 |
| Cross-Fixture | 3 | 0 |
| XML Well-formedness | 5 | 0 |

---

## Priority Suggestions

**High Priority** (core XSD compliance):
- DataSchemaModel root element tests ✅
- Relationship structure tests ✅
- XML well-formedness tests ✅

**Medium Priority** (element coverage):
- Element type coverage tests ✅
- Column property tests ✅ *(except IDENTITY and VARBINARY due to parser limitations)*
- Constraint tests ✅

**Lower Priority** (edge cases):
- Edge case tests ✅ *(except combined constraints)*
- Cross-fixture tests ✅
- Index property tests ✅

---

## Test Fixtures Added

- `tests/fixtures/element_types/` - Contains procedures, functions, sequences, UDTs, triggers, and schemas
- `tests/fixtures/column_properties/` - Contains table with various column types for property testing
- `tests/fixtures/index_properties/` - Contains indexes with various properties (unique, clustered, INCLUDE columns)
- `tests/fixtures/identity_column/` - Table with IDENTITY column for identity property testing
- `tests/fixtures/varbinary_max/` - Table with VARBINARY(MAX) column
- `tests/fixtures/empty_project/` - Empty project with no SQL files
- `tests/fixtures/only_schemas/` - Project with only schema definitions
- `tests/fixtures/reserved_keywords/` - Tables using SQL reserved keywords as identifiers
- `tests/fixtures/unicode_identifiers/` - Tables with Unicode/non-ASCII identifiers
- `tests/fixtures/large_table/` - Table with 55 columns (stress test)
- `tests/fixtures/all_constraints/` - Table with all constraint types combined
- `tests/fixtures/multiple_indexes/` - Multiple indexes on a single table
- `tests/fixtures/self_ref_fk/` - Table with self-referencing foreign key

## Known Issues

### Builder Bugs

1. **IDENTITY columns not extracted** (`test_column_identity_property` - FAILING)
   - The parser correctly parses tables with IDENTITY columns
   - However, `column_from_def()` in `src/model/builder.rs` has `is_identity` hardcoded to `false`
   - Fix: Extract IDENTITY option from `ColumnOption` in the builder

### Parser Limitations (sqlparser-rs)

1. **VARBINARY types cause fallback** (`test_column_varbinary_max_property` - FAILING)
   - Tables containing VARBINARY(n) or VARBINARY(MAX) columns cause sqlparser to fall back to RawStatement
   - This loses the column structure entirely
   - The table is stored as a script annotation instead of proper elements

2. **Complex inline constraints cause fallback** (`test_all_constraint_types_combined` - FAILING)
   - When multiple constraint types (PK, FK, UQ, CK, DF) are combined in a single CREATE TABLE statement
   - The parser falls back to storing the table as a `SqlInlineConstraintAnnotation`
   - Individual constraints are not extracted as separate elements
   - Workaround: Define constraints in separate ALTER TABLE statements
