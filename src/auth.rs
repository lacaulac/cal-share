use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::state::AppState;

const ISSUER: &str = "axum-ical-service";

/// Claims structure stored inside the non-expiring JSON Web Token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Identifier string identifying the calendar upload
    pub sub: String,
    /// Issuer tag
    pub iss: String,
    /// Timestamp when token was issued
    pub iat: usize,
}

impl Claims {
    pub fn new(identifier: String) -> Self {
        let iat = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as usize;

        Self {
            sub: identifier,
            iss: ISSUER.to_string(),
            iat,
        }
    }
}

/// Generates a signed, non-expiring JWT for a given calendar identifier.
pub fn generate_token(
    secret: &str,
    identifier: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let claims = Claims::new(identifier.to_string());
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Verifies a JWT and extracts claims without validating expiration (non-expiring).
pub fn verify_token(secret: &str, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::default();
    // Prompt specification: JSON Web Tokens should not expire.
    validation.validate_exp = false;
    //Create an empty hashset
    validation.required_spec_claims = std::collections::HashSet::new();
    validation.set_issuer(&[ISSUER]);

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;

    Ok(token_data.claims)
}

/// Axum extractor for authenticated uploads.
/// Extracts the Bearer token from the HTTP Authorization header and verifies it against AppState.
pub struct AuthClaims(pub Claims);

impl FromRequestParts<AppState> for AuthClaims {
    type Rejection = AuthError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AuthError::MissingHeader)?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .or_else(|| auth_header.strip_prefix("bearer "))
            .ok_or(AuthError::InvalidHeaderFormat)?
            .trim();

        if token.is_empty() {
            return Err(AuthError::MissingHeader);
        }

        let claims = verify_token(&state.inner.jwt_secret, token).unwrap();
        // .map_err(|_| AuthError::InvalidToken)?;

        // Ensure sub claim contains a safe identifier
        AppState::sanitize_identifier(&claims.sub)
            .map_err(|_| AuthError::InvalidIdentifierInToken)?;

        Ok(AuthClaims(claims))
    }
}

#[derive(Debug)]
pub enum AuthError {
    MissingHeader,
    InvalidHeaderFormat,
    InvalidToken,
    InvalidIdentifierInToken,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            AuthError::MissingHeader => (
                StatusCode::UNAUTHORIZED,
                "Missing 'Authorization: Bearer <token>' header",
            ),
            AuthError::InvalidHeaderFormat => (
                StatusCode::UNAUTHORIZED,
                "Invalid Authorization format. Expected 'Bearer <token>'",
            ),
            AuthError::InvalidToken => (
                StatusCode::UNAUTHORIZED,
                "Invalid or untrusted JSON Web Token",
            ),
            AuthError::InvalidIdentifierInToken => (
                StatusCode::BAD_REQUEST,
                "Identifier inside token contains invalid or unsafe characters",
            ),
        };

        let body = Json(serde_json::json!({
            "error": error_message,
            "status": status.as_u16(),
        }));

        (status, body).into_response()
    }
}
