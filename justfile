# rust-sqlpackage justfile
# Run `just --list` to see available recipes

# Default recipe: run checks and tests
default: check test

# ============================================================================
# Building
# ============================================================================

# Build debug binary
build:
    cargo build

# Build release binary (optimized, stripped)
release:
    cargo build --release

# Clean build artifacts
clean:
    cargo clean

# ============================================================================
# Testing
# ============================================================================

# Run all tests (parity tests skip if dotnet unavailable, deploy tests skip if no SQL Server)
test:
    cargo test
    cargo test --test e2e_tests deploy -- --ignored

# Run a specific test by name
test-one NAME:
    cargo test {{NAME}}

# ============================================================================
# Code Quality
# ============================================================================

# Run all checks (fmt, clippy, test)
check: fmt-check lint test

# Run clippy linter
lint:
    cargo clippy -- -D warnings

# Check code formatting
fmt-check:
    cargo fmt -- --check

# Format code
fmt:
    cargo fmt

# ============================================================================
# Running
# ============================================================================

# Run CLI with arguments (e.g., just run build --project foo.sqlproj)
run *ARGS:
    cargo run -- {{ARGS}}

# Build a sqlproj file (e.g., just build-project path/to/Database.sqlproj)
build-project PROJECT:
    cargo run -- build --project {{PROJECT}}

# Build a sqlproj file with verbose output
build-project-verbose PROJECT:
    cargo run -- build --project {{PROJECT}} --verbose

# ============================================================================
# Development
# ============================================================================

# Watch for changes and run tests (requires cargo-watch)
watch:
    cargo watch -x test

# Watch for changes and run clippy (requires cargo-watch)
watch-lint:
    cargo watch -x clippy

# Generate documentation
doc:
    cargo doc --no-deps --open

# ============================================================================
# CI
# ============================================================================

# Run CI checks (what CI should run)
ci: fmt-check lint test

# ============================================================================
# Benchmarking
# ============================================================================

# Run comparison benchmark (rust-sqlpackage vs DacFx)
benchmark FIXTURE="stress_test" ITERATIONS="10":
    ./benchmark.sh {{FIXTURE}} {{ITERATIONS}}

# Run Criterion benchmarks
bench:
    cargo bench
