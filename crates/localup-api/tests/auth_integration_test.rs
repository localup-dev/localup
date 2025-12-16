//! Integration tests for authentication endpoints

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use localup_api::{models::*, ApiServer, ApiServerConfig};
use localup_control::TunnelConnectionManager;
use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use serde_json::json;
use std::sync::Arc;
use tower::ServiceExt; // For `oneshot` method

/// Helper to create an in-memory database with migrations applied
async fn create_test_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("Failed to create in-memory database");

    // Run migrations
    localup_relay_db::migrator::Migrator::up(&db, None)
        .await
        .expect("Failed to run migrations");

    db
}

/// Helper to create a test API server
fn create_test_server(db: DatabaseConnection) -> ApiServer {
    let localup_manager = Arc::new(TunnelConnectionManager::new());
    let config = ApiServerConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(), // Random port
        enable_cors: true,
        cors_origins: None,
        jwt_secret: "test-secret".to_string(),
        tls_cert_path: None,
        tls_key_path: None,
    };

    ApiServer::new(config, localup_manager, db, true)
}

#[tokio::test]
async fn test_user_registration_success() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request_body = json!({
        "email": "test@example.com",
        "password": "SecurePassword123!",
        "full_name": "Test User"
    });

    let request = Request::builder()
        .uri("/api/auth/register")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::CREATED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let response_data: RegisterResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(response_data.user.email, "test@example.com");
    assert_eq!(response_data.user.full_name, Some("Test User".to_string()));
    assert_eq!(response_data.user.role, UserRole::User);
    assert!(response_data.user.is_active);
    assert!(!response_data.token.is_empty());
}

#[tokio::test]
async fn test_user_registration_duplicate_email() {
    let db = create_test_db().await;
    let server = create_test_server(db.clone());
    let app = server.build_router();

    let request_body = json!({
        "email": "duplicate@example.com",
        "password": "SecurePassword123!"
    });

    // Register first user
    let request1 = Request::builder()
        .uri("/api/auth/register")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response1 = app.oneshot(request1).await.unwrap();
    let status1 = response1.status();

    // Debug: Print response if not CREATED
    if status1 != StatusCode::CREATED {
        let body = axum::body::to_bytes(response1.into_body(), usize::MAX)
            .await
            .unwrap();
        eprintln!(
            "Unexpected status, body: {}",
            String::from_utf8_lossy(&body)
        );
        panic!("Expected 201, got {}", status1);
    }

    assert_eq!(status1, StatusCode::CREATED);

    // Try to register again with same email - create new router with same database
    let server2 = create_test_server(db);
    let app2 = server2.build_router();

    let request2 = Request::builder()
        .uri("/api/auth/register")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response2 = app2.oneshot(request2).await.unwrap();
    assert_eq!(response2.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response2.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(error.code, Some("EMAIL_EXISTS".to_string()));
}

#[tokio::test]
async fn test_user_registration_weak_password() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request_body = json!({
        "email": "test@example.com",
        "password": "short" // Less than 8 characters
    });

    let request = Request::builder()
        .uri("/api/auth/register")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(error.code, Some("WEAK_PASSWORD".to_string()));
}

#[tokio::test]
async fn test_user_registration_invalid_email() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let request_body = json!({
        "email": "not-an-email", // Missing @
        "password": "SecurePassword123!"
    });

    let request = Request::builder()
        .uri("/api/auth/register")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&request_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(error.code, Some("INVALID_EMAIL".to_string()));
}

