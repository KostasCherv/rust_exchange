//! Integration tests for auth: register, login, and user store.

use rust_exchange::api::auth::{self, AuthUserCredential};
use rust_exchange::api::routes::{AppState, UserStore, app_router};
use rust_exchange::orderbook::orderbook::OrderBook;
use rust_exchange::positions::SharedPositions;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

fn test_app_state(user_store: UserStore) -> AppState {
    let mut orderbooks = HashMap::new();
    orderbooks.insert(
        "BTCUSDT".to_string(),
        Arc::new(RwLock::new(OrderBook::new())),
    );
    let (ws_tx, _) = broadcast::channel(1000);
    let positions: SharedPositions = Arc::new(RwLock::new(HashMap::new()));
    let jwt_secret = b"test-jwt-secret".to_vec();
    AppState {
        orderbooks,
        ws_channel: ws_tx,
        positions,
        jwt_secret,
        user_store,
        db: None,
    }
}

/// Spawn app on a random port and return (base_url, guard that keeps server running).
async fn spawn_app(state: AppState) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://{}", addr);
    let app = app_router(state);
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (base_url, handle)
}

#[tokio::test]
async fn register_returns_201_with_user_id_and_username() {
    let user_store: UserStore = Arc::new(RwLock::new(HashMap::new()));
    let state = test_app_state(user_store);
    let (base_url, _handle) = spawn_app(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{}/auth/register", base_url))
        .json(&serde_json::json!({ "username": "alice", "password": "secret123" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status().as_u16(), 201);
    let json: serde_json::Value = res.json().await.unwrap();
    assert!(json.get("user_id").and_then(|v| v.as_str()).is_some());
    assert_eq!(json.get("username").and_then(|v| v.as_str()), Some("alice"));
}

#[tokio::test]
async fn register_empty_username_returns_400() {
    let user_store: UserStore = Arc::new(RwLock::new(HashMap::new()));
    let state = test_app_state(user_store);
    let (base_url, _handle) = spawn_app(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{}/auth/register", base_url))
        .json(&serde_json::json!({ "username": "", "password": "secret123" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status().as_u16(), 400);
    let json: serde_json::Value = res.json().await.unwrap();
    assert!(json.get("error").unwrap().as_str().unwrap().contains("required"));
}

#[tokio::test]
async fn register_empty_password_returns_400() {
    let user_store: UserStore = Arc::new(RwLock::new(HashMap::new()));
    let state = test_app_state(user_store);
    let (base_url, _handle) = spawn_app(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{}/auth/register", base_url))
        .json(&serde_json::json!({ "username": "alice", "password": "" }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status().as_u16(), 400);
    let json: serde_json::Value = res.json().await.unwrap();
    assert!(json.get("error").unwrap().as_str().unwrap().contains("required"));
}

#[tokio::test]
async fn register_duplicate_username_returns_400() {
    let user_store: UserStore = Arc::new(RwLock::new(HashMap::new()));
    let state = test_app_state(user_store);
    let (base_url, _handle) = spawn_app(state).await;
    let client = reqwest::Client::new();

    let r1 = client
        .post(format!("{}/auth/register", base_url))
        .json(&serde_json::json!({ "username": "bob", "password": "pass1" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r1.status().as_u16(), 201);

    let r2 = client
        .post(format!("{}/auth/register", base_url))
        .json(&serde_json::json!({ "username": "bob", "password": "pass2" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r2.status().as_u16(), 400);
    let json: serde_json::Value = r2.json().await.unwrap();
    assert!(json.get("error").unwrap().as_str().unwrap().contains("already taken"));
}

#[tokio::test]
async fn register_then_login_returns_token() {
    let user_store: UserStore = Arc::new(RwLock::new(HashMap::new()));
    let state = test_app_state(user_store);
    let (base_url, _handle) = spawn_app(state).await;
    let client = reqwest::Client::new();

    let reg = client
        .post(format!("{}/auth/register", base_url))
        .json(&serde_json::json!({ "username": "carol", "password": "mypass" }))
        .send()
        .await
        .unwrap();
    assert_eq!(reg.status().as_u16(), 201);

    let login = client
        .post(format!("{}/auth/login", base_url))
        .json(&serde_json::json!({ "username": "carol", "password": "mypass" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status().as_u16(), 200);
    let json: serde_json::Value = login.json().await.unwrap();
    assert!(json.get("token").and_then(|v| v.as_str()).is_some());
    assert!(json.get("user_id").and_then(|v| v.as_str()).is_some());
}

#[tokio::test]
async fn login_case_insensitive_username() {
    let user_store: UserStore = Arc::new(RwLock::new(HashMap::new()));
    let state = test_app_state(user_store);
    let (base_url, _handle) = spawn_app(state).await;
    let client = reqwest::Client::new();

    let _ = client
        .post(format!("{}/auth/register", base_url))
        .json(&serde_json::json!({ "username": "Alice", "password": "secret" }))
        .send()
        .await
        .unwrap();

    let login = client
        .post(format!("{}/auth/login", base_url))
        .json(&serde_json::json!({ "username": "alice", "password": "secret" }))
        .send()
        .await
        .unwrap();
    assert_eq!(login.status().as_u16(), 200);
    let json: serde_json::Value = login.json().await.unwrap();
    assert!(json.get("user_id").and_then(|v| v.as_str()).is_some());
}

#[tokio::test]
async fn login_wrong_password_returns_401() {
    let user_store: UserStore = Arc::new(RwLock::new(HashMap::new()));
    let state = test_app_state(user_store);
    let (base_url, _handle) = spawn_app(state).await;
    let client = reqwest::Client::new();

    let _ = client
        .post(format!("{}/auth/register", base_url))
        .json(&serde_json::json!({ "username": "dave", "password": "right" }))
        .send()
        .await
        .unwrap();

    let res = client
        .post(format!("{}/auth/login", base_url))
        .json(&serde_json::json!({ "username": "dave", "password": "wrong" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 401);
}

#[tokio::test]
async fn login_unknown_user_returns_401() {
    let user_store: UserStore = Arc::new(RwLock::new(HashMap::new()));
    let state = test_app_state(user_store);
    let (base_url, _handle) = spawn_app(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{}/auth/login", base_url))
        .json(&serde_json::json!({ "username": "nobody", "password": "any" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 401);
}

#[tokio::test]
async fn login_with_env_seeded_user() {
    let user_id = Uuid::new_v4();
    let password_hash = auth::hash_password("envpass").unwrap();
    let cred = AuthUserCredential {
        user_id,
        username: "seeded".to_string(),
        password_hash,
    };
    let mut map = HashMap::new();
    map.insert("seeded".to_string(), cred);
    let user_store: UserStore = Arc::new(RwLock::new(map));
    let state = test_app_state(user_store);
    let (base_url, _handle) = spawn_app(state).await;
    let client = reqwest::Client::new();

    let res = client
        .post(format!("{}/auth/login", base_url))
        .json(&serde_json::json!({ "username": "seeded", "password": "envpass" }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status().as_u16(), 200);
    let json: serde_json::Value = res.json().await.unwrap();
    let uid_str = json.get("user_id").and_then(|v| v.as_str()).unwrap();
    assert_eq!(uid_str, user_id.to_string());
}
