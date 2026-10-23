use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum_extra::extract::CookieJar;
use chrono::Utc;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use uuid::Uuid;

use super::*;

#[derive(Debug, Serialize)]
pub struct AuthBodyDto {
    pub id: Uuid,
    pub refresh_token: String,
    pub access_token: String,
    pub token_type: String,
}

impl AuthBodyDto {
    pub fn new(access_token: String, refresh_token: String, id: Uuid) -> Self {
        Self {
            id,
            access_token,
            token_type: "Bearer".to_string(),
            refresh_token,
        }
    }
}

static KEYS: LazyLock<Keys> = LazyLock::new(|| {
    let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    Keys::new(secret.as_bytes())
});

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub user_id: Uuid,
    pub role: crate::users::RoleType,
    pub exp: usize,
}

pub fn create_session_token(
    id: Uuid,
    role: crate::users::RoleType,
) -> Result<AuthBodyDto, ModuleError> {
    let expiration = Utc::now()
        .checked_add_signed(chrono::Duration::minutes(30))
        .expect("valid timestamp")
        .timestamp() as usize;

    let refresh_expiration = Utc::now()
        .checked_add_signed(chrono::Duration::days(7))
        .expect("valid timestamp")
        .timestamp() as usize;

    let mut claims = Claims {
        user_id: id,
        role,
        exp: expiration,
    };

    // Create the authorization token
    let token = encode(&Header::default(), &claims, &KEYS.encoding)
        .map_err(|_| ModuleError::InternalError("Could not create Token".into()))?;

    claims.exp = refresh_expiration;

    let refresh_token = encode(&Header::default(), &claims, &KEYS.encoding)
        .map_err(|_| ModuleError::InternalError("Could not create Token".into()))?;

    // Send the authorized token
    Ok(AuthBodyDto::new(token, refresh_token, id))
}

pub fn decode_token(token: &str) -> Result<Claims, ModuleError> {
    let token_data = decode::<Claims>(token, &KEYS.decoding, &Validation::default())
        .map_err(|_| ModuleError::InvalidToken)?;
    Ok(token_data.claims)
}

impl std::fmt::Display for Claims {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "User: {}", self.user_id)
    }
}

impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = ModuleError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let token = if let Some(auth_header) = parts.headers.get(axum::http::header::AUTHORIZATION)
        {
            let auth_str = auth_header
                .to_str()
                .map_err(|_| ModuleError::InvalidToken)?;
            if let Some(bearer_token) = auth_str.strip_prefix("Bearer ") {
                Some(bearer_token.trim().to_string())
            } else {
                Some(auth_str.trim().to_string())
            }
        } else {
            let jar = CookieJar::from_request_parts(parts, state)
                .await
                .map_err(|_| ModuleError::InternalError("Could not extract cookies".into()))?;

            jar.get("access_token")
                .map(|cookie| cookie.value().to_string())
        };

        let token = token.ok_or(ModuleError::CouldNotExtractToken(
            "Could not extract token.",
        ))?;

        decode_token(&token)
    }
}

struct Keys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl Keys {
    fn new(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }
}
