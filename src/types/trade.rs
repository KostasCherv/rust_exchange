use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trade {
    pub id: Uuid,
    pub maker_order_id: Uuid,
    pub taker_order_id: Uuid,
    pub price: i64,
    pub quantity: u64,
    pub timestamp: DateTime<Utc>,
}