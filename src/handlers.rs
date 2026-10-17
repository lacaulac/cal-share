use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use std::io::Write;
use tempfile::NamedTempFile;
use tokio::fs;

use crate::{
    auth::{generate_token, AuthClaims},
    state::AppState,
};

// -----------------------------------------------------------------------------
// 1. POST /create_user
// -----------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub secret: String,
    pub identifier: String,
}

#[derive(Debug, Serialize)]
pub struct CreateUserResponse {
    pub token: String,
    pub identifier: String,
    pub calendar_url: String,
    pub message: String,
}

/// Admin endpoint: Generates a new non-expiring JSON Web Token for the given identifier
/// when supplied with the matching instance JWT secret.
pub async fn create_user(
    State(state): State<AppState>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<Json<CreateUserResponse>, (StatusCode, Json<serde_json::Value>)> {
    // Constant-time-like comparison against the configured instance secret
    if payload.secret.trim() != state.inner.jwt_secret {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Invalid instance JWT secret provided",
                "status": 401
            })),
        ));
    }

    let sanitized_id = AppState::sanitize_identifier(&payload.identifier).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": err,
                "status": 400
            })),
        )
    })?;

    let token = generate_token(&state.inner.jwt_secret, &sanitized_id).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to generate JWT: {}", e),
                "status": 500
            })),
        )
    })?;

    Ok(Json(CreateUserResponse {
        token,
        identifier: sanitized_id.clone(),
        calendar_url: format!("/calendar/{}", sanitized_id),
        message: "Token generated successfully. Token does not expire.".to_string(),
    }))
}

// -----------------------------------------------------------------------------
// 2. POST /upload
// -----------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub status: String,
    pub identifier: String,
    pub bytes_written: usize,
    pub calendar_url: String,
}

/// Upload endpoint: Accepts a .ical file body, verifies the caller's JWT, and writes / replaces
/// the calendar file at `./data/calendars/{identifier}.ics`.
pub async fn upload_calendar(
    State(state): State<AppState>,
    AuthClaims(claims): AuthClaims,
    body: Bytes,
) -> Result<Json<UploadResponse>, (StatusCode, Json<serde_json::Value>)> {
    if body.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Uploaded calendar payload cannot be empty",
                "status": 400
            })),
        ));
    }

    let content = match std::str::from_utf8(&body) {
        Ok(c) => c,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "Uploaded file must be valid UTF-8 iCalendar data",
                    "status": 400
                })),
            ))
        }
    };

    // Basic iCalendar format check (must contain BEGIN:VCALENDAR)
    if !content.contains("BEGIN:VCALENDAR") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Uploaded payload does not appear to be an iCalendar file (missing BEGIN:VCALENDAR)",
                "status": 400
            })),
        ));
    }

    let target_path = state.calendar_path(&claims.sub).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": err,
                "status": 400
            })),
        )
    })?;

    // Perform atomic file write: write to temp file in target directory, then rename.
    // This ensures that readers never observe partially written files and replaces any existing file.
    let storage_dir = state.inner.storage_dir.clone();
    let body_bytes = body.to_vec();
    let bytes_len = body_bytes.len();

    tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let mut temp_file = NamedTempFile::new_in(&storage_dir)?;
        temp_file.write_all(&body_bytes)?;
        temp_file.flush()?;
        temp_file.persist(&target_path).map_err(|e| e.error)?;
        Ok(())
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Worker task failed: {}", e),
                "status": 500
            })),
        )
    })?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to write calendar file to disk: {}", e),
                "status": 500
            })),
        )
    })?;

    Ok(Json(UploadResponse {
        status: "success".to_string(),
        identifier: claims.sub.clone(),
        bytes_written: bytes_len,
        calendar_url: format!("/calendar/{}", claims.sub),
    }))
}

// -----------------------------------------------------------------------------
// 3. GET /calendar/:identifier
// -----------------------------------------------------------------------------

/// Calendar endpoint: Fetches and streams the .ical file for the given identifier.
pub async fn get_calendar(
    State(state): State<AppState>,
    Path(identifier): Path<String>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let target_path = state.calendar_path(&identifier).map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": err,
                "status": 400
            })),
        )
    })?;

    if !target_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("No calendar found for identifier '{}'", identifier),
                "status": 404
            })),
        ));
    }

    let contents = fs::read_to_string(&target_path).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": format!("Failed to read calendar file: {}", e),
                "status": 500
            })),
        )
    })?;

    let filename = format!("{}.ics", identifier);

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/calendar; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", filename),
        )
        .header(header::CACHE_CONTROL, "no-cache, must-revalidate")
        .body(contents.into_response().into_body())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": format!("Failed to construct HTTP response: {}", e),
                    "status": 500
                })),
            )
        })?;

    Ok(response)
}

// -----------------------------------------------------------------------------
// Health Check
// -----------------------------------------------------------------------------

pub async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "service": "axum-ical-service",
        "version": env!("CARGO_PKG_VERSION")
    }))
}
