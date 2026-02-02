#!/bin/bash

# Script to repeatedly run Claude Code with IMPLEMENTATION_PROMPT.md
# Exits early if "ALL TODO ITEMS COMPLETE" is detected

set -e

PROMPT_FILE="docs/IMPLEMENTATION_PROMPT.md"
MAX_ITERATIONS=40
ITERATION=0
CURRENT_BRANCH=$(git branch --show-current)

# Function to push with rebase and conflict resolution
push_with_rebase() {
    local max_rebase_retries=3
    local rebase_retry=0

    while [ $rebase_retry -lt $max_rebase_retries ]; do
        # Try to push
        if git push origin "$CURRENT_BRANCH" 2>/dev/null; then
            return 0
        fi

        # Push failed, try to set upstream if needed
        if git push -u origin "$CURRENT_BRANCH" 2>/dev/null; then
            return 0
        fi

        rebase_retry=$((rebase_retry + 1))
        echo -e "\nPush failed (attempt $rebase_retry of $max_rebase_retries). Trying pull --rebase..."

        # Fetch latest
        git fetch origin "$CURRENT_BRANCH"

        # Try rebase
        if git pull --rebase origin "$CURRENT_BRANCH" 2>&1; then
            echo "Rebase successful, retrying push..."
            continue
        fi

        # Check if there are conflicts
        if git status | grep -q "Unmerged paths\|both modified\|both added"; then
            echo -e "\nMerge conflicts detected. Launching Claude to resolve..."

            CONFLICT_FILES=$(git diff --name-only --diff-filter=U)
            CONFLICT_STATUS=$(git status)

            echo "Resolve all merge conflicts in the following files. The rebase is in progress.

Git status:
\`\`\`
$CONFLICT_STATUS
\`\`\`

Conflicting files:
$CONFLICT_FILES

For each file:
1. Carefully review the conflicts.
2. Resolve the conflicts making sure to preserve all intended changes.
3. Remove all conflict markers (e.g., \`<<<<<<<\`, \`=======\`, \`>>>>>>>\`).
4. Stage the resolved file with \`git add <file>\`

After resolving ALL conflicts, run \`git rebase --continue\` to complete the rebase.
Do NOT commit directly - the rebase will create the commit." | claude -p \
                --dangerously-skip-permissions \
                --output-format=stream-json \
                --model sonnet \
                --verbose \
                2>&1 | tee >(claude-stream-format > /dev/stderr)

            # Check if rebase completed
            if git status | grep -q "rebase in progress"; then
                echo "Rebase still in progress after conflict resolution attempt. Aborting rebase..."
                git rebase --abort
                return 1
            fi
        else
            # Some other rebase error
            echo "Rebase failed with non-conflict error. Aborting..."
            git rebase --abort 2>/dev/null || true
            return 1
        fi
    done

    echo "Failed to push after $max_rebase_retries attempts"
    return 1
}

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
        push_with_rebase

        echo "All done!"
        exit 0
    fi

    # Run clippy check and fix any errors
    echo -e "\n------------------------ CLIPPY CHECK ------------------------"
    CLIPPY_RETRIES=0
    MAX_CLIPPY_RETRIES=3
    CLIPPY_PASSED=false

    if cargo clippy --all-targets -- -D warnings 2>&1; then
        CLIPPY_PASSED=true
    else
        while [ $CLIPPY_RETRIES -lt $MAX_CLIPPY_RETRIES ]; do
            CLIPPY_RETRIES=$((CLIPPY_RETRIES + 1))
            echo -e "\nClippy failed (attempt $CLIPPY_RETRIES of $MAX_CLIPPY_RETRIES)"

            # Capture clippy errors and send to Claude for fixing
            CLIPPY_OUTPUT=$(cargo clippy --all-targets -- -D warnings 2>&1 || true)

            echo -e "\nLaunching Claude to fix clippy errors..."
            echo "Fix all clippy errors and warnings. Here is the clippy output:

\`\`\`
$CLIPPY_OUTPUT
\`\`\`

Run \`cargo clippy --all-targets -- -D warnings\` to verify fixes. Commit any changes with an appropriate message." | claude -p \
                --dangerously-skip-permissions \
                --output-format=stream-json \
                --model sonnet \
                --verbose \
                2>&1 | tee >(claude-stream-format > /dev/stderr)

            echo -e "\nRetrying clippy..."
            if cargo clippy --all-targets -- -D warnings 2>&1; then
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
    push_with_rebase

    echo -e "\nIteration $ITERATION complete. Continuing..."
done

echo "Loop finished without completion signal."
exit 1
