use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT claims: `sub` = user id (Uuid as string), `exp` (expiry), `iat` (issued at).
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub exp: i64,
    pub iat: i64,
}

/// Authenticated user extracted from JWT Bearer token.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: Uuid,
}

/// User credential for login validation (from env or config).
#[derive(Clone)]
pub struct AuthUserCredential {
    pub user_id: Uuid,
    pub username: String,
    pub password: String,
}

const JWT_EXPIRY_HOURS: i64 = 24;

impl Claims {
    pub fn new(user_id: Uuid) -> Self {
        let now = chrono::Utc::now();
        let exp = (now + chrono::Duration::hours(JWT_EXPIRY_HOURS)).timestamp();
        Self {
            sub: user_id.to_string(),
            exp,
            iat: now.timestamp(),
        }
    }
}

pub fn create_token(secret: &[u8], user_id: Uuid) -> Result<String, jsonwebtoken::errors::Error> {
    let claims = Claims::new(user_id);
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
}

pub fn decode_token(secret: &[u8], token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::default();
    validation.validate_exp = true;
    let token_data = decode::<Claims>(token, &DecodingKey::from_secret(secret), &validation)?;
    Ok(token_data.claims)
}

/// Constant-time comparison for password check (MVP: plain env comparison).
#[inline]
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}
