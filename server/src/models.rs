use serde::{Deserialize, Serialize};

// === Auth ===

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct CreateUserResponse {
    pub username: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub authorized: &'static str,
}

// === Progress (legacy KOSync) ===

#[derive(Debug, Deserialize)]
pub struct UpdateProgressRequest {
    pub document: String,
    pub progress: String,
    pub percentage: f64,
    pub device: String,
    pub device_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateProgressResponse {
    pub document: String,
    pub timestamp: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Progress {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<i64>,
}

// === Annotations (extended API) ===

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    pub datetime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub datetime_updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drawer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_edited: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pageno: Option<i32>,
    pub page: serde_json::Value, // string (xpointer) or number
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos0: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pos1: Option<serde_json::Value>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DocumentAnnotations {
    pub version: u64,
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub deleted: Vec<String>,
    pub updated_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAnnotationsRequest {
    pub annotations: Vec<Annotation>,
    #[serde(default)]
    pub deleted: Vec<String>,
    #[serde(default)]
    pub base_version: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct UpdateAnnotationsResponse {
    pub version: u64,
    pub timestamp: i64,
}

// === Errors ===

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub code: u32,
    pub message: String,
}

impl ErrorResponse {
    pub fn new(code: u32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
