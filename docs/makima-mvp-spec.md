# Makima — Backend MVP Specification

## 1. Vision

REST API backend written in Rust, deployed via Docker on Raspberry Pi, for tracking personal financial investments. The system serves a small group of users (2–5), automatically fetches market prices, and calculates portfolio performance. Designed as a pure backend on top of which various clients can be built (web app, mobile app, terminal TUI, bots, etc.).

---

## 2. Tech Stack

### 2.1 Core

- **Language**: Rust, edition 2024 (stable since Rust 1.85.0+).
- **HTTP framework**: Axum — modern, tower/tokio-based, ergonomic.
- **Database**: PostgreSQL — native support for precise numeric types (NUMERIC/DECIMAL), complex analytics queries, reliable.
- **Query layer**: sqlx with compile-time checked queries, `runtime-tokio-rustls` feature. No ORM: explicit SQL queries, type-safe, async-native.
- **Data architecture**: Repository pattern — each entity has a dedicated repository module as the sole point of database access. The rest of the application never interacts directly with sqlx or SQL.
- **Migrations**: sqlx-cli (`sqlx migrate run`). Versioned schema, incremental migrations.
- **Project structure**: Cargo workspace multi-crate:
  - `crates/api` — Axum handlers, router, middleware, request/response DTOs. Depends on `domain` and `db`.
  - `crates/domain` — domain models, pure business logic, trait definitions. Zero dependencies on frameworks or database.
  - `crates/db` — repository implementations, migrations, PostgreSQL access via sqlx. Depends on `domain`.
  - `crates/importer` — broker file parsers (Fineco, BG Saxo). Depends on `domain`.
  - `crates/price-fetcher` — Yahoo Finance, OpenFIGI clients, price and exchange rate update logic. Depends on `domain`.

### 2.2 Key libraries

| Function | Crate | Notes |
|---|---|---|
| Serialization | serde + serde_json | De facto standard |
| Password hashing | argon2 | GPU-attack resistant |
| JWT | jsonwebtoken | Access/refresh token management |
| HTTP client | reqwest | Async, rustls-tls (no OpenSSL), for Yahoo Finance and OpenFIGI |
| Logging | tracing + tracing-subscriber | Structured logging, JSON output |
| Input validation | garde | Derive macros on request structs |
| Configuration | config + dotenvy | 12-factor, env vars only. `.env.example` committed, `.env` gitignored |
| Task scheduling | tokio-cron-scheduler | Periodic jobs (price fetch) |
| API documentation | utoipa + utoipa-swagger-ui | Automatic OpenAPI/Swagger generation |
| CORS | tower-http (CorsLayer) | Configurable middleware |
| HTTP middleware | tower-http | TraceLayer, CompressionLayer, RequestIdLayer, PropagateRequestIdLayer, TimeoutLayer |
| Excel parsing | calamine | Reading .xlsx files for broker import |

### 2.3 Testing

- **Unit tests**: pure business logic tests in the `domain` crate and calculation logic, integrated in modules with `#[cfg(test)]`. Parser tests in `importer` with fixture files. Response parsing tests in `price-fetcher` with fake JSON responses.
- **Integration tests**: end-to-end API tests in `crates/api/tests/`. Dedicated PostgreSQL test database, setup/teardown per suite. Implicitly covers the `db` layer.
- **CI**: GitHub Actions with PostgreSQL service container. Pipeline: `cargo fmt --check` → `cargo clippy -- -D warnings` → `cargo test` → `cargo audit`.

---

## 3. Deployment

- **Target**: Raspberry Pi (arm64/armv7).
- **Containerization**: Docker, multi-stage build (compilation stage + Alpine-based minimal runtime stage). Alpine chosen for smallest image size on resource-constrained Raspberry Pi. Binary compiled with musl target (`aarch64-unknown-linux-musl` for ARM). All TLS via `rustls` (pure Rust, no OpenSSL dependency) to ensure musl/Alpine compatibility.
- **Composition**: docker-compose with two services: `makima-api` (backend) + `postgres` (database).
- **Migrations**: embedded in the binary via `sqlx::migrate!()` macro. Run automatically on application startup before the server begins accepting requests. If migration fails, the process exits with an error. No separate migration container needed.
- **Cross-compilation**: `cross` (crate) or Docker buildx for ARM builds from the development machine.
- **Network access**: Tailscale (LAN/VPN) for the MVP. Internet exposure planned for the future (with addition of Caddy or nginx reverse proxy).
- **Database backup**: `pg_dump` scheduled via cron on the Raspberry Pi (bash script, inside container or on host).
- **PostgreSQL tuning for Raspberry Pi 3** (1GB RAM): `max_connections = 20`, `shared_buffers = 128MB`, `work_mem = 4MB`. sqlx pool max size = 5.
- **Configuration**: environment variables only (12-factor). A `.env.example` file is committed to the repo with all parameters and placeholder values. Developers copy it to `.env` (gitignored) for local use. In production, the docker-compose `env_file` directive points to a local `.env` on the Pi.
- **No reverse proxy** for the MVP — Axum serves directly on the configured port.

