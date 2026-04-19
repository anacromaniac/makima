# Makima — Investment Tracker Backend

## Project
REST API backend in Rust for personal investment tracking.
Deploy target: Docker on Raspberry Pi. Network access via Tailscale.

## Shared workflow docs
- Use `docs/task-workflow.md` when implementing a task from the `tasks/` directory or continuing the next planned feature task.
- Keep reusable agent workflow guidance in tracked repo docs, not in tool-specific skill files.

## Stack
- Rust edition 2024, Cargo workspace multi-crate
- Axum (HTTP framework)
- PostgreSQL (database)
- sqlx (query layer, compile-time checked) + repository pattern
- async-trait (dyn-compatible async repository traits)
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
├── api/            # Axum handlers, router, middleware, DTOs, composition helpers. Depends on: application, db
├── application/    # Use-case orchestration and application services/ports. Depends on: domain
├── domain/         # Models, business logic, traits. Zero framework/db dependencies
├── db/             # Repository adapters using sqlx/PostgreSQL. Depends on: domain
├── importer/       # Broker parsers (Fineco, BG Saxo). Depends on: domain
└── price-fetcher/  # External market-data adapters (Yahoo Finance, OpenFIGI). Depends on: domain
```

---

## Coding conventions

### Architecture — Ports and Adapters
The project follows a strict ports-and-adapters (hexagonal) pattern. **Domain is the core, `application` is the use-case layer, and the other crates are adapters.**

**Layer rules (hard constraints):**
- `domain` crate: zero dependencies on sqlx, axum, or any infrastructure crate. Defines domain models, business logic, and **repository traits** (the "ports").
- `application` crate: owns use-case orchestration, application services, and any application-level adapter traits. Depends on `domain`, never on axum or sqlx.
- `db` crate: implements domain repository traits using sqlx/PostgreSQL (the "adapters"). Converts `sqlx::Error` → `domain::RepositoryError` — sqlx never leaks out.
- `api` crate: Axum handlers, DTOs, middleware, router composition, and shared app-state/build helpers. Depends on `application` services, not on concrete `db` types in handlers.
- DTOs (request/response types) live in `api`. Domain models live in `domain`. Never expose domain models directly in API responses.

**Repository trait pattern:**
- Traits with async methods are defined in `domain::traits` using `#[async_trait]`. Supertraits `Send + Sync` make them usable as `Arc<dyn Trait>`.
- `db` crate provides `Pg*Repository` structs (e.g. `PgUserRepository { pool: PgPool }`) that implement these traits. Each maps storage errors to `RepositoryError`.
- `application` services own `Arc<dyn ...Repository>` dependencies and expose use-case methods (`AuthService`, `PortfolioService`, etc.).
- `AppState` holds `Arc<ApplicationService>` instances, not repositories. `pool` is kept only for `/ready` health checks and migrations.
- The concrete adapter wiring lives in the API composition layer (`build_app_state`, `build_app_state_with_lookup`) and is invoked by `crates/api/src/main.rs`.
- `build_app_state_with_lookup` is still production composition: test-injected adapters should plug into the same repository-backed wrappers and fallback chains as runtime wiring, not bypass them with test-only shortcuts.

**Service / handler separation:**
- Application services live in `crates/application/src/<feature>.rs`. They must not import axum or sqlx. They take trait objects, return feature-specific application errors, and enforce ownership/business rules.
- `impl IntoResponse for FooHandlerError` lives in `api/*/handlers.rs` so HTTP concerns stay in the transport layer.
- `RepositoryError` variants (`Conflict`, `Internal`) are matched in application services and mapped to feature-specific errors (e.g. `AuthError::EmailAlreadyExists`).
- External HTTP integrations (for example OpenFIGI or Yahoo Finance) should be represented as small trait-backed adapters owned by the `application` layer and injected during app-state construction. Application services depend on the trait, not on `reqwest` clients directly.

**Adding a new feature (e.g. Portfolios):**
1. Add `PortfolioRepository` trait to `domain::traits`.
2. Add `PgPortfolioRepository` in `db` implementing it.
3. Add `PortfolioService` (or equivalent use-case type) to `crates/application/src/portfolios.rs`.
4. Add `portfolio_service: Arc<PortfolioService>` to `AppState`.
5. Wire `PgPortfolioRepository::new(pool.clone())` into the service in `build_app_state`.
6. Write handlers calling the application service; add `impl IntoResponse for PortfolioHandlerError` in `handlers.rs`.

