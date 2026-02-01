## Project Overview

rust-sqlpackage is a Rust implementation of the SQL Server database project compiler. It converts `.sqlproj` files to `.dacpac` packages, providing faster builds than the .NET DacFx toolchain.

## Build and Development Commands

This project uses `just` as a command runner. Run `just --list` to see all available recipes.

```bash
# Build
just build                              # Debug build
just release                            # Release build (optimized)

# Run tests
just test                               # All tests
just test-one test_name                 # Single test by name

# Run the CLI
just run build --project path/to/Database.sqlproj
just build-project path/to/Database.sqlproj
```

Before finishing a feature or committing, always run `just test`.

## Architecture

The codebase follows a pipeline architecture:

```
.sqlproj (XML) → SqlProject → SQL Files → AST → DatabaseModel → XML → .dacpac (ZIP)
```

## Code Guidelines

**Prefer parser over regex/string manipulation** when working with SQL text. Use sqlparser-rs tokenization or AST methods rather than regex patterns or string methods like `trim_end_matches`. See `docs/PARSER_REFACTORING_GUIDE.md` for patterns.

**Important:** Instead of using regex use the AST/Tokenization approach instead.

### Module Responsibilities

| Module | Path | Purpose |
|--------|------|---------|
| **project** | `src/project/` | Parse `.sqlproj` XML, extract SQL file paths and settings |
| **parser** | `src/parser/` | Parse T-SQL using sqlparser-rs, handle GO batch separators |
| **model** | `src/model/` | Transform AST into `DatabaseModel` with tables, views, constraints |
| **dacpac** | `src/dacpac/` | Generate model.xml, DacMetadata.xml, Origin.xml and package as ZIP |

### Data Flow

1. `parse_sqlproj()` reads XML, discovers SQL files (legacy or SDK-style glob patterns)
2. `parse_sql_file()` splits on GO, parses with sqlparser-rs or falls back to regex for procedures/functions
3. `build_model()` transforms AST statements into `ModelElement` variants (Table, View, Index, etc.)
4. `create_dacpac()` generates XML files and packages into ZIP

## Tests

**Important:** This project follows a TDD approach. Any new feature or bug fix must first have either a unit, integration or e2e test created for it.

See `TESTING.md` for detailed testing documentation including the parity testing strategy.

Test fixtures in `tests/fixtures/` are self-contained SQL projects.

## Local SQL Server for Testing

A compose file is provided in `docker/compose.yml` to run SQL Server locally.

```bash
# Start SQL Server
cd docker && podman-compose up -d

# Stop SQL Server
cd docker && podman-compose down
```

**Connection Details:**

| Property | Value |
|----------|-------|
| Host     | localhost |
| Port     | 1433 |
| User     | sa |
| Password | Password1 |

## Dacpac File Format

A `.dacpac` is a ZIP containing:
- `model.xml` - Database schema using MS namespace `http://schemas.microsoft.com/sqlserver/dac/Serialization/2012/02`
- `DacMetadata.xml` - Package metadata
- `Origin.xml` - Source/version info
- `[Content_Types].xml` - MIME types
