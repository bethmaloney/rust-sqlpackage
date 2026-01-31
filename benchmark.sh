#!/usr/bin/env bash
#
# Benchmark rust-sqlpackage vs DacFx (.NET build)
#
# Usage: ./benchmark.sh [fixture] [iterations]
#   fixture:    Test fixture name (default: stress_test)
#   iterations: Number of runs (default: 10)
#
# Example:
#   ./benchmark.sh stress_test 10
#   ./benchmark.sh e2e_comprehensive 5

set -euo pipefail

FIXTURE="${1:-stress_test}"
ITERATIONS="${2:-10}"
PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
FIXTURE_PATH="$PROJECT_DIR/tests/fixtures/$FIXTURE"
SQLPROJ="$FIXTURE_PATH/project.sqlproj"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== rust-sqlpackage vs DacFx Benchmark ===${NC}"
echo "Fixture: $FIXTURE"
echo "Project: $SQLPROJ"
echo "Iterations: $ITERATIONS"
echo ""

# Verify fixture exists
if [[ ! -f "$SQLPROJ" ]]; then
    echo -e "${RED}Error: Project file not found: $SQLPROJ${NC}"
    exit 1
fi

# Count SQL files
SQL_COUNT=$(find "$FIXTURE_PATH" -name "*.sql" | wc -l)
echo "SQL files: $SQL_COUNT"
echo ""

# Build rust-sqlpackage release binary
echo -e "${YELLOW}Building rust-sqlpackage (release)...${NC}"
cargo build --release --quiet
RUST_BIN="$PROJECT_DIR/target/release/rust-sqlpackage"

if [[ ! -x "$RUST_BIN" ]]; then
    echo -e "${RED}Error: rust-sqlpackage binary not found${NC}"
    exit 1
fi

# Create temp directory for outputs
TMPDIR=$(mktemp -d)
trap "rm -rf $TMPDIR" EXIT

# Arrays to store timing results
declare -a RUST_TIMES
declare -a DACFX_COLD_TIMES
declare -a DACFX_WARM_TIMES

