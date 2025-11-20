pub mod handlers;
pub mod middleware;
pub mod models;

use axum::{
    body::Body,
    http::{header, HeaderValue, Method, Response, StatusCode},
    middleware as axum_middleware,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use rust_embed::RustEmbed;
use std::{net::SocketAddr, sync::Arc};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use localup_control::TunnelConnectionManager;
use sea_orm::DatabaseConnection;

#[derive(RustEmbed)]
#[folder = "../../webapps/exit-node-portal/dist"]
struct PortalAssets;

/// Application state shared across handlers
pub struct AppState {
    pub localup_manager: Arc<TunnelConnectionManager>,
    pub db: DatabaseConnection,
    pub allow_signup: bool,
    pub jwt_secret: Option<String>,
}

/// OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Tunnel API",
        version = "0.1.0",
        description = "REST API for managing geo-distributed tunnels",
        contact(
            name = "Tunnel Team",
            email = "team@tunnel.io"
        )
    ),
    paths(
        handlers::list_tunnels,
        handlers::get_tunnel,
        handlers::delete_tunnel,
        handlers::get_localup_metrics,
        handlers::health_check,
        handlers::list_requests,
        handlers::get_request,
        handlers::replay_request,
        handlers::list_tcp_connections,
        handlers::upload_custom_domain,
        handlers::list_custom_domains,
        handlers::get_custom_domain,
        handlers::delete_custom_domain,
        handlers::initiate_challenge,
        handlers::complete_challenge,
        handlers::auth_config,
        handlers::register,
        handlers::login,
        handlers::logout,
        handlers::get_current_user,
        handlers::list_user_teams,
        handlers::create_auth_token,
        handlers::list_auth_tokens,
        handlers::get_auth_token,
        handlers::update_auth_token,
        handlers::delete_auth_token,
    ),
    components(
        schemas(
            models::TunnelProtocol,
            models::TunnelEndpoint,
            models::TunnelStatus,
            models::Tunnel,
            models::CreateTunnelRequest,
            models::CreateTunnelResponse,
            models::TunnelList,
            models::CapturedRequest,
            models::CapturedRequestList,
            models::CapturedRequestQuery,
            models::CapturedTcpConnection,
            models::CapturedTcpConnectionList,
            models::CapturedTcpConnectionQuery,
            models::TunnelMetrics,
            models::HealthResponse,
            models::ErrorResponse,
            models::CustomDomainStatus,
            models::CustomDomain,
            models::UploadCustomDomainRequest,
            models::UploadCustomDomainResponse,
            models::CustomDomainList,
            models::InitiateChallengeRequest,
            models::ChallengeInfo,
            models::InitiateChallengeResponse,
            models::CompleteChallengeRequest,
            models::RegisterRequest,
            models::RegisterResponse,
            models::LoginRequest,
            models::LoginResponse,
            models::UserRole,
            models::User,
            models::UserList,
            models::TeamRole,
            models::Team,
            models::TeamMember,
            models::TeamList,
            models::CreateAuthTokenRequest,
            models::CreateAuthTokenResponse,
            models::AuthToken,
            models::AuthTokenList,
            models::UpdateAuthTokenRequest,
            models::AuthConfig,
        )
    ),
    tags(
        (name = "tunnels", description = "Tunnel management endpoints"),
        (name = "traffic", description = "Traffic inspection endpoints"),
        (name = "domains", description = "Custom domain management endpoints"),
        (name = "auth", description = "Authentication and user management endpoints"),
        (name = "auth-tokens", description = "Auth token (API key) management endpoints"),
        (name = "system", description = "System health and info endpoints")
    )
)]
struct ApiDoc;

/// API server configuration
pub struct ApiServerConfig {
    /// Address to bind the API server
    pub bind_addr: SocketAddr,
    /// Enable CORS (for development)
    pub enable_cors: bool,
    /// Allowed CORS origins (if None, allows all)
    pub cors_origins: Option<Vec<String>>,
    /// JWT secret for signing auth tokens
    pub jwt_secret: Option<String>,
}

impl Default for ApiServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8080".parse().unwrap(),
            enable_cors: true,
            cors_origins: None,
            jwt_secret: None,
        }
    }
}

/// API Server
pub struct ApiServer {
    config: ApiServerConfig,
    state: Arc<AppState>,
}

impl ApiServer {
    /// Create a new API server
    pub fn new(
        config: ApiServerConfig,
        localup_manager: Arc<TunnelConnectionManager>,
        db: DatabaseConnection,
        allow_signup: bool,
    ) -> Self {
        let state = Arc::new(AppState {
            localup_manager,
            db,
            allow_signup,
            jwt_secret: config.jwt_secret.clone(),
        });

        Self { config, state }
    }

