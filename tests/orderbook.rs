//! Orderbook integration tests: matching engine, lifecycle, edge cases, WebSocket broadcasts.

use rust_exchange::api::routes::WsMessage;
use rust_exchange::orderbook::orderbook::OrderBook;
use rust_exchange::types::order::{OrderSide, OrderStatus, OrderType};
use std::time::Duration;
use tokio::sync::broadcast;
use uuid::Uuid;

const SYMBOL: &str = "BTCUSDT";

fn scale_price(p: i64) -> i64 {
    p * 100_000_000
}

// --- Matching engine ---

#[test]
fn no_match_order_rests() {
    let mut book = OrderBook::new();
    let user_id = Uuid::nil();
    let price = scale_price(50_000);
    let qty = 10u64;

    let (order, trades) = book.add_order(user_id, price, qty, OrderSide::Buy, OrderType::Limit, None, None);

    assert!(trades.is_empty());
    assert_eq!(order.quantity, qty);
    assert_eq!(order.status, OrderStatus::Pending);
    let bids = book.get_bids();
    assert_eq!(bids.len(), 1);
    assert_eq!(bids[0], (price, qty));
}

#[test]
fn full_fill_buy() {
    let mut book = OrderBook::new();
    let seller = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);
    let qty = 10u64;

    let (sell_order, sell_trades) =
        book.add_order(seller, price, qty, OrderSide::Sell, OrderType::Limit, None, None);
    assert!(sell_trades.is_empty());
    assert_eq!(sell_order.quantity, qty);

    let (buy_order, buy_trades) = book.add_order(buyer, price, qty, OrderSide::Buy, OrderType::Limit, None, None);
    assert_eq!(buy_trades.len(), 1);
    assert_eq!(buy_trades[0].price, price);
    assert_eq!(buy_trades[0].quantity, qty);
    assert_eq!(buy_order.quantity, 0);
    assert_eq!(buy_order.status, OrderStatus::Filled);

    assert!(book.get_bids().is_empty());
    assert!(book.get_asks().is_empty());
}

#[test]
fn full_fill_sell() {
    let mut book = OrderBook::new();
    let buyer = Uuid::new_v4();
    let seller = Uuid::new_v4();
    let price = scale_price(50_000);
    let qty = 10u64;

    let (_buy_order, buy_trades) = book.add_order(buyer, price, qty, OrderSide::Buy, OrderType::Limit, None, None);
    assert!(buy_trades.is_empty());

    let (sell_order, sell_trades) =
        book.add_order(seller, price, qty, OrderSide::Sell, OrderType::Limit, None, None);
    assert_eq!(sell_trades.len(), 1);
    assert_eq!(sell_trades[0].quantity, qty);
    assert_eq!(sell_order.quantity, 0);
    assert_eq!(sell_order.status, OrderStatus::Filled);

    assert!(book.get_bids().is_empty());
    assert!(book.get_asks().is_empty());
}

#[test]
fn partial_fill() {
    let mut book = OrderBook::new();
    let seller = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);

    let (sell_order, _) = book.add_order(seller, price, 10, OrderSide::Sell, OrderType::Limit, None, None);
    let (buy_order, buy_trades) =
        book.add_order(buyer, price, 4, OrderSide::Buy, OrderType::Limit, None, None);

    assert_eq!(buy_trades.len(), 1);
    assert_eq!(buy_trades[0].quantity, 4);
    assert_eq!(buy_order.quantity, 0);
    assert_eq!(buy_order.status, OrderStatus::Filled);

    let asks = book.get_asks();
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0], (price, 6));
    let resting = book.get_order_by_id(sell_order.id).unwrap();
    assert_eq!(resting.quantity, 6);
}

