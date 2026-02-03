use axum::{
    Router,
    extract::{FromRequestParts, Path, Query, State},
    http::StatusCode,
    http::request::Parts,
    response::Json,
    routing::{delete, get, post},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::api::auth::{self, AuthUser, AuthUserCredential};
use crate::api::ws::ws_handler;
use crate::orderbook::orderbook::SharedOrderBook;
use crate::persistence;
use crate::positions::{self, SharedPositions};
use crate::types::order::{Order, OrderSide, OrderStatus, OrderType};
use crate::types::position::Position;
use crate::types::trade::Trade;

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

/// In-memory user store keyed by lowercase username.
pub type UserStore = Arc<RwLock<HashMap<String, AuthUserCredential>>>;

// Application state containing all shared resources
#[derive(Clone)]
pub struct AppState {
    pub orderbooks: HashMap<String, SharedOrderBook>,
    pub ws_channel: broadcast::Sender<WsMessage>,
    pub positions: SharedPositions,
    pub jwt_secret: Vec<u8>,
    pub user_store: UserStore,
    pub db: Option<sqlx::PgPool>,
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

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = (StatusCode, Json<ErrorResponse>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                ErrorResponse::new(
                    "Missing Authorization header".to_string(),
                    StatusCode::UNAUTHORIZED,
                )
            })?;
        let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
            ErrorResponse::new(
                "Invalid Authorization format".to_string(),
                StatusCode::UNAUTHORIZED,
            )
        })?;
        let claims = auth::decode_token(&state.jwt_secret, token).map_err(|_| {
            ErrorResponse::new(
                "Invalid or expired token".to_string(),
                StatusCode::UNAUTHORIZED,
            )
        })?;
        let user_id = Uuid::parse_str(&claims.sub).map_err(|_| {
            ErrorResponse::new("Invalid token claims".to_string(), StatusCode::UNAUTHORIZED)
        })?;
        Ok(AuthUser { user_id })
    }
}

