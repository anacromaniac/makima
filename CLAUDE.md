# Makima — Investment Tracker Backend

## Project
REST API backend in Rust for personal investment tracking.
Deploy target: Docker on Raspberry Pi. Network access via Tailscale.

## Stack
- Rust edition 2024, Cargo workspace multi-crate
- Axum (HTTP framework)
- PostgreSQL (database)
- sqlx (query layer, compile-time checked) + repository pattern
- argon2 (password hashing), jsonwebtoken (JWT)
- tracing + tracing-subscriber (structured JSON logging)
- reqwest (HTTP client for Yahoo Finance and OpenFIGI)
- tower-http (middleware: TraceLayer, CorsLayer, CompressionLayer, RequestIdLayer, PropagateRequestIdLayer, TimeoutLayer)
- Custom security headers middleware (X-Content-Type-Options, X-Frame-Options, Referrer-Policy)
- utoipa (OpenAPI auto-generation)
- tokio-cron-scheduler (periodic jobs)
- calamine (Excel parsing for broker import)

## Workspace structure
```
crates/
├── api/            # Axum handlers, router, middleware, DTOs. Depends on: domain, db
├── domain/         # Models, business logic, traits. Zero framework/db dependencies
├── db/             # Repositories, sqlx migrations. Depends on: domain
├── importer/       # Broker parsers (Fineco, BG Saxo). Depends on: domain
└── price-fetcher/  # Yahoo Finance, OpenFIGI clients. Depends on: domain
```

---

## Coding conventions

### Architecture
- Repository pattern: all database access goes through repository structs in the `db` crate. No raw sqlx calls outside repositories.
- The `domain` crate must never import sqlx, axum, or any infrastructure crate. It defines traits that `db` implements.
- DTOs (request/response types) live in `api`. Domain models live in `domain`. Never expose domain models directly in API responses.

### Rust style
- Use `thiserror` for domain error types, map to HTTP errors in `api` with `IntoResponse`.
- Prefer `impl` blocks over free functions.
- All public types derive `Debug, Clone, Serialize, Deserialize` unless there's a reason not to.
- Use `Uuid` (v7) for all primary keys.
- Use `rust_decimal::Decimal` for all monetary/financial amounts. Never use f32/f64 for money.
- Use `chrono::DateTime<Utc>` for timestamps, `chrono::NaiveDate` for dates.
- Run `cargo fmt` and `cargo clippy -- -D warnings` before committing. Both must pass with zero warnings.

### Validation
- Use `garde` crate for input validation via derive macros.
- Call `.validate(&())` explicitly in handlers after extraction. Do not use axum-valid or automatic validation extractors.
- This gives full control over error response formatting.

### Documentation
- All public types, traits, functions, and methods must have `///` doc comments.
- Doc comments should explain *what* and *why*, not *how* (the code shows how).
- Each crate entry point (`lib.rs` or `main.rs`) must have `//!` module-level documentation explaining the crate's purpose.
- `cargo doc --no-deps` must build without warnings.

### Dependencies
- Use `rustls` for TLS everywhere. Never use `native-tls` or `openssl`. This ensures musl/Alpine compatibility and avoids C library dependencies.
- Disable default features on crates that default to `native-tls` (e.g. `reqwest`, `sqlx`) and explicitly enable `rustls` features.

### Database
- All schema changes go through sqlx migrations (`sqlx migrate add <name>`).
- Migrations are embedded in the binary via `sqlx::migrate!()` and run automatically on startup.
- Use PostgreSQL `NUMERIC` type for financial amounts, mapped to `rust_decimal::Decimal`.
- Column naming: `snake_case`. Table naming: plural `snake_case` (e.g. `transactions`, `price_history`).
- Every table has `id UUID PRIMARY KEY`, `created_at TIMESTAMPTZ`, `updated_at TIMESTAMPTZ`.
- Cascade delete: deleting a portfolio hard-deletes its transactions. Assets are shared and never cascade-deleted.
- Use SQL transactions (`sqlx::Transaction`) for multi-row operations (e.g. broker import). Rollback on any failure.
- Connection pool max size: 5 (configurable). Target is Raspberry Pi 3 with 1GB RAM.

### API
- All endpoints under `/api/v1/`.
- Paginated responses use a uniform wrapper: `{ "data": [...], "pagination": { "page", "limit", "total_items", "total_pages" } }`.
- Pagination parameters: `?page=1&limit=25` (default). Max limit: 100.
- Error responses: `{ "code": "VALIDATION_ERROR", "message": "human readable description" }`.
- All dates are `NaiveDate` (YYYY-MM-DD). All timestamps are `DateTime<Utc>`.
- Use `garde` derive macros on all request DTOs.

### Configuration
- Environment variables only (12-factor). No config files with secrets.
- `.env.example` committed to repo with all parameters and placeholder values.
- `.env` is gitignored. Developers copy `.env.example` to `.env` for local use.
- Docker production uses `env_file` directive pointing to a local `.env` on the Pi.

---

## Security rules