#[test]
fn multiple_price_levels_fifo() {
    let mut book = OrderBook::new();
    let user1 = Uuid::new_v4();
    let user2 = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);

    let (sell1, _) = book.add_order(user1, price, 2, OrderSide::Sell, OrderType::Limit, None, None);
    let (sell2, _) = book.add_order(user2, price, 2, OrderSide::Sell, OrderType::Limit, None, None);

    let (buy_order, trades) = book.add_order(buyer, price, 3, OrderSide::Buy, OrderType::Limit, None, None);

    assert_eq!(trades.len(), 2);
    assert_eq!(trades[0].quantity, 2);
    assert_eq!(trades[1].quantity, 1);
    assert_eq!(trades[0].maker_order_id, sell1.id);
    assert_eq!(trades[1].maker_order_id, sell2.id);
    assert_eq!(buy_order.quantity, 0);

    let asks = book.get_asks();
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0], (price, 1));
}

// --- Order lifecycle ---

#[test]
fn create_rest_get_order_by_id() {
    let mut book = OrderBook::new();
    let user_id = Uuid::new_v4();
    let (order, _) = book.add_order(
        user_id,
        scale_price(50_000),
        5,
        OrderSide::Buy,
        OrderType::Limit,
        None,
        None,
    );

    let found = book.get_order_by_id(order.id).unwrap();
    assert_eq!(found.id, order.id);
    assert_eq!(found.quantity, 5);
    assert_eq!(book.get_bids().len(), 1);
}

#[test]
fn create_match_full_fill_both_filled() {
    let mut book = OrderBook::new();
    let seller = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);
    let qty = 10u64;

    let (sell_order, _) = book.add_order(seller, price, qty, OrderSide::Sell, OrderType::Limit, None, None);
    let (buy_order, trades) = book.add_order(buyer, price, qty, OrderSide::Buy, OrderType::Limit, None, None);

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].price, price);
    assert_eq!(trades[0].quantity, qty);
    assert_eq!(buy_order.quantity, 0);
    assert_eq!(buy_order.status, OrderStatus::Filled);
    assert!(book.get_order_by_id(sell_order.id).is_none());
    assert!(book.get_bids().is_empty());
    assert!(book.get_asks().is_empty());
}

#[test]
fn create_match_partial_fill_remainder_on_book() {
    let mut book = OrderBook::new();
    let seller = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);

    let (sell_order, _) = book.add_order(seller, price, 10, OrderSide::Sell, OrderType::Limit, None, None);
    book.add_order(buyer, price, 4, OrderSide::Buy, OrderType::Limit, None, None);

    let resting = book.get_order_by_id(sell_order.id).unwrap();
    assert_eq!(resting.quantity, 6);
    assert_eq!(book.get_recent_trades(10).len(), 1);
}

// --- Edge cases ---

#[test]
fn cancel_removes_order_and_updates_book() {
    let mut book = OrderBook::new();
    let user_id = Uuid::new_v4();
    let (order, _) = book.add_order(
        user_id,
        scale_price(50_000),
        10,
        OrderSide::Buy,
        OrderType::Limit,
        None,
        None,
    );

    let removed = book.remove_order(order.id, None, None);
    assert!(removed.is_some());
    assert!(book.get_order_by_id(order.id).is_none());
    assert!(book.get_bids().is_empty());
}

#[test]
fn no_match_price_gap_both_rest() {
    let mut book = OrderBook::new();
    let buyer = Uuid::new_v4();
    let seller = Uuid::new_v4();

    let (buy_order, buy_trades) = book.add_order(
        buyer,
        scale_price(49_000),
        10,
        OrderSide::Buy,
        OrderType::Limit,
        None,
        None,
    );
    let (sell_order, sell_trades) = book.add_order(
        seller,
        scale_price(51_000),
        10,
        OrderSide::Sell,
        OrderType::Limit,
        None,
        None,
    );

    assert!(buy_trades.is_empty());
    assert!(sell_trades.is_empty());
    assert_eq!(buy_order.quantity, 10);
    assert_eq!(sell_order.quantity, 10);
    assert_eq!(book.get_bids().len(), 1);
    assert_eq!(book.get_asks().len(), 1);
}

