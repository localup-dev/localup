//! Integration tests for OAuth 2.0 Device Authorization Grant (RFC 8628)
//!
//! Tests the complete device authorization flow:
//! 1. Third-party app calls POST /api/device/authorize with registered client_id
//! 2. User visits /device, enters user_code, sees app info
//! 3. User approves (POST /api/device/verify) or denies (POST /api/device/deny)
//! 4. Third-party app polls POST /api/device/token until approved/denied/expired

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use localup_api::{models::*, ApiServer, ApiServerConfig, OAuthClientConfig};
use localup_control::TunnelConnectionManager;
use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt; // For `oneshot` method

const JWT_SECRET: &str = "test-secret";

/// Helper to create an in-memory database with migrations applied
async fn create_test_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");

    localup_relay_db::migrator::Migrator::up(&db, None)
        .await
        .expect("Failed to run migrations");

    db
}

/// Helper to create a test API server with OAuth clients registered
fn create_test_server_with_oauth(
    db: DatabaseConnection,
    oauth_clients: Vec<OAuthClientConfig>,
) -> ApiServer {
    let tunnel_manager = Arc::new(TunnelConnectionManager::new());
    let config = ApiServerConfig {
        http_addr: Some("127.0.0.1:0".parse().unwrap()),
        https_addr: None,
        enable_cors: true,
        cors_origins: None,
        jwt_secret: JWT_SECRET.to_string(),
        tls_cert_path: None,
        tls_key_path: None,
        social_auth: localup_api::SocialAuthConfig::default(),
        smtp: None,
        oauth_clients,
    };

    ApiServer::new(config, tunnel_manager, db, true)
}

/// Helper to create a test server with a default test OAuth client
fn create_test_server(db: DatabaseConnection) -> ApiServer {
    create_test_server_with_oauth(
        db,
        vec![OAuthClientConfig {
            client_id: "test-app".to_string(),
            display_name: "Test Application".to_string(),
        }],
    )
}

/// Helper: register a user and return their session token
async fn register_and_get_token(db: DatabaseConnection) -> (String, ApiServer) {
    let server = create_test_server(db);
    let app = server.build_router();

    let register_body = json!({
        "email": "device-test@example.com",
        "password": "SecurePassword123!",
        "full_name": "Device Test User"
    });

    let request = Request::builder()
        .uri("/api/auth/register")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let data: RegisterResponse = serde_json::from_slice(&body).unwrap();

    (data.token, server)
}

/// Helper: parse response body as JSON
async fn body_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

// ============================================================================
// POST /api/device/authorize — Initiate device flow
// ============================================================================

#[tokio::test]
async fn test_device_authorize_success() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_id": "test-app"}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // Verify response fields per RFC 8628
    assert!(
        !data.device_code.is_empty(),
        "device_code must not be empty"
    );
    assert_eq!(
        data.device_code.len(),
        40,
        "device_code should be 40-char hex"
    );
    assert_eq!(
        data.user_code.len(),
        9,
        "user_code should be XXXX-YYYY (9 chars)"
    );
    assert!(
        data.user_code.contains('-'),
        "user_code should contain a dash"
    );
    assert!(
        data.verification_uri.contains("/device"),
        "verification_uri should point to /device"
    );
    assert!(
        data.verification_uri_complete
            .as_ref()
            .unwrap()
            .contains(&data.user_code),
        "verification_uri_complete should contain user_code"
    );
    assert_eq!(
        data.expires_in, 600,
        "expires_in should be 600 seconds (10 min)"
    );
    assert_eq!(data.interval, 5, "interval should be 5 seconds");
}

#[tokio::test]
async fn test_device_authorize_with_scope() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"client_id": "test-app", "scope": "tunnels:read tunnels:write"}).to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(!data.device_code.is_empty());
    assert!(!data.user_code.is_empty());
}