### Rust style
- Use `thiserror` for domain error types, map to HTTP errors in `api` with `IntoResponse`.
- Prefer `impl` blocks over free functions.
- All public types derive `Debug, Clone, Serialize, Deserialize` unless there's a reason not to.
- Use `Uuid` (v7) for all primary keys. The `uuid` crate only has the `v7` feature enabled — always use `Uuid::now_v7()`, never `Uuid::new_v4()`.
- Use `rust_decimal::Decimal` for all monetary/financial amounts. Never use f32/f64 for money.
- Use `chrono::DateTime<Utc>` for timestamps, `chrono::NaiveDate` for dates.
- Run `cargo fmt` and `cargo clippy -- -D warnings` before committing. Both must pass with zero warnings.

### Validation
- Use `garde` crate for input validation via derive macros.
- Call `.validate()` in handlers after extraction for default context (`Context = ()`). Use `.validate_with(ctx)` only when a custom context struct is needed. Do not use axum-valid or automatic validation extractors.
- This gives full control over error response formatting.

### Documentation
- All public types, traits, functions, and methods must have `///` doc comments.
- Doc comments should explain *what* and *why*, not *how* (the code shows how).
- Each crate entry point (`lib.rs` or `main.rs`) must have `//!` module-level documentation explaining the crate's purpose.
- `cargo doc --no-deps` must build without warnings.

### Dependencies
- Use `rustls` for TLS everywhere. Never use `native-tls` or `openssl`. This ensures musl/Alpine compatibility and avoids C library dependencies.
- Disable default features on crates that default to `native-tls` (e.g. `reqwest`, `sqlx`) and explicitly enable `rustls` features.
- tower-http features: use specific compression features (`compression-gzip`, `compression-deflate`) instead of generic `compression`.
- tower-http `util` feature is required for `ServiceBuilderExt` extension methods.
- TimeoutLayer::new() is deprecated; use `TimeoutLayer::with_status_code()` instead.
- utoipa requires `features = ["uuid", "chrono"]` in the `api` crate to support `Uuid` and `DateTime<Utc>` fields in `#[derive(ToSchema)]`.
- utoipa does not provide `ToSchema` support for `rust_decimal::Decimal` in the current setup. API DTO fields using `Decimal` must annotate the OpenAPI schema explicitly with `#[schema(value_type = String)]` (or `Option<String>` for optional values).
- `AuthenticatedUser` derives `Debug` (it only contains a `Uuid`) so handlers accepting it can be annotated with `#[tracing::instrument]`.

### Database
- All schema changes go through sqlx migrations (`sqlx migrate add <name>`).
- Migrations are embedded in the binary via `sqlx::migrate!()` and run automatically on startup.
- **Migration path resolution**: In a Cargo workspace, `sqlx::migrate!()` resolves paths relative to the crate's `Cargo.toml`, not the workspace root. The `api` crate embeds migrations, so use `sqlx::migrate!("../../migrations")` from `crates/api/` to reference migrations at the project root. When building with Docker, ensure the `migrations` directory is copied into the build context.
- **sqlx type features**: The workspace sqlx dependency must include the `uuid`, `chrono`, and `rust_decimal` features so that `Uuid`, `DateTime<Utc>`, and `Decimal` values can be bound and decoded in repository queries.
- **Repository row types**: Each repository defines an internal `*Row` struct (e.g. `UserRow`) deriving `sqlx::FromRow` that mirrors the DB columns exactly. A `From<*Row> for DomainType` impl converts to the domain model. This keeps sqlx out of the domain crate.
- Use PostgreSQL `NUMERIC` type for financial amounts, mapped to `rust_decimal::Decimal`.
- Column naming: `snake_case`. Table naming: plural `snake_case` (e.g. `transactions`, `price_history`).
- Every table has `id UUID PRIMARY KEY`, `created_at TIMESTAMPTZ`, `updated_at TIMESTAMPTZ` (exception: `refresh_tokens` has no `updated_at` since it is append-only).
- Cascade delete: deleting a portfolio hard-deletes its transactions. Assets are shared and never cascade-deleted.
- Use SQL transactions (`sqlx::Transaction`) for multi-row operations (e.g. broker import). Rollback on any failure.
- **Derived numeric SQL output**: Queries that compute running costs, average prices, or similar derived `NUMERIC` values must round to a stable scale before decoding into `rust_decimal::Decimal` (for example `ROUND(..., 8)`). This avoids decode failures from repeating decimals produced by SQL division.
- Connection pool max size: 5 (configurable). Target is Raspberry Pi 3 with 1GB RAM.

