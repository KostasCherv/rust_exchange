//! Position tracking: update_position, get_positions, unrealized_pnl.
//! Testable without HTTP.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::types::order::{OrderSide, Price, Qty};
use crate::types::position::Position;

pub type SharedPositions = Arc<RwLock<HashMap<(Uuid, String), Position>>>;

/// Apply one trade leg: update or create position. Buy adds to position, Sell reduces.
/// Weighted average when adding; remove position when quantity becomes 0.
pub async fn update_position(
    store: &SharedPositions,
    user_id: Uuid,
    symbol: &str,
    side: OrderSide,
    trade_price: Price,
    trade_qty: Qty,
) {
    let mut guard = store.write().await;
    let key = (user_id, symbol.to_uppercase());
    let signed_qty = match side {
        OrderSide::Buy => trade_qty as i64,
        OrderSide::Sell => -(trade_qty as i64),
    };

    let (new_qty, new_avg) = match guard.get(&key) {
        Some(pos) => {
            let old_qty = pos.quantity;
            let new_qty = old_qty + signed_qty;

            if new_qty == 0 {
                guard.remove(&key);
                return;
            }

            // Same sign: same direction (adding to position) -> weighted average
            if (old_qty > 0 && signed_qty > 0) || (old_qty < 0 && signed_qty < 0) {
                let new_avg = (pos.average_price * old_qty + trade_price * signed_qty) / new_qty;
                (new_qty, new_avg)
            } else {
                // Reducing position: no change to average for remaining open quantity
                (new_qty, pos.average_price)
            }
        }
        None => (signed_qty, trade_price),
    };

    guard.insert(
        key,
        Position {
            user_id,
            symbol: symbol.to_uppercase(),
            quantity: new_qty,
            average_price: new_avg,
        },
    );
}

/// Returns positions for a user, optionally filtered by symbol.
pub async fn get_positions(
    store: &SharedPositions,
    user_id: Uuid,
    symbol_filter: Option<&str>,
) -> Vec<Position> {
    let guard = store.read().await;
    let symbol_upper = symbol_filter.map(|s| s.to_uppercase());
    guard
        .iter()
        .filter(|((uid, sym), _)| *uid == user_id && symbol_upper.as_ref().is_none_or(|s| sym == s))
        .map(|(_, pos)| pos.clone())
        .collect()
}

/// Unrealized P&L: (current_price - average_price) * quantity. Works for long and short.
pub fn unrealized_pnl(position: &Position, current_price: Price) -> i64 {
    (current_price - position.average_price) * position.quantity
}
