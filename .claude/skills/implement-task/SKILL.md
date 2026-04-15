---
name: implement-task
description: Implement a Makima development task. Use when asked to work on a task file from tasks/ directory, implement a feature, or when the user says "implement task" or "next task".
---

# Task Implementation Workflow

When implementing a task from the `tasks/` directory, follow this exact workflow:

## 1. Read and understand
- Read the task file completely.
- Read `CLAUDE.md` for project conventions and current status.
- If the task references specific sections of `docs/makima-mvp-spec.md`, read those sections.
- Do NOT start coding until you understand the full scope.

## 2. Implement
- Follow all conventions from `CLAUDE.md` (repository pattern, error handling, naming, etc.).
- Use Context7 to verify up-to-date API usage for external crates.
- Write `///` doc comments on all public items.
- Write tests as specified in the task's done criteria.

## 3. Verify
Run all checks and fix any issues before reporting completion:
```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo build
cargo test
cargo doc --no-deps
```
If any check fails, fix the issue and re-run. Do not report completion until all checks pass.

## 4. Create .http file
If the task added new API endpoints, create or update the corresponding `.http` file in the `http/` directory with example requests for each new endpoint.

## 5. Retrospective
Before updating any files, present a brief retrospective to the user:

### What went well
- Patterns that worked, clean integrations, things that were straightforward.

### What was tricky
- Unexpected issues, workarounds needed, ambiguities in the spec or conventions.

### Proposed CLAUDE.md changes
Suggest concrete additions or modifications, for example:
- New conventions discovered during implementation (e.g. "error mapping pattern for this type of handler").
- Gotchas or footguns worth documenting (e.g. "sqlx requires X when doing Y").
- Missing or unclear rules that caused hesitation.
- Dependency notes (e.g. "crate X required feature flag Y not mentioned in the spec").
- Feature or scope observations (e.g. "the spec doesn't cover edge case X, consider adding it").

### Proposed spec changes
If anything in `docs/makima-mvp-spec.md` was found to be incomplete, ambiguous, or incorrect, list the suggested changes.

**Wait for user confirmation before proceeding to step 6.** The user may accept all proposals, modify some, or reject them.

## 6. Update documentation
After user confirmation:
- Apply the approved CLAUDE.md changes.
- Mark the completed task checkbox as `[x]` in the `Current status` section.
- If this was the last task in a phase, update the phase status to `DONE`.
- If spec changes were approved, update `docs/makima-mvp-spec.md`.

## 7. Report
Provide a brief final summary:
- What was implemented
- Files created/modified
- Tests added
- Documentation changes applied
- Any remaining warnings or known limitations