#[test]
fn partial_fill_resting_fully_filled_incoming_rests() {
    let mut book = OrderBook::new();
    let seller = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);

    let (sell_order, _) = book.add_order(seller, price, 5, OrderSide::Sell, OrderType::Limit, None, None);
    let (buy_order, trades) = book.add_order(buyer, price, 10, OrderSide::Buy, OrderType::Limit, None, None);

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].quantity, 5);
    assert_eq!(buy_order.quantity, 5);
    assert_eq!(buy_order.status, OrderStatus::PartiallyFilled);
    assert!(book.get_order_by_id(sell_order.id).is_none());
    let bids = book.get_bids();
    assert_eq!(bids.len(), 1);
    assert_eq!(bids[0], (price, 5));
}

// --- Market orders ---

#[test]
fn market_buy_with_liquidity_full_fill() {
    let mut book = OrderBook::new();
    let seller = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);
    let qty = 10u64;

    book.add_order(seller, price, qty, OrderSide::Sell, OrderType::Limit, None, None);
    let (buy_order, trades) = book.add_order(
        buyer,
        0,
        qty,
        OrderSide::Buy,
        OrderType::Market,
        None,
        None,
    );

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].price, price);
    assert_eq!(trades[0].quantity, qty);
    assert_eq!(buy_order.quantity, 0);
    assert_eq!(buy_order.status, OrderStatus::Filled);
    assert!(book.get_bids().is_empty());
}

#[test]
fn market_buy_partial_fill_does_not_rest() {
    let mut book = OrderBook::new();
    let seller = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);

    book.add_order(seller, price, 3, OrderSide::Sell, OrderType::Limit, None, None);
    let (buy_order, trades) = book.add_order(
        buyer,
        0,
        10,
        OrderSide::Buy,
        OrderType::Market,
        None,
        None,
    );

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].quantity, 3);
    assert_eq!(buy_order.quantity, 7);
    assert_eq!(buy_order.status, OrderStatus::PartiallyFilled);
    assert!(book.get_bids().is_empty());
}

#[test]
fn market_buy_no_liquidity() {
    let mut book = OrderBook::new();
    let buyer = Uuid::new_v4();
    let qty = 5u64;

    let (order, trades) = book.add_order(
        buyer,
        0,
        qty,
        OrderSide::Buy,
        OrderType::Market,
        None,
        None,
    );

    assert!(trades.is_empty());
    assert_eq!(order.quantity, qty);
    assert_eq!(order.status, OrderStatus::Pending);
    assert!(book.get_bids().is_empty());
}

#[test]
fn market_sell_with_liquidity_full_fill() {
    let mut book = OrderBook::new();
    let buyer = Uuid::new_v4();
    let seller = Uuid::new_v4();
    let price = scale_price(50_000);
    let qty = 10u64;

    book.add_order(buyer, price, qty, OrderSide::Buy, OrderType::Limit, None, None);
    let (sell_order, trades) = book.add_order(
        seller,
        0,
        qty,
        OrderSide::Sell,
        OrderType::Market,
        None,
        None,
    );

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].price, price);
    assert_eq!(trades[0].quantity, qty);
    assert_eq!(sell_order.quantity, 0);
    assert_eq!(sell_order.status, OrderStatus::Filled);
    assert!(book.get_asks().is_empty());
}

#[test]
fn market_sell_partial_fill_does_not_rest() {
    let mut book = OrderBook::new();
    let buyer = Uuid::new_v4();
    let seller = Uuid::new_v4();
    let price = scale_price(50_000);

    book.add_order(buyer, price, 3, OrderSide::Buy, OrderType::Limit, None, None);
    let (sell_order, trades) = book.add_order(
        seller,
        0,
        10,
        OrderSide::Sell,
        OrderType::Market,
        None,
        None,
    );

    assert_eq!(trades.len(), 1);
    assert_eq!(trades[0].quantity, 3);
    assert_eq!(sell_order.quantity, 7);
    assert_eq!(sell_order.status, OrderStatus::PartiallyFilled);
    assert!(book.get_asks().is_empty());
}

