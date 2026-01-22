use axum::{extract::{Path, Query, State}, http::StatusCode, response::Json, Router, routing::{delete, get, post}};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::api::ws::ws_handler;
use crate::types::order::{Order, OrderSide};
use crate::types::trade::Trade;
use crate::orderbook::orderbook::SharedOrderBook;

// WebSocket message type for broadcasting
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum WsMessage {
    OrderBookUpdate {
        symbol: String,
        bids: Vec<(i64, u64)>,
        asks: Vec<(i64, u64)>,
    },
    Trade {
        symbol: String,
        trade: Trade,
    },
}

// Application state containing all shared resources
#[derive(Clone)]
pub struct AppState {
    pub orderbooks: HashMap<String, SharedOrderBook>,
    pub ws_channel: broadcast::Sender<WsMessage>,
}

// Error response structure
#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub code: u16,
}

impl ErrorResponse {
    pub fn new(message: String, status_code: StatusCode) -> (StatusCode, Json<Self>) {
        (
            status_code,
            Json(Self {
                error: message,
                code: status_code.as_u16(),
            }),
        )
    }
}

// Helper function to get orderbook by symbol
fn get_orderbook(state: &AppState, symbol: &str) -> Result<SharedOrderBook, (StatusCode, Json<ErrorResponse>)> {
    let normalized_symbol = symbol.to_uppercase();
    state
        .orderbooks
        .get(&normalized_symbol)
        .cloned()
        .ok_or_else(|| {
            ErrorResponse::new(
                format!("Symbol '{}' not found", normalized_symbol),
                StatusCode::NOT_FOUND,
            )
        })
}

async fn health() -> &'static str {
    "healthy"
}

#[derive(Deserialize)]
struct CreateOrderRequest {
    user_id: Uuid,
    symbol: String,
    price: i64,
    quantity: u64,
    side: OrderSide,
}

async fn create_order(
    State(state): State<AppState>,
    Json(body): Json<CreateOrderRequest>,
) -> Result<Json<Order>, (StatusCode, Json<ErrorResponse>)> {
    if body.symbol.is_empty() {
        return Err(ErrorResponse::new(
            "Symbol parameter is required".to_string(),
            StatusCode::BAD_REQUEST,
        ));
    }

    let orderbook = get_orderbook(&state, &body.symbol)?;
    let mut book = orderbook.write().await;
    let order = book.add_order(body.user_id, body.price, body.quantity, body.side);
    Ok(Json(order))
}

#[derive(Deserialize)]
struct OrderQuery {
    symbol: String,
}

async fn cancel_order(
    State(state): State<AppState>,
    Path(order_id): Path<Uuid>,
    Query(params): Query<OrderQuery>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    if params.symbol.is_empty() {
        return Err(ErrorResponse::new(
            "Symbol parameter is required".to_string(),
            StatusCode::BAD_REQUEST,
        ));
    }

    let orderbook = get_orderbook(&state, &params.symbol)?;
    let mut book = orderbook.write().await;
    match book.remove_order(order_id) {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(ErrorResponse::new(
            format!("Order '{}' not found", order_id),
            StatusCode::NOT_FOUND,
        )),
    }
}

async fn get_order(
    State(state): State<AppState>,
    Path(order_id): Path<Uuid>,
    Query(params): Query<OrderQuery>,
) -> Result<Json<Order>, (StatusCode, Json<ErrorResponse>)> {
    if params.symbol.is_empty() {
        return Err(ErrorResponse::new(
            "Symbol parameter is required".to_string(),
            StatusCode::BAD_REQUEST,
        ));
    }

    let orderbook = get_orderbook(&state, &params.symbol)?;
    let book = orderbook.read().await;
    match book.get_order_by_id(order_id) {
        Some(order) => Ok(Json(order)),
        None => Err(ErrorResponse::new(
            format!("Order '{}' not found", order_id),
            StatusCode::NOT_FOUND,
        )),
    }
}

#[derive(Serialize)]
struct OrderBookResponse {
    bids: Vec<(i64, u64)>,
    asks: Vec<(i64, u64)>,
}

#[derive(Deserialize)]
struct OrderBookQuery {
    symbol: String,
}

async fn get_order_book(
    State(state): State<AppState>,
    Query(params): Query<OrderBookQuery>,
) -> Result<Json<OrderBookResponse>, (StatusCode, Json<ErrorResponse>)> {
    if params.symbol.is_empty() {
        return Err(ErrorResponse::new(
            "Symbol parameter is required".to_string(),
            StatusCode::BAD_REQUEST,
        ));
    }

    let orderbook = get_orderbook(&state, &params.symbol)?;
    let book = orderbook.read().await;
    Ok(Json(OrderBookResponse {
        bids: book.get_bids(),
        asks: book.get_asks(),
    }))
}

#[derive(Deserialize)]
struct TradesQuery {
    symbol: String,
    limit: Option<usize>,
}

async fn get_trades(
    State(state): State<AppState>,
    Query(params): Query<TradesQuery>,
) -> Result<Json<Vec<Trade>>, (StatusCode, Json<ErrorResponse>)> {
    if params.symbol.is_empty() {
        return Err(ErrorResponse::new(
            "Symbol parameter is required".to_string(),
            StatusCode::BAD_REQUEST,
        ));
    }

    let orderbook = get_orderbook(&state, &params.symbol)?;
    let book = orderbook.read().await;
    let limit = params.limit.unwrap_or(100);
    Ok(Json(book.get_recent_trades(limit)))
}

pub fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/orders", post(create_order))
        .route("/orders/{id}", delete(cancel_order))
        .route("/orders/{id}", get(get_order))
        .route("/book", get(get_order_book))
        .route("/trades", get(get_trades))
        .route("/ws", get(ws_handler))
        .with_state(state)
}