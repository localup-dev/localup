//! JWT Authentication Middleware
//!
//! Provides authentication middleware for protected API endpoints.
//! Extracts JWT from Authorization header, validates it, and makes user context
//! available to handlers via Axum's Extension.

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
    Json,
};
use localup_auth::JwtValidator;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::models::ErrorResponse;

/// Authenticated user context extracted from JWT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    /// User ID (UUID string)
    pub user_id: String,
    /// User role (admin, user)
    pub role: String,
    /// Token type (session, auth)
    pub token_type: String,
    /// Team ID if this is a team token
    pub team_id: Option<String>,
    /// Team role if this is a team token
    pub team_role: Option<String>,
}

/// JWT validation state shared across middleware instances
#[derive(Clone)]
pub struct JwtState {
    pub validator: Arc<JwtValidator>,
}

impl JwtState {
    /// Create new JWT state with the given secret
    pub fn new(secret: &[u8]) -> Self {
        Self {
            validator: Arc::new(JwtValidator::new(secret)),
        }
    }
}

/// Authentication middleware that validates JWT session tokens
///
/// Extracts JWT from HTTP-only cookie or "Authorization: Bearer <token>" header,
/// validates signature and expiration, and injects AuthUser into request extensions.
///
/// # Requirements
/// - Token must be present in cookie or Authorization header
/// - Token must be valid (signature + expiration)
/// - Token type must be "session" (not "auth" tokens)
///
/// # Errors
/// Returns 401 Unauthorized if:
/// - Both cookie and Authorization header are missing
/// - Token is malformed or invalid
/// - Token is expired
/// - Token type is not "session"
pub async fn require_auth(
    state: axum::extract::State<Arc<JwtState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    // Try to extract token from cookie first (preferred for web apps)
    let token = if let Some(cookie_header) = request.headers().get(header::COOKIE) {
        cookie_header.to_str().ok().and_then(|cookies| {
            // Parse cookies and find session_token
            cookies
                .split(';')
                .map(|c| c.trim())
                .find(|c| c.starts_with("session_token="))
                .and_then(|c| c.strip_prefix("session_token="))
        })
    } else {
        None
    };

    // If not in cookie, fall back to Authorization header (for API clients)
    let token = match token {
        Some(t) => t.to_string(),
        None => {
            let auth_header = request
                .headers()
                .get(header::AUTHORIZATION)
                .and_then(|h| h.to_str().ok())
                .ok_or_else(|| {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(ErrorResponse {
                            error: "Missing authentication token (cookie or Authorization header)"
                                .to_string(),
                            code: Some("MISSING_AUTH".to_string()),
                        }),
                    )
                })?;

            // Extract Bearer token
            auth_header
                .strip_prefix("Bearer ")
                .ok_or_else(|| {
                    (
                        StatusCode::UNAUTHORIZED,
                        Json(ErrorResponse {
                            error: "Invalid Authorization header format. Expected 'Bearer <token>'"
                                .to_string(),
                            code: Some("INVALID_AUTH_FORMAT".to_string()),
                        }),
                    )
                })?
                .to_string()
        }
    };

    // Validate JWT and extract claims
    let claims = state.validator.validate(&token).map_err(|e| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: format!("Invalid or expired token: {}", e),
                code: Some("INVALID_TOKEN".to_string()),
            }),
        )
    })?;

    // Verify token type is "session" (not "auth" tokens)
    match &claims.token_type {
        Some(token_type) if token_type == "session" => {
            // Valid session token, continue
        }
        Some(token_type) => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: format!(
                        "Invalid token type '{}'. Expected 'session' token for API access",
                        token_type
                    ),
                    code: Some("INVALID_TOKEN_TYPE".to_string()),
                }),
            ));
        }
        None => {
            // Token doesn't have token_type claim (legacy token)
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Token missing 'token_type' claim".to_string(),
                    code: Some("MISSING_TOKEN_TYPE".to_string()),
                }),
            ));
        }
    }

    // Extract user_id from claims
    let user_id = claims.user_id.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Token missing 'user_id' claim".to_string(),
                code: Some("MISSING_USER_ID".to_string()),
            }),
        )
    })?;

    // Extract user_role (default to "user" if not present)
    let role = claims.user_role.unwrap_or_else(|| "user".to_string());

    // Create AuthUser context
    let auth_user = AuthUser {
        user_id,
        role,
        token_type: claims.token_type.unwrap(), // We already validated it exists
        team_id: claims.team_id,
        team_role: claims.team_role,
    };

    // Insert AuthUser into request extensions
    request.extensions_mut().insert(auth_user);

    // Continue to next middleware/handler
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request, middleware, routing::get, Router};
    use chrono::Duration;
    use localup_auth::JwtClaims;
    use tower::ServiceExt; // For oneshot()

    // Test handler that returns the authenticated user
    async fn protected_handler(axum::Extension(user): axum::Extension<AuthUser>) -> Json<AuthUser> {
        Json(user)
    }

    fn create_test_app(jwt_secret: &[u8]) -> Router {
        let jwt_state = Arc::new(JwtState::new(jwt_secret));

        Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn_with_state(
                jwt_state.clone(),
                require_auth,
            ))
            .with_state(jwt_state)
    }

    #[tokio::test]
    async fn test_auth_middleware_valid_session_token() {
        let jwt_secret = b"test-secret-key";
        let app = create_test_app(jwt_secret);

        // Create valid session token
        let claims = JwtClaims::new(
            "test-user-123".to_string(),
            "localup-relay".to_string(),
            "localup-web-ui".to_string(),
            Duration::hours(1),
        )
        .with_user_id("user-uuid-123".to_string())
        .with_user_role("admin".to_string())
        .with_token_type("session".to_string());

        let token = JwtValidator::encode(jwt_secret, &claims).unwrap();

        // Make request with valid token
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let auth_user: AuthUser = serde_json::from_slice(&body).unwrap();

        assert_eq!(auth_user.user_id, "user-uuid-123");
        assert_eq!(auth_user.role, "admin");
        assert_eq!(auth_user.token_type, "session");
    }

    #[tokio::test]
    async fn test_auth_middleware_missing_authorization_header() {
        let jwt_secret = b"test-secret-key";
        let app = create_test_app(jwt_secret);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert!(error
            .error
            .contains("Missing authentication token (cookie or Authorization header)"));
    }

    #[tokio::test]
    async fn test_auth_middleware_invalid_bearer_format() {
        let jwt_secret = b"test-secret-key";
        let app = create_test_app(jwt_secret);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("Authorization", "InvalidFormat token123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert!(error.error.contains("Invalid Authorization header format"));
    }

    #[tokio::test]
    async fn test_auth_middleware_expired_token() {
        let jwt_secret = b"test-secret-key";
        let app = create_test_app(jwt_secret);

        // Create expired token (negative duration)
        let claims = JwtClaims::new(
            "test-user-123".to_string(),
            "localup-relay".to_string(),
            "localup-web-ui".to_string(),
            Duration::seconds(-10), // Already expired
        )
        .with_user_id("user-uuid-123".to_string())
        .with_token_type("session".to_string());

        let token = JwtValidator::encode(jwt_secret, &claims).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert!(error.error.contains("Invalid or expired token"));
    }

    #[tokio::test]
    async fn test_auth_middleware_wrong_secret() {
        let jwt_secret = b"test-secret-key";
        let wrong_secret = b"wrong-secret-key";
        let app = create_test_app(jwt_secret);

        // Create token with wrong secret
        let claims = JwtClaims::new(
            "test-user-123".to_string(),
            "localup-relay".to_string(),
            "localup-web-ui".to_string(),
            Duration::hours(1),
        )
        .with_user_id("user-uuid-123".to_string())
        .with_token_type("session".to_string());

        let token = JwtValidator::encode(wrong_secret, &claims).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_auth_middleware_rejects_auth_token() {
        let jwt_secret = b"test-secret-key";
        let app = create_test_app(jwt_secret);

        // Create auth token (not session)
        let claims = JwtClaims::new(
            "test-token-id".to_string(),
            "localup-relay".to_string(),
            "localup-tunnel".to_string(),
            Duration::hours(1),
        )
        .with_user_id("user-uuid-123".to_string())
        .with_token_type("auth".to_string()); // Wrong type

        let token = JwtValidator::encode(jwt_secret, &claims).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert!(error.error.contains("Invalid token type"));
        assert!(error.error.contains("Expected 'session' token"));
    }

    #[tokio::test]
    async fn test_auth_middleware_missing_user_id() {
        let jwt_secret = b"test-secret-key";
        let app = create_test_app(jwt_secret);

        // Create token without user_id
        let claims = JwtClaims::new(
            "test-user-123".to_string(),
            "localup-relay".to_string(),
            "localup-web-ui".to_string(),
            Duration::hours(1),
        )
        .with_token_type("session".to_string());
        // Note: No .with_user_id()

        let token = JwtValidator::encode(jwt_secret, &claims).unwrap();

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let error: ErrorResponse = serde_json::from_slice(&body).unwrap();

        assert!(error.error.contains("missing 'user_id' claim"));
    }
}
