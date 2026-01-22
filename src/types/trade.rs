use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

type Price = i64;
type Qty = u64;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub id: Uuid,
    pub maker_order_id: Uuid,
    pub taker_order_id: Uuid,
    pub price: Price,
    pub quantity: Qty,
    pub timestamp: DateTime<Utc>,
}