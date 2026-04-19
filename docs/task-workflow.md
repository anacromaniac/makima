# Task Workflow

Use this workflow when implementing a task from the `tasks/` directory or when continuing the next planned feature task.

## 1. Read and understand
- Read the task file completely.
- Read `AGENTS.md` for project conventions and current status.
- If the task references specific sections of `docs/makima-mvp-spec.md`, read those sections.
- Do not start coding until the full scope is clear.

## 2. Implement
- Follow all conventions from `AGENTS.md` such as architecture, repository pattern, error handling, naming, and testing expectations.
- Verify up-to-date API usage for external crates before adopting new patterns.
- Write `///` doc comments on all public items.
- Add tests required by the task's done criteria.
- For API features, treat integration tests as part of the feature itself, not as a later polish task.
- When the feature exposes HTTP endpoints, add or update the feature's integration tests under `crates/api/tests/` in the same task.
- Reuse the shared API integration test harness instead of duplicating app/database setup in each test module.

## 3. Verify
Run the full verification set before reporting completion:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo build
cargo test
cargo doc --no-deps
```

If any check fails, fix the issue and re-run it. Do not report completion until the required checks pass, or explicitly call out what could not be run.

## 4. Update HTTP examples
- If the task adds new API endpoints, create or update the corresponding `.http` file in `http/` with example requests for each new endpoint.

## API integration test expectations
- Keep one integration test file per feature in `crates/api/tests/` (for example `auth.rs`, `users.rs`, `portfolios.rs`).
- Cover the minimum matrix for each protected resource: happy path, validation failure, unauthenticated request, invalid token when relevant, not found, and ownership isolation when relevant.
- Prefer real request/response assertions through the Axum app with a dedicated PostgreSQL test database.
- Do not mark a feature task complete until its endpoint behavior is covered end-to-end.

## 5. Retrospective before repo-doc updates
Before updating repo guidance or specs, present a brief retrospective to the user:

### What went well
- Patterns that worked, clean integrations, and straightforward parts.

### What was tricky
- Unexpected issues, workarounds, or ambiguities in the spec or conventions.

### Proposed `AGENTS.md` changes
Suggest concrete additions or modifications, for example:
- New conventions discovered during implementation.
- Gotchas or footguns worth documenting.
- Missing or unclear rules that caused hesitation.
- Dependency notes such as required feature flags or integration constraints.
- Feature or scope observations that should be captured for future work.

### Proposed spec changes
- If anything in `docs/makima-mvp-spec.md` is incomplete, ambiguous, or incorrect, list the suggested updates.

Wait for user confirmation before applying documentation changes beyond the implementation itself.

## 6. Update documentation after confirmation
After user confirmation:
- Apply approved `AGENTS.md` changes.
- Mark the completed task checkbox as `[x]` in the current status section if applicable.
- If the task completes the last item in a phase, update that phase status.
- If spec changes were approved, update `docs/makima-mvp-spec.md`.

## 7. Report
Provide a brief final summary covering:
- What was implemented
- Files created or modified
- Tests added
- Documentation changes applied
- Remaining warnings or known limitations
- A suggested commit message in conventional commits format

## Conventional commit types
- `feat`: new feature
- `fix`: bug fix
- `docs`: documentation only
- `style`: formatting or non-functional code style changes
- `refactor`: internal code change without new behavior or bug fix
- `perf`: performance improvement
- `test`: test additions or corrections
- `build`: build system or dependency changes
- `ci`: CI configuration changes
- `chore`: other non-source changes
- `revert`: revert of a previous change