// Helper function to get orderbook by symbol
fn get_orderbook(
    state: &AppState,
    symbol: &str,
) -> Result<SharedOrderBook, (StatusCode, Json<ErrorResponse>)> {
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
struct RegisterRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct RegisterResponse {
    user_id: Uuid,
    username: String,
}

async fn register(
    State(state): State<AppState>,
    Json(body): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<RegisterResponse>), (StatusCode, Json<ErrorResponse>)> {
    let username = body.username.trim();
    let password = body.password.trim();
    if username.is_empty() || password.is_empty() {
        return Err(ErrorResponse::new(
            "Username and password are required".to_string(),
            StatusCode::BAD_REQUEST,
        ));
    }
    let key = username.to_lowercase();
    let mut store = state.user_store.write().await;
    if store.get(&key).is_some() {
        return Err(ErrorResponse::new(
            "Username already taken".to_string(),
            StatusCode::BAD_REQUEST,
        ));
    }
    let password_hash = auth::hash_password(password).map_err(|_| {
        ErrorResponse::new(
            "Failed to hash password".to_string(),
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    })?;
    let user_id = Uuid::new_v4();
    if let Some(ref db) = state.db {
        persistence::insert_user(db, user_id, &key, &password_hash)
            .await
            .map_err(|_| {
                ErrorResponse::new(
                    "Failed to create user".to_string(),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
            })?;
    }
    let credential = AuthUserCredential {
        user_id,
        username: username.to_string(),
        password_hash,
    };
    store.insert(key, credential);
    Ok((
        StatusCode::CREATED,
        Json(RegisterResponse {
            user_id,
            username: username.to_string(),
        }),
    ))
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<ErrorResponse>)> {
    let key = body.username.trim().to_lowercase();
    let user_id = if let Some(ref db) = state.db {
        let user_row = persistence::get_user_by_username(db, &key).await.map_err(|_| {
            ErrorResponse::new(
                "Failed to look up user".to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })?;
        let user_row = user_row.ok_or_else(|| {
            ErrorResponse::new(
                "Invalid username or password".to_string(),
                StatusCode::UNAUTHORIZED,
            )
        })?;
        if !auth::verify_password(&body.password, &user_row.password_hash) {
            return Err(ErrorResponse::new(
                "Invalid username or password".to_string(),
                StatusCode::UNAUTHORIZED,
            ));
        }
        user_row.id
    } else {
        let store = state.user_store.read().await;
        let cred = store.get(&key).ok_or_else(|| {
            ErrorResponse::new(
                "Invalid username or password".to_string(),
                StatusCode::UNAUTHORIZED,
            )
        })?;
        if !auth::verify_password(&body.password, &cred.password_hash) {
            return Err(ErrorResponse::new(
                "Invalid username or password".to_string(),
                StatusCode::UNAUTHORIZED,
            ));
        }
        cred.user_id
    };
    let token = auth::create_token(&state.jwt_secret, user_id).map_err(|_| {
        ErrorResponse::new(
            "Failed to create token".to_string(),
            StatusCode::INTERNAL_SERVER_ERROR,
        )
    })?;
    Ok(Json(LoginResponse {
        token,
        user_id,
    }))
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    user_id: Uuid,
}

#[derive(Deserialize)]
struct CreateOrderRequest {
    symbol: String,
    price: i64,
    quantity: u64,
    side: OrderSide,
    #[serde(default)]
    order_type: OrderType,
}

async fn create_order(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateOrderRequest>,
) -> Result<Json<Order>, (StatusCode, Json<ErrorResponse>)> {
    if body.symbol.is_empty() {
        return Err(ErrorResponse::new(
            "Symbol parameter is required".to_string(),
            StatusCode::BAD_REQUEST,
        ));
    }

    let normalized_symbol = body.symbol.to_uppercase();
    let orderbook = get_orderbook(&state, &normalized_symbol)?;
    let (order, trades) = {
        let mut book = orderbook.write().await;
        book.add_order(
            auth.user_id,
            body.price,
            body.quantity,
            body.side,
            body.order_type,
            Some(&state.ws_channel),
            Some(&normalized_symbol),
        )
    };

    if body.order_type == OrderType::Market && trades.is_empty() {
        return Err(ErrorResponse::new(
            "Market order could not be filled: no liquidity".to_string(),
            StatusCode::BAD_REQUEST,
        ));
    }

    // Update positions for each trade (taker = order.side, maker = opposite)
    let maker_side = match order.side {
        OrderSide::Buy => OrderSide::Sell,
        OrderSide::Sell => OrderSide::Buy,
    };
    for trade in &trades {
        positions::update_position(
            &state.positions,
            trade.maker_user_id,
            &normalized_symbol,
            maker_side,
            trade.price,
            trade.quantity,
        )
        .await;
        positions::update_position(
            &state.positions,
            trade.taker_user_id,
            &normalized_symbol,
            order.side,
            trade.price,
            trade.quantity,
        )
        .await;
    }

    if let Some(ref db) = state.db {
        let _ = persistence::insert_order(
            db,
            order.id,
            order.user_id,
            &normalized_symbol,
            order.side,
            order.order_type,
            order.price,
            order.quantity,
            order.status,
            order.timestamp,
        )
        .await;
        for trade in &trades {
            let _ = persistence::insert_trade(
                db,
                trade.id,
                trade.maker_order_id,
                trade.taker_order_id,
                trade.maker_user_id,
                trade.taker_user_id,
                &normalized_symbol,
                trade.price,
                trade.quantity,
                trade.timestamp,
            )
            .await;
        }
        let mut keys = std::collections::HashSet::new();
        keys.insert((order.user_id, normalized_symbol.clone()));
        for t in &trades {
            keys.insert((t.maker_user_id, normalized_symbol.clone()));
            keys.insert((t.taker_user_id, normalized_symbol.clone()));
        }
        for (uid, sym) in keys {
            let pos_list =
                positions::get_positions(&state.positions, uid, Some(&sym)).await;
            if let Some(pos) = pos_list.into_iter().next() {
                let _ = persistence::upsert_position(
                    db,
                    uid,
                    &sym,
                    pos.quantity,
                    pos.average_price,
                )
                .await;
            }
        }
    }

    Ok(Json(order))
}

#[derive(Deserialize)]
struct OrderQuery {
    symbol: String,
}

async fn cancel_order(
    auth: AuthUser,
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

    let normalized_symbol = params.symbol.to_uppercase();
    let orderbook = get_orderbook(&state, &normalized_symbol)?;
    {
        let book = orderbook.read().await;
        if let Some(order) = book.get_order_by_id(order_id)
            && order.user_id != auth.user_id
        {
            return Err(ErrorResponse::new(
                "Forbidden: order does not belong to you".to_string(),
                StatusCode::FORBIDDEN,
            ));
        }
    }
    let mut book = orderbook.write().await;
    match book.remove_order(order_id, Some(&state.ws_channel), Some(&normalized_symbol)) {
        Some(_) => {
            if let Some(ref db) = state.db {
                let _ = persistence::update_order_status(db, order_id, OrderStatus::Cancelled).await;
            }
            Ok(StatusCode::NO_CONTENT)
        }
        None => Err(ErrorResponse::new(
            format!("Order '{}' not found", order_id),
            StatusCode::NOT_FOUND,
        )),
    }
}

async fn get_order(
    auth: AuthUser,
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

    if let Some(ref db) = state.db {
        let row = persistence::get_order_by_id(db, order_id).await.map_err(|_| {
            ErrorResponse::new(
                "Failed to look up order".to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })?;
        let row = row.ok_or_else(|| {
            ErrorResponse::new(
                format!("Order '{}' not found", order_id),
                StatusCode::NOT_FOUND,
            )
        })?;
        if row.user_id != auth.user_id {
            return Err(ErrorResponse::new(
                "Forbidden: order does not belong to you".to_string(),
                StatusCode::FORBIDDEN,
            ));
        }
        let order = persistence::order_row_to_order_display(&row).ok_or_else(|| {
            ErrorResponse::new(
                "Invalid order data".to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })?;
        return Ok(Json(order));
    }

    let orderbook = get_orderbook(&state, &params.symbol)?;
    let book = orderbook.read().await;
    match book.get_order_by_id(order_id) {
        Some(order) => {
            if order.user_id != auth.user_id {
                return Err(ErrorResponse::new(
                    "Forbidden: order does not belong to you".to_string(),
                    StatusCode::FORBIDDEN,
                ));
            }
            Ok(Json(order))
        }
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

#[derive(Deserialize)]
struct TradesMeQuery {
    symbol: Option<String>,
    limit: Option<usize>,
}

async fn get_trades_me(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<TradesMeQuery>,
) -> Result<Json<Vec<Trade>>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(100);
    let user_id = auth.user_id;

    let symbol_opt = params
        .symbol
        .as_deref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());

    if let Some(ref db) = state.db {
        let trades = persistence::list_trades_for_user(db, user_id, symbol_opt, limit)
            .await
            .map_err(|_| {
                ErrorResponse::new(
                    "Failed to load trades".to_string(),
                    StatusCode::INTERNAL_SERVER_ERROR,
                )
            })?;
        return Ok(Json(trades));
    }

    let trades: Vec<Trade> = if let Some(symbol) = symbol_opt {
        let orderbook = get_orderbook(&state, symbol)?;
        let book = orderbook.read().await;
        book.get_recent_trades(limit)
    } else {
        let mut all = Vec::new();
        for orderbook in state.orderbooks.values() {
            let book = orderbook.read().await;
            all.extend(book.get_recent_trades(limit));
        }
        all
    };

    let mut filtered: Vec<Trade> = trades
        .into_iter()
        .filter(|t| t.maker_user_id == user_id || t.taker_user_id == user_id)
        .collect();
    filtered.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    filtered.truncate(limit);
    Ok(Json(filtered))
}

async fn get_trades(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<TradesQuery>,
) -> Result<Json<Vec<Trade>>, (StatusCode, Json<ErrorResponse>)> {
    let _ = auth; // require auth; trades are market-wide for symbol
    if params.symbol.is_empty() {
        return Err(ErrorResponse::new(
            "Symbol parameter is required".to_string(),
            StatusCode::BAD_REQUEST,
        ));
    }

    let limit = params.limit.unwrap_or(100);

    if let Some(ref db) = state.db {
        let trades = persistence::list_trades(db, &params.symbol, limit).await.map_err(|_| {
            ErrorResponse::new(
                "Failed to load trades".to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })?;
        return Ok(Json(trades));
    }

    let orderbook = get_orderbook(&state, &params.symbol)?;
    let book = orderbook.read().await;
    Ok(Json(book.get_recent_trades(limit)))
}

#[derive(Deserialize)]
struct PositionsQuery {
    symbol: Option<String>,
}

async fn get_positions(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<PositionsQuery>,
) -> Result<Json<Vec<Position>>, (StatusCode, Json<ErrorResponse>)> {
    if let Some(ref db) = state.db {
        let rows = persistence::list_positions_for_user(
            db,
            auth.user_id,
            params.symbol.as_deref(),
        )
        .await
        .map_err(|_| {
            ErrorResponse::new(
                "Failed to load positions".to_string(),
                StatusCode::INTERNAL_SERVER_ERROR,
            )
        })?;
        let positions = rows
            .into_iter()
            .map(|r| Position {
                user_id: r.user_id,
                symbol: r.symbol,
                quantity: r.quantity,
                average_price: r.average_price,
            })
            .collect();
        return Ok(Json(positions));
    }

    let positions =
        positions::get_positions(&state.positions, auth.user_id, params.symbol.as_deref()).await;
    Ok(Json(positions))
}

pub fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .route("/orders", post(create_order))
        .route("/orders/{id}", delete(cancel_order))
        .route("/orders/{id}", get(get_order))
        .route("/book", get(get_order_book))
        .route("/trades/me", get(get_trades_me))
        .route("/trades", get(get_trades))
        .route("/positions", get(get_positions))
        .route("/ws", get(ws_handler))
        .with_state(state)
}