#[tokio::test]
async fn test_device_authorize_empty_client_id() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_id": ""}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let data = body_json(response).await;
    assert_eq!(data["code"], "invalid_request");
}

#[tokio::test]
async fn test_device_authorize_unregistered_client_id() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_id": "unknown-app"}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let data = body_json(response).await;
    assert_eq!(data["code"], "invalid_client");
}

#[tokio::test]
async fn test_device_authorize_no_oauth_clients_registered() {
    let db = create_test_db().await;
    // Server with no OAuth clients
    let server = create_test_server_with_oauth(db, Vec::new());
    let app = server.build_router();

    let request = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_id": "test-app"}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let data = body_json(response).await;
    assert_eq!(data["code"], "invalid_client");
    assert!(
        data["error"].as_str().unwrap().contains("not configured"),
        "Error should mention device auth is not configured"
    );
}

// ============================================================================
// GET /api/device/info — Look up device authorization by user_code
// ============================================================================

#[tokio::test]
async fn test_device_info_valid_code() {
    let db = create_test_db().await;
    let server = create_test_server(db);

    // Step 1: Create a device authorization
    let app = server.build_router();
    let authorize_req = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"client_id": "test-app", "scope": "tunnels:read"}).to_string(),
        ))
        .unwrap();

    let authorize_resp = app.oneshot(authorize_req).await.unwrap();
    assert_eq!(authorize_resp.status(), StatusCode::OK);

    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // Step 2: Look up by user_code
    let app2 = server.build_router();
    let info_req = Request::builder()
        .uri(format!(
            "/api/device/info?user_code={}",
            auth_data.user_code
        ))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let info_resp = app2.oneshot(info_req).await.unwrap();
    assert_eq!(info_resp.status(), StatusCode::OK);

    let info: DeviceAuthorizationInfo = serde_json::from_slice(
        &axum::body::to_bytes(info_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(info.valid);
    assert_eq!(info.client_id, Some("test-app".to_string()));
    assert_eq!(info.client_name, Some("Test Application".to_string()));
    assert_eq!(info.scope, Some("tunnels:read".to_string()));
    assert!(info.message.contains("Approve"));
}

#[tokio::test]
async fn test_device_info_invalid_format() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request = Request::builder()
        .uri("/api/device/info?user_code=ABC")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let info: DeviceAuthorizationInfo = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(!info.valid);
    assert_eq!(info.client_id, None);
    assert_eq!(info.client_name, None);
    assert!(info.message.contains("Invalid code format"));
}

#[tokio::test]
async fn test_device_info_nonexistent_code() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request = Request::builder()
        .uri("/api/device/info?user_code=ABCD-EFGH")
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let info: DeviceAuthorizationInfo = serde_json::from_slice(
        &axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(!info.valid);
    assert!(info.message.contains("Invalid or expired"));
}

