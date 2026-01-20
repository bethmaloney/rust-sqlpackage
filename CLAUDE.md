## Project Overview

rust-sqlpackage is a Rust implementation of the SQL Server database project compiler. It converts `.sqlproj` files to `.dacpac` packages, providing faster builds than the .NET DacFx toolchain.

## Build and Development Commands

```bash
# Build
cargo build
cargo build --release

# Run tests
cargo test                              # All tests
cargo test --test integration_tests     # Integration tests only
cargo test --test unit_tests            # Unit tests only
cargo test test_name                    # Single test by name

# Lint and format
cargo clippy
cargo fmt

# Run the CLI
cargo run -- build --project path/to/Database.sqlproj
cargo run -- build --project path/to/Database.sqlproj --output out.dacpac --verbose
```

## Architecture

The codebase follows a pipeline architecture:

```
.sqlproj (XML) → SqlProject → SQL Files → AST → DatabaseModel → XML → .dacpac (ZIP)
```

### Module Responsibilities

| Module | Path | Purpose |
|--------|------|---------|
| **project** | `src/project/` | Parse `.sqlproj` XML, extract SQL file paths and settings |
| **parser** | `src/parser/` | Parse T-SQL using sqlparser-rs, handle GO batch separators |
| **model** | `src/model/` | Transform AST into `DatabaseModel` with tables, views, constraints |
| **dacpac** | `src/dacpac/` | Generate model.xml, DacMetadata.xml, Origin.xml and package as ZIP |

### Key Entry Points

- **CLI**: `src/main.rs` - clap-based argument parsing
- **Library**: `src/lib.rs` - `build_dacpac(options: BuildOptions) -> Result<PathBuf>`

### Data Flow

1. `parse_sqlproj()` reads XML, discovers SQL files (legacy or SDK-style glob patterns)
2. `parse_sql_file()` splits on GO, parses with sqlparser-rs or falls back to regex for procedures/functions
3. `build_model()` transforms AST statements into `ModelElement` variants (Table, View, Index, etc.)
4. `create_dacpac()` generates XML files and packages into ZIP

## Test Fixtures

Test fixtures in `tests/fixtures/` are self-contained SQL projects:

| Fixture | Tests |
|---------|-------|
| `simple_table/` | Basic single table |
| `constraints/` | PK, FK, unique, check constraints |
| `indexes/` | Index definitions with clustered/nonclustered indexes |
| `views/` | View definitions |
| `pre_post_deploy/` | Deployment scripts |
| `build_with_exclude/` | SDK-style project with exclusions |

## Dacpac File Format

A `.dacpac` is a ZIP containing:
- `model.xml` - Database schema using MS namespace `http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02`
- `DacMetadata.xml` - Package metadata
- `Origin.xml` - Source/version info
- `[Content_Types].xml` - MIME types

Compatible with SSMS, Azure Data Studio, SqlPackage CLI, and DacFx API.