### API
- All endpoints under `/api/v1/`.
- Paginated responses use a uniform wrapper: `{ "data": [...], "pagination": { "page", "limit", "total_items", "total_pages" } }`.
- Pagination parameters: `?page=1&limit=25` (default). Max limit: 100.
- Error responses: `{ "code": "VALIDATION_ERROR", "message": "human readable description" }`.
- All dates are `NaiveDate` (YYYY-MM-DD). All timestamps are `DateTime<Utc>`.
- Use `garde` derive macros on all request DTOs.
- Middleware stack order in `ServiceBuilder` is outermost-to-innermost (first added = outermost).
- **AppState**: All handlers use `State<AppState>` — never `State<PgPool>` directly. `AppState` holds the pool, JWT secret, and application services. When constructing `AppState`, compute any values from `AppConfig` that would cause a partial-move error (e.g. `server_address()`) before consuming config fields into `AppState`.
- **JWT extractor**: Protected handlers use the `AuthenticatedUser` extractor (implements `FromRequestParts`) which validates the Bearer token and exposes `user_id: Uuid`. No DB hit required for JWT verification.
- **Shared reference data**: Endpoints for shared resources such as `assets` still require JWT authentication by default, but they do not perform ownership filtering because the records are global.
- **Paginated list endpoints — local schema type required**: `domain::PaginationMeta` cannot derive `ToSchema` (the domain crate has no utoipa dependency). Each feature that exposes a paginated list endpoint must define a local `PaginationMetaResponse` struct in its `handlers.rs` that mirrors `PaginationMeta` and derives `ToSchema`. The `From<PaginatedResult<T>>` impl on the paginated response type converts the domain type to the local schema type.
- **Handler validation error pattern**: When a handler can fail with both a garde validation error and a service error, define a `pub(crate) <Feature>HandlerError` enum in `handlers.rs` that has `Validation(String)` and `Service(<Feature>Error)` variants, both implementing `IntoResponse`. Use this as the `Err` type so the handler returns a single named error type. See `portfolios/handlers.rs` → `PortfolioHandlerError` as the template.
- **Ownership isolation**: Handlers that operate on a resource owned by a user must return 404 (not 403) when the resource does not exist *or* belongs to a different user. This prevents leaking the existence of other users' data. Implement the check in the service layer by calling `find_by_id` and filtering on `user_id`.
- **Transaction asset auto-creation**: For transaction create/update flows, handlers pass `asset_isin` to the application service. The service is responsible for resolving the shared asset by ISIN and auto-creating it through an injected reference-data adapter when it does not exist. Keep this logic out of handlers.
- **Asset price backfill and scheduled refresh**: When an asset gains a Yahoo ticker for the first time (during create or update), the application service is responsible for triggering best-effort historical backfill through an injected price adapter. The daily scheduled refresh job is infrastructure wiring started from `crates/api/src/main.rs` with concrete repositories and Yahoo adapters; keep cron/job setup out of handlers.
- **Broker import normalization**: Broker parsers return normalized parsed rows, not `NewTransaction` directly. The application import service is responsible for portfolio assignment, asset resolution/creation, FX lookup, duplicate detection, chronological ordering, and final `NewTransaction` construction.
- **Broker import ordering**: Imported broker rows must be normalized to chronological order before no-short-sell validation and persistence. Real broker exports may be newest-first.

### Configuration
- Environment variables only (12-factor). No config files with secrets.
- `.env.example` committed to repo with all parameters and placeholder values.
- `.env` is gitignored. Developers copy `.env.example` to `.env` for local use.
- **Quoting**: Values containing spaces or special characters (like cron expressions) should be quoted in `.env` files to avoid parsing errors by dotenvy.
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
- **Initialization order**: `.env` must be loaded BEFORE `tracing_subscriber::init()`, otherwise `RUST_LOG` won't be read.

---

## Testing

