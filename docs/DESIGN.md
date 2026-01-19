# rust-sqlpackage Design Document

## Overview

rust-sqlpackage is a Rust implementation of the SQL Server database project compiler, converting `.sqlproj` files to `.dacpac` packages. This provides a significant performance improvement over the .NET-based DacFx toolchain.

## Goals

1. **Performance**: Sub-second build times for large database projects
2. **Compatibility**: Generate dacpac files deployable via standard SQL Server tooling
3. **Simplicity**: Focus on common DDL/DML statements used in typical projects

## Non-Goals

- Full T-SQL language coverage (CLR, external tables, etc.)
- Dacpac deployment (use SqlPackage for that)
- Schema comparison/diff

---

## Reference Documentation

### Official Microsoft Specifications

| Resource | URL |
|----------|-----|
| DACPAC File Format Spec | https://learn.microsoft.com/en-us/openspecs/sql_data_portability/ms-dacpac/e539cf5f-67bb-4756-a11f-0b7704791bbd |
| DACPAC Introduction | https://learn.microsoft.com/en-us/openspecs/sql_data_portability/ms-dacpac/c62984e2-0ab5-430d-b0e1-9b38835cc244 |
| Unpack DACPAC File | https://learn.microsoft.com/en-us/sql/tools/sql-database-projects/concepts/data-tier-applications/unpack-dacpac-file |

### Microsoft GitHub Repositories

| Repository | Description | URL |
|------------|-------------|-----|
| DacFx | Official DacFx SDK and Microsoft.Build.Sql | https://github.com/microsoft/DacFx |
| SqlScriptDOM | T-SQL parser (open source) | https://github.com/microsoft/SqlScriptDOM |
| SqlParser | SQL binding library (closed source) | https://github.com/microsoft/SqlParser |

### Rust Dependencies

| Crate | Purpose | URL |
|-------|---------|-----|
| sqlparser | T-SQL parsing | https://docs.rs/sqlparser/ |
| quick-xml | XML generation | https://docs.rs/quick-xml/ |
| roxmltree | XML parsing | https://docs.rs/roxmltree/ |
| zip | ZIP file creation | https://docs.rs/zip/ |
| clap | CLI argument parsing | https://docs.rs/clap/ |

---

## Architecture

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   .sqlproj      │────▶│   SqlProject    │────▶│  SQL Files      │
│   (XML)         │     │   (parsed)      │     │  (.sql)         │
└─────────────────┘     └─────────────────┘     └────────┬────────┘
                                                         │
                                                         ▼
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   .dacpac       │◀────│  DatabaseModel  │◀────│  SQL AST        │
│   (ZIP)         │     │  (internal)     │     │  (sqlparser)    │
└─────────────────┘     └─────────────────┘     └─────────────────┘
```

### Components

#### 1. Project Parser (`src/project/`)
- Parses `.sqlproj` XML files
- Extracts SQL file paths, target platform, references
- Supports both legacy and SDK-style projects

#### 2. T-SQL Parser (`src/parser/`)
- Uses `sqlparser-rs` crate with MSSQL dialect
- Handles GO batch separators
- Produces AST for each SQL statement

#### 3. Model Builder (`src/model/`)
- Transforms SQL AST into internal database model
- Extracts tables, views, indexes, constraints
- Resolves schema references

#### 4. Dacpac Generator (`src/dacpac/`)
- Generates `model.xml` with database schema
- Generates `DacMetadata.xml` with package info
- Generates `Origin.xml` with source info
- Packages all into ZIP file

---

## DACPAC File Format

A `.dacpac` file is a ZIP archive containing:

```
Database.dacpac
├── model.xml           # Database schema (required)
├── DacMetadata.xml     # Package metadata (required)
├── Origin.xml          # Source information (required)
└── [Content_Types].xml # MIME types (required)
```

### model.xml Schema

Uses namespace: `http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02`

```xml
<DataSchemaModel
  FileFormatVersion="1.2"
  SchemaVersion="2.9"
  DspName="Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider"
  CollationLcid="1033"
  CollationCaseSensitive="False">
  <Model>
    <Element Type="SqlTable" Name="[dbo].[Users]">
      <Relationship>
        <Attribute Name="Name" Value="Schema"/>
        <Entry><References Name="[dbo]"/></Entry>
      </Relationship>
      <Relationship>
        <Attribute Name="Name" Value="Columns"/>
        <Entry>
          <Element Type="SqlSimpleColumn" Name="[dbo].[Users].[Id]">
            <Property Name="IsNullable" Value="False"/>
            <!-- Type specifier -->
          </Element>
        </Entry>
      </Relationship>
    </Element>
  </Model>
