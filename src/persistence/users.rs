//! User persistence: list and insert.

use sqlx::{FromRow, PgPool};
use uuid::Uuid;

/// Row returned from DB (username is stored lowercase).
#[derive(FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
}

/// List all users (username is lowercase in DB).
pub async fn list_users(pool: &PgPool) -> Result<Vec<UserRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, UserRow>("SELECT id, username, password_hash FROM users")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// Get a user by username (lowercase). For login when reading from DB.
pub async fn get_user_by_username(
    pool: &PgPool,
    username_lowercase: &str,
) -> Result<Option<UserRow>, sqlx::Error> {
    let row = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, password_hash FROM users WHERE username = $1",
    )
    .bind(username_lowercase)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Insert a user. Username must already be lowercase.
pub async fn insert_user(
    pool: &PgPool,
    id: Uuid,
    username: &str,
    password_hash: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO users (id, username, password_hash) VALUES ($1, $2, $3)")
        .bind(id)
        .bind(username)
        .bind(password_hash)
        .execute(pool)
        .await?;
    Ok(())
}
