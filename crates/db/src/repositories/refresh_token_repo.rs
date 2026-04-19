//! Repository for refresh token database operations.

use chrono::Utc;
use domain::{NewRefreshToken, RefreshToken};
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

/// Provides database access for the `refresh_tokens` table.
pub struct RefreshTokenRepository;

impl RefreshTokenRepository {
    /// Persist a new refresh token record.
    pub async fn create(
        pool: &PgPool,
        new_token: &NewRefreshToken,
    ) -> Result<RefreshToken, sqlx::Error> {
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
        .fetch_one(pool)
        .await
        .map(Into::into)
    }

    /// Look up a refresh token by its SHA-256 hash.
    pub async fn find_by_hash(
        pool: &PgPool,
        token_hash: &str,
    ) -> Result<Option<RefreshToken>, sqlx::Error> {
        sqlx::query_as::<_, RefreshTokenRow>(
            "SELECT id, user_id, token_hash, expires_at, revoked, created_at
             FROM refresh_tokens WHERE token_hash = $1",
        )
        .bind(token_hash)
        .fetch_optional(pool)
        .await
        .map(|opt| opt.map(Into::into))
    }

    /// Mark a single refresh token as revoked.
    pub async fn revoke(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }

    /// Revoke every refresh token belonging to a user (force logout on all devices).
    pub async fn revoke_all_for_user(pool: &PgPool, user_id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE refresh_tokens SET revoked = true WHERE user_id = $1")
            .bind(user_id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
