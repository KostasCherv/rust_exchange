use rust_exchange::api::routes::{app_router, AppState};
use rust_exchange::orderbook::orderbook::{OrderBook, SharedOrderBook};
use std::sync::Arc;
use tokio::sync::RwLock;

#[tokio::main]
async fn main() {
    let orderbook: SharedOrderBook = Arc::new(RwLock::new(OrderBook::new()));

    let app_state = AppState {
        orderbook,
        // Add more shared resources here as needed:
        // users: Arc::new(RwLock::new(UserManager::new())),
        // accounts: Arc::new(RwLock::new(AccountManager::new())),
    };

    let app = app_router(app_state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}