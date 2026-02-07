# rust-sqlpackage

A fast Rust compiler for SQL Server database projects. Compiles `.sqlproj` files to `.dacpac` packages with 100% schema parity to Microsoft's DacFx toolchain.

## Performance

Benchmarked on a 135-file SQL project (stress_test fixture):

| Build Type | Time | vs rust-sqlpackage |
|------------|------|-------------------|
| .NET DacFx (cold build) | 4.14s | 116x slower |
| .NET DacFx (warm/incremental) | 1.51s | 42x slower |
| **rust-sqlpackage** | **0.04s** | - |

- **Cold build**: Full rebuild after cleaning bin/obj directories
- **Warm build**: Incremental build with no source changes

rust-sqlpackage produces identical dacpac output regardless of prior build state.

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
./target/release/rust-sqlpackage --help
```

## Usage

```bash
# Basic build
rust-sqlpackage build --project path/to/Database.sqlproj

# With options
rust-sqlpackage build \
  --project Database.sqlproj \
  --output bin/Release/Database.dacpac \
  --target-platform Sql160 \
  --verbose
```

### Options

| Flag | Description |
|------|-------------|
| `-p, --project` | Path to the .sqlproj file (required) |
| `-o, --output` | Output path for .dacpac (default: `bin/Debug/<name>.dacpac`) |
| `-t, --target-platform` | SQL Server version: Sql130, Sql140, Sql150, Sql160 (default: Sql160) |
| `-v, --verbose` | Enable verbose output |

### Comparing Dacpacs

The `compare` command lets you verify that rust-sqlpackage produces identical output to .NET DacFx for your project. Build your `.sqlproj` with both tools, then compare the resulting dacpacs:

```bash
rust-sqlpackage compare rust-output.dacpac dotnet-output.dacpac
```

This performs a semantic comparison of the two dacpac files, checking:

- **model.xml** - Element-by-element schema comparison (order-independent)
- **DacMetadata.xml / [Content_Types].xml** - XML structure comparison
- **predeploy.sql / postdeploy.sql** - Text comparison
- **Unexpected files** - Detects files present in one dacpac but not the other

The command exits with code 0 if the dacpacs are equivalent, or code 1 if differences are found.

## Supported Features

### SQL Objects

| Object | Support Level | Notes |
|--------|---------------|-------|
| Tables | Full | Columns, data types, nullable, defaults, identity, ROWGUIDCOL, SPARSE, FILESTREAM, computed columns, column COLLATE |
| Views | Full | Definition preserved, SCHEMABINDING, CHECK OPTION, VIEW_METADATA |
| Stored Procedures | Full | Schema/name/definition extracted; parameters stored as-is; NATIVE_COMPILATION detected |
| Functions | Full | Scalar, table-valued (inline and multi-statement); parameters stored as-is; NATIVE_COMPILATION detected |
| Indexes | Full | Clustered/nonclustered, unique, INCLUDE, filtered, fill factor, PAD_INDEX, compression (ROW, PAGE, COLUMNSTORE, COLUMNSTORE_ARCHIVE) |
| Columnstore Indexes | Full | CREATE CLUSTERED/NONCLUSTERED COLUMNSTORE INDEX, DATA_COMPRESSION, filtered |
| Schemas | Full | Auto-created for all objects, AUTHORIZATION clause |
| Sequences | Full | All options (START, INCREMENT, MIN/MAX, CYCLE, CACHE) |
| User-Defined Types | Full | Table types with columns/constraints, scalar/alias types |
| Synonyms | Full | CREATE SYNONYM with 1-part through 4-part target names, cross-database references |
| Temporal Tables | Full | SYSTEM_VERSIONING, PERIOD FOR SYSTEM_TIME, history table references, GENERATED ALWAYS columns |
| Security Objects | Full | CREATE USER, CREATE ROLE, ALTER ROLE ADD/DROP MEMBER, GRANT/DENY/REVOKE permissions |
| Triggers | Full | DML triggers (INSERT, UPDATE, DELETE), AFTER and INSTEAD OF |
| Full-Text Catalogs | Full | CREATE FULLTEXT CATALOG with all options |
| Full-Text Indexes | Full | Language specifications, change tracking, stoplist |
| Extended Properties | Full | sp_addextendedproperty at table, column, and object levels |
| Filegroups | Full | ALTER DATABASE ADD FILEGROUP, MEMORY_OPTIMIZED_DATA |
| Partition Functions | Full | RANGE LEFT/RIGHT, boundary values, all data types |
| Partition Schemes | Full | Partition function reference, filegroup mappings |
| Graph Tables | Full | CREATE TABLE AS NODE / AS EDGE |
| Dynamic Data Masking | Full | MASKED WITH (FUNCTION = 'default()/email()/partial()/random()') on columns |

### Constraints

| Constraint | Support Level | Notes |
|------------|---------------|-------|
| Primary Key | Full | Clustered (default) or nonclustered, composite keys |
| Foreign Key | Full | Single and composite columns, referential actions |
| Unique | Full | Clustered or nonclustered |
| Check | Full | Column-level and table-level |
| Default | Full | Named and inline, DEFAULT FOR syntax |

### Deployment Scripts

- Pre-deployment and post-deployment scripts
- SQLCMD `:r` include directive (with nested includes)
- SQLCMD `:setvar` variable substitution

### Project File Features

- Legacy `<Build Include="">` items
- SDK-style glob patterns (`**/*.sql`)
- `<Build Remove="">` exclusions
- `<ArtifactReference>` dacpac references
- `<PackageReference>` NuGet packages (e.g., Microsoft.SqlServer.Dacpacs.Master)
- Target platform detection (Sql130-Sql160)
- SQLCMD variables with default values
- Database options (collation, ANSI settings, page verify mode, etc.)

### Not Yet Supported

These features are supported by .NET DacFx but not yet implemented:

| Feature | Notes |
|---------|-------|
| Assembly/CLR Objects | CLR-based functions, procedures, and triggers |
| External Tables | External data sources and tables |
| Memory-Optimized Tables | WITH (MEMORY_OPTIMIZED = ON) table option (filegroups are supported) |
| XML Indexes | CREATE XML INDEX (primary and secondary) |
| Spatial Indexes | CREATE SPATIAL INDEX |
| Service Broker | Queues, services, contracts, message types |
| Database-level Triggers | DDL triggers (DML triggers are supported) |
| Row-Level Security | CREATE SECURITY POLICY |
| Always Encrypted | ENCRYPTED WITH on columns |
| Ledger Tables | WITH (LEDGER = ON) |

**Note on silently skipped statements:** Server-level security objects (CREATE LOGIN, certificates, keys, credentials) and `ALTER DATABASE SCOPED CONFIGURATION` statements will **not cause build errors**. These are silently skipped during compilation, consistent with DacFx behavior (which also does not include these in dacpac output).

### Known Limitations

These are intentional differences from .NET DacFx output that don't affect deployment:

| Limitation | Impact | Notes |
|------------|--------|-------|
| Internal procedure objects | None | DacFx tracks temp tables, CTEs, and table variables inside procedures as `SqlDynamicColumnSource` elements. These are for code analysis only and not required for deployment. |
| External dacpac column resolution | Minimal | Column references to tables defined in `<PackageReference>` dacpacs are not schema-aware. Unqualified columns may not resolve correctly if a local table has the same column name. Use explicit table aliases or qualified column names when referencing external package tables. |

### CLI Limitations vs SqlPackage

This tool supports the `build` and `compare` actions. The following SqlPackage actions are not implemented:

- `deploy` - Deploy dacpac to database
- `extract` - Extract schema from database to dacpac
- `script` - Generate deployment script
- `publish` - Publish with deployment report
- `drift-report` - Compare database to dacpac

### No Code Analysis

Unlike .NET DacFx, this tool does not perform static code analysis or validation. It will not warn about:

- Missing object references (e.g., a stored procedure calling a non-existent procedure)
- Unresolved column or table references in queries
- Type mismatches in expressions
- Deprecated syntax usage
- Best practice violations

The tool focuses purely on schema extraction and dacpac generation. If your SQL compiles with DacFx, it will work here, but you won't get the same build-time warnings.

## Project File Support

Supports both legacy and SDK-style `.sqlproj` files:

```xml
<!-- Legacy style -->
<ItemGroup>
  <Build Include="Tables/Users.sql" />
</ItemGroup>

<!-- SDK style (auto-glob) -->
<!-- All .sql files are included automatically -->
```

**Note:** Legacy `.sqlproj` format (non-SDK style) is supported on a best-effort basis. SDK-style projects are recommended for full compatibility.

## Output Format

Generates standard `.dacpac` packages (ZIP files containing `model.xml`, `DacMetadata.xml`, `Origin.xml`, `[Content_Types].xml`) compatible with:

- SQL Server Management Studio
- Azure Data Studio
- SqlPackage CLI
- DacFx API
- SQL Server 2016-2022

### Parity Status

This project achieves 100% schema parity with Microsoft DacFx across 44+ test fixtures:

- **Element Inventory**: All element types, names, and counts match
- **Property Comparison**: All element properties match exactly
- **SqlPackage Equivalence**: SqlPackage reports zero schema differences when comparing outputs

## Development

```bash
# Run tests
cargo test

# Check for issues
cargo clippy

# Format code
cargo fmt
```

## License

MIT
