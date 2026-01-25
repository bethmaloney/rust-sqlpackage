#!/bin/bash

# Script to repeatedly run Claude Code with fix_tests_prompt.md
# Exits early if "ALL TODO ITEMS COMPLETE" is detected

set -e

PROMPT_FILE="docs/fix_tests_prompt.md"
MAX_ITERATIONS=20
ITERATION=0
CURRENT_BRANCH=$(git branch --show-current)

# Ensure prompt file exists
if [ ! -f "$PROMPT_FILE" ]; then
    echo "Error: Prompt file not found: $PROMPT_FILE"
    exit 1
fi

echo "Starting fix_tests loop on branch: $CURRENT_BRANCH"
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