#[test]
fn market_sell_no_liquidity() {
    let mut book = OrderBook::new();
    let seller = Uuid::new_v4();
    let qty = 5u64;

    let (order, trades) = book.add_order(
        seller,
        0,
        qty,
        OrderSide::Sell,
        OrderType::Market,
        None,
        None,
    );

    assert!(trades.is_empty());
    assert_eq!(order.quantity, qty);
    assert_eq!(order.status, OrderStatus::Pending);
    assert!(book.get_asks().is_empty());
}

// --- WebSocket broadcasts ---

#[tokio::test]
async fn trade_broadcast_on_match() {
    let mut book = OrderBook::new();
    let (tx, _) = broadcast::channel(32);
    let mut rx = tx.subscribe();
    let seller = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);
    let qty = 10u64;

    book.add_order(seller, price, qty, OrderSide::Sell, OrderType::Limit, None, None);
    book.add_order(buyer, price, qty, OrderSide::Buy, OrderType::Limit, Some(&tx), Some(SYMBOL));

    let msg = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("timeout waiting for Trade")
        .expect("recv");
    match &msg {
        WsMessage::Trade { symbol, trade } => {
            assert_eq!(symbol, SYMBOL);
            assert_eq!(trade.price, price);
            assert_eq!(trade.quantity, qty);
        }
        _ => panic!("expected Trade, got {:?}", msg),
    }
}

#[tokio::test]
async fn orderbook_update_broadcast_after_trade() {
    let mut book = OrderBook::new();
    let (tx, _) = broadcast::channel(32);
    let mut rx = tx.subscribe();
    let seller = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);
    let qty = 10u64;

    book.add_order(seller, price, qty, OrderSide::Sell, OrderType::Limit, Some(&tx), Some(SYMBOL));
    book.add_order(buyer, price, qty, OrderSide::Buy, OrderType::Limit, Some(&tx), Some(SYMBOL));

    let mut seen_trade = false;
    let mut seen_empty_ob = false;
    for _ in 0..4 {
        let msg = tokio::time::timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("timeout")
            .expect("recv");
        match &msg {
            WsMessage::Trade { symbol, .. } => {
                assert_eq!(symbol, SYMBOL);
                seen_trade = true;
            }
            WsMessage::OrderBookUpdate { symbol, bids, asks } => {
                assert_eq!(symbol, SYMBOL);
                if bids.is_empty() && asks.is_empty() {
                    seen_empty_ob = true;
                    break;
                }
            }
        }
    }
    assert!(seen_trade, "expected at least one Trade message");
    assert!(seen_empty_ob, "expected OrderBookUpdate with empty book");
}

#[tokio::test]
async fn cancel_broadcast_orderbook_update() {
    let mut book = OrderBook::new();
    let (tx, _) = broadcast::channel(32);
    let mut rx = tx.subscribe();
    let user_id = Uuid::new_v4();

    let (order, _) = book.add_order(
        user_id,
        scale_price(50_000),
        10,
        OrderSide::Buy,
        OrderType::Limit,
        Some(&tx),
        Some(SYMBOL),
    );
    let _first_ob = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("timeout")
        .expect("recv");

    book.remove_order(order.id, Some(&tx), Some(SYMBOL));
    let msg = tokio::time::timeout(Duration::from_millis(100), rx.recv())
        .await
        .expect("timeout")
        .expect("recv");
    match &msg {
        WsMessage::OrderBookUpdate { symbol, bids, .. } => {
            assert_eq!(symbol, SYMBOL);
            assert!(bids.is_empty());
        }
        _ => panic!("expected OrderBookUpdate after cancel, got {:?}", msg),
    }
}
