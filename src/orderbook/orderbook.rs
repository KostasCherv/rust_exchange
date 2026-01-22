use std::collections::{BTreeMap, HashMap, VecDeque};
use uuid::Uuid;
use chrono::Utc;

use crate::types::order::{Order, OrderSide, OrderType, OrderStatus};
type PriceLevel = VecDeque<OrderId>;
type Price = i64;
type OrderId = Uuid;
type Qty = u64;

pub struct OrderBook {
    bids: BTreeMap<Price, PriceLevel>,
    asks: BTreeMap<Price, PriceLevel>,
    orders: HashMap<OrderId, Order>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::new(),
        }
    }

    pub fn add_order(&mut self, user_id: Uuid, price: Price, qty: Qty, side: OrderSide) -> Order {
        let order = Order {
            id: Uuid::new_v4(),
            user_id,
            side,
            order_type: OrderType::Limit, // Default to Limit for now
            price,
            quantity: qty,
            status: OrderStatus::Pending,
            timestamp: Utc::now(),
        };

        // Store order once in lookup map (single source of truth)
        let order_id = order.id;
        self.orders.insert(order_id, order.clone());

        // Add only OrderId to price level (FIFO queue)
        match side {
            OrderSide::Buy => {
                self.bids.entry(price)
                    .or_insert_with(VecDeque::new)
                    .push_back(order_id);
            }
            OrderSide::Sell => {
                self.asks.entry(price)
                    .or_insert_with(VecDeque::new)
                    .push_back(order_id);
            }
        }

        order
    }

    pub fn best_bid(&self) -> Option<Price> {
        self.bids.iter()
            .rev()
            .next()
            .map(|(&price, _)| price)
    }

    pub fn best_ask(&self) -> Option<Price> {
        self.asks.iter()
            .next()
            .map(|(&price, _)| price)
    }
}