---

## 4. MVP Features — IN scope

### 4.1 Authentication and users

- Registration with email + password. Password hashed with argon2.
- Login returns: JWT access token (short expiration, 1 hour) + opaque refresh token (long expiration, 7 days).
- **Access token**: JWT, verified without DB hit. Contains user_id and expiry. Signed with HS256.
- **Refresh token**: random opaque string (not JWT). Stored as SHA-256 hash in the `refresh_tokens` DB table. Never stored in plaintext.
- **Refresh token rotation**: every time a refresh token is used, it is invalidated and a new pair (access + refresh) is issued. Each refresh token is single-use.
- **Stolen token detection**: if a previously rotated (already used) refresh token is presented, all refresh tokens for that user are revoked (force logout on all devices).
- **Password change / logout**: all refresh tokens for the user are deleted from the DB. Active access tokens remain valid until expiry (max 1h).
- Token refresh endpoint.
- Password change endpoint.
- All protected endpoints require `Authorization: Bearer <access_token>` header.
- Simple multi-user: 2–5 users, all with equal permissions. No admin role.
- Complete isolation: each user only sees their own portfolios, transactions, and data.
- No email verification for the MVP: registration is direct.

### 4.2 Portfolios

- Each user can create multiple portfolios (e.g. "Fineco Account", "BG Saxo Account", "Long-term Bonds").
- Full CRUD: create, read, update (name/description), delete.
- **Cascade delete**: deleting a portfolio hard-deletes all its transactions. Assets are not affected (they are shared across users).
- Base currency: EUR (fixed for the MVP).
- A portfolio contains positions, derived from transaction aggregation.

### 4.3 Assets (financial instruments)

- Shared asset table across all users (an asset exists only once in the system).
- Primary identification: **ISIN** (e.g. `IE00BK5BQT80`).
- **Yahoo ticker**: automatically mapped via OpenFIGI from the ISIN. Used internally for price fetching from Yahoo Finance.
- Fields: ISIN, Yahoo ticker, human-readable name, asset class, quotation currency, exchange.
- **Asset class** — enum: `Stock`, `Bond`, `Commodity`, `Alternative`, `Crypto`, `CashEquivalent`.
- Automatic creation: when a broker import or manual transaction references an ISIN not present in the system, the asset is automatically created by fetching data from OpenFIGI.
- Manual CRUD endpoints for assets (useful for corrections or assets not covered by OpenFIGI).

### 4.4 Transactions

- Transaction types — enum: `Buy`, `Sell`, `Dividend`, `Coupon`.
- Common fields: portfolio, asset (ISIN), trade date, settlement date, type, notes.
- Buy/Sell fields: quantity, unit price, commission, transaction currency.
- Dividend/Coupon fields: gross amount, tax withheld, net amount, currency.
- Multi-currency: each transaction records the currency in which it occurred (EUR, USD, GBP, etc.). At insertion time, the system first uses the latest stored rate to EUR if available, otherwise it attempts a live Yahoo Finance lookup, then saves the resolved rate on the transaction.
- **No short selling constraint**: a Sell transaction is rejected (HTTP 400) if the sold quantity exceeds the currently held quantity for that asset in the portfolio.
- Full CRUD on transactions. Modification/deletion recalculates positions.
- Savings plans (PAC): no special entity. Each recurring purchase is recorded as a single Buy transaction.

### 4.5 Positions

- A position is derived data: the aggregation of all Buy/Sell transactions for a given asset in a given portfolio.
- Calculated fields: held quantity, average cost price, current market value, absolute gain/loss, percentage gain/loss.
- **Closed positions**: when quantity reaches 0, the position remains visible with a `closed = true` flag. The client can filter to show or hide closed positions.
- Transaction history is always maintained, even for closed positions.
- Positions are not directly persisted entities: they are calculated on-the-fly by aggregating transactions.

### 4.6 Prices and history

