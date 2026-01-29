//! Position tracking integration tests: update_position, get_positions, unrealized_pnl.

use rust_exchange::positions::{SharedPositions, get_positions, unrealized_pnl, update_position};
use rust_exchange::types::order::OrderSide;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

fn scale_price(p: i64) -> i64 {
    p * 100_000_000
}

fn fresh_store() -> SharedPositions {
    Arc::new(RwLock::new(HashMap::new()))
}

#[tokio::test]
async fn update_position_new_position() {
    let store = fresh_store();
    let user_id = Uuid::new_v4();
    let price = scale_price(50_000);
    let qty = 10u64;

    update_position(&store, user_id, "BTCUSDT", OrderSide::Buy, price, qty).await;

    let positions = get_positions(&store, user_id, None).await;
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].user_id, user_id);
    assert_eq!(positions[0].symbol, "BTCUSDT");
    assert_eq!(positions[0].quantity, 10);
    assert_eq!(positions[0].average_price, price);
}

#[tokio::test]
async fn update_position_add_weighted_average() {
    let store = fresh_store();
    let user_id = Uuid::new_v4();
    let p1 = scale_price(50_000);
    let p2 = scale_price(52_000);

    update_position(&store, user_id, "BTCUSDT", OrderSide::Buy, p1, 10).await;
    update_position(&store, user_id, "BTCUSDT", OrderSide::Buy, p2, 5).await;

    let positions = get_positions(&store, user_id, None).await;
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].quantity, 15);
    let expected_avg = (p1 * 10 + p2 * 5) / 15;
    assert_eq!(positions[0].average_price, expected_avg);
}

#[tokio::test]
async fn update_position_reduce_position() {
    let store = fresh_store();
    let user_id = Uuid::new_v4();
    let price = scale_price(50_000);

    update_position(&store, user_id, "BTCUSDT", OrderSide::Buy, price, 10).await;
    update_position(&store, user_id, "BTCUSDT", OrderSide::Sell, price, 4).await;

    let positions = get_positions(&store, user_id, None).await;
    assert_eq!(positions.len(), 1);
    assert_eq!(positions[0].quantity, 6);
    assert_eq!(positions[0].average_price, price);
}

#[tokio::test]
async fn update_position_close_position_removed() {
    let store = fresh_store();
    let user_id = Uuid::new_v4();
    let price = scale_price(50_000);

    update_position(&store, user_id, "BTCUSDT", OrderSide::Buy, price, 10).await;
    update_position(&store, user_id, "BTCUSDT", OrderSide::Sell, price, 10).await;

    let positions = get_positions(&store, user_id, None).await;
    assert!(positions.is_empty());
}

#[tokio::test]
async fn get_positions_filter_by_symbol() {
    let store = fresh_store();
    let user_id = Uuid::new_v4();
    let price = scale_price(50_000);

    update_position(&store, user_id, "BTCUSDT", OrderSide::Buy, price, 5).await;
    update_position(&store, user_id, "ETHUSDT", OrderSide::Buy, price, 3).await;

    let btc_only = get_positions(&store, user_id, Some("BTCUSDT")).await;
    assert_eq!(btc_only.len(), 1);
    assert_eq!(btc_only[0].symbol, "BTCUSDT");
    assert_eq!(btc_only[0].quantity, 5);

    let eth_only = get_positions(&store, user_id, Some("ETHUSDT")).await;
    assert_eq!(eth_only.len(), 1);
    assert_eq!(eth_only[0].symbol, "ETHUSDT");

    let all = get_positions(&store, user_id, None).await;
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn unrealized_pnl_long() {
    let store = fresh_store();
    let user_id = Uuid::new_v4();
    let avg = scale_price(50_000);
    let current = scale_price(52_000);

    update_position(&store, user_id, "BTCUSDT", OrderSide::Buy, avg, 10).await;
    let positions = get_positions(&store, user_id, None).await;
    let pos = &positions[0];

    let pnl = unrealized_pnl(pos, current);
    let expected = (current - avg) * 10;
    assert_eq!(pnl, expected);
    assert!(pnl > 0);
}

#[tokio::test]
async fn unrealized_pnl_short() {
    let store = fresh_store();
    let user_id = Uuid::new_v4();
    let avg = scale_price(50_000);
    let current = scale_price(48_000);

    update_position(&store, user_id, "BTCUSDT", OrderSide::Sell, avg, 10).await;
    let positions = get_positions(&store, user_id, None).await;
    let pos = &positions[0];
    assert!(pos.quantity < 0);

    let pnl = unrealized_pnl(pos, current);
    let expected = (current - avg) * pos.quantity;
    assert_eq!(pnl, expected);
    assert!(pnl > 0);
}
