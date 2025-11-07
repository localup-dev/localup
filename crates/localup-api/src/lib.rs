pub mod handlers;
pub mod models;

use axum::{
    body::Body,
    http::{header, HeaderValue, Method, Response, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use rust_embed::RustEmbed;
use std::{net::SocketAddr, sync::Arc};
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};
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
        )
    ),
    tags(
        (name = "tunnels", description = "Tunnel management endpoints"),
        (name = "traffic", description = "Traffic inspection endpoints"),
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
}

impl Default for ApiServerConfig {
    fn default() -> Self {
        Self {
            bind_addr: "127.0.0.1:8080".parse().unwrap(),
            enable_cors: true,
            cors_origins: None,
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
    ) -> Self {
        let state = Arc::new(AppState {
            localup_manager,
            db,
        });

        Self { config, state }
    }

    /// Build the router with all routes
    fn build_router(&self) -> Router {
        // Get the OpenAPI spec
        let api_doc = ApiDoc::openapi();

        // Build API routes
        // NOTE: Axum 0.8+ uses {param} syntax, not :param
        // NOTE: SwaggerUi automatically serves /api/openapi.json, don't add it manually
        let api_router = Router::new()
            .route("/api/tunnels", get(handlers::list_tunnels))
            .route(
                "/api/tunnels/{id}",
                get(handlers::get_tunnel).delete(handlers::delete_tunnel),
            )
            .route(
                "/api/tunnels/{id}/metrics",
                get(handlers::get_localup_metrics),
            )
            .route("/api/health", get(handlers::health_check))
            .route("/api/requests", get(handlers::list_requests))
            .route("/api/requests/{id}", get(handlers::get_request))
            .route("/api/requests/{id}/replay", post(handlers::replay_request))
            .route("/api/tcp-connections", get(handlers::list_tcp_connections))
            .with_state(self.state.clone());

        // Merge with Swagger UI
        // SwaggerUi automatically creates a route for /api/openapi.json
        let router = Router::new()
            .merge(SwaggerUi::new("/swagger-ui").url("/api/openapi.json", api_doc))
            .merge(api_router)
            .fallback(serve_portal);

        // Configure CORS
        let cors = if self.config.enable_cors {
            let cors_layer = CorsLayer::new()
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
                .allow_origin(Any); // TODO: Use config.cors_origins if specified

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
) -> Result<(), anyhow::Error> {
    let config = ApiServerConfig {
        bind_addr,
        enable_cors: true,
        cors_origins: Some(vec!["http://localhost:3000".to_string()]),
    };

    let server = ApiServer::new(config, localup_manager, db);
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
