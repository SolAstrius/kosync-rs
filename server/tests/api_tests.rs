use axum::http::HeaderName;
use axum::http::HeaderValue;
use axum_test::TestServer;
use kosync_server::{create_router, AppState, Database};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

fn setup_test_server() -> (TestServer, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Database::open(db_path.to_str().unwrap()).unwrap();
    let state = AppState { db: Arc::new(db) };
    let app = create_router(state);
    let server = TestServer::new(app).unwrap();
    (server, temp_dir)
}

fn md5_hash(s: &str) -> String {
    format!("{:x}", md5::compute(s))
}

fn auth_user_header() -> HeaderName {
    HeaderName::from_static("x-auth-user")
}

fn auth_key_header() -> HeaderName {
    HeaderName::from_static("x-auth-key")
}

// === Health Check ===

#[tokio::test]
async fn test_healthcheck() {
    let (server, _dir) = setup_test_server();

    let response = server.get("/healthcheck").await;

    response.assert_status_ok();
    response.assert_json(&json!({"state": "OK"}));
}

// === User Registration ===

#[tokio::test]
async fn test_register_user() {
    let (server, _dir) = setup_test_server();

    let response = server
        .post("/users/create")
        .json(&json!({
            "username": "testuser",
            "password": md5_hash("testpass")
        }))
        .await;

    response.assert_status(axum::http::StatusCode::CREATED);
    response.assert_json(&json!({"username": "testuser"}));
}

#[tokio::test]
async fn test_register_duplicate_user() {
    let (server, _dir) = setup_test_server();

    // First registration
    server
        .post("/users/create")
        .json(&json!({
            "username": "testuser",
            "password": md5_hash("testpass")
        }))
        .await;

    // Second registration (should fail)
    let response = server
        .post("/users/create")
        .json(&json!({
            "username": "testuser",
            "password": md5_hash("testpass")
        }))
        .await;

    response.assert_status(axum::http::StatusCode::PAYMENT_REQUIRED);
    response.assert_json(&json!({
        "code": 2002,
        "message": "User already exists"
    }));
}

#[tokio::test]
async fn test_register_invalid_username() {
    let (server, _dir) = setup_test_server();

    // Empty username
    let response = server
        .post("/users/create")
        .json(&json!({
            "username": "",
            "password": "somepass"
        }))
        .await;

    response.assert_status(axum::http::StatusCode::FORBIDDEN);

    // Username with colon
    let response = server
        .post("/users/create")
        .json(&json!({
            "username": "user:name",
            "password": "somepass"
        }))
        .await;

    response.assert_status(axum::http::StatusCode::FORBIDDEN);
}

// === User Authentication ===

#[tokio::test]
async fn test_auth_success() {
    let (server, _dir) = setup_test_server();
    let userkey = md5_hash("testpass");

    // Register first
    server
        .post("/users/create")
        .json(&json!({
            "username": "testuser",
            "password": &userkey
        }))
        .await;

    // Auth
    let response = server
        .get("/users/auth")
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .await;

    response.assert_status_ok();
    response.assert_json(&json!({"authorized": "OK"}));
}

#[tokio::test]
async fn test_auth_wrong_password() {
    let (server, _dir) = setup_test_server();

    // Register first
    server
        .post("/users/create")
        .json(&json!({
            "username": "testuser",
            "password": md5_hash("testpass")
        }))
        .await;

    // Auth with wrong password
    let wrong_key = md5_hash("wrongpass");
    let response = server
        .get("/users/auth")
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&wrong_key).unwrap())
        .await;

    response.assert_status(axum::http::StatusCode::UNAUTHORIZED);
    response.assert_json(&json!({
        "code": 2001,
        "message": "Unauthorized"
    }));
}

// === Progress Sync ===

