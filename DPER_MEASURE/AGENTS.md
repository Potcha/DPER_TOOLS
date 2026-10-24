# AGENTS.md — DPER_MEASURE

## Purpose and precedence

These instructions define durable working rules for the `DPER_MEASURE` tool.

- Follow direct user instructions first, then applicable repository instructions.
- More specific `AGENTS.md` or `AGENTS.override.md` files may refine these rules for their directory.
- Keep this file focused on recurring rules; do not add temporary task details.

## Mission and scope

- Make the smallest complete change that satisfies the requested work, based on the actual code and documentation. Do not infer product behavior from the tool's name.
- Preserve existing user changes and keep work inside `DPER_MEASURE/` unless the task explicitly requires an authorized shared change.
- Before each task, inspect the relevant files and available repository guidance. Do not repeat broad discovery when reliable context already answers the question.
- Follow the current `CONTRIBUTING.md` and applicable CI workflows. Confirm this tool's branch, path, and release rules from current repository guidance; do not infer them from sibling tools.

## Model and reasoning-effort routing

Choose the least costly model available in the active session that can meet the task's quality and safety requirements. Choose model and reasoning effort independently, and increase effort only when the work requires it. Do not escalate only because a task takes time or touches many files.

- Use **GPT-6 Luna** for focused repository lookup, routine documentation, small established changes, and straightforward checks or analysis.
- Use **GPT-6.1 Sol** for substantial engineering reasoning, cross-component changes, difficult debugging, architecture, security, or high-risk review. Match Medium, High, or Max effort to the actual difficulty; do not default to Max.
- Use **GPT-6 Astra** only for exceptional ambiguity, novel design, or a problem that remains low-confidence after a serious Sol attempt. Use it directly only when that need is clear from the outset.
- Keep routine follow-up with the least costly suitable model after the difficult part is resolved. Use only models and controls actually available in the session; do not claim an unavailable model was used.

## Sub-agent efficiency

Use sub-agents when a bounded assignment is expected to improve quality, elapsed time, context efficiency, or isolation enough to justify coordination cost. The goal is useful delegation, not maximum agent count.

- Delegate independent research, review, or implementation slices with clear boundaries. Parallelize only work that can proceed independently.
- Do not spawn an agent for a tiny operation, every file, or work that duplicates the coordinator's investigation. Batch related small tasks when practical.
- Give each agent only the context and files it needs, plus a concrete question or deliverable. Ask for concise findings and evidence, not a repository-wide dump.
- Avoid overlapping edits. State whether the assignment is read-only or may change specific files.
- Reuse verified context and results instead of rescanning or rerunning unchanged work. Do not save quota by skipping reasoning, safety, or checks that the task requires.
- The coordinating agent owns scope, decisions, integration, and final review. Inspect delegated changes and evidence; do not accept them blindly.

## Local workflow and delivery

- Preserve unrelated or pre-existing work. Do not reset, stash, delete, switch branches, commit, push, or change shared repository configuration without explicit authorization.
- Until the user has sent the exact phrase `OK process`, make only the requested local code or documentation changes. Do not run validation or make Jira, Git, or GitHub updates; do not commit, push, or create or update a PR.
- When validation is authorized, use the narrowest meaningful existing checks first. Avoid repeating checks unless relevant files, tools, or failures changed. Report only checks actually run and their actual results.
- Keep changes and documentation proportionate to the request. Do not mix unrelated cleanup, refactoring, or speculative features into the task.

## Maintaining these instructions

Keep this file concise and current. Add durable rules when they prevent recurring mistakes; remove rules that no longer apply, and put narrower directory-specific guidance closer to the files it governs.