- **Daily job**: scheduled task (tokio-cron-scheduler) that runs after European market close (e.g. 22:00 CET). Updates prices for all assets present in at least one portfolio.
- **Manual refresh**: endpoint `POST /api/v1/assets/{isin}/refresh-price` to force a single asset's price update on-demand.
- **Provider**: Yahoo Finance (unofficial API). If the fetch fails or the asset is not covered, the price remains unchanged and the user can enter it manually.
- **Manual fallback**: endpoint to manually insert/override an asset's price.
- **Price history**: `price_history(asset_id, date, close_price, currency)` table. Each daily fetch inserts a new row (INSERT, not UPDATE). This accumulates history organically.
- **Backfill**: on first asset insertion (or on request), the system fetches historical prices from Yahoo Finance (e.g. last 5 years) and populates the history table.
- **Yahoo Finance rate limiting strategy**:
  - Configurable delay between requests (default: 1–2 seconds). With 50 assets the daily job takes ~1–2 minutes.
  - Batch multiple tickers per request where Yahoo's API supports it (up to ~10 tickers per call).
  - Retry with exponential backoff on HTTP 429: wait 30s, then 60s. Max 3 retries per asset, then mark as failed and continue.
  - If the job fails partway through, already-updated assets keep their new prices. The next job run (or manual refresh) will retry the rest.

### 4.7 Exchange rates

- Fetched from Yahoo Finance (pairs like `EURUSD=X`).
- Updated by the same daily job as prices.
- The daily job determines which FX pairs to refresh from the distinct non-EUR currencies present in transactions and updates each currency-to-EUR rate.
- Stored in a dedicated `exchange_rates(from_currency, to_currency, date, rate)` table.
- Transaction creation uses the latest available stored rate for the transaction currency before falling back to a live Yahoo lookup.
- Used to convert foreign currency transactions and positions to EUR.

### 4.8 Broker import

- **Upload via API**: endpoint `POST /api/v1/import/{broker}` accepting a broker export file as multipart upload. The MVP supports Fineco `.xls` exports and BG Saxo `.xlsx` exports.
- **Pluggable parser system**: `BrokerImporter` trait in the `domain` crate with a method to parse a file and return normalized parsed rows. Each broker implements the trait.
- **MVP parsers**: Fineco, BG Saxo.
- **Import behavior**:
  1. Parse the Excel file according to the broker's format.
  2. Validate all rows. If any row has invalid data (malformed ISIN, negative quantity, invalid date, etc.), reject the entire import with a detailed error listing all problematic rows. No partial inserts.
  3. Normalize valid parsed rows into chronological order before business-rule validation and persistence because broker exports may be newest-first.
  4. For each valid parsed row: look up the asset by ISIN. If it doesn't exist, attempt to enrich it via OpenFIGI. If OpenFIGI fails, create the asset from broker-provided metadata with `yahoo_ticker = NULL` and include a warning in the response. The user can map the ticker manually later.
  5. Resolve `exchange_rate_to_base` for non-EUR transactions using the latest available lookup. If lookup fails, keep the import successful, store a fallback value, and include a warning in the response.
  6. Insert all assets and transactions in a single SQL transaction. If any INSERT fails, rollback everything.
  7. Return a summary response: imported transactions count, created assets, warnings (assets without ticker, duplicate rows skipped, missing FX lookup, backfill failures), errors if any.
- **Duplicate handling**: basic detection to avoid importing the same file twice (e.g. file hash or check on date+ISIN+quantity+price).

### 4.9 Analytics

- **Gain/loss per position**: calculated on-the-fly as `(current_price × quantity) - (average_cost × quantity)`. Returned both as absolute value and percentage.
- **Total portfolio gain/loss**: sum of gain/loss across all open positions, converted to EUR.
- **Asset allocation by class**: percentage distribution of portfolio value by asset class (Stock, Bond, Commodity, etc.).
- All calculations are on-the-fly, no persisted snapshots.

### 4.10 Operational endpoints

- `GET /health` — liveness check. Returns 200 if the service is running.
- `GET /ready` — readiness check. Returns 200 if the service is running and the database is reachable.

### 4.11 API design

- **Format**: JSON (request and response).
- **Versioning**: all endpoints under `/api/v1/...`.
- **Pagination**: offset/limit on all list endpoints. Parameters: `page` (default 1), `limit` (default 25, max 100). Paginated response wrapper:
  ```json
  {
    "data": [ ... ],
    "pagination": {
      "page": 1,
      "limit": 25,
      "total_items": 142,
      "total_pages": 6
    }
  }
  ```
