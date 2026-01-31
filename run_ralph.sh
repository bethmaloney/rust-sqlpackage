#!/bin/bash

# Script to repeatedly run Claude Code with IMPLEMENTATION_PROMPT.md
# Exits early if "ALL TODO ITEMS COMPLETE" is detected

set -e

PROMPT_FILE="docs/IMPLEMENTATION_PROMPT.md"
MAX_ITERATIONS=20
ITERATION=0
CURRENT_BRANCH=$(git branch --show-current)

# Ensure prompt file exists
if [ ! -f "$PROMPT_FILE" ]; then
    echo "Error: Prompt file not found: $PROMPT_FILE"
    exit 1
fi

echo "Starting implementation loop on branch: $CURRENT_BRANCH"
echo "Max iterations: $MAX_ITERATIONS"
echo "Prompt file: $PROMPT_FILE"
echo ""

while true; do
    ITERATION=$((ITERATION + 1))
    echo -e "\n======================== ITERATION $ITERATION of $MAX_ITERATIONS ========================\n"

    if [ $ITERATION -gt $MAX_ITERATIONS ]; then
        echo "Reached max iterations: $MAX_ITERATIONS"
        break
    fi

    # Run Claude Code iteration
    # -p: Headless mode (non-interactive, reads from stdin)
    # --dangerously-skip-permissions: Auto-approve all tool calls
    # --output-format=stream-json: Structured output for logging/monitoring
    # --model opus: Use Opus for complex reasoning
    # claude-stream-format: Converts stream-json to readable output with emoji indicators
    OUTPUT=$(cat "$PROMPT_FILE" | claude -p \
        --dangerously-skip-permissions \
        --output-format=stream-json \
        --model opus \
        --verbose \
        2>&1 | tee >(claude-stream-format > /dev/stderr))

    # Check for completion signal
    if echo "$OUTPUT" | grep -q "ALL TODO ITEMS COMPLETE"; then
        echo -e "\n======================== SUCCESS ========================"
        echo "Detected 'ALL TODO ITEMS COMPLETE' - exiting loop"

        # Final push
        git push origin "$CURRENT_BRANCH" 2>/dev/null || git push -u origin "$CURRENT_BRANCH"

        echo "All done!"
        exit 0
    fi

    # Run clippy check and fix any errors
    echo -e "\n------------------------ CLIPPY CHECK ------------------------"
    CLIPPY_RETRIES=0
    MAX_CLIPPY_RETRIES=3
    CLIPPY_PASSED=false

    if cargo clippy --all-targets --all-features -- -D warnings 2>&1; then
        CLIPPY_PASSED=true
    else
        while [ $CLIPPY_RETRIES -lt $MAX_CLIPPY_RETRIES ]; do
            CLIPPY_RETRIES=$((CLIPPY_RETRIES + 1))
            echo -e "\nClippy failed (attempt $CLIPPY_RETRIES of $MAX_CLIPPY_RETRIES)"

            # Capture clippy errors and send to Claude for fixing
            CLIPPY_OUTPUT=$(cargo clippy --all-targets --all-features -- -D warnings 2>&1 || true)

            echo -e "\nLaunching Claude to fix clippy errors..."
            echo "Fix all clippy errors and warnings. Here is the clippy output:

\`\`\`
$CLIPPY_OUTPUT
\`\`\`

Run \`cargo clippy --all-targets --all-features -- -D warnings\` to verify fixes. Commit any changes with an appropriate message." | claude -p \
                --dangerously-skip-permissions \
                --output-format=stream-json \
                --model sonnet \
                --verbose \
                2>&1 | tee >(claude-stream-format > /dev/stderr)

            echo -e "\nRetrying clippy..."
            if cargo clippy --all-targets --all-features -- -D warnings 2>&1; then
                CLIPPY_PASSED=true
                break
            fi
        done
    fi

    if [ "$CLIPPY_PASSED" = true ]; then
        echo -e "------------------------ CLIPPY PASSED ------------------------\n"
    else
        echo -e "------------------------ CLIPPY FAILED (continuing anyway) ------------------------\n"
    fi

    # Push changes after each iteration
    echo -e "\nPushing changes..."
    git push origin "$CURRENT_BRANCH" 2>/dev/null || {
        echo "Creating remote branch..."
        git push -u origin "$CURRENT_BRANCH"
    }

    echo -e "\nIteration $ITERATION complete. Continuing..."
done

echo "Loop finished without completion signal."
exit 1
