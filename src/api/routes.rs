use axum::{extract::{Path, Query, State}, http::StatusCode, response::Json, Router, routing::{delete, get, post}};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::order::{Order, OrderSide};
use crate::types::trade::Trade;
use crate::orderbook::orderbook::SharedOrderBook;

// Application state containing all shared resources
#[derive(Clone)]
pub struct AppState {
    pub orderbook: SharedOrderBook,
    // Add more shared resources here as needed:
    // pub users: SharedUserManager,
    // pub accounts: SharedAccountManager,
}

async fn health() -> &'static str {
    "healthy"
}

#[derive(Deserialize)]
struct CreateOrderRequest {
    user_id: Uuid,
    price: i64,
    quantity: u64,
    side: OrderSide,
}

async fn create_order(
    State(state): State<AppState>,
    Json(body): Json<CreateOrderRequest>,
) -> Result<Json<Order>, StatusCode> {
    let mut book = state.orderbook.write().await;
    let order = book.add_order(body.user_id, body.price, body.quantity, body.side);
    Ok(Json(order))
}

async fn cancel_order(
    State(state): State<AppState>,
    Path(order_id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let mut book = state.orderbook.write().await;
    match book.remove_order(order_id) {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn get_order(
    State(state): State<AppState>,
    Path(order_id): Path<Uuid>,
) -> Result<Json<Order>, StatusCode> {
    let book = state.orderbook.read().await;
    match book.get_order_by_id(order_id) {
        Some(order) => Ok(Json(order)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

#[derive(Serialize)]
struct OrderBookResponse {
    bids: Vec<(i64, u64)>,
    asks: Vec<(i64, u64)>,
}

async fn get_order_book(
    State(state): State<AppState>,
) -> Json<OrderBookResponse> {
    let book = state.orderbook.read().await;
    Json(OrderBookResponse {
        bids: book.get_bids(),
        asks: book.get_asks(),
    })
}

#[derive(Deserialize)]
struct TradesQuery {
    limit: Option<usize>,
}

async fn get_trades(
    State(state): State<AppState>,
    Query(params): Query<TradesQuery>,
) -> Json<Vec<Trade>> {
    let book = state.orderbook.read().await;
    let limit = params.limit.unwrap_or(100);
    Json(book.get_recent_trades(limit))
}

pub fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/orders", post(create_order))
        .route("/orders/{id}", delete(cancel_order))
        .route("/orders/{id}", get(get_order))
        .route("/book", get(get_order_book))
        .route("/trades", get(get_trades))
        .with_state(state)
}