- **Date handling**: all dates (trade date, settlement date, etc.) are `NaiveDate` (YYYY-MM-DD, no timezone). Timestamps (created_at, updated_at) are `DateTime<Utc>`.
- **CORS**: configurable middleware via environment variables. Allowed origins configurable to prepare for the web app.
- **Middleware stack**: TraceLayer (request/response logging), CorsLayer, CompressionLayer (gzip/brotli), RequestIdLayer + PropagateRequestIdLayer (unique request ID per request, propagated in logs), TimeoutLayer (global 30s timeout), custom security headers middleware (X-Content-Type-Options: nosniff, X-Frame-Options: DENY, Referrer-Policy: no-referrer). Add Strict-Transport-Security when TLS is enabled in the future.
- **Documentation**: OpenAPI 3.0 auto-generated with utoipa. Swagger UI served by the backend itself (e.g. `/swagger-ui`).
- **Errors**: uniform JSON error responses with fields `code` and `message`.

---

## 5. MVP Endpoint Map

### Auth
- `POST   /api/v1/auth/register` — Register new user.
- `POST   /api/v1/auth/login` — Login, returns access + refresh token.
- `POST   /api/v1/auth/refresh` — Refresh access token.
- `PUT    /api/v1/auth/password` — Change password (authenticated).

### User
- `GET    /api/v1/users/me` — Current user profile.

### Portfolios
- `GET    /api/v1/portfolios` — List user's portfolios.
- `POST   /api/v1/portfolios` — Create portfolio.
- `GET    /api/v1/portfolios/{id}` — Portfolio detail.
- `PUT    /api/v1/portfolios/{id}` — Update portfolio.
- `DELETE /api/v1/portfolios/{id}` — Delete portfolio.
- `GET    /api/v1/portfolios/{id}/summary` — Summary: total value, gain/loss, asset allocation.
- `GET    /api/v1/portfolios/{id}/positions` — List positions (with filter `?show_closed=true/false`).

### Transactions
- `GET    /api/v1/portfolios/{id}/transactions` — List transactions (paginated, filterable by type/asset/date).
- `POST   /api/v1/portfolios/{id}/transactions` — Record transaction.
- `GET    /api/v1/transactions/{id}` — Transaction detail.
- `PUT    /api/v1/transactions/{id}` — Update transaction.
- `DELETE /api/v1/transactions/{id}` — Delete transaction.

### Assets
- `GET    /api/v1/assets` — List assets in the system (paginated, filterable by class/name).
- `GET    /api/v1/assets/{isin}` — Asset detail.
- `POST   /api/v1/assets` — Manual asset creation.
- `PUT    /api/v1/assets/{isin}` — Update asset.
- `POST   /api/v1/assets/{isin}/refresh-price` — Force price update.
- `PUT    /api/v1/assets/{isin}/price` — Manual price entry.
- `GET    /api/v1/assets/{isin}/price-history` — Price history (paginated, filterable by date range).

### Import
- `POST   /api/v1/import/{broker}` — Upload Excel file and import transactions. `{broker}` = `fineco` | `bgsaxo`.

### Operational
- `GET    /health` — Liveness.
- `GET    /ready` — Readiness.

---

## 6. Configuration

All configuration is managed through environment variables only (12-factor). A `.env.example` file is committed to the repo with all parameters and placeholder values. Developers copy it to `.env` (gitignored) for local use. In production, docker-compose uses `env_file` to load a local `.env` on the Pi. Key parameters:

- `DATABASE_URL` — PostgreSQL connection string.
- `JWT_SECRET` — Secret for JWT signing.
- `JWT_ACCESS_TOKEN_EXPIRY` — Access token duration (default: `1h`).
- `JWT_REFRESH_TOKEN_EXPIRY` — Refresh token duration (default: `7d`).
- `YAHOO_FINANCE_BASE_URL` — Base URL for Yahoo Finance API.
- `OPENFIGI_API_KEY` — API key for OpenFIGI (optional, the service has a free tier without key).
- `PRICE_UPDATE_CRON` — Cron expression for the daily job (default: `0 0 22 * * *` = every day at 22:00).
- `CORS_ALLOWED_ORIGINS` — Comma-separated list of allowed origins.
- `SERVER_HOST` — Listen host (default: `0.0.0.0`).
- `SERVER_PORT` — Listen port (default: `3000`).
- `RUST_LOG` — tracing log level (default: `makima=info`).
- `BACKFILL_YEARS` — Years of price history to fetch on first import (default: `5`).
- `DATABASE_POOL_MAX_SIZE` — Maximum number of sqlx pool connections (default: `5`). Keep low on Raspberry Pi to conserve RAM.
- `YAHOO_REQUEST_DELAY_MS` — Milliseconds to wait between Yahoo Finance requests (default: `1500`).

---

## 7. Database Schema (conceptual model)

