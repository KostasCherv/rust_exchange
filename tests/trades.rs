//! Trade creation and structure integration tests: add_order trades, get_recent_trades, trade fields.

use rust_exchange::orderbook::orderbook::OrderBook;
use rust_exchange::types::order::OrderSide;
use uuid::Uuid;

fn scale_price(p: i64) -> i64 {
    p * 100_000_000
}

#[test]
fn trade_creation_on_match_fields() {
    let mut book = OrderBook::new();
    let seller = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);
    let qty = 10u64;

    let (sell_order, _) = book.add_order(seller, price, qty, OrderSide::Sell, None, None);
    let (buy_order, trades) = book.add_order(buyer, price, qty, OrderSide::Buy, None, None);

    assert_eq!(trades.len(), 1);
    let t = &trades[0];
    assert_eq!(t.price, price);
    assert_eq!(t.quantity, qty);
    assert_eq!(t.maker_order_id, sell_order.id);
    assert_eq!(t.taker_order_id, buy_order.id);
    assert_eq!(t.maker_user_id, seller);
    assert_eq!(t.taker_user_id, buyer);
}

#[test]
fn multiple_trades_fifo_recent_first() {
    let mut book = OrderBook::new();
    let user1 = Uuid::new_v4();
    let user2 = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);

    let (sell1, _) = book.add_order(user1, price, 2, OrderSide::Sell, None, None);
    let (sell2, _) = book.add_order(user2, price, 2, OrderSide::Sell, None, None);
    let (_buy_order, trades) = book.add_order(buyer, price, 3, OrderSide::Buy, None, None);

    assert_eq!(trades.len(), 2);
    assert_eq!(trades[0].quantity, 2);
    assert_eq!(trades[1].quantity, 1);
    assert_eq!(trades[0].maker_order_id, sell1.id);
    assert_eq!(trades[1].maker_order_id, sell2.id);

    let recent = book.get_recent_trades(10);
    assert_eq!(recent.len(), 2);
    assert_eq!(recent[0].quantity, 1);
    assert_eq!(recent[1].quantity, 2);
}

#[test]
fn trade_storage_after_match() {
    let mut book = OrderBook::new();
    let seller = Uuid::new_v4();
    let buyer = Uuid::new_v4();
    let price = scale_price(50_000);
    let qty = 5u64;

    let (sell_order, _) = book.add_order(seller, price, qty, OrderSide::Sell, None, None);
    let (buy_order, trades) = book.add_order(buyer, price, qty, OrderSide::Buy, None, None);

    assert_eq!(trades.len(), 1);
    let stored = book.get_all_trades();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].price, price);
    assert_eq!(stored[0].quantity, qty);
    assert_eq!(stored[0].maker_user_id, seller);
    assert_eq!(stored[0].taker_user_id, buyer);
    assert_eq!(stored[0].maker_order_id, sell_order.id);
    assert_eq!(stored[0].taker_order_id, buy_order.id);

    let recent = book.get_recent_trades(10);
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].id, stored[0].id);
}