# Function to calculate median
median() {
    local arr=("$@")
    local n=${#arr[@]}
    IFS=$'\n' sorted=($(sort -n <<<"${arr[*]}"))
    unset IFS
    local mid=$((n / 2))
    if (( n % 2 == 0 )); then
        echo "scale=3; (${sorted[$mid-1]} + ${sorted[$mid]}) / 2" | bc
    else
        echo "${sorted[$mid]}"
    fi
}

# Function to calculate min
min() {
    local arr=("$@")
    printf '%s\n' "${arr[@]}" | sort -n | head -1
}

# Function to calculate max
max() {
    local arr=("$@")
    printf '%s\n' "${arr[@]}" | sort -n | tail -1
}

# ============================================================================
# Benchmark rust-sqlpackage
# ============================================================================
echo -e "${GREEN}Benchmarking rust-sqlpackage...${NC}"

for ((i=1; i<=ITERATIONS; i++)); do
    OUTPUT="$TMPDIR/rust_$i.dacpac"
    START=$(date +%s%N)
    "$RUST_BIN" build --project "$SQLPROJ" --output "$OUTPUT" > /dev/null 2>&1
    END=$(date +%s%N)
    ELAPSED_MS=$(echo "scale=3; ($END - $START) / 1000000" | bc)
    RUST_TIMES+=("$ELAPSED_MS")
    printf "  Run %2d: %8.2f ms\n" "$i" "$ELAPSED_MS"
done

RUST_MEDIAN=$(median "${RUST_TIMES[@]}")
RUST_MIN=$(min "${RUST_TIMES[@]}")
RUST_MAX=$(max "${RUST_TIMES[@]}")

echo ""
echo -e "  ${GREEN}rust-sqlpackage Results:${NC}"
echo "    Median: ${RUST_MEDIAN} ms"
echo "    Min:    ${RUST_MIN} ms"
echo "    Max:    ${RUST_MAX} ms"
echo ""

# ============================================================================
# Benchmark DacFx (.NET build)
# ============================================================================
if command -v dotnet &> /dev/null; then
    echo -e "${GREEN}Benchmarking .NET DacFx...${NC}"

    # Cold builds (clean before each build)
    echo "  Cold builds (with clean):"
    for ((i=1; i<=ITERATIONS; i++)); do
        # Clean bin/obj directories
        rm -rf "$FIXTURE_PATH/bin" "$FIXTURE_PATH/obj" 2>/dev/null || true

        START=$(date +%s%N)
        dotnet build "$SQLPROJ" -c Release > /dev/null 2>&1
        END=$(date +%s%N)
        ELAPSED_MS=$(echo "scale=3; ($END - $START) / 1000000" | bc)
        DACFX_COLD_TIMES+=("$ELAPSED_MS")
        printf "    Run %2d: %8.2f ms\n" "$i" "$ELAPSED_MS"
    done

    DACFX_COLD_MEDIAN=$(median "${DACFX_COLD_TIMES[@]}")
    DACFX_COLD_MIN=$(min "${DACFX_COLD_TIMES[@]}")
    DACFX_COLD_MAX=$(max "${DACFX_COLD_TIMES[@]}")

    echo ""

    # Warm builds (no clean between builds)
    echo "  Warm builds (incremental):"
    # Initial build to warm cache
    dotnet build "$SQLPROJ" -c Release > /dev/null 2>&1

    for ((i=1; i<=ITERATIONS; i++)); do
        START=$(date +%s%N)
        dotnet build "$SQLPROJ" -c Release > /dev/null 2>&1
        END=$(date +%s%N)
        ELAPSED_MS=$(echo "scale=3; ($END - $START) / 1000000" | bc)
        DACFX_WARM_TIMES+=("$ELAPSED_MS")
        printf "    Run %2d: %8.2f ms\n" "$i" "$ELAPSED_MS"
    done

    DACFX_WARM_MEDIAN=$(median "${DACFX_WARM_TIMES[@]}")
    DACFX_WARM_MIN=$(min "${DACFX_WARM_TIMES[@]}")
    DACFX_WARM_MAX=$(max "${DACFX_WARM_TIMES[@]}")

    # Clean up DacFx build artifacts
    rm -rf "$FIXTURE_PATH/bin" "$FIXTURE_PATH/obj" 2>/dev/null || true

    echo ""
    echo -e "  ${GREEN}.NET DacFx Cold Build Results:${NC}"
    echo "    Median: ${DACFX_COLD_MEDIAN} ms"
    echo "    Min:    ${DACFX_COLD_MIN} ms"
    echo "    Max:    ${DACFX_COLD_MAX} ms"
    echo ""
    echo -e "  ${GREEN}.NET DacFx Warm Build Results:${NC}"
    echo "    Median: ${DACFX_WARM_MEDIAN} ms"
    echo "    Min:    ${DACFX_WARM_MIN} ms"
    echo "    Max:    ${DACFX_WARM_MAX} ms"
    echo ""

    # ============================================================================
    # Summary
    # ============================================================================
    echo -e "${BLUE}=== Summary ===${NC}"
    echo ""
    printf "| %-25s | %12s | %12s |\n" "Build Type" "Time (ms)" "vs rust-sqlpackage"
    printf "|%-27s|%14s|%20s|\n" "---------------------------" "--------------" "--------------------"
    printf "| %-25s | %12.2f | %18s |\n" "rust-sqlpackage" "$RUST_MEDIAN" "-"

    COLD_RATIO=$(echo "scale=1; $DACFX_COLD_MEDIAN / $RUST_MEDIAN" | bc)
    WARM_RATIO=$(echo "scale=1; $DACFX_WARM_MEDIAN / $RUST_MEDIAN" | bc)

    printf "| %-25s | %12.2f | %15.1fx slower |\n" ".NET DacFx (cold build)" "$DACFX_COLD_MEDIAN" "$COLD_RATIO"
    printf "| %-25s | %12.2f | %15.1fx slower |\n" ".NET DacFx (warm build)" "$DACFX_WARM_MEDIAN" "$WARM_RATIO"
    echo ""

    # Convert to seconds for README format
    RUST_SEC=$(echo "scale=2; $RUST_MEDIAN / 1000" | bc)
    DACFX_COLD_SEC=$(echo "scale=2; $DACFX_COLD_MEDIAN / 1000" | bc)
    DACFX_WARM_SEC=$(echo "scale=2; $DACFX_WARM_MEDIAN / 1000" | bc)

    echo -e "${YELLOW}README-ready format:${NC}"
    echo ""
    echo "Benchmarked on a ${SQL_COUNT}-file SQL project (${FIXTURE} fixture):"
    echo ""
    echo "| Build Type | Time | vs rust-sqlpackage |"
    echo "|------------|------|-------------------|"
    echo "| .NET DacFx (cold build) | ${DACFX_COLD_SEC}s | ${COLD_RATIO}x slower |"
    echo "| .NET DacFx (warm/incremental) | ${DACFX_WARM_SEC}s | ${WARM_RATIO}x slower |"
    echo "| **rust-sqlpackage** | **${RUST_SEC}s** | - |"
    echo ""
else
    echo -e "${YELLOW}Warning: dotnet not found, skipping DacFx benchmark${NC}"
    echo ""
    echo -e "${BLUE}=== rust-sqlpackage Only ===${NC}"
    RUST_SEC=$(echo "scale=2; $RUST_MEDIAN / 1000" | bc)
    echo ""
    echo "rust-sqlpackage median: ${RUST_SEC}s (${RUST_MEDIAN}ms)"
    echo ""
fi