</DataSchemaModel>
```

### Supported Element Types

| Type | Description |
|------|-------------|
| `SqlSchema` | Database schema |
| `SqlTable` | Table definition |
| `SqlView` | View definition |
| `SqlSimpleColumn` | Table column |
| `SqlPrimaryKeyConstraint` | Primary key |
| `SqlForeignKeyConstraint` | Foreign key |
| `SqlUniqueConstraint` | Unique constraint |
| `SqlCheckConstraint` | Check constraint |
| `SqlIndex` | Index definition |
| `SqlProcedure` | Stored procedure |
| `SqlScalarFunction` | Scalar function |
| `SqlTableValuedFunction` | Table-valued function |

---

## SQL Server Version Support

| Version | DSP Name | Notes |
|---------|----------|-------|
| SQL Server 2016 | `Sql130DatabaseSchemaProvider` | |
| SQL Server 2017 | `Sql140DatabaseSchemaProvider` | |
| SQL Server 2019 | `Sql150DatabaseSchemaProvider` | |
| SQL Server 2022 | `Sql160DatabaseSchemaProvider` | Default |

---

## Known Limitations

### sqlparser-rs Limitations

The `sqlparser` crate has good but incomplete MSSQL support:

1. **CREATE FUNCTION** - Complex struct, varies by version
2. **CREATE PROCEDURE** - Parameter handling differs
3. **MERGE statements** - Limited support
4. **OUTPUT clause** - May not parse correctly
5. **T-SQL specific syntax** - Some edge cases

### Workarounds

For unsupported statements, we:
1. Store raw SQL in the model
2. Extract schema/name via regex if needed
3. Skip parsing details, preserve definition

---

## Testing Strategy

### Unit Tests
- Parser tests for each SQL statement type
- Model builder tests for AST transformation
- XML generation tests for schema compliance

### Integration Tests
- Compare output with .NET-generated dacpac
- Deploy to SQL Server and verify schema
- Round-trip: build → deploy → extract → compare

### Performance Benchmarks
- Measure build time vs .NET DacFx
- Track memory usage
- Profile hot paths

---

## Future Enhancements

1. **Stored Procedure/Function Support**
   - Parse CREATE PROCEDURE/FUNCTION
   - Extract parameters and return types

2. **Schema Validation**
   - Validate against official XSD
   - Compare with reference dacpacs

3. **Incremental Builds**
   - Cache parsed SQL AST
   - Only rebuild changed files

4. **Watch Mode**
   - Auto-rebuild on file changes
   - Fast feedback loop

5. **LSP Integration**
   - Provide diagnostics to editors
   - Syntax highlighting support

---

## References

- [DACFx 3.0 File Formats](https://www.sqlskills.com/blogs/bobb/dacfx-3-0-the-new-file-formats/) - Bob Beauchemin's analysis
- [ScriptDOM Announcement](https://techcommunity.microsoft.com/blog/azuresqlblog/scriptdom-net-library-for-t-sql-parsing-is-now-open-source/3804284) - Microsoft open-sourcing T-SQL parser
- [sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs) - Apache DataFusion SQL parser