### Code generation
- Always use parameterized queries via sqlx (`$1`, `$2`). Never build SQL with `format!()` or string concatenation.
- Hash passwords exclusively with argon2. Never store plaintext or use weak hashing (md5, sha256).
- No secrets, API keys, or credentials hardcoded in source code. Everything goes through environment variables.
- Never expose internal error details (stack traces, SQL errors, file paths) in API responses. Log them server-side, return generic error codes to clients.
- Validate all user input with garde before processing. Never trust client data.

### Authentication
- JWT tokens must have short expiration (access: 1h, refresh: 7d).
- Every endpoint except /health, /ready, /auth/* must require a valid JWT.
- Always verify user ownership before returning or modifying any resource (portfolio, transaction, etc.).

### Dependencies
- Use Context7 MCP to verify you are using up-to-date API patterns for all external crates.
- When adding new dependencies, resolve the library ID with Context7 first, then query the docs for current version numbers and usage patterns.
- Prefer well-maintained crates with high download counts. Avoid unmaintained or obscure dependencies.
- Run `cargo audit` to check for known vulnerabilities in dependencies.

### Filesystem and environment
- Never read or access `.env`, `.ssh`, credentials files, or private keys.
- Never execute arbitrary shell commands from user input.
- Never write to paths outside the project directory.

### Logging
- Never log passwords, password hashes, JWT tokens, refresh tokens, or API keys.
- Implement `Debug` manually for types containing sensitive fields, masking the values (e.g. `password_hash: [REDACTED]`).
- Request IDs (from RequestIdLayer) must be included in all log entries for traceability.

---

## Testing

### Unit tests
- Live in the same file as the code they test, inside `#[cfg(test)]` modules.
- `domain` crate: calculations (gain/loss, asset allocation), validations, currency conversions, transaction aggregation. No database, no network, no filesystem.
- `importer` crate: parser correctness for Fineco and BG Saxo formats. Use fixture `.xlsx` files committed in the repo under `crates/importer/fixtures/`. Assert parsed output matches expected transactions.
- `price-fetcher` crate: test JSON response parsing/mapping with fake responses only. No real network calls. Real API calls are `#[ignore]` tests for manual verification.

### Integration tests
- Live in `crates/api/tests/`.
- Test full request/response cycles: build the Axum app, send HTTP requests, assert status codes and JSON bodies.
- Use a dedicated PostgreSQL test database. Each test suite runs migrations on setup and cleans up after.
- Cover: authentication flows, CRUD operations, ownership isolation (user A cannot see user B's data), error responses for invalid input.
- These implicitly cover the `db` layer (repositories, migrations, constraints). No separate db tests for the MVP.

### Naming
- Test functions: `test_<what>_<expected_outcome>` (e.g. `test_login_with_wrong_password_returns_401`).
- One assertion focus per test. Multiple asserts are fine if they test the same behavior.

### Running
- `cargo test` runs everything (unit + integration).
- Integration tests require `DATABASE_URL` pointing to the test database.
- CI pipeline: `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo test` → `cargo audit`.

### Manual testing
- HTTP request files in `http/` directory, one per resource (e.g. `auth.http`, `portfolios.http`, `transactions.http`).
- Committed to the repo as living API documentation.
- Each `.http` file covers all endpoints for that resource with example payloads.

---

## Current status

### Phase 0: Scaffolding — IN PROGRESS
- [x] Cargo workspace with all crates (api, domain, db, importer, price-fetcher)
- [x] `.gitattributes` (LF line endings, xlsx binary)
- [x] `.env.example` with all config parameters
- [ ] docker-compose (backend + PostgreSQL)
- [ ] Axum server boots, /health and /ready respond
- [ ] sqlx-cli configured, first empty migration runs
- [ ] tracing + structured JSON logging
- [ ] Configuration loading from env/file
- [ ] CORS middleware
- [ ] OpenAPI/Swagger UI served at /swagger-ui

### Phase 1: Domain model — NOT STARTED
- [ ] All domain structs, enums, error types in `domain` crate
- [ ] BrokerImporter trait definition
- [ ] Unit tests for core domain logic

### Phase 2: Features — NOT STARTED
- [ ] 2a: Auth (users, register, login, JWT, middleware)
- [ ] 2b: Portfolios (CRUD)
- [ ] 2c: Assets (CRUD, OpenFIGI integration)
- [ ] 2d: Transactions (CRUD, multi-currency, no-short-sell validation)
- [ ] 2e: Positions (on-the-fly calculation, closed position flag)
- [ ] 2f: Prices (Yahoo Finance client, daily job, manual refresh, price history, backfill)
- [ ] 2g: Exchange rates (Yahoo Finance, daily job)
- [ ] 2h: Broker import (upload endpoint, Fineco parser, BG Saxo parser)
- [ ] 2i: Analytics (gain/loss, asset allocation)

### Phase 3: Polish — NOT STARTED
- [ ] OpenAPI annotations on all endpoints via utoipa
- [ ] Pagination uniform across all list endpoints
- [ ] Error handling review
- [ ] README and config documentation

---

## Reference

- Full MVP specification: see `docs/makima-mvp-spec.md` for complete details on data model, endpoint map, database schema, configuration, and out-of-scope features.
- Always consult the MVP spec when making architectural decisions or when task scope is unclear.
