//! Trade persistence: insert on match, list for API.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::types::trade::Trade;

#[derive(Debug, FromRow)]
pub struct TradeRow {
    pub id: Uuid,
    pub maker_order_id: Uuid,
    pub taker_order_id: Uuid,
    pub maker_user_id: Uuid,
    pub taker_user_id: Uuid,
    #[allow(dead_code)]
    pub symbol: String,
    pub price: i64,
    pub quantity: i64,
    pub created_at: DateTime<Utc>,
}

fn trade_row_to_trade(row: &TradeRow) -> Trade {
    Trade {
        id: row.id,
        maker_order_id: row.maker_order_id,
        taker_order_id: row.taker_order_id,
        maker_user_id: row.maker_user_id,
        taker_user_id: row.taker_user_id,
        price: row.price,
        quantity: row.quantity as u64,
        timestamp: row.created_at,
    }
}

/// List recent trades for a symbol (for GET /trades).
pub async fn list_trades(
    pool: &PgPool,
    symbol: &str,
    limit: usize,
) -> Result<Vec<Trade>, sqlx::Error> {
    let rows = sqlx::query_as::<_, TradeRow>(
        "SELECT id, maker_order_id, taker_order_id, maker_user_id, taker_user_id, symbol, price, quantity, created_at \
         FROM trades WHERE symbol = $1 ORDER BY created_at DESC LIMIT $2",
    )
    .bind(symbol)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows.iter().map(trade_row_to_trade).collect())
}

/// List trades for a user (maker or taker), optional symbol (for GET /trades/me).
pub async fn list_trades_for_user(
    pool: &PgPool,
    user_id: Uuid,
    symbol_opt: Option<&str>,
    limit: usize,
) -> Result<Vec<Trade>, sqlx::Error> {
    let rows = if let Some(symbol) = symbol_opt {
        sqlx::query_as::<_, TradeRow>(
            "SELECT id, maker_order_id, taker_order_id, maker_user_id, taker_user_id, symbol, price, quantity, created_at \
             FROM trades WHERE (maker_user_id = $1 OR taker_user_id = $1) AND symbol = $2 ORDER BY created_at DESC LIMIT $3",
        )
        .bind(user_id)
        .bind(symbol)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, TradeRow>(
            "SELECT id, maker_order_id, taker_order_id, maker_user_id, taker_user_id, symbol, price, quantity, created_at \
             FROM trades WHERE maker_user_id = $1 OR taker_user_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(user_id)
        .bind(limit as i64)
        .fetch_all(pool)
        .await?
    };
    Ok(rows.iter().map(trade_row_to_trade).collect())
}

/// Insert a single trade (call after each match).
#[allow(clippy::too_many_arguments)]
pub async fn insert_trade(
    pool: &PgPool,
    id: Uuid,
    maker_order_id: Uuid,
    taker_order_id: Uuid,
    maker_user_id: Uuid,
    taker_user_id: Uuid,
    symbol: &str,
    price: i64,
    quantity: u64,
    created_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO trades (id, maker_order_id, taker_order_id, maker_user_id, taker_user_id, symbol, price, quantity, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(maker_order_id)
    .bind(taker_order_id)
    .bind(maker_user_id)
    .bind(taker_user_id)
    .bind(symbol)
    .bind(price)
    .bind(quantity as i64)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}
