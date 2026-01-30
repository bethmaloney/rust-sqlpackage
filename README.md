# rust-sqlpackage

A fast Rust compiler for SQL Server database projects. Compiles `.sqlproj` files to `.dacpac` packages with 100% schema parity to Microsoft's DacFx toolchain.

## Performance

| Tool | Build Time |
|------|------------|
| .NET DacFx (cold) | ~30 seconds |
| rust-sqlpackage | ~2 seconds |

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

## Supported Features

### SQL Objects

| Object | Support Level | Notes |
|--------|---------------|-------|
| Tables | Full | Columns, data types, nullable, defaults, identity, ROWGUIDCOL, SPARSE, FILESTREAM, computed columns |
| Views | Full | Definition preserved, SCHEMABINDING, CHECK OPTION, VIEW_METADATA |
| Stored Procedures | Full | Schema/name/definition extracted; parameters stored as-is |
| Functions | Full | Scalar, table-valued (inline and multi-statement); parameters stored as-is |
| Indexes | Full | Clustered/nonclustered, unique, INCLUDE, filtered, fill factor, compression (ROW, PAGE, COLUMNSTORE) |
| Schemas | Full | Auto-created for all objects, AUTHORIZATION clause |
| Sequences | Full | All options (START, INCREMENT, MIN/MAX, CYCLE, CACHE) |
| User-Defined Types | Full | Table types with columns/constraints, scalar/alias types |
| Triggers | Full | DML triggers (INSERT, UPDATE, DELETE), AFTER and INSTEAD OF |
| Full-Text Catalogs | Full | CREATE FULLTEXT CATALOG with all options |
| Full-Text Indexes | Full | Language specifications, change tracking, stoplist |
| Extended Properties | Full | sp_addextendedproperty at table, column, and object levels |

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
| Synonyms | CREATE SYNONYM statements |
| Assembly/CLR Objects | CLR-based functions and procedures |
| External Tables | External data sources and tables |
| Temporal Tables | System-versioned tables |
| Graph Tables | NODE and EDGE tables (syntax recognized but limited support) |
| Security Objects | Users, roles, permissions, certificates |
| Service Broker | Queues, services, contracts, message types |
| Database-level Triggers | DDL triggers |
| Partition Functions/Schemes | Table partitioning |

### CLI Limitations vs SqlPackage

This tool only supports the `build` action. The following SqlPackage actions are not implemented:

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
