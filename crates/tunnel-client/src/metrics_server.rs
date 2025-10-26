//! HTTP server for exposing tunnel metrics via web interface using Axum
//!
//! This module provides an HTTP server that exposes metrics data
//! through REST API endpoints and serves a web dashboard.

use crate::metrics::{
    HttpMetric, MetricsEvent, MetricsStats, MetricsStore, TcpConnectionState, TcpMetric,
};
use crate::metrics_service::{MetricsService, MetricsServiceError, ReplayRequest, ReplayResponse};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{sse::Event, IntoResponse, Response, Sse},
    routing::{delete, get, post},
    Json, Router,
};
use futures::stream::Stream;
use problem_details::ProblemDetails;
use rust_embed::RustEmbed;
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;
use tunnel_proto::Endpoint;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

#[derive(RustEmbed)]
#[folder = "../../webapps/dashboard/dist"]
struct DashboardAssets;

/// Convert service errors to Problem Details responses
fn service_error_to_problem(error: MetricsServiceError) -> impl IntoResponse {
    let (status, title, detail) = match &error {
        MetricsServiceError::MetricNotFound(id) => (
            StatusCode::NOT_FOUND,
            "Metric Not Found",
            format!("Metric with ID '{}' was not found", id),
        ),
        MetricsServiceError::ReplayFailed(msg) => (
            StatusCode::BAD_GATEWAY,
            "Replay Failed",
            format!("Failed to replay request: {}", msg),
        ),
        MetricsServiceError::InvalidRequest(msg) => {
            (StatusCode::BAD_REQUEST, "Invalid Request", msg.clone())
        }
    };

    let problem = ProblemDetails::new()
        .with_status(status)
        .with_title(title)
        .with_detail(detail);

    (status, Json(problem))
}

/// Metrics HTTP server
pub struct MetricsServer {
    addr: SocketAddr,
    pub(crate) metrics: MetricsStore,
    endpoints: Vec<Endpoint>,
    local_upstream: String,
}

/// OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
    paths(
        handle_api_metrics,
        handle_api_stats,
        handle_api_metric_by_id,
        handle_api_clear,
        handle_api_replay,
        handle_api_tcp_connections,
        handle_api_tcp_connection_by_id,
    ),
    components(
        schemas(
            ReplayRequest,
            ReplayResponse,
            HttpMetric,
            TcpMetric,
            TcpConnectionState,
            crate::metrics::BodyData,
            crate::metrics::BodyContent,
            MetricsStats,
            crate::metrics::DurationPercentiles,
        )
    ),
    tags(
        (name = "tunnel-cli", description = "Tunnel CLI Metrics API endpoints")
    ),
    info(
        title = "Tunnel CLI Metrics API",
        version = "1.0.0",
        description = "API for tunnel client metrics collection, inspection, and replay"
    )
)]
struct ApiDoc;

/// Shared application state
#[derive(Clone)]
struct AppState {
    service: MetricsService,
    metrics: MetricsStore, // Keep for SSE stream
}

impl MetricsServer {
    /// Create a new metrics server
    pub fn new(
        addr: SocketAddr,
        metrics: MetricsStore,
        endpoints: Vec<Endpoint>,
        local_upstream: String,
    ) -> Self {
        Self {
            addr,
            metrics,
            endpoints,
            local_upstream,
        }
    }

