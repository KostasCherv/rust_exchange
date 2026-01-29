use rust_exchange::api::auth::AuthUserCredential;
use rust_exchange::api::routes::{AppState, app_router};
use rust_exchange::orderbook::orderbook::{OrderBook, SharedOrderBook};
use rust_exchange::positions::SharedPositions;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use uuid::Uuid;

fn load_auth_config() -> Vec<AuthUserCredential> {
    let mut users = Vec::new();
    if let (Ok(user_id), Ok(username), Ok(password)) = (
        env::var("AUTH_USER_ID"),
        env::var("AUTH_USERNAME"),
        env::var("AUTH_PASSWORD"),
    ) && let Ok(uuid) = Uuid::parse_str(&user_id)
    {
        users.push(AuthUserCredential {
            user_id: uuid,
            username,
            password,
        });
    }
    users
}

#[tokio::main]
async fn main() {
    let mut orderbooks: HashMap<String, SharedOrderBook> = HashMap::new();

    orderbooks.insert(
        "BTCUSDT".to_string(),
        Arc::new(RwLock::new(OrderBook::new())),
    );
    orderbooks.insert(
        "ETHUSDT".to_string(),
        Arc::new(RwLock::new(OrderBook::new())),
    );

    let (ws_tx, _) = broadcast::channel::<rust_exchange::api::routes::WsMessage>(1000);

    let positions: SharedPositions = Arc::new(RwLock::new(HashMap::new()));

    let jwt_secret = env::var("JWT_SECRET")
        .unwrap_or_else(|_| "dev-secret-change-in-production".to_string())
        .into_bytes();

    let auth_users = load_auth_config();

    let app_state = AppState {
        orderbooks,
        ws_channel: ws_tx,
        positions,
        jwt_secret,
        auth_users,
    };

    let app = app_router(app_state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