### Unit tests
- Live in the same file as the code they test, inside `#[cfg(test)]` modules.
- `domain` crate: calculations (gain/loss, asset allocation), validations, currency conversions, transaction aggregation. No database, no network, no filesystem.
- `importer` crate: parser correctness for Fineco and BG Saxo formats. Use fixture broker exports committed in the repo under `crates/importer/fixtures/`. Real fixtures may be either `.xls` or `.xlsx`. Assert parsed output matches expected transactions.
- `price-fetcher` crate: test JSON response parsing/mapping with fake responses only. No real network calls. Real API calls are `#[ignore]` tests for manual verification.

### Integration tests
- Live in `crates/api/tests/`.
- Test full request/response cycles: build the Axum app, send HTTP requests, assert status codes and JSON bodies.
- Use a dedicated PostgreSQL test database provisioned automatically by the shared `testcontainers` harness. Each test suite runs migrations on setup and cleans up after.
- Cover: authentication flows, CRUD operations, ownership isolation (user A cannot see user B's data), error responses for invalid input.
- These implicitly cover the `db` layer (repositories, migrations, constraints). No separate db tests for the MVP.
- Treat integration tests as part of each feature task's done criteria, not as a later polish pass.
- Maintain a shared API integration harness in `crates/api/tests/` that builds the real Axum app with real repositories and test configuration.
- When a feature depends on an external HTTP service, inject a stub implementation through the shared test harness so integration tests remain deterministic and offline.
- Prefer one integration test file per feature (for example `auth.rs`, `users.rs`, `portfolios.rs`) instead of one large catch-all suite.
- The harness should start PostgreSQL automatically with `testcontainers`, run migrations, and expose helpers for common authenticated flows.
- For protected resources, the minimum coverage matrix is: happy path, validation failure, unauthenticated request, invalid token when relevant, not found, and ownership isolation when relevant.
- New Phase 2 endpoint work is not complete until its integration tests are added or updated in the same task.

### Naming
- Test functions: `test_<what>_<expected_outcome>` (e.g. `test_login_with_wrong_password_returns_401`).
- One assertion focus per test. Multiple asserts are fine if they test the same behavior.

### Running
- `cargo test` runs everything (unit + integration).
- Integration tests require Docker daemon access so `testcontainers` can start PostgreSQL automatically; no manual test `DATABASE_URL` is required.
- CI pipeline: `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo test` → `cargo audit`.

### Manual testing
- HTTP request files in `http/` directory, one per resource (e.g. `auth.http`, `portfolios.http`, `transactions.http`).
- Committed to the repo as living API documentation.
- Each `.http` file covers all endpoints for that resource with example payloads.

---

## Current status

### Phase 0: Scaffolding — IN PROGRESS
- [x] Cargo workspace with all crates (api, application, domain, db, importer, price-fetcher)
- [x] `.gitattributes` (LF line endings, xlsx binary)
- [x] `.env.example` with all config parameters
- [x] docker-compose (backend + PostgreSQL)
- [x] Axum server boots, /health and /ready respond
- [x] sqlx-cli configured, first empty migration runs
- [x] tracing + structured JSON logging
- [x] Configuration loading from env/file
- [x] CORS middleware
- [x] OpenAPI/Swagger UI served at /swagger-ui

### Phase 1: Domain model — DONE
- [x] All domain structs, enums, error types in `domain` crate
- [x] BrokerImporter trait definition
- [x] Unit tests for core domain logic

### Phase 2: Features — IN PROGRESS
- [x] 2a: Auth (users, register, login, JWT, middleware)
- [x] 2b: Portfolios (CRUD)
- [x] 2.0: API integration test harness
- [x] 2.1a: Auth integration tests retrofit
- [x] 2.2a: Authenticated user integration tests retrofit
- [x] 2.3a: Portfolio integration tests retrofit
- [x] 2c: Assets (CRUD, OpenFIGI integration)
- [x] 2d: Transactions (CRUD, multi-currency, no-short-sell validation)
- [x] 2e: Positions (on-the-fly calculation, closed position flag)
- [x] 2f: Prices (Yahoo Finance client, daily job, manual refresh, price history, backfill)
- [x] 2g: Exchange rates (Yahoo Finance, daily job)
- [x] 2h: Broker import (upload endpoint, Fineco parser, BG Saxo parser)
- [ ] 2i: Analytics (gain/loss, asset allocation)

### Phase 3: Polish — NOT STARTED
