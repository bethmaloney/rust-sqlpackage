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

## Supported SQL Statements

- `CREATE TABLE` (with columns, constraints, indexes)
- `CREATE VIEW`
- `CREATE INDEX`
- `CREATE SCHEMA`
- Primary Key, Foreign Key, Unique, Check constraints

### Coming Soon

- `CREATE PROCEDURE`
- `CREATE FUNCTION`
- Triggers

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

Generates standard `.dacpac` packages compatible with:
- SQL Server Management Studio
- Azure Data Studio
- SqlPackage CLI
- DacFx API

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