### users
- `id` UUID PK
- `email` VARCHAR UNIQUE NOT NULL
- `password_hash` VARCHAR NOT NULL
- `created_at` TIMESTAMPTZ
- `updated_at` TIMESTAMPTZ

### refresh_tokens
- `id` UUID PK
- `user_id` UUID FK → users
- `token_hash` VARCHAR NOT NULL (SHA-256 hash of the opaque token)
- `expires_at` TIMESTAMPTZ NOT NULL
- `revoked` BOOLEAN DEFAULT false
- `created_at` TIMESTAMPTZ

### portfolios
- `id` UUID PK
- `user_id` UUID FK → users
- `name` VARCHAR NOT NULL
- `description` TEXT
- `base_currency` VARCHAR DEFAULT 'EUR'
- `created_at` TIMESTAMPTZ
- `updated_at` TIMESTAMPTZ

### assets
- `id` UUID PK
- `isin` VARCHAR UNIQUE NOT NULL
- `yahoo_ticker` VARCHAR
- `name` VARCHAR NOT NULL
- `asset_class` ENUM (Stock, Bond, Commodity, Alternative, Crypto, CashEquivalent)
- `currency` VARCHAR NOT NULL
- `exchange` VARCHAR
- `created_at` TIMESTAMPTZ
- `updated_at` TIMESTAMPTZ

### transactions
- `id` UUID PK
- `portfolio_id` UUID FK → portfolios (ON DELETE CASCADE)
- `asset_id` UUID FK → assets (ON DELETE RESTRICT)
- `transaction_type` ENUM (Buy, Sell, Dividend, Coupon)
- `date` DATE NOT NULL
- `settlement_date` DATE
- `quantity` NUMERIC(18,8) — NULL for Dividend/Coupon
- `unit_price` NUMERIC(18,8) — NULL for Dividend/Coupon
- `commission` NUMERIC(18,4) DEFAULT 0
- `currency` VARCHAR NOT NULL
- `exchange_rate_to_base` NUMERIC(18,8) NOT NULL — Rate to EUR at time of operation; EUR uses `1`, broker import currently persists a fallback numeric value plus warning when lookup is unavailable
- `gross_amount` NUMERIC(18,4) — For Dividend/Coupon
- `tax_withheld` NUMERIC(18,4) — For Dividend/Coupon
- `net_amount` NUMERIC(18,4) — For Dividend/Coupon
- `notes` TEXT
- `import_hash` VARCHAR — For import duplicate detection
- `created_at` TIMESTAMPTZ
- `updated_at` TIMESTAMPTZ

### price_history
- `id` UUID PK
- `asset_id` UUID FK → assets
- `date` DATE NOT NULL
- `close_price` NUMERIC(18,8) NOT NULL
- `currency` VARCHAR NOT NULL
- `source` VARCHAR — 'yahoo', 'manual'
- UNIQUE(asset_id, date)

### exchange_rates
- `id` UUID PK
- `from_currency` VARCHAR NOT NULL
- `to_currency` VARCHAR NOT NULL
- `date` DATE NOT NULL
- `rate` NUMERIC(18,8) NOT NULL
- UNIQUE(from_currency, to_currency, date)

---

## 8. Out of scope for MVP

The following features are explicitly excluded from the MVP. They will be evaluated for future iterations.

- **Portfolio benchmark vs index** (e.g. comparison with S&P500) — requires time-weighted return calculation and dedicated UI.
- **Periodic portfolio snapshots** — daily save of total portfolio value for historical charts. For the MVP everything is calculated on-the-fly.
- **Notifications** — alerts on prices, bond maturities, incoming dividends.
- **Rate limiting** — not needed with 2–5 users on a private network.
- **CSV/JSON export** — clients read everything via API.
- **Short selling** — asset quantity cannot be negative.
- **Email verification** — direct registration without confirmation.
- **Reverse proxy** — Axum serves directly. To be added with internet exposure.
- **Internet exposure** — Tailscale only for the MVP.
- **Roles and permissions** — all users are equal, no admin.
- **Crypto support** — the asset class exists in the enum but no specific parser or logic is implemented.
- **Multi-currency portfolio base** — base currency is fixed to EUR.
- **Tax calculations** — no capital gain calculation logic for tax declarations.
- **Automatic reconciliation** across brokers — the user imports manually from each broker.
- **Graceful shutdown** — orderly shutdown of in-flight requests and background jobs on container stop. To be added post-MVP.
- **Soft delete** — for the MVP, deleting a portfolio cascades hard-deletes to its transactions. Soft delete with recovery to be evaluated later.
- **Separate migration container** — migrations are embedded in the binary for the MVP. Separate init container approach to be considered when scaling.