#[tokio::test]
async fn test_update_and_get_progress() {
    let (server, _dir) = setup_test_server();
    let userkey = md5_hash("testpass");
    let doc_hash = md5_hash("test_document.epub");

    // Register
    server
        .post("/users/create")
        .json(&json!({
            "username": "testuser",
            "password": &userkey
        }))
        .await;

    // Update progress
    let response = server
        .put("/syncs/progress")
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .json(&json!({
            "document": &doc_hash,
            "progress": "/body/DocFragment[5]/body/p[10]",
            "percentage": 0.32,
            "device": "TestDevice",
            "device_id": "test-device-123"
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["document"], doc_hash);
    assert!(body["timestamp"].as_i64().unwrap() > 0);

    // Get progress
    let response = server
        .get(&format!("/syncs/progress/{}", doc_hash))
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["document"], doc_hash);
    assert_eq!(body["progress"], "/body/DocFragment[5]/body/p[10]");
    assert_eq!(body["percentage"], 0.32);
    assert_eq!(body["device"], "TestDevice");
    assert_eq!(body["device_id"], "test-device-123");
}

#[tokio::test]
async fn test_get_nonexistent_progress() {
    let (server, _dir) = setup_test_server();
    let userkey = md5_hash("testpass");
    let doc_hash = md5_hash("nonexistent.epub");

    // Register
    server
        .post("/users/create")
        .json(&json!({
            "username": "testuser",
            "password": &userkey
        }))
        .await;

    // Get progress for non-existent document
    let response = server
        .get(&format!("/syncs/progress/{}", doc_hash))
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    // Should return empty object (no progress field)
    assert!(body.get("progress").is_none() || body["progress"].is_null());
}

#[tokio::test]
async fn test_progress_overwrites() {
    let (server, _dir) = setup_test_server();
    let userkey = md5_hash("testpass");
    let doc_hash = md5_hash("test.epub");

    // Register
    server
        .post("/users/create")
        .json(&json!({
            "username": "testuser",
            "password": &userkey
        }))
        .await;

    // First update
    server
        .put("/syncs/progress")
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .json(&json!({
            "document": &doc_hash,
            "progress": "page1",
            "percentage": 0.1,
            "device": "Device1",
            "device_id": "dev1"
        }))
        .await;

    // Second update (should overwrite)
    server
        .put("/syncs/progress")
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .json(&json!({
            "document": &doc_hash,
            "progress": "page50",
            "percentage": 0.5,
            "device": "Device2",
            "device_id": "dev2"
        }))
        .await;

    // Get progress - should be the latest
    let response = server
        .get(&format!("/syncs/progress/{}", doc_hash))
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .await;

    let body: serde_json::Value = response.json();
    assert_eq!(body["progress"], "page50");
    assert_eq!(body["percentage"], 0.5);
    assert_eq!(body["device"], "Device2");
}

// === Annotations Sync ===

#[tokio::test]
async fn test_update_and_get_annotations() {
    let (server, _dir) = setup_test_server();
    let userkey = md5_hash("testpass");
    let doc_hash = md5_hash("test.epub");

    // Register
    server
        .post("/users/create")
        .json(&json!({
            "username": "testuser",
            "password": &userkey
        }))
        .await;

    // Update annotations
    let response = server
        .put(&format!("/syncs/annotations/{}", doc_hash))
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .json(&json!({
            "annotations": [
                {
                    "datetime": "2024-01-15 10:30:00",
                    "drawer": "highlight",
                    "color": "#ffff00",
                    "text": "Important text",
                    "page": "/body/p[1]",
                    "pos0": "/body/p[1]/text()[1]",
                    "pos1": "/body/p[1]/text()[10]"
                },
                {
                    "datetime": "2024-01-15 11:00:00",
                    "page": "/body/p[5]",
                    "note": "Page bookmark"
                }
            ],
            "deleted": []
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["version"], 1);

    // Get annotations
    let response = server
        .get(&format!("/syncs/annotations/{}", doc_hash))
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["version"], 1);
    assert_eq!(body["annotations"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_annotations_merge() {
    let (server, _dir) = setup_test_server();
    let userkey = md5_hash("testpass");
    let doc_hash = md5_hash("test.epub");

    // Register
    server
        .post("/users/create")
        .json(&json!({
            "username": "testuser",
            "password": &userkey
        }))
        .await;

    // First device uploads annotations
    server
        .put(&format!("/syncs/annotations/{}", doc_hash))
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .json(&json!({
            "annotations": [
                {
                    "datetime": "2024-01-15 10:00:00",
                    "text": "Highlight 1",
                    "page": "/body/p[1]",
                    "pos0": "/body/p[1]",
                    "pos1": "/body/p[1]"
                }
            ],
            "deleted": [],
            "base_version": 0
        }))
        .await;

    // Second device uploads different annotations
    let response = server
        .put(&format!("/syncs/annotations/{}", doc_hash))
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .json(&json!({
            "annotations": [
                {
                    "datetime": "2024-01-15 11:00:00",
                    "text": "Highlight 2",
                    "page": "/body/p[2]",
                    "pos0": "/body/p[2]",
                    "pos1": "/body/p[2]"
                }
            ],
            "deleted": [],
            "base_version": 1
        }))
        .await;

    response.assert_status_ok();

    // Get merged annotations
    let response = server
        .get(&format!("/syncs/annotations/{}", doc_hash))
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .await;

    let body: serde_json::Value = response.json();
    // Should have both annotations merged
    assert_eq!(body["annotations"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_annotations_deletion_tracking() {
    let (server, _dir) = setup_test_server();
    let userkey = md5_hash("testpass");
    let doc_hash = md5_hash("test.epub");

    // Register
    server
        .post("/users/create")
        .json(&json!({
            "username": "testuser",
            "password": &userkey
        }))
        .await;

    // Upload annotation
    server
        .put(&format!("/syncs/annotations/{}", doc_hash))
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .json(&json!({
            "annotations": [
                {
                    "datetime": "2024-01-15 10:00:00",
                    "text": "To be deleted",
                    "page": "/body/p[1]",
                    "pos0": "/body/p[1]",
                    "pos1": "/body/p[1]"
                }
            ],
            "deleted": []
        }))
        .await;

    // Delete annotation
    server
        .put(&format!("/syncs/annotations/{}", doc_hash))
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .json(&json!({
            "annotations": [],
            "deleted": ["2024-01-15 10:00:00"]
        }))
        .await;

    // Get annotations
    let response = server
        .get(&format!("/syncs/annotations/{}", doc_hash))
        .add_header(auth_user_header(), HeaderValue::from_static("testuser"))
        .add_header(auth_key_header(), HeaderValue::from_str(&userkey).unwrap())
        .await;

    let body: serde_json::Value = response.json();
    assert_eq!(body["annotations"].as_array().unwrap().len(), 0);
    assert!(body["deleted"]
        .as_array()
        .unwrap()
        .contains(&json!("2024-01-15 10:00:00")));
}

// === Authorization Tests ===

#[tokio::test]
async fn test_progress_requires_auth() {
    let (server, _dir) = setup_test_server();

    // Try to get progress without auth
    let response = server.get("/syncs/progress/somehash").await;

    response.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_annotations_requires_auth() {
    let (server, _dir) = setup_test_server();

    // Try to get annotations without auth
    let response = server.get("/syncs/annotations/somehash").await;

    response.assert_status(axum::http::StatusCode::UNAUTHORIZED);
}
