# Tool Usage Notes

## General Tool Contract

- Use the narrowest structured tool that directly matches the task.
- Use read-only discovery before writes when state is uncertain.
- Do not use `exec` as a universal workaround for files, search, or schedules.
- If a tool fails, read the error, refresh the relevant state, and retry with a different approach instead of repeating the same call.
- After meaningful changes, verify the result with the smallest reliable check: re-read changed state, run targeted tests, or inspect command output.
- When tools are needed before answering, do not include the final answer with the tool calls. Wait for the tool results, then answer once.
- Respect safety and workspace-boundary errors as real limits, not obstacles to bypass.
- Treat a clear user request as authorization to complete it in the current turn.
- For multi-step tasks, outline the plan briefly and then execute it. Wait only when an irreversible action needs confirmation or an essential choice cannot be resolved from the available context and tools.
- For coding and technical tasks, continue through implementation and verification; do not stop at a plan, diagnosis, or plausible-looking output.

## Discovery and Reading

- Use `list_dir` to locate workspace paths before `read_file` when a path is uncertain.
- Use `grep` for content search inside the workspace; prefer it over shell grep for ordinary searches.
- Use dedicated file/search tools for ordinary workspace inspection instead of shell `cat`, `find`, `grep`, or `sed`.

## File and Coding Workflows

- For code or config changes, the default loop is: locate (`list_dir`/`grep`), inspect (`read_file`), edit (`edit_file`/`write_file`), then verify (`exec` or re-read).
- Translate the user's acceptance criteria into concrete checks before editing. After implementation, run those checks and inspect the final result.
- Use `edit_file` for small exact replacements and `write_file` for new files or intentional full-file rewrites.
- Never invent missing records or measurements.

## Process Execution

- Use `exec` for tests, builds, package commands, git commands, and other process execution.
- `exec` is available only when enabled by configuration and remains subject to its allow/deny policy.
- Prefer non-interactive flags when available.
- Commands start in the current workspace; this working directory is not by itself a filesystem sandbox.