    /// Start the metrics server
    pub async fn run(self) -> Result<(), std::io::Error> {
        info!("📊 Metrics server listening on http://{}", self.addr);
        info!("   Dashboard: http://{}/", self.addr);
        info!("   API:       http://{}/api/metrics", self.addr);
        info!("   Swagger:   http://{}/swagger-ui/", self.addr);

        let service =
            MetricsService::new(self.metrics.clone(), self.endpoints, self.local_upstream);

        let state = AppState {
            service,
            metrics: self.metrics,
        };

        // Configure CORS
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        // Build router with Swagger UI
        let app = Router::new()
            .route("/", get(handle_dashboard))
            .route("/assets/{*path}", get(handle_assets))
            .route("/vite.svg", get(handle_assets_root))
            .route("/api/info", get(handle_api_info))
            .route("/api/metrics", get(handle_api_metrics))
            .route("/api/metrics", delete(handle_api_clear))
            .route("/api/metrics/stats", get(handle_api_stats))
            .route("/api/metrics/stream", get(handle_sse_stream))
            .route("/api/metrics/{id}", get(handle_api_metric_by_id))
            .route("/api/replay", post(handle_api_replay))
            // TCP connection endpoints
            .route("/api/tcp/connections", get(handle_api_tcp_connections))
            .route(
                "/api/tcp/connections/{id}",
                get(handle_api_tcp_connection_by_id),
            )
            .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
            .fallback(get(handle_spa_fallback)) // SPA fallback for client-side routing
            .layer(cors)
            .with_state(state);

        // Start server
        let listener = tokio::net::TcpListener::bind(&self.addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }
}

/// Serve the dashboard HTML (index.html from embedded assets)
async fn handle_dashboard() -> impl IntoResponse {
    serve_embedded_file("index.html")
}

/// Serve assets from the /assets/ directory
async fn handle_assets(Path(path): Path<String>) -> impl IntoResponse {
    serve_embedded_file(&format!("assets/{}", path))
}

/// Serve root-level assets (like vite.svg)
async fn handle_assets_root(Path(filename): Path<String>) -> impl IntoResponse {
    serve_embedded_file(&filename)
}

/// SPA fallback - serve index.html for all unmatched routes
async fn handle_spa_fallback() -> impl IntoResponse {
    serve_embedded_file("index.html")
}

/// Helper function to serve embedded files
fn serve_embedded_file(path: &str) -> Response {
    match DashboardAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .body(Body::from(content.data))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("404 Not Found"))
            .unwrap(),
    }
}

/// Return tunnel endpoint information
async fn handle_api_info(State(state): State<AppState>) -> Json<Vec<Endpoint>> {
    Json(state.service.get_info())
}

/// Replay a request directly to the local upstream server
#[utoipa::path(
    post,
    path = "/api/replay",
    tag = "tunnel-cli",
    summary = "Replay an HTTP request",
    description = "Replays a captured HTTP request directly to the local upstream server",
    request_body = ReplayRequest,
    responses(
        (status = 200, description = "Request replayed successfully", body = ReplayResponse),
        (status = 400, description = "Invalid request parameters"),
        (status = 502, description = "Replay request failed")
    )
)]
async fn handle_api_replay(
    State(state): State<AppState>,
    Json(replay_req): Json<ReplayRequest>,
) -> Result<Json<ReplayResponse>, impl IntoResponse> {
    match state.service.replay_request(replay_req).await {
        Ok(response) => Ok(Json(response)),
        Err(e) => Err(service_error_to_problem(e)),
    }
}

/// Get metrics with optional offset/limit
#[utoipa::path(
    get,
    path = "/api/metrics",
    tag = "tunnel-cli",
    summary = "List HTTP metrics",
    description = "Returns a paginated list of captured HTTP request/response metrics",
    params(
        ("offset" = Option<usize>, Query, description = "Offset for pagination (default: 0)"),
        ("limit" = Option<usize>, Query, description = "Limit for pagination (default: 100)")
    ),
    responses(
        (status = 200, description = "List of HTTP metrics", body = Vec<HttpMetric>)
    )
)]
async fn handle_api_metrics(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<crate::metrics::HttpMetric>> {
    let offset = params
        .get("offset")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(0);
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(100);

    let metrics = state.service.get_metrics(offset, limit).await;
    Json(metrics)
}

/// Get aggregated statistics
#[utoipa::path(
    get,
    path = "/api/metrics/stats",
    tag = "tunnel-cli",
    summary = "Get metrics statistics",
    description = "Returns aggregated statistics including request counts, success/failure rates, duration percentiles, and status code distribution",
    responses(
        (status = 200, description = "Aggregated metrics statistics", body = MetricsStats)
    )
)]
async fn handle_api_stats(State(state): State<AppState>) -> Json<crate::metrics::MetricsStats> {
    let stats = state.service.get_stats().await;
    Json(stats)
}

