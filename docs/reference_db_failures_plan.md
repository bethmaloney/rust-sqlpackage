# Reference Database Comparison: Failures and Differences

This document tracks all identified differences between rust-sqlpackage output and the reference dotnet DacFx tool output when building the same SQL project.

## Status Legend

- [ ] Not started
- [x] Completed
- [~] In progress

---

## Critical Bugs

### 1. `GO;` Batch Separator Not Handled

- **Status:** [ ] Not started
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

- **Status:** [ ] Not started
- **Severity:** Low

**Description:**
Dotnet emits `SqlInlineConstraintAnnotation` elements on columns that have inline constraints (e.g., default values defined inline). These annotations track which constraints were defined inline vs as named table-level constraints. Rust-sqlpackage does not emit these annotations.

**Example in dotnet output:**
```xml
<Element Type="SqlSimpleColumn" Name="[dbo].[Table].[Column]">
    <Annotation Type="SqlInlineConstraintAnnotation" Disambiguator="123" />
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

- **Status:** [ ] Not started
- **Severity:** Medium

**Description:**
Extended properties defined via `sp_addextendedproperty` (e.g., `MS_Description` for documentation) are parsed by the fallback parser but not emitted to model.xml.

**Example SQL:**
```sql
EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'Description text',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'TableName',
    @level2type = N'COLUMN', @level2name = N'ColumnName';
```

**Note:** The parser already extracts this information (`ExtractedExtendedProperty`), but it's not being written to the model.xml output.

---

## Metadata File Differences

### 6. DacMetadata.xml Root Element

- **Status:** [ ] Not started
- **Severity:** Low

**Description:**
The root element name differs between implementations:

| Implementation | Root Element |
|----------------|--------------|
| Rust | `<DacMetadata>` |
| Dotnet | `<DacType>` |

Rust also includes an empty `<Description>` element that dotnet omits.

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

- **Status:** [ ] Not started
- **Severity:** Low

**Description:**
Different content type used for XML files:

| Implementation | XML Content Type |
|----------------|------------------|
| Rust | `application/xml` |
| Dotnet | `text/xml` |

---

## Priority Order

1. **#1 - GO; Batch Separator** - Critical, causes real data loss
2. **#5 - SqlExtendedProperty** - Medium, already parsed but not emitted
3. **#6-8 - Metadata differences** - Low, cosmetic compatibility
4. **#2-4 - Annotation/computed elements** - Low, deep analysis features

---

## Testing Strategy

After each fix:
1. Rebuild dacpac from reference project
2. Compare element counts with dotnet output
3. Verify specific affected tables/objects are now present
4. Run existing test suite to ensure no regressions
