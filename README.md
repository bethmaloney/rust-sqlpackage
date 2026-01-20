# rust-sqlpackage

A fast Rust compiler for SQL Server database projects. Compiles `.sqlproj` files to `.dacpac` packages.

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
| Tables | ✅ Full | Columns, data types, nullable, defaults, identity |
| Views | ✅ Full | Definition preserved as-is |
| Stored Procedures | ✅ Partial | Schema/name extracted; parameters not parsed |
| Functions | ✅ Partial | Scalar, table-valued, inline; parameters not parsed |
| Indexes | ✅ Full | Clustered/nonclustered, unique, INCLUDE columns |
| Schemas | ✅ Full | Auto-created for all objects |
| Sequences | ✅ Full | CREATE SEQUENCE statements |
| User-Defined Types | ✅ Full | Table types and custom types |

### Constraints

- Primary Key, Foreign Key, Unique, Check, Default

### Deployment Scripts

- Pre-deployment and post-deployment scripts
- SQLCMD `:r` include directive (with nested includes)
- SQLCMD `:setvar` variable substitution

### Project File Features

- Legacy `<Build Include="">` items
- SDK-style glob patterns (`**/*.sql`)
- `<Build Remove="">` exclusions
- `<ArtifactReference>` dacpac references
- Target platform detection (Sql130-Sql160)

### Not Yet Supported

- Triggers
- Synonyms
- Assembly/CLR objects
- Full-text catalogs/indexes
- Extended properties
- Full procedure/function parameter extraction

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

## Output Format

Generates standard `.dacpac` packages (ZIP files containing `model.xml`, `DacMetadata.xml`, `Origin.xml`) compatible with:

- SQL Server Management Studio
- Azure Data Studio
- SqlPackage CLI
- DacFx API
- SQL Server 2016-2022

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