/// Get a specific metric by ID
#[utoipa::path(
    get,
    path = "/api/metrics/{id}",
    tag = "tunnel-cli",
    summary = "Get metric by ID",
    description = "Returns a specific HTTP metric by its unique identifier",
    params(
        ("id" = String, Path, description = "Unique metric identifier")
    ),
    responses(
        (status = 200, description = "HTTP metric found", body = HttpMetric),
        (status = 404, description = "Metric not found")
    )
)]
async fn handle_api_metric_by_id(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<crate::metrics::HttpMetric>, impl IntoResponse> {
    match state.service.get_metric_by_id(&id).await {
        Ok(metric) => Ok(Json(metric)),
        Err(e) => Err(service_error_to_problem(e)),
    }
}

/// Clear all metrics
#[utoipa::path(
    delete,
    path = "/api/metrics",
    tag = "tunnel-cli",
    summary = "Clear all metrics",
    description = "Deletes all captured HTTP metrics and resets statistics",
    responses(
        (status = 204, description = "Metrics cleared successfully")
    )
)]
async fn handle_api_clear(State(state): State<AppState>) -> StatusCode {
    state.service.clear_metrics().await;
    StatusCode::NO_CONTENT
}

/// Server-Sent Events stream for real-time updates
async fn handle_sse_stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    info!("SSE client connected");

    // Subscribe to metrics updates
    let rx = state.metrics.subscribe();
    let broadcast_stream = BroadcastStream::new(rx);

    // Send initial stats
    let initial_stats = state.metrics.get_stats().await;
    let initial_event = MetricsEvent::Stats {
        stats: initial_stats,
    };

    // Convert broadcast stream to SSE events
    let stream = futures::stream::once(async move {
        let json = serde_json::to_string(&initial_event).unwrap_or_default();
        Ok(Event::default().data(json))
    })
    .chain(broadcast_stream.filter_map(|result| {
        let event = match result {
            Ok(event) => event,
            Err(_) => return None,
        };

        let json = match serde_json::to_string(&event) {
            Ok(json) => json,
            Err(_) => return None,
        };

        Some(Ok(Event::default().data(json)))
    }));

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
            .text("keep-alive"),
    )
}

// ========== TCP Connection Handlers ==========

/// Get TCP connections with optional offset/limit
#[utoipa::path(
    get,
    path = "/api/tcp/connections",
    tag = "tunnel-cli",
    summary = "Get TCP connections",
    description = "Retrieves all TCP connections with optional pagination",
    params(
        ("offset" = Option<usize>, Query, description = "Offset for pagination"),
        ("limit" = Option<usize>, Query, description = "Limit number of results")
    ),
    responses(
        (status = 200, description = "List of TCP connections", body = Vec<TcpMetric>)
    )
)]
async fn handle_api_tcp_connections(
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<TcpMetric>> {
    let offset = params
        .get("offset")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let limit = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);

    if offset == 0 && limit >= 100 {
        // No pagination - return all
        Json(state.metrics.get_all_tcp_connections().await)
    } else {
        // With pagination
        Json(
            state
                .metrics
                .get_tcp_connections_paginated(offset, limit)
                .await,
        )
    }
}

/// Get a specific TCP connection by ID
#[utoipa::path(
    get,
    path = "/api/tcp/connections/{id}",
    tag = "tunnel-cli",
    summary = "Get TCP connection by ID",
    description = "Retrieves a single TCP connection by its ID",
    params(
        ("id" = String, Path, description = "Connection ID")
    ),
    responses(
        (status = 200, description = "TCP connection details", body = TcpMetric),
        (status = 404, description = "Connection not found")
    )
)]
async fn handle_api_tcp_connection_by_id(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<TcpMetric>, StatusCode> {
    match state.metrics.get_tcp_connection_by_id(&id).await {
        Some(conn) => Ok(Json(conn)),
        None => Err(StatusCode::NOT_FOUND),
    }
}
