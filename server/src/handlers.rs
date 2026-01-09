use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use serde_json::json;

use crate::error::{AppError, Result};
use crate::models::*;
use crate::AppState;

// === Auth helpers ===

fn extract_auth(headers: &HeaderMap) -> Result<(&str, &str)> {
    let user = headers
        .get("x-auth-user")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty() && !s.contains(':'))
        .ok_or(AppError::Unauthorized)?;

    let key = headers
        .get("x-auth-key")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .ok_or(AppError::Unauthorized)?;

    Ok((user, key))
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<String> {
    let (user, key) = extract_auth(headers)?;
    if state.db.verify_user(user, key)? {
        Ok(user.to_string())
    } else {
        Err(AppError::Unauthorized)
    }
}

// === User endpoints ===

pub async fn create_user(
    State(state): State<AppState>,
    Json(req): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<CreateUserResponse>)> {
    if req.username.is_empty() || req.username.contains(':') {
        return Err(AppError::InvalidRequest("invalid username".into()));
    }
    if req.password.is_empty() {
        return Err(AppError::InvalidRequest("invalid password".into()));
    }

    if state.db.create_user(&req.username, &req.password)? {
        Ok((
            StatusCode::CREATED,
            Json(CreateUserResponse {
                username: req.username,
            }),
        ))
    } else {
        Err(AppError::UserExists)
    }
}

pub async fn auth_user(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthResponse>> {
    authorize(&state, &headers)?;
    Ok(Json(AuthResponse { authorized: "OK" }))
}

// === Progress endpoints (legacy KOSync) ===

pub async fn get_progress(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(document): Path<String>,
) -> Result<Json<Progress>> {
    let username = authorize(&state, &headers)?;

    if document.is_empty() || document.contains(':') {
        return Err(AppError::DocumentMissing);
    }

    let progress = state.db.get_progress(&username, &document)?;
    Ok(Json(progress))
}

pub async fn update_progress(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<UpdateProgressRequest>,
) -> Result<Json<UpdateProgressResponse>> {
    let username = authorize(&state, &headers)?;

    if req.document.is_empty() || req.document.contains(':') {
        return Err(AppError::DocumentMissing);
    }
    if req.progress.is_empty() || req.device.is_empty() {
        return Err(AppError::InvalidRequest("missing required fields".into()));
    }

    let timestamp = state.db.set_progress(
        &username,
        &req.document,
        &req.progress,
        req.percentage,
        &req.device,
        req.device_id.as_deref(),
    )?;

    Ok(Json(UpdateProgressResponse {
        document: req.document,
        timestamp,
    }))
}

// === Annotations endpoints (extended API) ===

pub async fn get_annotations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(document): Path<String>,
) -> Result<Json<DocumentAnnotations>> {
    let username = authorize(&state, &headers)?;

    if document.is_empty() || document.contains(':') {
        return Err(AppError::DocumentMissing);
    }

    let annotations = state.db.get_annotations(&username, &document)?;
    Ok(Json(annotations))
}

pub async fn update_annotations(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(document): Path<String>,
    Json(req): Json<UpdateAnnotationsRequest>,
) -> Result<Json<UpdateAnnotationsResponse>> {
    let username = authorize(&state, &headers)?;

    if document.is_empty() || document.contains(':') {
        return Err(AppError::DocumentMissing);
    }

    let (version, timestamp) = state.db.update_annotations(
        &username,
        &document,
        req.annotations,
        req.deleted,
        req.base_version,
    )?;

    Ok(Json(UpdateAnnotationsResponse { version, timestamp }))
}

// === Health check ===

pub async fn healthcheck() -> Json<serde_json::Value> {
    Json(json!({ "state": "OK" }))
}
