//! Repository for user-related database operations.

use chrono::Utc;
use domain::User;
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

/// Provides database access for the `users` table.
pub struct UserRepository;

impl UserRepository {
    /// Persist a new user with the given email and pre-hashed password.
    ///
    /// Returns `Err` with database error code `23505` on duplicate email.
    pub async fn create(
        pool: &PgPool,
        email: &str,
        password_hash: &str,
    ) -> Result<User, sqlx::Error> {
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
        .fetch_one(pool)
        .await
        .map(Into::into)
    }

    /// Find a user by email address.
    pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, UserRow>(
            "SELECT id, email, password_hash, created_at, updated_at
             FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(pool)
        .await
        .map(|opt| opt.map(Into::into))
    }

    /// Find a user by primary key.
    pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as::<_, UserRow>(
            "SELECT id, email, password_hash, created_at, updated_at
             FROM users WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(pool)
        .await
        .map(|opt| opt.map(Into::into))
    }

    /// Update the stored password hash for a user.
    pub async fn update_password(
        pool: &PgPool,
        id: Uuid,
        new_password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now();
        sqlx::query("UPDATE users SET password_hash = $1, updated_at = $2 WHERE id = $3")
            .bind(new_password_hash)
            .bind(now)
            .bind(id)
            .execute(pool)
            .await?;
        Ok(())
    }
}