#[tokio::test]
async fn test_device_info_code_without_dash() {
    let db = create_test_db().await;
    let server = create_test_server(db);

    // Step 1: Create a device authorization
    let app = server.build_router();
    let authorize_req = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_id": "test-app"}).to_string()))
        .unwrap();

    let authorize_resp = app.oneshot(authorize_req).await.unwrap();
    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // Step 2: Look up without dash (should still work — normalization)
    let code_no_dash = auth_data.user_code.replace('-', "");
    let app2 = server.build_router();
    let info_req = Request::builder()
        .uri(format!("/api/device/info?user_code={}", code_no_dash))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let info_resp = app2.oneshot(info_req).await.unwrap();
    let info: DeviceAuthorizationInfo = serde_json::from_slice(
        &axum::body::to_bytes(info_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(info.valid, "Code without dash should still be found");
    assert_eq!(info.client_id, Some("test-app".to_string()));
}

#[tokio::test]
async fn test_device_info_case_insensitive() {
    let db = create_test_db().await;
    let server = create_test_server(db);

    // Step 1: Create a device authorization
    let app = server.build_router();
    let authorize_req = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_id": "test-app"}).to_string()))
        .unwrap();

    let authorize_resp = app.oneshot(authorize_req).await.unwrap();
    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // Step 2: Look up with lowercase (should still work — normalization)
    let code_lower = auth_data.user_code.to_lowercase();
    let app2 = server.build_router();
    let info_req = Request::builder()
        .uri(format!("/api/device/info?user_code={}", code_lower))
        .method("GET")
        .body(Body::empty())
        .unwrap();

    let info_resp = app2.oneshot(info_req).await.unwrap();
    let info: DeviceAuthorizationInfo = serde_json::from_slice(
        &axum::body::to_bytes(info_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(info.valid, "Lowercase code should still be found");
}

// ============================================================================
// POST /api/device/verify — Authenticated user approves
// ============================================================================

#[tokio::test]
async fn test_device_verify_approve() {
    let db = create_test_db().await;

    // Step 1: Register user and get session token
    let (session_token, _) = register_and_get_token(db.clone()).await;

    let server2 = create_test_server(db.clone());
    let app2 = server2.build_router();
    let authorize_req = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_id": "test-app"}).to_string()))
        .unwrap();

    let authorize_resp = app2.oneshot(authorize_req).await.unwrap();
    assert_eq!(authorize_resp.status(), StatusCode::OK);

    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // Step 3: Approve with authenticated session
    let server3 = create_test_server(db);
    let app3 = server3.build_router();
    let verify_req = Request::builder()
        .uri("/api/device/verify")
        .method("POST")
        .header("content-type", "application/json")
        .header("Authorization", format!("Bearer {}", session_token))
        .body(Body::from(
            json!({"user_code": auth_data.user_code}).to_string(),
        ))
        .unwrap();

    let verify_resp = app3.oneshot(verify_req).await.unwrap();
    assert_eq!(verify_resp.status(), StatusCode::OK);

    let verify_data: DeviceVerifyResponse = serde_json::from_slice(
        &axum::body::to_bytes(verify_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(verify_data.approved);
    assert_eq!(verify_data.client_id, "test-app");
    assert!(verify_data.message.contains("authorized successfully"));
}

#[tokio::test]
async fn test_device_verify_requires_auth() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    // Try to verify without auth token
    let request = Request::builder()
        .uri("/api/device/verify")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"user_code": "ABCD-EFGH"}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_device_verify_invalid_code() {
    let db = create_test_db().await;
    let (session_token, _) = register_and_get_token(db.clone()).await;

    let server = create_test_server(db);
    let app = server.build_router();

    // Try to approve a non-existent code
    let request = Request::builder()
        .uri("/api/device/verify")
        .method("POST")
        .header("content-type", "application/json")
        .header("Authorization", format!("Bearer {}", session_token))
        .body(Body::from(json!({"user_code": "ABCD-EFGH"}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let data = body_json(response).await;
    assert_eq!(data["code"], "invalid_grant");
}

#[tokio::test]
async fn test_device_verify_invalid_format() {
    let db = create_test_db().await;
    let (session_token, _) = register_and_get_token(db.clone()).await;

    let server = create_test_server(db);
    let app = server.build_router();

    let request = Request::builder()
        .uri("/api/device/verify")
        .method("POST")
        .header("content-type", "application/json")
        .header("Authorization", format!("Bearer {}", session_token))
        .body(Body::from(json!({"user_code": "ABC"}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let data = body_json(response).await;
    assert_eq!(data["code"], "invalid_request");
}

// ============================================================================
// POST /api/device/deny — Authenticated user denies
// ============================================================================

#[tokio::test]
async fn test_device_deny() {
    let db = create_test_db().await;
    let (session_token, _) = register_and_get_token(db.clone()).await;

    // Create device authorization
    let server2 = create_test_server(db.clone());
    let app2 = server2.build_router();
    let authorize_req = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_id": "test-app"}).to_string()))
        .unwrap();

    let authorize_resp = app2.oneshot(authorize_req).await.unwrap();
    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // Deny the authorization
    let server3 = create_test_server(db);
    let app3 = server3.build_router();
    let deny_req = Request::builder()
        .uri("/api/device/deny")
        .method("POST")
        .header("content-type", "application/json")
        .header("Authorization", format!("Bearer {}", session_token))
        .body(Body::from(
            json!({"user_code": auth_data.user_code}).to_string(),
        ))
        .unwrap();

    let deny_resp = app3.oneshot(deny_req).await.unwrap();
    assert_eq!(deny_resp.status(), StatusCode::OK);

    let deny_data: DeviceVerifyResponse = serde_json::from_slice(
        &axum::body::to_bytes(deny_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(!deny_data.approved);
    assert_eq!(deny_data.client_id, "test-app");
    assert!(deny_data.message.contains("denied"));
}

#[tokio::test]
async fn test_device_deny_requires_auth() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request = Request::builder()
        .uri("/api/device/deny")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"user_code": "ABCD-EFGH"}).to_string()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

// ============================================================================
// POST /api/device/token — Poll for access token
// ============================================================================

#[tokio::test]
async fn test_device_token_authorization_pending() {
    let db = create_test_db().await;
    let server = create_test_server(db);

    // Step 1: Create device authorization
    let app = server.build_router();
    let authorize_req = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_id": "test-app"}).to_string()))
        .unwrap();

    let authorize_resp = app.oneshot(authorize_req).await.unwrap();
    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // Step 2: Poll — should get authorization_pending (user hasn't approved yet)
    let app2 = server.build_router();
    let token_req = Request::builder()
        .uri("/api/device/token")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": auth_data.device_code,
                "client_id": "test-app"
            })
            .to_string(),
        ))
        .unwrap();

    let token_resp = app2.oneshot(token_req).await.unwrap();
    assert_eq!(token_resp.status(), StatusCode::BAD_REQUEST);

    let data = body_json(token_resp).await;
    assert_eq!(data["error"], "authorization_pending");
}

#[tokio::test]
async fn test_device_token_wrong_grant_type() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request = Request::builder()
        .uri("/api/device/token")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "grant_type": "authorization_code",
                "device_code": "abc123",
                "client_id": "test-app"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let data = body_json(response).await;
    assert_eq!(data["error"], "unsupported_grant_type");
}

#[tokio::test]
async fn test_device_token_invalid_device_code() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request = Request::builder()
        .uri("/api/device/token")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": "nonexistent-device-code-000000000000000000",
                "client_id": "test-app"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let data = body_json(response).await;
    assert_eq!(data["error"], "invalid_grant");
}

#[tokio::test]
async fn test_device_token_after_approval() {
    let db = create_test_db().await;
    let (session_token, _) = register_and_get_token(db.clone()).await;

    // Step 1: Create device authorization
    let server = create_test_server(db.clone());
    let app = server.build_router();
    let authorize_req = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_id": "test-app"}).to_string()))
        .unwrap();

    let authorize_resp = app.oneshot(authorize_req).await.unwrap();
    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // Step 2: Approve
    let server2 = create_test_server(db.clone());
    let app2 = server2.build_router();
    let verify_req = Request::builder()
        .uri("/api/device/verify")
        .method("POST")
        .header("content-type", "application/json")
        .header("Authorization", format!("Bearer {}", session_token))
        .body(Body::from(
            json!({"user_code": auth_data.user_code}).to_string(),
        ))
        .unwrap();

    let verify_resp = app2.oneshot(verify_req).await.unwrap();
    assert_eq!(verify_resp.status(), StatusCode::OK);

    // Step 3: Poll for token — should succeed now
    let server3 = create_test_server(db);
    let app3 = server3.build_router();
    let token_req = Request::builder()
        .uri("/api/device/token")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": auth_data.device_code,
                "client_id": "test-app"
            })
            .to_string(),
        ))
        .unwrap();

    let token_resp = app3.oneshot(token_req).await.unwrap();
    assert_eq!(token_resp.status(), StatusCode::OK);

    let token_data: DeviceTokenResponse = serde_json::from_slice(
        &axum::body::to_bytes(token_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(
        !token_data.access_token.is_empty(),
        "access_token must not be empty"
    );
    assert!(
        token_data.access_token.starts_with("eyJ"),
        "access_token should be a JWT"
    );
    assert_eq!(token_data.token_type, "Bearer");
    assert_eq!(
        token_data.expires_in, 86400,
        "Token should be valid for 24 hours"
    );
}

#[tokio::test]
async fn test_device_token_after_denial() {
    let db = create_test_db().await;
    let (session_token, _) = register_and_get_token(db.clone()).await;

    // Step 1: Create device authorization
    let server = create_test_server(db.clone());
    let app = server.build_router();
    let authorize_req = Request::builder()
        .uri("/api/device/authorize")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(json!({"client_id": "test-app"}).to_string()))
        .unwrap();

    let authorize_resp = app.oneshot(authorize_req).await.unwrap();
    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // Step 2: Deny
    let server2 = create_test_server(db.clone());
    let app2 = server2.build_router();
    let deny_req = Request::builder()
        .uri("/api/device/deny")
        .method("POST")
        .header("content-type", "application/json")
        .header("Authorization", format!("Bearer {}", session_token))
        .body(Body::from(
            json!({"user_code": auth_data.user_code}).to_string(),
        ))
        .unwrap();

    let deny_resp = app2.oneshot(deny_req).await.unwrap();
    assert_eq!(deny_resp.status(), StatusCode::OK);

    // Step 3: Poll for token — should get access_denied
    let server3 = create_test_server(db);
    let app3 = server3.build_router();
    let token_req = Request::builder()
        .uri("/api/device/token")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                "device_code": auth_data.device_code,
                "client_id": "test-app"
            })
            .to_string(),
        ))
        .unwrap();

    let token_resp = app3.oneshot(token_req).await.unwrap();
    assert_eq!(token_resp.status(), StatusCode::BAD_REQUEST);

    let data = body_json(token_resp).await;
    assert_eq!(data["error"], "access_denied");
}