#[tokio::test]
async fn test_user_login_success() {
    let db = create_test_db().await;
    let server = create_test_server(db.clone());
    let app = server.build_router();

    // Register user first
    let register_body = json!({
        "email": "login@example.com",
        "password": "SecurePassword123!",
        "full_name": "Login Test"
    });

    let register_request = Request::builder()
        .uri("/api/auth/register")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let register_response = app.oneshot(register_request).await.unwrap();
    assert_eq!(register_response.status(), StatusCode::CREATED);

    // Now login - create new router with same database
    let server2 = create_test_server(db);
    let app2 = server2.build_router();

    let login_body = json!({
        "email": "login@example.com",
        "password": "SecurePassword123!"
    });

    let login_request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&login_body).unwrap()))
        .unwrap();

    let login_response = app2.oneshot(login_request).await.unwrap();

    assert_eq!(login_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(login_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let response_data: LoginResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(response_data.user.email, "login@example.com");
    assert_eq!(response_data.user.full_name, Some("Login Test".to_string()));
    assert!(!response_data.token.is_empty());
}

#[tokio::test]
async fn test_user_login_invalid_email() {
    let db = create_test_db().await;
    let server = create_test_server(db);
    let app = server.build_router();

    let login_body = json!({
        "email": "nonexistent@example.com",
        "password": "SecurePassword123!"
    });

    let request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&login_body).unwrap()))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(error.code, Some("INVALID_CREDENTIALS".to_string()));
}

#[tokio::test]
async fn test_user_login_wrong_password() {
    let db = create_test_db().await;
    let server = create_test_server(db.clone());
    let app = server.build_router();

    // Register user first
    let register_body = json!({
        "email": "wrongpass@example.com",
        "password": "CorrectPassword123!"
    });

    let register_request = Request::builder()
        .uri("/api/auth/register")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let register_response = app.oneshot(register_request).await.unwrap();
    assert_eq!(register_response.status(), StatusCode::CREATED);

    // Try login with wrong password - create new router with same database
    let server2 = create_test_server(db);
    let app2 = server2.build_router();

    let login_body = json!({
        "email": "wrongpass@example.com",
        "password": "WrongPassword123!"
    });

    let login_request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&login_body).unwrap()))
        .unwrap();

    let login_response = app2.oneshot(login_request).await.unwrap();

    assert_eq!(login_response.status(), StatusCode::UNAUTHORIZED);

    let body = axum::body::to_bytes(login_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

    assert_eq!(error.code, Some("INVALID_CREDENTIALS".to_string()));
}

#[tokio::test]
async fn test_user_registration_and_login_full_flow() {
    let db = create_test_db().await;
    let server = create_test_server(db.clone());
    let app = server.build_router();

    // 1. Register user
    let register_body = json!({
        "email": "fullflow@example.com",
        "password": "SecurePassword123!",
        "full_name": "Full Flow Test"
    });

    let register_request = Request::builder()
        .uri("/api/auth/register")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&register_body).unwrap()))
        .unwrap();

    let register_response = app.oneshot(register_request).await.unwrap();
    assert_eq!(register_response.status(), StatusCode::CREATED);

    let register_body_bytes = axum::body::to_bytes(register_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let register_data: RegisterResponse = serde_json::from_slice(&register_body_bytes).unwrap();

    let user_id_from_register = register_data.user.id.clone();
    let token_from_register = register_data.token.clone();

    // 2. Login with same credentials - create new router with same database
    let server2 = create_test_server(db);
    let app2 = server2.build_router();

    let login_body = json!({
        "email": "fullflow@example.com",
        "password": "SecurePassword123!"
    });

    let login_request = Request::builder()
        .uri("/api/auth/login")
        .method("POST")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&login_body).unwrap()))
        .unwrap();

    let login_response = app2.oneshot(login_request).await.unwrap();
    assert_eq!(login_response.status(), StatusCode::OK);

    let login_body_bytes = axum::body::to_bytes(login_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let login_data: LoginResponse = serde_json::from_slice(&login_body_bytes).unwrap();

    // 3. Verify user ID is the same
    assert_eq!(login_data.user.id, user_id_from_register);

    // 4. Verify both tokens are valid JWTs (not empty, starts with "eyJ")
    assert!(token_from_register.starts_with("eyJ"));
    assert!(login_data.token.starts_with("eyJ"));

    // 5. Verify user data consistency
    assert_eq!(login_data.user.email, "fullflow@example.com");
    assert_eq!(
        login_data.user.full_name,
        Some("Full Flow Test".to_string())
    );
    assert_eq!(login_data.user.role, UserRole::User);
    assert!(login_data.user.is_active);
}
