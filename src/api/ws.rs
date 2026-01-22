use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashSet;
use tokio::{select, sync::broadcast};

use crate::api::routes::{AppState, WsMessage};
use crate::types::trade::Trade;

// Subscription action enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionAction {
    Subscribe,
    Unsubscribe,
}

// Subscription message from client
#[derive(Debug, Deserialize)]
struct SubscriptionMessage {
    action: SubscriptionAction,
    symbol: String,
}

// Subscription status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SubscriptionStatus {
    Success,
    Error,
}

// Acknowledgment message to client
#[derive(Debug, Serialize)]
struct SubscriptionAck {
    status: SubscriptionStatus,
    message: String,
    symbol: Option<String>,
}

// WebSocket handler - accepts upgrade and handles the connection
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

// Handle individual WebSocket connection
async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut broadcast_receiver = state.ws_channel.subscribe();
    let mut subscribed_symbols: HashSet<String> = HashSet::new();

    loop {
        select! {
            // Handle incoming broadcast messages and send to client (if subscribed)
            result = broadcast_receiver.recv() => {
                match result {
                    Ok(ws_msg) => {
                        // Check if client is subscribed to this symbol
                        let symbol = match &ws_msg {
                            WsMessage::OrderBookUpdate { symbol, .. } => symbol,
                            WsMessage::Trade { symbol, .. } => symbol,
                        };

                        // Only send if client is subscribed to this symbol
                        if subscribed_symbols.contains(symbol) {
                            if let Ok(json) = serde_json::to_string(&ws_msg) {
                                if socket.send(Message::Text(json.into())).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Broadcast channel closed
                        return;
                    }
                }
            }
            // Handle incoming messages from client
            result = socket.recv() => {
                match result {
                    Some(Ok(Message::Text(text))) => {
                        // Parse subscription message
                        match serde_json::from_str::<SubscriptionMessage>(&text) {
                            Ok(sub_msg) => {
                                let normalized_symbol = sub_msg.symbol.to_uppercase();
                                
                                // Validate symbol exists
                                let symbol_exists = state.orderbooks.contains_key(&normalized_symbol);
                                
                                let ack = match sub_msg.action {
                                    SubscriptionAction::Subscribe => {
                                        if symbol_exists {
                                            subscribed_symbols.insert(normalized_symbol.clone());
                                            SubscriptionAck {
                                                status: SubscriptionStatus::Success,
                                                message: format!("Subscribed to {}", normalized_symbol),
                                                symbol: Some(normalized_symbol),
                                            }
                                        } else {
                                            SubscriptionAck {
                                                status: SubscriptionStatus::Error,
                                                message: format!("Symbol '{}' not found", normalized_symbol),
                                                symbol: None,
                                            }
                                        }
                                    }
                                    SubscriptionAction::Unsubscribe => {
                                        subscribed_symbols.remove(&normalized_symbol);
                                        SubscriptionAck {
                                            status: SubscriptionStatus::Success,
                                            message: format!("Unsubscribed from {}", normalized_symbol),
                                            symbol: Some(normalized_symbol),
                                        }
                                    }
                                };
                                
                                // Send acknowledgment back to client
                                if let Ok(ack_json) = serde_json::to_string(&ack) {
                                    if socket.send(Message::Text(ack_json.into())).await.is_err() {
                                        return;
                                    }
                                }
                            }
                            Err(_) => {
                                // Invalid JSON - send error acknowledgment
                                let error_ack = SubscriptionAck {
                                    status: SubscriptionStatus::Error,
                                    message: "Invalid message format. Expected: {\"action\": \"subscribe\", \"symbol\": \"BTCUSDT\"}".to_string(),
                                    symbol: None,
                                };
                                if let Ok(ack_json) = serde_json::to_string(&error_ack) {
                                    let _ = socket.send(Message::Text(ack_json.into())).await;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        // Client closed connection
                        return;
                    }
                    Some(Err(_)) | None => {
                        // Client disconnected or error
                        return;
                    }
                    _ => {
                        // Ignore other message types (binary, ping, pong)
                    }
                }
            }
        }
    }
}

// Helper function to broadcast trades
pub fn broadcast_trades(ws_channel: &broadcast::Sender<WsMessage>, symbol: &str, trades: &[Trade]) {
    for trade in trades {
        let _ = ws_channel.send(WsMessage::Trade {
            symbol: symbol.to_string(),
            trade: trade.clone(),
        });
    }
}

// Helper function to broadcast orderbook update
pub fn broadcast_orderbook_update(
    ws_channel: &broadcast::Sender<WsMessage>,
    symbol: &str,
    book: &crate::orderbook::orderbook::OrderBook,
) {
    let bids = book.get_bids();
    let asks = book.get_asks();
    let _ = ws_channel.send(WsMessage::OrderBookUpdate {
        symbol: symbol.to_string(),
        bids,
        asks,
    });
}