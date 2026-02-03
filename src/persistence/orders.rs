//! Order persistence: insert, update status, list open by symbol.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

fn side_to_str(side: crate::types::order::OrderSide) -> &'static str {
    match side {
        crate::types::order::OrderSide::Buy => "Buy",
        crate::types::order::OrderSide::Sell => "Sell",
    }
}

fn order_type_to_str(ot: crate::types::order::OrderType) -> &'static str {
    match ot {
        crate::types::order::OrderType::Limit => "Limit",
        crate::types::order::OrderType::Market => "Market",
    }
}

fn status_to_str(s: crate::types::order::OrderStatus) -> &'static str {
    match s {
        crate::types::order::OrderStatus::Pending => "Pending",
        crate::types::order::OrderStatus::PartiallyFilled => "PartiallyFilled",
        crate::types::order::OrderStatus::Filled => "Filled",
        crate::types::order::OrderStatus::Cancelled => "Cancelled",
    }
}

/// Insert an order (after create or match).
#[allow(clippy::too_many_arguments)]
pub async fn insert_order(
    pool: &PgPool,
    id: Uuid,
    user_id: Uuid,
    symbol: &str,
    side: crate::types::order::OrderSide,
    order_type: crate::types::order::OrderType,
    price: i64,
    quantity: u64,
    status: crate::types::order::OrderStatus,
    created_at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO orders (id, user_id, symbol, side, order_type, price, quantity, status, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(user_id)
    .bind(symbol)
    .bind(side_to_str(side))
    .bind(order_type_to_str(order_type))
    .bind(price)
    .bind(quantity as i64)
    .bind(status_to_str(status))
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update order status (e.g. on cancel or fill).
pub async fn update_order_status(
    pool: &PgPool,
    id: Uuid,
    status: crate::types::order::OrderStatus,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE orders SET status = $1 WHERE id = $2")
        .bind(status_to_str(status))
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, sqlx::FromRow)]
pub struct OrderRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub symbol: String,
    pub side: String,
    pub order_type: String,
    pub price: i64,
    pub quantity: i64,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// Get a single order by id (for GET /orders/{id}).
pub async fn get_order_by_id(
    pool: &PgPool,
    order_id: Uuid,
) -> Result<Option<OrderRow>, sqlx::Error> {
    let row = sqlx::query_as::<_, OrderRow>(
        "SELECT id, user_id, symbol, side, order_type, price, quantity, status, created_at \
         FROM orders WHERE id = $1",
    )
    .bind(order_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// List open orders (Pending or PartiallyFilled) for a symbol, for hydration.
pub async fn list_open_orders_by_symbol(
    pool: &PgPool,
    symbol: &str,
) -> Result<Vec<OrderRow>, sqlx::Error> {
    let rows = sqlx::query_as::<_, OrderRow>(
        "SELECT id, user_id, symbol, side, order_type, price, quantity, status, created_at \
         FROM orders WHERE symbol = $1 AND status IN ('Pending', 'PartiallyFilled') ORDER BY created_at",
    )
    .bind(symbol)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

fn str_to_side(s: &str) -> Option<crate::types::order::OrderSide> {
    match s {
        "Buy" => Some(crate::types::order::OrderSide::Buy),
        "Sell" => Some(crate::types::order::OrderSide::Sell),
        _ => None,
    }
}

fn str_to_order_type(s: &str) -> Option<crate::types::order::OrderType> {
    match s {
        "Limit" => Some(crate::types::order::OrderType::Limit),
        "Market" => Some(crate::types::order::OrderType::Market),
        _ => None,
    }
}

fn str_to_status(s: &str) -> Option<crate::types::order::OrderStatus> {
    match s {
        "Pending" => Some(crate::types::order::OrderStatus::Pending),
        "PartiallyFilled" => Some(crate::types::order::OrderStatus::PartiallyFilled),
        "Filled" => Some(crate::types::order::OrderStatus::Filled),
        "Cancelled" => Some(crate::types::order::OrderStatus::Cancelled),
        _ => None,
    }
}

/// Convert OrderRow to Order for hydration. Skips invalid rows (quantity > 0).
pub fn order_row_to_order(row: &OrderRow) -> Option<crate::types::order::Order> {
    let side = str_to_side(&row.side)?;
    let order_type = str_to_order_type(&row.order_type)?;
    let status = str_to_status(&row.status)?;
    let quantity = row.quantity.try_into().ok().filter(|&q: &u64| q > 0)?;
    Some(crate::types::order::Order {
        id: row.id,
        user_id: row.user_id,
        side,
        order_type,
        price: row.price,
        quantity,
        status,
        timestamp: row.created_at,
    })
}

/// Convert OrderRow to Order for display (GET /orders/{id}). Allows quantity >= 0 (filled orders).
pub fn order_row_to_order_display(row: &OrderRow) -> Option<crate::types::order::Order> {
    let side = str_to_side(&row.side)?;
    let order_type = str_to_order_type(&row.order_type)?;
    let status = str_to_status(&row.status)?;
    let quantity = row.quantity.max(0) as u64;
    Some(crate::types::order::Order {
        id: row.id,
        user_id: row.user_id,
        side,
        order_type,
        price: row.price,
        quantity,
        status,
        timestamp: row.created_at,
    })
}
