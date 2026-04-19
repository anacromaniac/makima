//! PostgreSQL implementation of the [`domain::UserRepository`] port.

use async_trait::async_trait;
use chrono::Utc;
use domain::{RepositoryError, User, UserRepository};
use sqlx::PgPool;
use uuid::Uuid;

/// Internal row type mirroring the `users` table. Implements [`sqlx::FromRow`]
/// without introducing sqlx as a dependency of the domain crate.
#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    email: String,
    password_hash: String,
    created_at: chrono::DateTime<Utc>,
    updated_at: chrono::DateTime<Utc>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        User {
            id: row.id,
            email: row.email,
            password_hash: row.password_hash,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

/// PostgreSQL-backed implementation of [`UserRepository`].
pub struct PgUserRepository {
    pool: PgPool,
}

impl PgUserRepository {
    /// Create a new repository backed by the given connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for PgUserRepository {
    async fn create(&self, email: &str, password_hash: &str) -> Result<User, RepositoryError> {
        let id = Uuid::now_v7();
        let now = Utc::now();
        sqlx::query_as::<_, UserRow>(
            "INSERT INTO users (id, email, password_hash, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $4)
             RETURNING id, email, password_hash, created_at, updated_at",
        )
        .bind(id)
        .bind(email)
        .bind(password_hash)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map(Into::into)
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e
                && db_err.code().as_deref() == Some("23505")
            {
                return RepositoryError::Conflict("email already registered".to_string());
            }
            RepositoryError::Internal(e.to_string())
        })
    }

    async fn find_by_email(&self, email: &str) -> Result<Option<User>, RepositoryError> {
        sqlx::query_as::<_, UserRow>(
            "SELECT id, email, password_hash, created_at, updated_at
             FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map(|opt| opt.map(Into::into))
        .map_err(|e| RepositoryError::Internal(e.to_string()))
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, RepositoryError> {
        sqlx::query_as::<_, UserRow>(
            "SELECT id, email, password_hash, created_at, updated_at
             FROM users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map(|opt| opt.map(Into::into))
        .map_err(|e| RepositoryError::Internal(e.to_string()))
    }

    async fn update_password(&self, id: Uuid, new_hash: &str) -> Result<(), RepositoryError> {
        let now = Utc::now();
        sqlx::query("UPDATE users SET password_hash = $1, updated_at = $2 WHERE id = $3")
            .bind(new_hash)
            .bind(now)
            .bind(id)
            .execute(&self.pool)
            .await
            .map(|_| ())
            .map_err(|e| RepositoryError::Internal(e.to_string()))
    }
}