#[tokio::test]
async fn test_device_token_consumed_after_retrieval() {
    let db = create_test_db().await;
    let (session_token, _) = register_and_get_token(db.clone()).await;

    // Step 1: Create + approve
    let server = create_test_server(db.clone());
    let app = server.build_router();
    let authorize_resp = app
        .oneshot(
            Request::builder()
                .uri("/api/device/authorize")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(json!({"client_id": "test-app"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    let server2 = create_test_server(db.clone());
    let app2 = server2.build_router();
    let verify_resp = app2
        .oneshot(
            Request::builder()
                .uri("/api/device/verify")
                .method("POST")
                .header("content-type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    json!({"user_code": auth_data.user_code}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_resp.status(), StatusCode::OK);

    // Step 2: First token retrieval — success
    let server3 = create_test_server(db.clone());
    let app3 = server3.build_router();
    let token_body = json!({
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        "device_code": auth_data.device_code,
        "client_id": "test-app"
    });

    let first_resp = app3
        .oneshot(
            Request::builder()
                .uri("/api/device/token")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(token_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(first_resp.status(), StatusCode::OK);

    // Step 3: Second token retrieval — should fail (status is now "consumed")
    let server4 = create_test_server(db);
    let app4 = server4.build_router();
    let second_resp = app4
        .oneshot(
            Request::builder()
                .uri("/api/device/token")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                        "device_code": auth_data.device_code,
                        "client_id": "test-app"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    // Should get an error — either "server_error" (unknown status "consumed") or "invalid_grant"
    assert_eq!(second_resp.status(), StatusCode::BAD_REQUEST);
}

// ============================================================================
// Full end-to-end flow
// ============================================================================

#[tokio::test]
async fn test_full_device_authorization_flow() {
    let db = create_test_db().await;

    // Step 1: Register a user (simulating the user who will approve)
    let (session_token, _) = register_and_get_token(db.clone()).await;

    // Step 2: Third-party app initiates device authorization
    let server = create_test_server(db.clone());
    let app = server.build_router();
    let authorize_resp = app
        .oneshot(
            Request::builder()
                .uri("/api/device/authorize")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"client_id": "test-app", "scope": "tunnels:read"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorize_resp.status(), StatusCode::OK);

    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // Step 3: User looks up the device info on verification page
    let server3 = create_test_server(db.clone());
    let app3 = server3.build_router();
    let info_resp = app3
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/device/info?user_code={}",
                    auth_data.user_code
                ))
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(info_resp.status(), StatusCode::OK);
    let info: DeviceAuthorizationInfo = serde_json::from_slice(
        &axum::body::to_bytes(info_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(info.valid);
    assert_eq!(info.client_id, Some("test-app".to_string()));
    assert_eq!(info.client_name, Some("Test Application".to_string()));
    assert_eq!(info.scope, Some("tunnels:read".to_string()));

    // Step 4: User approves
    let server4 = create_test_server(db.clone());
    let app4 = server4.build_router();
    let verify_resp = app4
        .oneshot(
            Request::builder()
                .uri("/api/device/verify")
                .method("POST")
                .header("content-type", "application/json")
                .header("Authorization", format!("Bearer {}", session_token))
                .body(Body::from(
                    json!({"user_code": auth_data.user_code}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(verify_resp.status(), StatusCode::OK);
    let verify_data: DeviceVerifyResponse = serde_json::from_slice(
        &axum::body::to_bytes(verify_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(verify_data.approved);

    // Step 5: Third-party app polls — should get the token
    let server5 = create_test_server(db.clone());
    let app5 = server5.build_router();
    let token_resp = app5
        .oneshot(
            Request::builder()
                .uri("/api/device/token")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
                        "device_code": auth_data.device_code,
                        "client_id": "test-app"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(token_resp.status(), StatusCode::OK);

    let token_data: DeviceTokenResponse = serde_json::from_slice(
        &axum::body::to_bytes(token_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(!token_data.access_token.is_empty());
    assert!(token_data.access_token.starts_with("eyJ"));
    assert_eq!(token_data.token_type, "Bearer");
    assert_eq!(token_data.expires_in, 86400);

    // Step 6: Verify the JWT contains the approving user's ID
    let validator = localup_auth::JwtValidator::new(JWT_SECRET.as_bytes());
    let claims = validator.validate(&token_data.access_token).unwrap();
    assert!(
        claims.user_id.is_some(),
        "JWT should contain the approving user's ID"
    );

    // Step 7: Verify code is no longer valid for info lookup (status changed)
    let server6 = create_test_server(db);
    let app6 = server6.build_router();
    let info_resp2 = app6
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/device/info?user_code={}",
                    auth_data.user_code
                ))
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let info2: DeviceAuthorizationInfo = serde_json::from_slice(
        &axum::body::to_bytes(info_resp2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(
        !info2.valid,
        "Code should no longer be valid after approval"
    );
}

// ============================================================================
// Client name resolution
// ============================================================================

#[tokio::test]
async fn test_device_info_client_name_from_registered_clients() {
    let db = create_test_db().await;
    let server = create_test_server_with_oauth(
        db,
        vec![
            OAuthClientConfig {
                client_id: "my-cli".to_string(),
                display_name: "My CLI Tool".to_string(),
            },
            OAuthClientConfig {
                client_id: "my-ide".to_string(),
                display_name: "My IDE Plugin".to_string(),
            },
        ],
    );

    // Create authorization for "my-cli"
    let app = server.build_router();
    let authorize_resp = app
        .oneshot(
            Request::builder()
                .uri("/api/device/authorize")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(json!({"client_id": "my-cli"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorize_resp.status(), StatusCode::OK);

    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // Look up info — should include client_name
    let app2 = server.build_router();
    let info_resp = app2
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/device/info?user_code={}",
                    auth_data.user_code
                ))
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let info: DeviceAuthorizationInfo = serde_json::from_slice(
        &axum::body::to_bytes(info_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    assert!(info.valid);
    assert_eq!(info.client_id, Some("my-cli".to_string()));
    assert_eq!(info.client_name, Some("My CLI Tool".to_string()));
}

// ============================================================================
// Slow-down enforcement (RFC 8628 Section 3.5)
// ============================================================================

#[tokio::test]
async fn test_device_token_slow_down() {
    let db = create_test_db().await;
    let server = create_test_server(db);

    // Create device authorization
    let app = server.build_router();
    let authorize_resp = app
        .oneshot(
            Request::builder()
                .uri("/api/device/authorize")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(json!({"client_id": "test-app"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let auth_data: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(authorize_resp.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    let token_body = json!({
        "grant_type": "urn:ietf:params:oauth:grant-type:device_code",
        "device_code": auth_data.device_code,
        "client_id": "test-app"
    });

    // First poll — should get authorization_pending (sets last_polled_at)
    let app2 = server.build_router();
    let resp1 = app2
        .oneshot(
            Request::builder()
                .uri("/api/device/token")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(token_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::BAD_REQUEST);
    let data1 = body_json(resp1).await;
    assert_eq!(data1["error"], "authorization_pending");

    // Immediate second poll — should get slow_down (faster than 5s interval)
    let app3 = server.build_router();
    let resp2 = app3
        .oneshot(
            Request::builder()
                .uri("/api/device/token")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(token_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::BAD_REQUEST);
    let data2 = body_json(resp2).await;
    assert_eq!(data2["error"], "slow_down");
    assert!(
        data2["error_description"]
            .as_str()
            .unwrap()
            .contains("wait at least"),
        "slow_down should tell client to wait"
    );
}

// ============================================================================
// Multiple concurrent device authorizations
// ============================================================================

#[tokio::test]
async fn test_multiple_concurrent_device_authorizations() {
    let db = create_test_db().await;
    let server = create_test_server(db);

    // Create two separate device authorizations
    let app1 = server.build_router();
    let resp1 = app1
        .oneshot(
            Request::builder()
                .uri("/api/device/authorize")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(json!({"client_id": "test-app"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let auth1: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(resp1.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    let app2 = server.build_router();
    let resp2 = app2
        .oneshot(
            Request::builder()
                .uri("/api/device/authorize")
                .method("POST")
                .header("content-type", "application/json")
                .body(Body::from(json!({"client_id": "test-app"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let auth2: DeviceAuthorizationResponse = serde_json::from_slice(
        &axum::body::to_bytes(resp2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();

    // They should have different codes
    assert_ne!(auth1.device_code, auth2.device_code);
    assert_ne!(auth1.user_code, auth2.user_code);

    // Both should be independently queryable
    let app3 = server.build_router();
    let info1 = app3
        .oneshot(
            Request::builder()
                .uri(format!("/api/device/info?user_code={}", auth1.user_code))
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let info1_data: DeviceAuthorizationInfo = serde_json::from_slice(
        &axum::body::to_bytes(info1.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(info1_data.valid);

    let app4 = server.build_router();
    let info2 = app4
        .oneshot(
            Request::builder()
                .uri(format!("/api/device/info?user_code={}", auth2.user_code))
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let info2_data: DeviceAuthorizationInfo = serde_json::from_slice(
        &axum::body::to_bytes(info2.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(info2_data.valid);
}
