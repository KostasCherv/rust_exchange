use rust_exchange::api::auth::AuthUserCredential;
use rust_exchange::api::routes::{AppState, UserStore, app_router};
use rust_exchange::orderbook::orderbook::{OrderBook, SharedOrderBook};
use rust_exchange::persistence::{self, PgPool};
use rust_exchange::positions::SharedPositions;
use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool: PgPool = persistence::create_pool_and_migrate(&database_url)
        .await
        .expect("create pool and run migrations");

    let user_rows = persistence::list_users(&pool).await.expect("load users from DB");
    let user_store: UserStore = Arc::new(RwLock::new(
        user_rows
            .into_iter()
            .map(|r| {
                (
                    r.username.clone(),
                    AuthUserCredential {
                        user_id: r.id,
                        username: r.username,
                        password_hash: r.password_hash,
                    },
                )
            })
            .collect(),
    ));

    let mut orderbooks: HashMap<String, SharedOrderBook> = HashMap::new();
    for symbol in &["BTCUSDT", "ETHUSDT"] {
        let mut book = OrderBook::new();
        if let Ok(rows) = persistence::list_open_orders_by_symbol(&pool, symbol).await {
            for row in &rows {
                if let Some(order) = persistence::order_row_to_order(row) {
                    book.restore_order(order);
                }
            }
        }
        orderbooks.insert((*symbol).to_string(), Arc::new(RwLock::new(book)));
    }

    let (ws_tx, _) = broadcast::channel::<rust_exchange::api::routes::WsMessage>(1000);
    let positions: SharedPositions = Arc::new(RwLock::new({
        let mut map = HashMap::new();
        if let Ok(rows) = persistence::list_positions(&pool).await {
            use rust_exchange::types::position::Position;
            for r in rows {
                map.insert(
                    (r.user_id, r.symbol.clone()),
                    Position {
                        user_id: r.user_id,
                        symbol: r.symbol,
                        quantity: r.quantity,
                        average_price: r.average_price,
                    },
                );
            }
        }
        map
    }));

    let jwt_secret = env::var("JWT_SECRET")
        .unwrap_or_else(|_| "dev-secret-change-in-production".to_string())
        .into_bytes();

    let app_state = AppState {
        orderbooks,
        ws_channel: ws_tx,
        positions,
        jwt_secret,
        user_store,
        db: Some(pool),
    };

    let app = app_router(app_state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
