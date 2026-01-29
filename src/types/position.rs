use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::order::Price;

/// Position per (user, symbol). Quantity is signed: positive = long, negative = short.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub user_id: Uuid,
    pub symbol: String,
    pub quantity: i64,
    pub average_price: Price,
}
