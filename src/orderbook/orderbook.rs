use std::collections::{BTreeMap, HashMap, VecDeque};
use std::collections::btree_map::Entry;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::Utc;

use crate::types::order::{Order, OrderId, OrderSide, OrderStatus, OrderType, Price, Qty};
use crate::types::trade::Trade;

type PriceLevel = VecDeque<OrderId>;

// Type alias for shared OrderBook state
pub type SharedOrderBook = Arc<RwLock<OrderBook>>;

pub struct OrderBook {
    bids: BTreeMap<Price, PriceLevel>,
    asks: BTreeMap<Price, PriceLevel>,
    orders: HashMap<OrderId, Order>,
    trades: VecDeque<Trade>,
}

impl OrderBook {
    pub fn new() -> Self {
        Self {
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            orders: HashMap::new(),
            trades: VecDeque::new(),
        }
    }

    pub fn add_order(&mut self, user_id: Uuid, price: Price, qty: Qty, side: OrderSide) -> Order {
        // Create the order
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

        // Try to match the order first
        let (trades, matched_order) = self.match_order(order);
        
        // Store all trades
        self.store_trades(trades);

        // If there's remaining quantity, add it to the book
        if matched_order.quantity > 0 {
            let order_id = matched_order.id;
            
            // Store order in lookup map
            self.orders.insert(order_id, matched_order.clone());

            // Add only OrderId to price level (FIFO queue)
            match matched_order.side {
                OrderSide::Buy => {
                    self.bids.entry(matched_order.price)
                        .or_insert_with(VecDeque::new)
                        .push_back(order_id);
                }
                OrderSide::Sell => {
                    self.asks.entry(matched_order.price)
                        .or_insert_with(VecDeque::new)
                        .push_back(order_id);
                }
            }
        }
        // If quantity is 0, order is fully filled and already has correct status

        matched_order
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


    pub fn remove_order(&mut self, order_id: OrderId) -> Option<Order> {
        // First, get the order to find its price and side
        let order = self.orders.get(&order_id)?;
        let price = order.price;
        let side = order.side;

        // Select the correct price level map based on side
        let price_levels = match side {
            OrderSide::Buy => &mut self.bids,
            OrderSide::Sell => &mut self.asks,
        };

        // Remove order ID from the price level's queue
        if let Entry::Occupied(mut entry) = price_levels.entry(price) {
            let queue = entry.get_mut();
            queue.retain(|&oid| oid != order_id);
            
            // If the queue is now empty, remove this price level completely
            if queue.is_empty() {
                entry.remove();
            }
        }

        // Remove the order from the global order map and return it
        self.orders.remove(&order_id)
    }

    pub fn get_order_by_id(&self, order_id: OrderId) -> Option<Order> {
        self.orders.get(&order_id).cloned()
    }


    // Match a buy order against asks
    // Iterate through asks from lowest price, match until order filled or no more matches
    pub fn match_buy_order(&mut self, order: &mut Order) -> Vec<Trade> {
        let mut trades = Vec::new();
        let original_qty = order.quantity;

        // Continue matching while there are asks and buy price >= ask price
        while order.quantity > 0 {
            // Get best ask price
            let ask_price = match self.best_ask() {
                Some(price) => price,
                None => break, // No more asks to match
            };

            // Check if buy order can match (buy price must be >= ask price)
            if order.price < ask_price {
                break; // Can't match, price too low
            }

            // Get the price level queue for this ask price
            if let Entry::Occupied(mut entry) = self.asks.entry(ask_price) {
                let queue = entry.get_mut();

                // Get first order from queue (FIFO)
                if let Some(maker_order_id) = queue.front().copied() {
                    // Lookup full maker order (clone to avoid borrow issues)
                    if let Some(maker_order) = self.orders.get(&maker_order_id).cloned() {
                        // Calculate match quantity (min of both)
                        let match_qty = order.quantity.min(maker_order.quantity);

                        // Create trade (maker price = ask price)
                        let trade = Self::create_trade(maker_order_id, order.id, ask_price, match_qty);
                        trades.push(trade);

                        // Update incoming order quantity
                        order.quantity -= match_qty;
                        order.status = Self::update_order_status(original_qty, order.quantity);

                        // Update maker order
                        let mut updated_maker = maker_order;
                        updated_maker.quantity -= match_qty;
                        let maker_original_qty = updated_maker.quantity + match_qty;
                        updated_maker.status = Self::update_order_status(maker_original_qty, updated_maker.quantity);

                        // If maker order is fully filled, remove it
                        if updated_maker.quantity == 0 {
                            queue.pop_front(); // Remove from queue (FIFO)
                            self.orders.remove(&maker_order_id); // Remove from HashMap

                            // If price level is now empty, remove it
                            if queue.is_empty() {
                                entry.remove();
                            }
                        } else {
                            // Update maker order in HashMap
                            self.orders.insert(maker_order_id, updated_maker);
                        }
                    } else {
                        // Order not found in HashMap (shouldn't happen, but handle gracefully)
                        queue.pop_front(); // Remove invalid reference
                        if queue.is_empty() {
                            entry.remove();
                        }
                    }
                } else {
                    // Queue is empty, remove price level
                    entry.remove();
                }
            } else {
                break; // Price level doesn't exist (shouldn't happen after best_ask check)
            }
        }

        trades
    }

    // Match a sell order against bids  
    // Iterate through bids from highest price, match until order filled or no more matches
    pub fn match_sell_order(&mut self, order: &mut Order) -> Vec<Trade> {
        let mut trades = Vec::new();
        let original_qty = order.quantity;

        // Continue matching while there are bids and sell price <= bid price
        while order.quantity > 0 {
            // Get best bid price
            let bid_price = match self.best_bid() {
                Some(price) => price,
                None => break, // No more bids to match
            };

            // Check if sell order can match (sell price must be <= bid price)
            if order.price > bid_price {
                break; // Can't match, price too low
            }

            // Get the price level queue for this bid price
            if let Entry::Occupied(mut entry) = self.bids.entry(bid_price) {
                let queue = entry.get_mut();

                // Get first order from queue (FIFO)
                if let Some(maker_order_id) = queue.front().copied() {
                    // Lookup full maker order (clone to avoid borrow issues)
                    if let Some(maker_order) = self.orders.get(&maker_order_id).cloned() {
                        // Calculate match quantity (min of both)
                        let match_qty = order.quantity.min(maker_order.quantity);

                        // Create trade (maker price = bid price)
                        let trade = Self::create_trade(maker_order_id, order.id, bid_price, match_qty);
                        trades.push(trade);

                        // Update incoming order quantity
                        order.quantity -= match_qty;
                        order.status = Self::update_order_status(original_qty, order.quantity);

                        // Update maker order
                        let mut updated_maker = maker_order;
                        updated_maker.quantity -= match_qty;
                        let maker_original_qty = updated_maker.quantity + match_qty;
                        updated_maker.status = Self::update_order_status(maker_original_qty, updated_maker.quantity);

                        // If maker order is fully filled, remove it
                        if updated_maker.quantity == 0 {
                            queue.pop_front(); // Remove from queue (FIFO)
                            self.orders.remove(&maker_order_id); // Remove from HashMap

                            // If price level is now empty, remove it
                            if queue.is_empty() {
                                entry.remove();
                            }
                        } else {
                            // Update maker order in HashMap
                            self.orders.insert(maker_order_id, updated_maker);
                        }
                    } else {
                        // Order not found in HashMap (shouldn't happen, but handle gracefully)
                        queue.pop_front(); // Remove invalid reference
                        if queue.is_empty() {
                            entry.remove();
                        }
                    }
                } else {
                    // Queue is empty, remove price level
                    entry.remove();
                }
            } else {
                break; // Price level doesn't exist (shouldn't happen after best_bid check)
            }
        }

        trades
    }


    // Main matching function - processes incoming order and matches with opposite side
    // Returns vector of trades created and the order (with updated quantity/status)
    pub fn match_order(&mut self, mut order: Order) -> (Vec<Trade>, Order) {
        let trades = match order.side {
            OrderSide::Buy => self.match_buy_order(&mut order),
            OrderSide::Sell => self.match_sell_order(&mut order)
        };
        
        // Always return the order (even if fully filled, quantity will be 0)
        (trades, order)
    }

    // Store trades and maintain size limit
    fn store_trades(&mut self, trades: Vec<Trade>) {
        // Add all new trades
        for trade in trades {
            self.trades.push_back(trade);
        }
        
        // Keep only recent trades (limit to last 1000)
        const MAX_TRADES: usize = 1000;
        while self.trades.len() > MAX_TRADES {
            self.trades.pop_front();
        }
    }

    // Get recent trades (most recent first)
    pub fn get_recent_trades(&self, limit: usize) -> Vec<Trade> {
        self.trades
            .iter()
            .rev()
            .take(limit)
            .cloned()
            .collect()
    }

    // Get all trades (for debugging/testing)
    pub fn get_all_trades(&self) -> Vec<Trade> {
        self.trades.iter().cloned().collect()
    }

    // Get bids as Vec of (price, total_quantity) pairs
    // Returns highest bid prices first
    pub fn get_bids(&self) -> Vec<(Price, Qty)> {
        self.bids
            .iter()
            .rev()
            .map(|(&price, level)| {
                let total_qty: Qty = level
                    .iter()
                    .filter_map(|&order_id| self.orders.get(&order_id))
                    .map(|order| order.quantity)
                    .sum();
                (price, total_qty)
            })
            .collect()
    }

    // Get asks as Vec of (price, total_quantity) pairs
    // Returns lowest ask prices first
    pub fn get_asks(&self) -> Vec<(Price, Qty)> {
        self.asks
            .iter()
            .map(|(&price, level)| {
                let total_qty: Qty = level
                    .iter()
                    .filter_map(|&order_id| self.orders.get(&order_id))
                    .map(|order| order.quantity)
                    .sum();
                (price, total_qty)
            })
            .collect()
    }

    // Helper: Create a Trade object from matched orders
    // maker = resting order, taker = incoming order, qty = matched quantity
    fn create_trade(maker_order_id: OrderId, taker_order_id: OrderId, price: Price, qty: Qty) -> Trade {
        Trade {
            id: Uuid::new_v4(),
            maker_order_id,
            taker_order_id,
            price,
            quantity: qty,
            timestamp: Utc::now(),
        }
    }

    // Helper: Update order status based on remaining quantity
    // Returns new OrderStatus (Filled, PartiallyFilled, or unchanged)
    fn update_order_status(original_qty: Qty, remaining_qty: Qty) -> OrderStatus {
        if remaining_qty == 0 {
            OrderStatus::Filled
        } else if remaining_qty < original_qty {
            OrderStatus::PartiallyFilled
        } else {
            OrderStatus::Pending
        }
    }

}