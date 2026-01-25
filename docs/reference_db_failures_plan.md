# Reference Database Comparison: Failures and Differences

This document tracks all identified differences between rust-sqlpackage output and the reference dotnet DacFx tool output when building the same SQL project.

## Status Legend

- [ ] Not started
- [x] Completed
- [~] In progress

---

## Critical Bugs

### 1. `GO;` Batch Separator Not Handled

- **Status:** [x] Completed
- **Severity:** Critical
- **Impact:** Tables and other objects in affected files are missing from output

**Description:**
The batch splitter only recognizes `GO` on its own line. SQL files that use `GO;` (with trailing semicolon) are not split into batches correctly. This causes only the first statement in each file to be parsed, resulting in missing tables, indexes, and other objects.

**Location:** `src/parser/tsql_parser.rs` - `split_batches()` function

**Fix:**
Modify the batch splitting logic to also handle `GO;` as a valid batch terminator:
```rust
// Current check
if trimmed.eq_ignore_ascii_case("go")

// Should also handle
if trimmed.eq_ignore_ascii_case("go") || trimmed.eq_ignore_ascii_case("go;")
```

---

## Missing Element Types

### 2. SqlInlineConstraintAnnotation

- **Status:** [x] Completed
- **Severity:** Low

**Description:**
Dotnet emits `SqlInlineConstraintAnnotation` elements on columns that have inline constraints (e.g., default values defined inline). These annotations track which constraints were defined inline vs as named table-level constraints.

**Implementation:**
- Added `inline_constraint_disambiguator: Option<u32>` field to `ColumnElement` struct
- Updated `column_from_def()` and `column_from_fallback_table()` to set disambiguator when columns have inline DEFAULT, CHECK, PRIMARY KEY, or UNIQUE constraints
- Updated `write_column_with_type()` in model_xml.rs to emit the annotation

**Example output:**
```xml
<Element Type="SqlSimpleColumn" Name="[dbo].[Table].[Column]">
    <Annotation Type="SqlInlineConstraintAnnotation" Disambiguator="123456" />
</Element>
```

---

### 3. SqlComputedColumn

- **Status:** [ ] Not started
- **Severity:** Low

**Description:**
Dotnet parses stored procedure and function bodies to extract computed columns from CTEs, temp tables, and other query constructs. Rust-sqlpackage does not perform this deep analysis of procedure bodies.

**Example:** Columns in CTE definitions within stored procedures:
```sql
WITH CTE AS (SELECT a.field AS computed_col FROM ...)
```

---

### 4. SqlDynamicColumnSource

- **Status:** [ ] Not started
- **Severity:** Low

**Description:**
Related to computed/dynamic columns within procedure bodies. Dotnet tracks the source of dynamically computed columns. Rust-sqlpackage does not emit these elements.

---

### 5. SqlExtendedProperty

- **Status:** [x] Completed
- **Severity:** Medium

**Description:**
Extended properties defined via `sp_addextendedproperty` (e.g., `MS_Description` for documentation) are now fully supported.

**Implementation:**
- The fallback parser extracts `ExtractedExtendedProperty` from `sp_addextendedproperty` calls
- The model builder converts these to `ExtendedPropertyElement` structs
- The `write_extended_property()` function in model_xml.rs emits the XML elements

**Example output:**
```xml
<Element Type="SqlExtendedProperty" Name="[dbo].[TableName].[MS_Description]">
  <Property Name="Value">
    <Value><![CDATA[Description text]]></Value>
  </Property>
  <Relationship Name="ExtendedObject">
    <Entry>
      <References Name="[dbo].[TableName]"/>
    </Entry>
  </Relationship>
</Element>
```

For column-level properties, the Name includes the column: `[dbo].[TableName].[ColumnName].[MS_Description]`

---

## Metadata File Differences

### 6. DacMetadata.xml Root Element

- **Status:** [x] Completed
- **Severity:** Low

**Description:**
Fixed the root element name and empty Description handling to match dotnet behavior.

**Implementation:**
- Changed root element from `<DacMetadata>` to `<DacType>` (per MS XSD schema)
- Empty `<Description>` element is now omitted (matches dotnet behavior)

---

### 7. Origin.xml Format Differences

- **Status:** [ ] Not started
- **Severity:** Low

**Description:**
Several structural differences in Origin.xml:

| Aspect | Rust | Dotnet |
|--------|------|--------|
| ProductSchema | `<ProductSchema><MajorVersion Value="160"/></ProductSchema>` | `<ProductSchema>http://schemas.microsoft.com/...</ProductSchema>` |
| Checksums position | Before Operation | After Operation |
| ProductName/Version | Not included | Included |
| ModelSchemaVersion | Not included | Included |

---

### 8. Content_Types.xml MIME Type

- **Status:** [x] Completed
- **Severity:** Low

**Description:**
Fixed the XML content type in `[Content_Types].xml` to match dotnet behavior.

**Implementation:**
- Changed `ContentType="application/xml"` to `ContentType="text/xml"` in `generate_content_types_xml()` function in `src/dacpac/packager.rs`

| Implementation | XML Content Type |
|----------------|------------------|
| Rust | `text/xml` |
| Dotnet | `text/xml` |

---

## Priority Order

1. **#1 - GO; Batch Separator** - ~~Critical, causes real data loss~~ ✓ Completed
2. **#5 - SqlExtendedProperty** - ~~Medium, already parsed but not emitted~~ ✓ Completed
3. **#6 - DacMetadata.xml Root Element** - ~~Low, cosmetic compatibility~~ ✓ Completed
4. **#8 - Content_Types.xml MIME Type** - ~~Low, cosmetic compatibility~~ ✓ Completed
5. **#7 - Origin.xml Format Differences** - Low, cosmetic compatibility
6. **#2-4 - Annotation/computed elements** - Low, deep analysis features

---

## Testing Strategy

After each fix:
1. Rebuild dacpac from reference project
2. Compare element counts with dotnet output
3. Verify specific affected tables/objects are now present
4. Run existing test suite to ensure no regressions
