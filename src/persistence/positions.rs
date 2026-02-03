//! Position persistence: upsert and list for hydration.

use sqlx::PgPool;
use uuid::Uuid;

/// Upsert a position (insert or update on conflict).
pub async fn upsert_position(
    pool: &PgPool,
    user_id: Uuid,
    symbol: &str,
    quantity: i64,
    average_price: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO positions (user_id, symbol, quantity, average_price) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (user_id, symbol) DO UPDATE SET quantity = $3, average_price = $4",
    )
    .bind(user_id)
    .bind(symbol)
    .bind(quantity)
    .bind(average_price)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
pub struct PositionRow {
    pub user_id: Uuid,
    pub symbol: String,
    pub quantity: i64,
    pub average_price: i64,
}

/// List all positions for hydration.
pub async fn list_positions(pool: &PgPool) -> Result<Vec<PositionRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, PositionRow>(
        "SELECT user_id, symbol, quantity, average_price FROM positions",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List positions for a user, optional symbol filter (for GET /positions).
pub async fn list_positions_for_user(
    pool: &PgPool,
    user_id: Uuid,
    symbol_filter: Option<&str>,
) -> Result<Vec<PositionRow>, sqlx::Error> {
    let rows = if let Some(symbol) = symbol_filter {
        sqlx::query_as::<_, PositionRow>(
            "SELECT user_id, symbol, quantity, average_price FROM positions WHERE user_id = $1 AND symbol = $2",
        )
        .bind(user_id)
        .bind(symbol)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, PositionRow>(
            "SELECT user_id, symbol, quantity, average_price FROM positions WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(pool)
        .await?
    };
    Ok(rows)
}