    /// Build the router with all routes
    pub fn build_router(&self) -> Router {
        // Get the OpenAPI spec
        let api_doc = ApiDoc::openapi();

        // Create JWT state for authentication middleware
        // TODO: Make JWT secret configurable via environment variable or config
        let jwt_secret = b"temporary-secret-change-me-in-production";
        let jwt_state = Arc::new(middleware::JwtState::new(jwt_secret));

        // Build PUBLIC routes (no authentication required)
        let public_router = Router::new()
            .route("/api/health", get(handlers::health_check))
            .route("/api/auth/config", get(handlers::auth_config))
            .route("/api/auth/register", post(handlers::register))
            .route("/api/auth/login", post(handlers::login))
            .route("/api/auth/logout", post(handlers::logout))
            .with_state(self.state.clone());

        // Build PROTECTED routes (require session token authentication)
        let protected_router = Router::new()
            // Auth endpoints (require session token authentication)
            .route("/api/auth/me", get(handlers::get_current_user))
            .route("/api/teams", get(handlers::list_user_teams))
            .route("/api/tunnels", get(handlers::list_tunnels))
            .route(
                "/api/tunnels/{id}",
                get(handlers::get_tunnel).delete(handlers::delete_tunnel),
            )
            .route(
                "/api/tunnels/{id}/metrics",
                get(handlers::get_localup_metrics),
            )
            .route("/api/requests", get(handlers::list_requests))
            .route("/api/requests/{id}", get(handlers::get_request))
            .route("/api/requests/{id}/replay", post(handlers::replay_request))
            .route("/api/tcp-connections", get(handlers::list_tcp_connections))
            .route(
                "/api/domains",
                get(handlers::list_custom_domains).post(handlers::upload_custom_domain),
            )
            .route(
                "/api/domains/{domain}",
                get(handlers::get_custom_domain).delete(handlers::delete_custom_domain),
            )
            .route(
                "/api/domains/challenge/initiate",
                post(handlers::initiate_challenge),
            )
            .route(
                "/api/domains/challenge/complete",
                post(handlers::complete_challenge),
            )
            // Auth token management routes (require session token authentication)
            .route(
                "/api/auth-tokens",
                get(handlers::list_auth_tokens).post(handlers::create_auth_token),
            )
            .route(
                "/api/auth-tokens/{id}",
                get(handlers::get_auth_token)
                    .patch(handlers::update_auth_token)
                    .delete(handlers::delete_auth_token),
            )
            .with_state(self.state.clone())
            .layer(axum_middleware::from_fn_with_state(
                jwt_state.clone(),
                middleware::require_auth,
            ));

        // Merge public and protected routers
        let api_router = public_router.merge(protected_router);

        // Merge with Swagger UI
        // SwaggerUi automatically creates a route for /api/openapi.json
        let router = Router::new()
            .merge(SwaggerUi::new("/swagger-ui").url("/api/openapi.json", api_doc))
            .merge(api_router)
            .fallback(serve_portal);

        // Configure CORS
        let cors = if self.config.enable_cors {
            use tower_http::cors::AllowOrigin;

            // For cookie-based auth, we MUST allow credentials
            // When allow_credentials is true, we CANNOT use allow_origin(Any)
            // We must specify exact origins
            let cors_layer = CorsLayer::new()
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::DELETE,
                    Method::PATCH,
                ])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::COOKIE])
                .allow_credentials(true) // Required for cookies
                .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
                    // Allow common development origins
                    let origin_str = origin.to_str().unwrap_or("");
                    origin_str.starts_with("http://localhost:")
                        || origin_str.starts_with("http://127.0.0.1:")
                        || origin_str.starts_with("https://localhost:")
                        || origin_str.starts_with("https://127.0.0.1:")
                }));

            Some(cors_layer)
        } else {
            None
        };

        // Build middleware stack
        let mut router = router.layer(TraceLayer::new_for_http());

        if let Some(cors) = cors {
            router = router.layer(cors);
        }

        router
    }

    /// Start the API server
    pub async fn start(self) -> Result<(), anyhow::Error> {
        let router = self.build_router();

        info!("Starting API server on {}", self.config.bind_addr);
        info!(
            "OpenAPI spec: http://{}/api/openapi.json",
            self.config.bind_addr
        );
        info!("Swagger UI: http://{}/swagger-ui", self.config.bind_addr);

        let listener = tokio::net::TcpListener::bind(self.config.bind_addr).await?;

        axum::serve(listener, router)
            .await
            .map_err(|e| anyhow::anyhow!("Server error: {}", e))?;

        Ok(())
    }
}

/// Convenience function to create and start an API server
pub async fn run_api_server(
    bind_addr: SocketAddr,
    localup_manager: Arc<TunnelConnectionManager>,
    db: DatabaseConnection,
    allow_signup: bool,
) -> Result<(), anyhow::Error> {
    let config = ApiServerConfig {
        bind_addr,
        enable_cors: true,
        cors_origins: Some(vec!["http://localhost:3000".to_string()]),
        jwt_secret: None,
    };

    let server = ApiServer::new(config, localup_manager, db, allow_signup);
    server.start().await
}

/// Serve static files from embedded portal assets
async fn serve_portal(req: axum::extract::Request) -> impl IntoResponse {
    let path = req.uri().path();
    let path = path.trim_start_matches('/');

    // Try to serve the requested file
    if let Some(content) = PortalAssets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        let mut response = Response::new(Body::from(content.data.to_vec()));
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(mime.as_ref()).unwrap(),
        );
        return response;
    }

    // If not found and not an API route, serve index.html (SPA fallback)
    if !path.starts_with("api") && !path.starts_with("swagger-ui") {
        if let Some(content) = PortalAssets::get("index.html") {
            let mut response = Response::new(Body::from(content.data.to_vec()));
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
            return response;
        }
    }

    // 404 Not Found
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("Not Found"))
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openapi_generation() {
        // Ensure OpenAPI spec can be generated without panics
        let _api_doc = ApiDoc::openapi();
    }
}
