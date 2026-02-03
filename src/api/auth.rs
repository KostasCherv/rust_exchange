use argon2::password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
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

/// User credential for login validation (from DB or in-memory). Holds only password hash.
#[derive(Clone)]
pub struct AuthUserCredential {
    pub user_id: Uuid,
    pub username: String,
    pub password_hash: String,
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

/// Hash a plaintext password for storage. Uses Argon2.
pub fn hash_password(plain: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(plain.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Verify a plaintext password against a stored hash.
pub fn verify_password(plain: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(p) => p,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(plain.as_bytes(), &parsed)
        .is_ok()
}
