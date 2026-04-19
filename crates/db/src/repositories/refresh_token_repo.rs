//! PostgreSQL implementation of the [`domain::RefreshTokenRepository`] port.

use async_trait::async_trait;
use chrono::Utc;
use domain::{NewRefreshToken, RefreshToken, RefreshTokenRepository, RepositoryError};
use sqlx::PgPool;
use uuid::Uuid;

/// Internal row type mirroring the `refresh_tokens` table.
#[derive(sqlx::FromRow)]
struct RefreshTokenRow {
    id: Uuid,
    user_id: Uuid,
    token_hash: String,
    expires_at: chrono::DateTime<Utc>,
    revoked: bool,
    created_at: chrono::DateTime<Utc>,
}

impl From<RefreshTokenRow> for RefreshToken {
    fn from(row: RefreshTokenRow) -> Self {
        RefreshToken {
            id: row.id,
            user_id: row.user_id,
            token_hash: row.token_hash,
            expires_at: row.expires_at,
            revoked: row.revoked,
            created_at: row.created_at,
        }
    }
}

/// PostgreSQL-backed implementation of [`RefreshTokenRepository`].
pub struct PgRefreshTokenRepository {
    pool: PgPool,
}

impl PgRefreshTokenRepository {
    /// Create a new repository backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RefreshTokenRepository for PgRefreshTokenRepository {
    async fn create(&self, new_token: &NewRefreshToken) -> Result<RefreshToken, RepositoryError> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        sqlx::query_as::<_, RefreshTokenRow>(
            "INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, revoked, created_at)
             VALUES ($1, $2, $3, $4, false, $5)
             RETURNING id, user_id, token_hash, expires_at, revoked, created_at",
        )
        .bind(id)
        .bind(new_token.user_id)
        .bind(&new_token.token_hash)
        .bind(new_token.expires_at)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map(Into::into)
        .map_err(|e| RepositoryError::Internal(e.to_string()))
    }

    async fn find_by_hash(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshToken>, RepositoryError> {
        sqlx::query_as::<_, RefreshTokenRow>(
            "SELECT id, user_id, token_hash, expires_at, revoked, created_at
             FROM refresh_tokens WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map(|opt| opt.map(Into::into))
        .map_err(|e| RepositoryError::Internal(e.to_string()))
    }

    async fn revoke(&self, id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| RepositoryError::Internal(e.to_string()))
    }

    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<(), RepositoryError> {
        sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE user_id = $1")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| RepositoryError::Internal(e.to_string()))
    }
}
