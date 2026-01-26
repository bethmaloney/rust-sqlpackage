0. Study @docs/IMPLEMENTATION_PLAN.md.
1. Your task is to implement functionality per the specifications. Follow @docs/IMPLEMENTATION_PLAN.md and choose the most important item to address. Before making changes, search the codebase (don't assume not implemented) using Sonnet subagents. You may use up to 500 parallel Sonnet subagents for searches/reads and only 1 Sonnet subagent for build/tests. Use Opus subagents when complex reasoning is needed (debugging, architectural decisions).
2. After implementing functionality or resolving problems, run the tests for that unit of code that was improved. If functionality is missing then it's your job to add it as per the application specifications.
3. When you discover issues, immediately update @docs/IMPLEMENTATION_PLAN.md with your findings using a subagent. When resolved, update and remove the item.
4. When the tests pass, update @docs/IMPLEMENTATION_PLAN.md, then `git add -A` then `git commit` with a message describing the changes.

999. You must implement ONE and only ONE item from the implementation plan
9999. If there are no further items then check for any failing tests or formatting issues. If you discover any issues add them to the plan and then finish
99999. Important: If there are no more further items and no other issues then you must reply "ALL TODO ITEMS COMPLETE"
999999. Important: Single sources of truth, no migrations/adapters. If tests unrelated to your work fail, resolve them as part of the increment.
9999999. You may add extra logging if required to debug issues.
99999999. Keep @docs/IMPLEMENTATION_PLAN.md current with learnings using a subagent — future work depends on this to avoid duplicating efforts. Update especially after finishing your turn.
999999999. When you learn something new about how to run the application, update @AGENTS.md using a subagent but keep it brief. For example if you run commands multiple times before learning the correct command then that file should be updated.
9999999999. For any bugs you notice, resolve them or document them in @docs/IMPLEMENTATION_PLAN.md using a subagent even if it is unrelated to the current piece of work.
99999999999. Implement functionality completely. Placeholders and stubs waste efforts and time redoing the same work.
999999999999. When @docs/IMPLEMENTATION_PLAN.md becomes large periodically clean out the items that are completed from the file using a subagent.
999999999999999. IMPORTANT: Keep @AGENTS.md operational only — status updates and progress notes belong in `IMPLEMENTATION_PLAN.md`. A bloated AGENTS.md pollutes every future loop's context.
