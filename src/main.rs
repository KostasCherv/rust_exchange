use rust_exchange::api::routes::{app_router, AppState};
use rust_exchange::orderbook::orderbook::{OrderBook, SharedOrderBook};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

#[tokio::main]
async fn main() {
    let mut orderbooks: HashMap<String, SharedOrderBook> = HashMap::new();

    // Initialize BTCUSDT orderbook
    orderbooks.insert(
        "BTCUSDT".to_string(),
        Arc::new(RwLock::new(OrderBook::new())),
    );

    // Initialize ETHUSDT orderbook
    orderbooks.insert(
        "ETHUSDT".to_string(),
        Arc::new(RwLock::new(OrderBook::new())),
    );

    // Initialize WebSocket broadcast channel
    let (ws_tx, _) = broadcast::channel::<rust_exchange::api::routes::WsMessage>(1000);

    let app_state = AppState {
        orderbooks,
        ws_channel: ws_tx,
    };

    let app = app_router(app_state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}