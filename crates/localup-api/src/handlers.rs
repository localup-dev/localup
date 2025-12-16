use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::middleware::AuthUser;
use crate::models::*;
use crate::AppState;

/// Determine if the request came over HTTPS by checking headers
/// Checks X-Forwarded-Proto (common for reverse proxies) and falls back to server config
fn is_request_secure(headers: &HeaderMap, server_is_https: bool) -> bool {
    // Check X-Forwarded-Proto header (set by reverse proxies like nginx, Cloudflare, etc.)
    if let Some(proto) = headers.get("x-forwarded-proto") {
        if let Ok(proto_str) = proto.to_str() {
            return proto_str.eq_ignore_ascii_case("https");
        }
    }

    // Check X-Forwarded-Ssl header (alternative header used by some proxies)
    if let Some(ssl) = headers.get("x-forwarded-ssl") {
        if let Ok(ssl_str) = ssl.to_str() {
            return ssl_str.eq_ignore_ascii_case("on");
        }
    }

    // Fall back to server's TLS configuration
    server_is_https
}

/// Create a session cookie with the appropriate Secure flag based on HTTPS mode
fn create_session_cookie(token: &str, is_https: bool) -> String {
    let secure_flag = if is_https { "; Secure" } else { "" };
    format!(
        "session_token={}; HttpOnly; SameSite=Lax; Path=/; Max-Age={}{}",
        token,
        7 * 24 * 60 * 60, // 7 days in seconds
        secure_flag
    )
}

/// Create a cookie that clears the session (for logout)
fn create_logout_cookie(is_https: bool) -> String {
    let secure_flag = if is_https { "; Secure" } else { "" };
    format!(
        "session_token=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0{}",
        secure_flag
    )
}

/// List all tunnels (active and optionally inactive)
#[utoipa::path(
    get,
    path = "/api/tunnels",
    params(
        ("include_inactive" = Option<bool>, Query, description = "Include inactive/disconnected tunnels from history (default: false)")
    ),
    responses(
        (status = 200, description = "List of tunnels", body = TunnelList),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tunnels"
)]
pub async fn list_tunnels(
    State(state): State<Arc<AppState>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<TunnelList>, (StatusCode, Json<ErrorResponse>)> {
    let include_inactive = query
        .get("include_inactive")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    debug!("Listing tunnels (include_inactive={})", include_inactive);

    // Get active tunnel IDs
    let active_localup_ids = state.localup_manager.list_tunnels().await;
    let mut tunnels = Vec::new();

    // Add active tunnels
    for localup_id in &active_localup_ids {
        if let Some(endpoints) = state.localup_manager.get_endpoints(localup_id).await {
            let tunnel = Tunnel {
                id: localup_id.clone(),
                endpoints: endpoints
                    .iter()
                    .map(|e| TunnelEndpoint {
                        protocol: match &e.protocol {
                            localup_proto::Protocol::Http { subdomain, .. } => {
                                TunnelProtocol::Http {
                                    subdomain: subdomain
                                        .clone()
                                        .unwrap_or_else(|| "unknown".to_string()),
                                }
                            }
                            localup_proto::Protocol::Https { subdomain, .. } => {
                                TunnelProtocol::Https {
                                    subdomain: subdomain
                                        .clone()
                                        .unwrap_or_else(|| "unknown".to_string()),
                                }
                            }
                            localup_proto::Protocol::Tcp { port } => {
                                TunnelProtocol::Tcp { port: *port }
                            }
                            localup_proto::Protocol::Tls {
                                port: _,
                                sni_pattern,
                            } => TunnelProtocol::Tls {
                                domain: sni_pattern.clone(),
                            },
                        },
                        public_url: e.public_url.clone(),
                        port: e.port,
                    })
                    .collect(),
                status: TunnelStatus::Connected,
                region: "us-east-1".to_string(), // TODO: Get from config
                connected_at: chrono::Utc::now(), // TODO: Track actual connection time
                local_addr: None,                // Client-side information
            };
            tunnels.push(tunnel);
        }
    }

    // If include_inactive, query database for historical tunnel IDs
    if include_inactive {
        use localup_relay_db::entities::prelude::*;
        use sea_orm::{EntityTrait, QuerySelect};

        // Get unique tunnel IDs from TCP connections
        let tcp_tunnel_ids: Vec<String> = CapturedTcpConnection::find()
            .select_only()
            .column(localup_relay_db::entities::captured_tcp_connection::Column::LocalupId)
            .distinct()
            .into_tuple::<String>()
            .all(&state.db)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Database error: {}", e),
                        code: None,
                    }),
                )
            })?;

        // Get unique tunnel IDs from HTTP requests
        let http_tunnel_ids: Vec<String> = CapturedRequest::find()
            .select_only()
            .column(localup_relay_db::entities::captured_request::Column::LocalupId)
            .distinct()
            .into_tuple::<String>()
            .all(&state.db)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: format!("Database error: {}", e),
                        code: None,
                    }),
                )
            })?;

        // Combine and deduplicate
        let mut all_tunnel_ids: Vec<String> = tcp_tunnel_ids
            .into_iter()
            .chain(http_tunnel_ids.into_iter())
            .filter(|id| !active_localup_ids.contains(id)) // Exclude already active tunnels
            .collect();
        all_tunnel_ids.sort();
        all_tunnel_ids.dedup();

        // Add inactive tunnels (minimal info since they're not connected)
        for inactive_id in all_tunnel_ids {
            tunnels.push(Tunnel {
                id: inactive_id.clone(),
                endpoints: vec![], // No endpoints for inactive tunnels
                status: TunnelStatus::Disconnected,
                region: "unknown".to_string(),
                connected_at: chrono::Utc::now(), // Use current time as placeholder
                local_addr: None,
            });
        }
    }

    let total = tunnels.len();

    Ok(Json(TunnelList { tunnels, total }))
}

/// Get a specific tunnel by ID
#[utoipa::path(
    get,
    path = "/api/tunnels/{id}",
    params(
        ("id" = String, Path, description = "Tunnel ID")
    ),
    responses(
        (status = 200, description = "Tunnel information", body = Tunnel),
        (status = 404, description = "Tunnel not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tunnels"
)]
pub async fn get_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Tunnel>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Getting tunnel: {}", id);

    // First, check if tunnel is active in memory
    if let Some(endpoints) = state.localup_manager.get_endpoints(&id).await {
        let tunnel = Tunnel {
            id: id.clone(),
            endpoints: endpoints
                .iter()
                .map(|e| TunnelEndpoint {
                    protocol: match &e.protocol {
                        localup_proto::Protocol::Http { subdomain, .. } => TunnelProtocol::Http {
                            subdomain: subdomain.clone().unwrap_or_else(|| "unknown".to_string()),
                        },
                        localup_proto::Protocol::Https { subdomain, .. } => TunnelProtocol::Https {
                            subdomain: subdomain.clone().unwrap_or_else(|| "unknown".to_string()),
                        },
                        localup_proto::Protocol::Tcp { port } => {
                            TunnelProtocol::Tcp { port: *port }
                        }
                        localup_proto::Protocol::Tls {
                            port: _,
                            sni_pattern,
                        } => TunnelProtocol::Tls {
                            domain: sni_pattern.clone(),
                        },
                    },
                    public_url: e.public_url.clone(),
                    port: e.port,
                })
                .collect(),
            status: TunnelStatus::Connected,
            region: "us-east-1".to_string(),
            connected_at: chrono::Utc::now(),
            local_addr: None,
        };

        return Ok(Json(tunnel));
    }

    // If not active, check if it exists in database history (disconnected tunnel)
    use localup_relay_db::entities::prelude::*;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    // Check TCP connections table
    let tcp_exists = CapturedTcpConnection::find()
        .filter(localup_relay_db::entities::captured_tcp_connection::Column::LocalupId.eq(&id))
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Database error checking TCP connections: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                    code: Some("DATABASE_ERROR".to_string()),
                }),
            )
        })?;

    // Check HTTP requests table
    let http_exists = CapturedRequest::find()
        .filter(localup_relay_db::entities::captured_request::Column::LocalupId.eq(&id))
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Database error checking HTTP requests: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                    code: Some("DATABASE_ERROR".to_string()),
                }),
            )
        })?;

    // If found in database history, return as disconnected tunnel with endpoints
    if tcp_exists.is_some() || http_exists.is_some() {
        let mut endpoints = vec![];

        // Add TCP endpoint if found
        if let Some(tcp_conn) = tcp_exists {
            endpoints.push(TunnelEndpoint {
                protocol: TunnelProtocol::Tcp {
                    port: tcp_conn.target_port as u16,
                },
                public_url: format!("tcp://relay:{}", tcp_conn.target_port),
                port: Some(tcp_conn.target_port as u16),
            });
        }

        // Add HTTP/HTTPS endpoint if found
        if let Some(http_req) = http_exists {
            // Extract subdomain from host header (e.g., "myapp.localhost" -> "myapp")
            let subdomain = http_req
                .host
                .as_ref()
                .and_then(|h| h.split('.').next())
                .unwrap_or("unknown")
                .to_string();

            // Assume HTTP for now (we don't store TLS info in captured_requests)
            endpoints.push(TunnelEndpoint {
                protocol: TunnelProtocol::Http {
                    subdomain: subdomain.clone(),
                },
                public_url: http_req
                    .host
                    .clone()
                    .unwrap_or_else(|| format!("{}.localhost", subdomain)),
                port: Some(80), // Default HTTP port
            });
        }

        let tunnel = Tunnel {
            id: id.clone(),
            endpoints,
            status: TunnelStatus::Disconnected,
            region: "unknown".to_string(),
            connected_at: chrono::Utc::now(), // TODO: Get actual connected_at from DB
            local_addr: None,
        };

        return Ok(Json(tunnel));
    }

    // Tunnel not found anywhere (neither active nor in history)
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("Tunnel '{}' not found", id),
            code: Some("TUNNEL_NOT_FOUND".to_string()),
        }),
    ))
}

/// Delete a tunnel
#[utoipa::path(
    delete,
    path = "/api/tunnels/{id}",
    params(
        ("id" = String, Path, description = "Tunnel ID")
    ),
    responses(
        (status = 204, description = "Tunnel deleted successfully"),
        (status = 404, description = "Tunnel not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tunnels"
)]
pub async fn delete_tunnel(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    info!("Deleting tunnel: {}", id);

    // Unregister the tunnel
    state.localup_manager.unregister(&id).await;

    Ok(StatusCode::NO_CONTENT)
}

/// Get tunnel metrics
#[utoipa::path(
    get,
    path = "/api/tunnels/{id}/metrics",
    params(
        ("id" = String, Path, description = "Tunnel ID")
    ),
    responses(
        (status = 200, description = "Tunnel metrics", body = TunnelMetrics),
        (status = 404, description = "Tunnel not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tunnels"
)]
pub async fn get_localup_metrics(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<TunnelMetrics>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Getting metrics for tunnel: {}", id);

    // TODO: Implement actual metrics collection
    let metrics = TunnelMetrics {
        localup_id: id,
        total_requests: 0,
        requests_per_minute: 0.0,
        avg_latency_ms: 0.0,
        error_rate: 0.0,
        total_bandwidth_bytes: 0,
    };

    Ok(Json(metrics))
}

/// Health check endpoint
#[utoipa::path(
    get,
    path = "/api/health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse)
    ),
    tag = "system"
)]
pub async fn health_check(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let localup_ids = state.localup_manager.list_tunnels().await;

    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        active_tunnels: localup_ids.len(),
    })
}

/// List captured requests (traffic inspector)
#[utoipa::path(
    get,
    path = "/api/requests",
    params(
        ("localup_id" = Option<String>, Query, description = "Filter by tunnel ID"),
        ("method" = Option<String>, Query, description = "Filter by HTTP method (GET, POST, etc.)"),
        ("path" = Option<String>, Query, description = "Filter by path (supports partial match)"),
        ("status" = Option<u16>, Query, description = "Filter by exact status code"),
        ("status_min" = Option<u16>, Query, description = "Filter by minimum status code"),
        ("status_max" = Option<u16>, Query, description = "Filter by maximum status code"),
        ("offset" = Option<usize>, Query, description = "Pagination offset (default: 0)"),
        ("limit" = Option<usize>, Query, description = "Pagination limit (default: 100, max: 1000)")
    ),
    responses(
        (status = 200, description = "List of captured requests", body = CapturedRequestList),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "traffic"
)]
pub async fn list_requests(
    State(state): State<Arc<AppState>>,
    Query(query): Query<crate::models::CapturedRequestQuery>,
) -> Result<Json<CapturedRequestList>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Listing captured requests with filters: {:?}", query);

    use localup_relay_db::entities::captured_request::Column;
    use localup_relay_db::entities::prelude::*;
    use sea_orm::{ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};

    // Build query with filters
    let mut query_builder = CapturedRequest::find();

    // Apply filters
    let mut condition = Condition::all();

    if let Some(ref localup_id) = query.localup_id {
        condition = condition.add(Column::LocalupId.eq(localup_id));
    }

    if let Some(method) = &query.method {
        condition = condition.add(Column::Method.eq(method.to_uppercase()));
    }

    if let Some(ref path) = query.path {
        condition = condition.add(Column::Path.contains(path));
    }

    if let Some(status) = query.status {
        condition = condition.add(Column::Status.eq(status as i32));
    } else {
        // Apply status range if no exact status specified
        if let Some(status_min) = query.status_min {
            condition = condition.add(Column::Status.gte(status_min as i32));
        }
        if let Some(status_max) = query.status_max {
            condition = condition.add(Column::Status.lte(status_max as i32));
        }
    }

    query_builder = query_builder
        .filter(condition)
        .order_by_desc(Column::CreatedAt);

    // Apply pagination
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(100).min(1000); // Cap at 1000

    // Get paginator
    let paginator = query_builder.paginate(&state.db, limit as u64);

    // Get total count
    let total = paginator.num_items().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Database error: {}", e),
                code: None,
            }),
        )
    })? as usize;

    // Get page of results
    let page_num = offset / limit;
    let captured: Vec<localup_relay_db::entities::captured_request::Model> =
        paginator.fetch_page(page_num as u64).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                    code: None,
                }),
            )
        })?;

    let requests: Vec<crate::models::CapturedRequest> = captured
        .into_iter()
        .map(|req| {
            let headers: Vec<(String, String)> =
                serde_json::from_str(&req.headers).unwrap_or_default();
            let response_headers: Option<Vec<(String, String)>> = req
                .response_headers
                .and_then(|h| serde_json::from_str(&h).ok());

            let size_bytes = req.body.as_ref().map(|b| b.len()).unwrap_or(0)
                + req.response_body.as_ref().map(|b| b.len()).unwrap_or(0);

            crate::models::CapturedRequest {
                id: req.id,
                localup_id: req.localup_id,
                method: req.method,
                path: req.path,
                headers,
                body: req.body,
                status: req.status.map(|s| s as u16),
                response_headers,
                response_body: req.response_body,
                timestamp: req.created_at,
                duration_ms: req.latency_ms.map(|l| l as u64),
                size_bytes,
            }
        })
        .collect();

    Ok(Json(CapturedRequestList {
        requests,
        total,
        offset,
        limit,
    }))
}

/// Get a specific captured request
#[utoipa::path(
    get,
    path = "/api/requests/{id}",
    params(
        ("id" = String, Path, description = "Request ID")
    ),
    responses(
        (status = 200, description = "Captured request details", body = CapturedRequest),
        (status = 404, description = "Request not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "traffic"
)]
pub async fn get_request(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<CapturedRequest>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Getting captured request: {}", id);

    // TODO: Implement request retrieval
    Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse {
            error: format!("Request '{}' not found", id),
            code: Some("REQUEST_NOT_FOUND".to_string()),
        }),
    ))
}

/// Replay a captured request
#[utoipa::path(
    post,
    path = "/api/requests/{id}/replay",
    params(
        ("id" = String, Path, description = "Request ID")
    ),
    responses(
        (status = 200, description = "Request replayed successfully", body = CapturedRequest),
        (status = 404, description = "Request not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "traffic"
)]
pub async fn replay_request(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<CapturedRequest>, (StatusCode, Json<ErrorResponse>)> {
    info!("Replaying request: {}", id);

    // TODO: Implement request replay
    Err((
        StatusCode::NOT_IMPLEMENTED,
        Json(ErrorResponse {
            error: "Request replay not yet implemented".to_string(),
            code: Some("NOT_IMPLEMENTED".to_string()),
        }),
    ))
}

/// List captured TCP connections (traffic inspector)
#[utoipa::path(
    get,
    path = "/api/tcp-connections",
    params(
        ("localup_id" = Option<String>, Query, description = "Filter by tunnel ID"),
        ("client_addr" = Option<String>, Query, description = "Filter by client address (partial match)"),
        ("target_port" = Option<u16>, Query, description = "Filter by target port"),
        ("offset" = Option<usize>, Query, description = "Pagination offset (default: 0)"),
        ("limit" = Option<usize>, Query, description = "Pagination limit (default: 100, max: 1000)")
    ),
    responses(
        (status = 200, description = "List of TCP connections", body = CapturedTcpConnectionList),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "traffic"
)]
pub async fn list_tcp_connections(
    State(state): State<Arc<AppState>>,
    Query(query): Query<crate::models::CapturedTcpConnectionQuery>,
) -> Result<Json<crate::models::CapturedTcpConnectionList>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Listing TCP connections with filters: {:?}", query);

    use localup_relay_db::entities::captured_tcp_connection::Column;
    use localup_relay_db::entities::prelude::*;
    use sea_orm::{ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};

    // Build query with filters
    let mut query_builder = CapturedTcpConnection::find();
    let mut condition = Condition::all();

    if let Some(ref localup_id) = query.localup_id {
        condition = condition.add(Column::LocalupId.eq(localup_id));
    }

    if let Some(ref client_addr) = query.client_addr {
        condition = condition.add(Column::ClientAddr.contains(client_addr));
    }

    if let Some(target_port) = query.target_port {
        condition = condition.add(Column::TargetPort.eq(target_port as i32));
    }

    query_builder = query_builder
        .filter(condition)
        .order_by_desc(Column::ConnectedAt);

    // Apply pagination
    let offset = query.offset.unwrap_or(0);
    let limit = query.limit.unwrap_or(100).min(1000); // Cap at 1000

    // Get paginator
    let paginator = query_builder.paginate(&state.db, limit as u64);

    // Get total count
    let total = paginator.num_items().await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Database error: {}", e),
                code: None,
            }),
        )
    })? as usize;

    // Get page of results
    let page_num = offset / limit;
    let captured: Vec<localup_relay_db::entities::captured_tcp_connection::Model> =
        paginator.fetch_page(page_num as u64).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                    code: None,
                }),
            )
        })?;

    let connections: Vec<crate::models::CapturedTcpConnection> = captured
        .into_iter()
        .map(|conn| crate::models::CapturedTcpConnection {
            id: conn.id,
            localup_id: conn.localup_id,
            client_addr: conn.client_addr,
            target_port: conn.target_port as u16,
            bytes_received: conn.bytes_received,
            bytes_sent: conn.bytes_sent,
            connected_at: conn.connected_at.into(),
            disconnected_at: conn.disconnected_at.map(|dt| dt.into()),
            duration_ms: conn.duration_ms,
            disconnect_reason: conn.disconnect_reason,
        })
        .collect();

    Ok(Json(crate::models::CapturedTcpConnectionList {
        connections,
        total,
        offset,
        limit,
    }))
}

// Custom domain management handlers

use base64::Engine;
use chrono::Utc;
use localup_cert::AcmeClient;
use localup_relay_db::entities::custom_domain;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

/// Upload a custom domain certificate
#[utoipa::path(
    post,
    path = "/api/domains",
    request_body = UploadCustomDomainRequest,
    responses(
        (status = 201, description = "Certificate uploaded successfully", body = UploadCustomDomainResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn upload_custom_domain(
    State(state): State<Arc<AppState>>,
    Json(req): Json<UploadCustomDomainRequest>,
) -> Result<(StatusCode, Json<UploadCustomDomainResponse>), (StatusCode, Json<ErrorResponse>)> {
    info!("Uploading custom domain certificate for: {}", req.domain);

    // Decode base64 PEM content
    let cert_pem = base64::engine::general_purpose::STANDARD
        .decode(&req.cert_pem)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid base64 in cert_pem: {}", e),
                    code: Some("INVALID_BASE64".to_string()),
                }),
            )
        })?;

    let key_pem = base64::engine::general_purpose::STANDARD
        .decode(&req.key_pem)
        .map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Invalid base64 in key_pem: {}", e),
                    code: Some("INVALID_BASE64".to_string()),
                }),
            )
        })?;

    // Save to temporary files
    let cert_dir = std::path::Path::new("./.certs");
    tokio::fs::create_dir_all(cert_dir).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to create cert directory: {}", e),
                code: Some("CERT_DIR_CREATE_FAILED".to_string()),
            }),
        )
    })?;

    let cert_path = cert_dir.join(format!("{}.crt", req.domain));
    let key_path = cert_dir.join(format!("{}.key", req.domain));

    tokio::fs::write(&cert_path, &cert_pem).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to write certificate: {}", e),
                code: Some("CERT_WRITE_FAILED".to_string()),
            }),
        )
    })?;

    tokio::fs::write(&key_path, &key_pem).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to write private key: {}", e),
                code: Some("KEY_WRITE_FAILED".to_string()),
            }),
        )
    })?;

    // Validate certificate can be loaded
    AcmeClient::load_certificate_from_files(
        cert_path.to_str().unwrap(),
        key_path.to_str().unwrap(),
    )
    .await
    .map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Invalid certificate or key: {}", e),
                code: Some("INVALID_CERT".to_string()),
            }),
        )
    })?;

    info!("Certificate validated successfully for {}", req.domain);

    // TODO: Extract expiration from certificate
    let expires_at = Utc::now() + chrono::Duration::days(90);

    // Save to database
    let domain_model = custom_domain::ActiveModel {
        domain: Set(req.domain.clone()),
        cert_path: Set(Some(cert_path.to_string_lossy().to_string())),
        key_path: Set(Some(key_path.to_string_lossy().to_string())),
        status: Set(localup_relay_db::entities::custom_domain::DomainStatus::Active),
        provisioned_at: Set(Utc::now()),
        expires_at: Set(Some(expires_at)),
        auto_renew: Set(req.auto_renew),
        error_message: Set(None),
    };

    domain_model.insert(&state.db).await.map_err(|e| {
        error!("Database error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to save domain: {}", e),
                code: Some("DB_INSERT_FAILED".to_string()),
            }),
        )
    })?;

    info!("Custom domain {} saved to database", req.domain);

    Ok((
        StatusCode::CREATED,
        Json(UploadCustomDomainResponse {
            domain: req.domain,
            status: CustomDomainStatus::Active,
            message: "Certificate uploaded and validated successfully".to_string(),
        }),
    ))
}

/// List all custom domains
#[utoipa::path(
    get,
    path = "/api/domains",
    responses(
        (status = 200, description = "List of custom domains", body = CustomDomainList),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn list_custom_domains(
    State(state): State<Arc<AppState>>,
) -> Result<Json<CustomDomainList>, (StatusCode, Json<ErrorResponse>)> {
    info!("Listing custom domains");

    let domains = custom_domain::Entity::find()
        .all(&state.db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to list domains: {}", e),
                    code: Some("DB_QUERY_FAILED".to_string()),
                }),
            )
        })?;

    let total = domains.len();
    let domains = domains
        .into_iter()
        .map(|d| crate::models::CustomDomain {
            domain: d.domain,
            status: match d.status {
                localup_relay_db::entities::custom_domain::DomainStatus::Pending => {
                    CustomDomainStatus::Pending
                }
                localup_relay_db::entities::custom_domain::DomainStatus::Active => {
                    CustomDomainStatus::Active
                }
                localup_relay_db::entities::custom_domain::DomainStatus::Expired => {
                    CustomDomainStatus::Expired
                }
                localup_relay_db::entities::custom_domain::DomainStatus::Failed => {
                    CustomDomainStatus::Failed
                }
            },
            provisioned_at: d.provisioned_at,
            expires_at: d.expires_at,
            auto_renew: d.auto_renew,
            error_message: d.error_message,
        })
        .collect();

    Ok(Json(CustomDomainList { domains, total }))
}

/// Get a specific custom domain
#[utoipa::path(
    get,
    path = "/api/domains/{domain}",
    params(
        ("domain" = String, Path, description = "Domain name")
    ),
    responses(
        (status = 200, description = "Custom domain details", body = CustomDomain),
        (status = 404, description = "Domain not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn get_custom_domain(
    State(state): State<Arc<AppState>>,
    Path(domain): Path<String>,
) -> Result<Json<crate::models::CustomDomain>, (StatusCode, Json<ErrorResponse>)> {
    info!("Getting custom domain: {}", domain);

    let domain_model = custom_domain::Entity::find()
        .filter(custom_domain::Column::Domain.eq(&domain))
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to get domain: {}", e),
                    code: Some("DB_QUERY_FAILED".to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Domain not found: {}", domain),
                    code: Some("DOMAIN_NOT_FOUND".to_string()),
                }),
            )
        })?;

    Ok(Json(crate::models::CustomDomain {
        domain: domain_model.domain,
        status: match domain_model.status {
            localup_relay_db::entities::custom_domain::DomainStatus::Pending => {
                CustomDomainStatus::Pending
            }
            localup_relay_db::entities::custom_domain::DomainStatus::Active => {
                CustomDomainStatus::Active
            }
            localup_relay_db::entities::custom_domain::DomainStatus::Expired => {
                CustomDomainStatus::Expired
            }
            localup_relay_db::entities::custom_domain::DomainStatus::Failed => {
                CustomDomainStatus::Failed
            }
        },
        provisioned_at: domain_model.provisioned_at,
        expires_at: domain_model.expires_at,
        auto_renew: domain_model.auto_renew,
        error_message: domain_model.error_message,
    }))
}

/// Delete a custom domain
#[utoipa::path(
    delete,
    path = "/api/domains/{domain}",
    params(
        ("domain" = String, Path, description = "Domain name")
    ),
    responses(
        (status = 204, description = "Domain deleted successfully"),
        (status = 404, description = "Domain not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn delete_custom_domain(
    State(state): State<Arc<AppState>>,
    Path(domain): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    info!("Deleting custom domain: {}", domain);

    // Find the domain first to get file paths
    let domain_model = custom_domain::Entity::find()
        .filter(custom_domain::Column::Domain.eq(&domain))
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to find domain: {}", e),
                    code: Some("DB_QUERY_FAILED".to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Domain not found: {}", domain),
                    code: Some("DOMAIN_NOT_FOUND".to_string()),
                }),
            )
        })?;

    // Delete certificate files
    if let Some(cert_path) = &domain_model.cert_path {
        let _ = tokio::fs::remove_file(cert_path).await;
    }
    if let Some(key_path) = &domain_model.key_path {
        let _ = tokio::fs::remove_file(key_path).await;
    }

    // Delete from database
    custom_domain::Entity::delete_by_id(domain)
        .exec(&state.db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to delete domain: {}", e),
                    code: Some("DB_DELETE_FAILED".to_string()),
                }),
            )
        })?;

    info!("Custom domain {} deleted successfully", domain_model.domain);

    Ok(StatusCode::NO_CONTENT)
}

/// Initiate ACME challenge for a domain
#[utoipa::path(
    post,
    path = "/api/domains/challenge/initiate",
    request_body = InitiateChallengeRequest,
    responses(
        (status = 200, description = "Challenge initiated", body = InitiateChallengeResponse),
        (status = 501, description = "ACME not yet implemented", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn initiate_challenge(
    Json(req): Json<InitiateChallengeRequest>,
) -> Result<Json<InitiateChallengeResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!(
        "Initiating {} challenge for domain: {}",
        req.challenge_type, req.domain
    );

    // For now, return a mock response showing what the user needs to do
    // In the future, this will call the ACME client
    let challenge_id = uuid::Uuid::new_v4().to_string();
    let expires_at = Utc::now() + chrono::Duration::hours(1);

    let challenge = if req.challenge_type == "dns-01" {
        ChallengeInfo::Dns01 {
            domain: req.domain.clone(),
            record_name: format!("_acme-challenge.{}", req.domain),
            record_value: format!("mock-dns-token-{}", &challenge_id[..8]),
            instructions: vec![
                "1. Add a TXT record to your DNS:".to_string(),
                format!("   Name: _acme-challenge.{}", req.domain),
                format!("   Value: mock-dns-token-{}", &challenge_id[..8]),
                "2. Wait for DNS propagation (can take up to 48 hours)".to_string(),
                "3. Call POST /api/domains/challenge/complete with the challenge_id".to_string(),
            ],
        }
    } else {
        // Default to HTTP-01
        let token = format!("mock-http-token-{}", &challenge_id[..8]);
        ChallengeInfo::Http01 {
            domain: req.domain.clone(),
            token: token.clone(),
            key_authorization: format!("{}.mock-key-auth", token),
            file_path: format!("http://{}/.well-known/acme-challenge/{}", req.domain, token),
            instructions: vec![
                "1. Create the directory: .well-known/acme-challenge/".to_string(),
                format!("2. Create a file named: {}", token),
                format!("3. File content: {}.mock-key-auth", token),
                format!(
                    "4. Ensure it's accessible at: http://{}/.well-known/acme-challenge/{}",
                    req.domain, token
                ),
                "5. Call POST /api/domains/challenge/complete with the challenge_id".to_string(),
            ],
        }
    };

    Ok(Json(InitiateChallengeResponse {
        domain: req.domain,
        challenge,
        challenge_id,
        expires_at,
    }))
}

/// Complete/verify ACME challenge
#[utoipa::path(
    post,
    path = "/api/domains/challenge/complete",
    request_body = CompleteChallengeRequest,
    responses(
        (status = 200, description = "Challenge completed, certificate issued", body = UploadCustomDomainResponse),
        (status = 501, description = "ACME not yet implemented", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn complete_challenge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompleteChallengeRequest>,
) -> Result<Json<UploadCustomDomainResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Completing ACME challenge for domain: {}", req.domain);

    // Check if we have an ACME client configured
    let acme_client = state.acme_client.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ACME client not configured. Set ACME_EMAIL environment variable to enable Let's Encrypt.".to_string(),
                code: Some("ACME_NOT_CONFIGURED".to_string()),
            }),
        )
    })?;

    // Complete the ACME order
    let client = acme_client.read().await;
    let _cert = client.complete_order(&req.domain).await.map_err(|e| {
        error!("ACME order completion failed: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to complete ACME challenge: {}", e),
                code: Some("ACME_COMPLETE_FAILED".to_string()),
            }),
        )
    })?;
    drop(client);

    // Get certificate paths
    let acme_client_read = acme_client.read().await;
    let cert_dir = acme_client_read.cert_dir();
    let cert_path = format!("{}/{}.crt", cert_dir, req.domain);
    let key_path = format!("{}/{}.key", cert_dir, req.domain);
    drop(acme_client_read);

    // Calculate expiration (Let's Encrypt certs are valid for 90 days)
    let expires_at = Utc::now() + chrono::Duration::days(90);

    // Save to database
    let domain_model = custom_domain::ActiveModel {
        domain: Set(req.domain.clone()),
        cert_path: Set(Some(cert_path)),
        key_path: Set(Some(key_path)),
        status: Set(localup_relay_db::entities::custom_domain::DomainStatus::Active),
        provisioned_at: Set(Utc::now()),
        expires_at: Set(Some(expires_at)),
        auto_renew: Set(true),
        error_message: Set(None),
    };

    // Try to update if exists, otherwise insert
    use sea_orm::sea_query::OnConflict;
    custom_domain::Entity::insert(domain_model)
        .on_conflict(
            OnConflict::column(custom_domain::Column::Domain)
                .update_columns([
                    custom_domain::Column::CertPath,
                    custom_domain::Column::KeyPath,
                    custom_domain::Column::Status,
                    custom_domain::Column::ProvisionedAt,
                    custom_domain::Column::ExpiresAt,
                    custom_domain::Column::AutoRenew,
                    custom_domain::Column::ErrorMessage,
                ])
                .to_owned(),
        )
        .exec(&state.db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to save domain: {}", e),
                    code: Some("DB_INSERT_FAILED".to_string()),
                }),
            )
        })?;

    // Remove challenge from memory
    let mut challenges = state.acme_challenges.write().await;
    challenges.retain(|_, v| !v.contains(&req.domain));

    info!("Certificate issued successfully for {}", req.domain);

    Ok(Json(UploadCustomDomainResponse {
        domain: req.domain,
        status: CustomDomainStatus::Active,
        message: "Let's Encrypt certificate issued successfully".to_string(),
    }))
}

/// Serve ACME HTTP-01 challenge response
///
/// This endpoint serves the key authorization for ACME HTTP-01 challenges.
/// Let's Encrypt will request this URL to verify domain ownership.
#[utoipa::path(
    get,
    path = "/.well-known/acme-challenge/{token}",
    params(
        ("token" = String, Path, description = "ACME challenge token")
    ),
    responses(
        (status = 200, description = "Key authorization"),
        (status = 404, description = "Challenge not found")
    ),
    tag = "domains"
)]
pub async fn serve_acme_challenge(
    State(state): State<Arc<AppState>>,
    Path(token): Path<String>,
) -> Result<String, StatusCode> {
    let challenges = state.acme_challenges.read().await;

    if let Some(key_authorization) = challenges.get(&token) {
        debug!("Serving ACME challenge for token: {}", token);
        Ok(key_authorization.clone())
    } else {
        debug!("ACME challenge not found for token: {}", token);
        Err(StatusCode::NOT_FOUND)
    }
}

/// Request Let's Encrypt certificate for a domain
///
/// This initiates the ACME HTTP-01 challenge flow and provisions a certificate.
/// The domain must resolve to this server for the challenge to succeed.
#[utoipa::path(
    post,
    path = "/api/domains/{domain}/certificate",
    params(
        ("domain" = String, Path, description = "Domain name to get certificate for")
    ),
    responses(
        (status = 200, description = "Certificate provisioning started", body = InitiateChallengeResponse),
        (status = 400, description = "Invalid domain", body = ErrorResponse),
        (status = 503, description = "ACME not configured", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn request_acme_certificate(
    State(state): State<Arc<AppState>>,
    Path(domain): Path<String>,
) -> Result<Json<InitiateChallengeResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!("Requesting Let's Encrypt certificate for: {}", domain);

    // Check if we have an ACME client configured
    let acme_client = state.acme_client.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ACME client not configured. Set ACME_EMAIL environment variable to enable Let's Encrypt.".to_string(),
                code: Some("ACME_NOT_CONFIGURED".to_string()),
            }),
        )
    })?;

    // Initiate the ACME order
    let client = acme_client.read().await;
    let challenge_state = client.initiate_order(&domain).await.map_err(|e| {
        error!("ACME order initiation failed: {}", e);
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: format!("Failed to initiate certificate request: {}", e),
                code: Some("ACME_INIT_FAILED".to_string()),
            }),
        )
    })?;

    // Store the challenge for serving
    if let (Some(token), Some(key_auth)) =
        (&challenge_state.token, &challenge_state.key_authorization)
    {
        let mut challenges = state.acme_challenges.write().await;
        challenges.insert(token.clone(), key_auth.clone());
        info!("Stored ACME challenge for token: {}", token);
    }

    // Create the challenge info for response
    let challenge = ChallengeInfo::Http01 {
        domain: domain.clone(),
        token: challenge_state.token.clone().unwrap_or_default(),
        key_authorization: challenge_state
            .key_authorization
            .clone()
            .unwrap_or_default(),
        file_path: format!(
            "http://{}/.well-known/acme-challenge/{}",
            domain,
            challenge_state.token.as_ref().unwrap_or(&String::new())
        ),
        instructions: vec![
            "1. Ensure your domain DNS points to this server".to_string(),
            "2. The challenge response is automatically served at the URL above".to_string(),
            "3. Call POST /api/domains/challenge/complete to complete the verification".to_string(),
        ],
    };

    Ok(Json(InitiateChallengeResponse {
        domain,
        challenge,
        challenge_id: challenge_state.challenge_id,
        expires_at: challenge_state.expires_at,
    }))
}

// ============================================================================
// Authentication Handlers
// ============================================================================

use chrono::Duration;
use localup_auth::{hash_password, verify_password, JwtClaims, JwtValidator};
use localup_relay_db::entities::{prelude::User as UserEntity, user};
use uuid::Uuid;

/// Get authentication configuration
#[utoipa::path(
    get,
    path = "/api/auth/config",
    responses(
        (status = 200, description = "Authentication configuration", body = AuthConfig),
    ),
    tag = "auth"
)]
pub async fn auth_config(State(state): State<Arc<AppState>>) -> Json<crate::models::AuthConfig> {
    Json(crate::models::AuthConfig {
        signup_enabled: state.allow_signup,
        relay: state.relay_config.clone(),
    })
}

/// Register a new user
#[utoipa::path(
    post,
    path = "/api/auth/register",
    request_body = RegisterRequest,
    responses(
        (status = 201, description = "User registered successfully", body = RegisterResponse),
        (status = 400, description = "Invalid request or email already exists", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn register(
    State(state): State<Arc<AppState>>,
    req_headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::user;

    // Check if public signup is allowed
    if !state.allow_signup {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "Public registration is disabled. Please contact your administrator for an invitation.".to_string(),
                code: Some("SIGNUP_DISABLED".to_string()),
            }),
        ));
    }

    // Validate email format (basic check)
    if !req.email.contains('@') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid email format".to_string(),
                code: Some("INVALID_EMAIL".to_string()),
            }),
        ));
    }

    // Validate password length
    if req.password.len() < 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Password must be at least 8 characters".to_string(),
                code: Some("WEAK_PASSWORD".to_string()),
            }),
        ));
    }

    // Check if email already exists
    let existing_user = UserEntity::find()
        .filter(user::Column::Email.eq(&req.email))
        .one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error checking email: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;

    if existing_user.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Email already registered".to_string(),
                code: Some("EMAIL_EXISTS".to_string()),
            }),
        ));
    }

    // Hash password
    let password_hash = hash_password(&req.password).map_err(|e| {
        tracing::error!("Password hashing error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
                code: Some("HASH_ERROR".to_string()),
            }),
        )
    })?;

    // Create user
    let user_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    let new_user = user::ActiveModel {
        id: Set(user_id),
        email: Set(req.email.clone()),
        password_hash: Set(password_hash),
        full_name: Set(req.full_name.clone()),
        role: Set(user::UserRole::User),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
    };

    let user = new_user.insert(&state.db).await.map_err(|e| {
        tracing::error!("Database error creating user: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
                code: Some("DB_ERROR".to_string()),
            }),
        )
    })?;

    // Auto-create default auth token for tunnel authentication
    use localup_relay_db::entities::auth_token;
    use sha2::{Digest, Sha256};

    let auth_token_id = Uuid::new_v4();
    let jwt_secret = state.jwt_secret.as_bytes();

    // Generate JWT auth token (never expires for default token)
    let auth_claims = JwtClaims::new(
        auth_token_id.to_string(),
        "localup-relay".to_string(),
        "localup-tunnel".to_string(),
        Duration::days(36500), // ~100 years for "never expires"
    )
    .with_user_id(user_id.to_string())
    .with_token_type("auth".to_string());

    let auth_token_jwt = JwtValidator::encode(jwt_secret, &auth_claims).map_err(|e| {
        tracing::error!("JWT encoding error for auth token: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
                code: Some("JWT_ERROR".to_string()),
            }),
        )
    })?;

    // Hash the auth token using SHA-256
    let mut hasher = Sha256::new();
    hasher.update(auth_token_jwt.as_bytes());
    let auth_token_hash = format!("{:x}", hasher.finalize());

    // Store auth token in database
    let new_auth_token = auth_token::ActiveModel {
        id: Set(auth_token_id),
        user_id: Set(user_id),
        team_id: Set(None),
        name: Set("Default".to_string()),
        description: Set(Some(
            "Auto-generated default authentication token".to_string(),
        )),
        token_hash: Set(auth_token_hash),
        last_used_at: Set(None),
        expires_at: Set(None), // Never expires
        is_active: Set(true),
        created_at: Set(now),
    };

    new_auth_token
        .insert(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error creating default auth token: {}", e);
            // Log error but don't fail registration - user can create token later
            tracing::warn!(
                "Failed to create default auth token for user {}, they can create one later",
                user_id
            );
        })
        .ok(); // Ignore error to not block registration

    tracing::info!("Created default auth token for user {}", user_id);

    // Generate session token (7 days validity)
    let jwt_secret = state.jwt_secret.as_bytes();
    let user_role_str = match user.role {
        user::UserRole::Admin => "admin",
        user::UserRole::User => "user",
    };
    let claims = JwtClaims::new(
        user_id.to_string(),
        "localup-relay".to_string(),
        "localup-web-ui".to_string(),
        Duration::days(7),
    )
    .with_user_id(user_id.to_string())
    .with_user_role(user_role_str.to_string())
    .with_token_type("session".to_string());
    let token = JwtValidator::encode(jwt_secret, &claims).map_err(|e| {
        tracing::error!("JWT encoding error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
                code: Some("JWT_ERROR".to_string()),
            }),
        )
    })?;

    let expires_at = now + Duration::days(7);

    // Create HTTP-only cookie with session token (with Secure flag for HTTPS)
    let is_secure = is_request_secure(&req_headers, state.is_https);
    let cookie = create_session_cookie(&token, is_secure);

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie.parse().unwrap());

    let response = Json(RegisterResponse {
        user: crate::models::User {
            id: user.id.to_string(),
            email: user.email,
            full_name: user.full_name,
            role: match user.role {
                user::UserRole::Admin => crate::models::UserRole::Admin,
                user::UserRole::User => crate::models::UserRole::User,
            },
            is_active: user.is_active,
            created_at: user.created_at,
            updated_at: user.updated_at,
        },
        token, // Will be removed from model next
        expires_at,
        auth_token: auth_token_jwt,
    });

    Ok((StatusCode::CREATED, headers, response))
}

/// Login with email and password
#[utoipa::path(
    post,
    path = "/api/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 401, description = "Invalid credentials", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn login(
    State(state): State<Arc<AppState>>,
    req_headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Find user by email
    let user = UserEntity::find()
        .filter(user::Column::Email.eq(&req.email))
        .one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "Invalid email or password".to_string(),
                    code: Some("INVALID_CREDENTIALS".to_string()),
                }),
            )
        })?;

    // Check if account is active
    if !user.is_active {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Account is disabled".to_string(),
                code: Some("ACCOUNT_DISABLED".to_string()),
            }),
        ));
    }

    // Verify password
    let password_valid = verify_password(&req.password, &user.password_hash).map_err(|e| {
        tracing::error!("Password verification error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
                code: Some("VERIFY_ERROR".to_string()),
            }),
        )
    })?;

    if !password_valid {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Invalid email or password".to_string(),
                code: Some("INVALID_CREDENTIALS".to_string()),
            }),
        ));
    }

    // Auto-create default auth token if user doesn't have one (for existing users)
    use localup_relay_db::entities::auth_token;
    use sha2::{Digest, Sha256};

    let existing_default_token = AuthTokenEntity::find()
        .filter(auth_token::Column::UserId.eq(user.id))
        .filter(auth_token::Column::Name.eq("Default"))
        .one(&state.db)
        .await
        .ok()
        .flatten();

    if existing_default_token.is_none() {
        let auth_token_id = Uuid::new_v4();
        let jwt_secret = state.jwt_secret.as_bytes();
        let now = chrono::Utc::now();

        // Generate JWT auth token (never expires for default token)
        let auth_claims = JwtClaims::new(
            auth_token_id.to_string(),
            "localup-relay".to_string(),
            "localup-tunnel".to_string(),
            Duration::days(36500), // ~100 years
        )
        .with_user_id(user.id.to_string())
        .with_token_type("auth".to_string());

        if let Ok(auth_token_jwt) = JwtValidator::encode(jwt_secret, &auth_claims) {
            // Hash the auth token using SHA-256
            let mut hasher = Sha256::new();
            hasher.update(auth_token_jwt.as_bytes());
            let auth_token_hash = format!("{:x}", hasher.finalize());

            // Store auth token in database
            let new_auth_token = auth_token::ActiveModel {
                id: Set(auth_token_id),
                user_id: Set(user.id),
                team_id: Set(None),
                name: Set("Default".to_string()),
                description: Set(Some(
                    "Auto-generated default authentication token".to_string(),
                )),
                token_hash: Set(auth_token_hash),
                last_used_at: Set(None),
                expires_at: Set(None), // Never expires
                is_active: Set(true),
                created_at: Set(now),
            };

            if let Err(e) = new_auth_token.insert(&state.db).await {
                tracing::warn!(
                    "Failed to create default auth token for user {} on login: {}",
                    user.id,
                    e
                );
            } else {
                tracing::info!(
                    "Created default auth token for existing user {} on login",
                    user.id
                );
            }
        }
    }

    // Generate session token (7 days validity)
    let jwt_secret = state.jwt_secret.as_bytes();
    let now = chrono::Utc::now();
    let user_role_str = match user.role {
        user::UserRole::Admin => "admin",
        user::UserRole::User => "user",
    };
    let claims = JwtClaims::new(
        user.id.to_string(),
        "localup-relay".to_string(),
        "localup-web-ui".to_string(),
        Duration::days(7),
    )
    .with_user_id(user.id.to_string())
    .with_user_role(user_role_str.to_string())
    .with_token_type("session".to_string());
    let token = JwtValidator::encode(jwt_secret, &claims).map_err(|e| {
        tracing::error!("JWT encoding error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
                code: Some("JWT_ERROR".to_string()),
            }),
        )
    })?;

    let expires_at = now + Duration::days(7);

    // Create HTTP-only cookie with session token (with Secure flag for HTTPS)
    let is_secure = is_request_secure(&req_headers, state.is_https);
    let cookie = create_session_cookie(&token, is_secure);

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie.parse().unwrap());

    let response = Json(LoginResponse {
        user: crate::models::User {
            id: user.id.to_string(),
            email: user.email,
            full_name: user.full_name,
            role: match user.role {
                user::UserRole::Admin => crate::models::UserRole::Admin,
                user::UserRole::User => crate::models::UserRole::User,
            },
            is_active: user.is_active,
            created_at: user.created_at,
            updated_at: user.updated_at,
        },
        token, // Will be removed from model next
        expires_at,
    });

    Ok((headers, response))
}

/// Logout (clear session cookie)
#[utoipa::path(
    post,
    path = "/api/auth/logout",
    responses(
        (status = 200, description = "Logout successful"),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn logout(
    State(state): State<Arc<AppState>>,
    req_headers: HeaderMap,
) -> impl IntoResponse {
    // Clear the session cookie by setting Max-Age=0 (with Secure flag for HTTPS)
    let is_secure = is_request_secure(&req_headers, state.is_https);
    let cookie = create_logout_cookie(is_secure);

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie.parse().unwrap());

    (
        headers,
        Json(serde_json::json!({
            "message": "Logged out successfully"
        })),
    )
}

/// Get current authenticated user
#[utoipa::path(
    get,
    path = "/api/auth/me",
    responses(
        (status = 200, description = "Current user info", body = inline(Object)),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn get_current_user(
    Extension(auth_user): Extension<crate::middleware::AuthUser>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Find user in database
    let user = UserEntity::find_by_id(Uuid::parse_str(&auth_user.user_id).unwrap())
        .one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "User not found".to_string(),
                    code: Some("USER_NOT_FOUND".to_string()),
                }),
            )
        })?;

    Ok(Json(serde_json::json!({
        "user": {
            "id": user.id.to_string(),
            "email": user.email,
            "username": user.full_name,
            "role": user.role,
            "is_active": user.is_active,
            "created_at": user.created_at,
        }
    })))
}

/// Get user's teams
#[utoipa::path(
    get,
    path = "/api/teams",
    responses(
        (status = 200, description = "List of user's teams", body = inline(Object)),
        (status = 401, description = "Not authenticated", body = ErrorResponse)
    ),
    tag = "teams"
)]
pub async fn list_user_teams(
    Extension(auth_user): Extension<crate::middleware::AuthUser>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::{prelude::*, team_member};
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let user_id = Uuid::parse_str(&auth_user.user_id).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Invalid user ID format".to_string(),
                code: Some("INVALID_USER_ID".to_string()),
            }),
        )
    })?;

    // Find all team memberships for this user
    let team_memberships = TeamMember::find()
        .filter(team_member::Column::UserId.eq(user_id))
        .find_also_related(Team)
        .all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding team memberships: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;

    // Map to response format
    let teams: Vec<serde_json::Value> = team_memberships
        .into_iter()
        .filter_map(|(membership, team_opt)| {
            team_opt.map(|team| {
                let role_str = match membership.role {
                    team_member::TeamRole::Owner => "owner",
                    team_member::TeamRole::Admin => "admin",
                    team_member::TeamRole::Member => "member",
                };
                serde_json::json!({
                    "id": team.id.to_string(),
                    "name": team.name,
                    "slug": team.slug,
                    "role": role_str,
                    "created_at": team.created_at,
                })
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "teams": teams
    })))
}

// ============================================================================
// Auth Token Management Handlers
// ============================================================================

use localup_relay_db::entities::{auth_token, prelude::AuthToken as AuthTokenEntity};
use sha2::{Digest, Sha256};

/// Create a new auth token (API key for tunnel authentication)
#[utoipa::path(
    post,
    path = "/api/auth-tokens",
    request_body = CreateAuthTokenRequest,
    responses(
        (status = 201, description = "Auth token created successfully", body = CreateAuthTokenResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth-tokens",
    security(("bearer_auth" = []))
)]
pub async fn create_auth_token(
    State(state): State<Arc<AppState>>,
    Extension(auth_user): Extension<AuthUser>,
    Json(req): Json<CreateAuthTokenRequest>,
) -> Result<(StatusCode, Json<CreateAuthTokenResponse>), (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::auth_token;

    // Validate name
    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Token name cannot be empty".to_string(),
                code: Some("INVALID_NAME".to_string()),
            }),
        ));
    }

    // Get user_id from authenticated user
    let user_id = Uuid::parse_str(&auth_user.user_id).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Invalid user ID format".to_string(),
                code: Some("INVALID_USER_ID".to_string()),
            }),
        )
    })?;

    let token_id = Uuid::new_v4();
    let now = chrono::Utc::now();

    // Calculate expiration
    let expires_at = req.expires_in_days.map(|days| now + Duration::days(days));

    // Generate JWT auth token
    let jwt_secret_bytes = state.jwt_secret.as_bytes();

    let mut claims = JwtClaims::new(
        token_id.to_string(),
        "localup-relay".to_string(),
        "localup-tunnel".to_string(),
        if let Some(exp_at) = expires_at {
            exp_at - now
        } else {
            Duration::days(36500) // ~100 years for "never expires"
        },
    )
    .with_user_id(user_id.to_string())
    .with_token_type("auth".to_string());

    if let Some(ref team_id_str) = req.team_id {
        claims = claims.with_team_id(team_id_str.clone());
    }

    let token = JwtValidator::encode(jwt_secret_bytes, &claims).map_err(|e| {
        tracing::error!("JWT encoding error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
                code: Some("JWT_ERROR".to_string()),
            }),
        )
    })?;

    // Hash the token using SHA-256
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());

    // Store auth token in database
    let team_id_uuid = req.team_id.as_ref().and_then(|id| Uuid::parse_str(id).ok());

    let new_token = auth_token::ActiveModel {
        id: Set(token_id),
        user_id: Set(user_id),
        team_id: Set(team_id_uuid),
        name: Set(req.name.clone()),
        description: Set(req.description.clone()),
        token_hash: Set(token_hash),
        last_used_at: Set(None),
        expires_at: Set(expires_at),
        is_active: Set(true),
        created_at: Set(now),
    };

    let saved_token = new_token.insert(&state.db).await.map_err(|e| {
        tracing::error!("Database error creating auth token: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
                code: Some("DB_ERROR".to_string()),
            }),
        )
    })?;

    Ok((
        StatusCode::CREATED,
        Json(CreateAuthTokenResponse {
            id: saved_token.id.to_string(),
            name: saved_token.name,
            token, // SHOWN ONLY ONCE!
            expires_at: saved_token.expires_at,
            created_at: saved_token.created_at,
        }),
    ))
}

/// List user's auth tokens
#[utoipa::path(
    get,
    path = "/api/auth-tokens",
    responses(
        (status = 200, description = "List of auth tokens", body = AuthTokenList),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth-tokens",
    security(("bearer_auth" = []))
)]
pub async fn list_auth_tokens(
    State(state): State<Arc<AppState>>,
    Extension(auth_user): Extension<AuthUser>,
) -> Result<Json<AuthTokenList>, (StatusCode, Json<ErrorResponse>)> {
    // Parse user_id from authenticated user
    let user_id = Uuid::parse_str(&auth_user.user_id).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Invalid user ID format".to_string(),
                code: Some("INVALID_USER_ID".to_string()),
            }),
        )
    })?;

    // Query all auth tokens for this user
    let token_records = AuthTokenEntity::find()
        .filter(auth_token::Column::UserId.eq(user_id))
        .all(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error listing tokens: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;

    // Convert to response format
    let tokens: Vec<AuthToken> = token_records
        .into_iter()
        .map(|t| AuthToken {
            id: t.id.to_string(),
            user_id: t.user_id.to_string(),
            team_id: t.team_id.map(|id| id.to_string()),
            name: t.name,
            description: t.description,
            last_used_at: t.last_used_at,
            expires_at: t.expires_at,
            is_active: t.is_active,
            created_at: t.created_at,
        })
        .collect();

    let total = tokens.len();

    Ok(Json(AuthTokenList { tokens, total }))
}

/// Get specific auth token details
#[utoipa::path(
    get,
    path = "/api/auth-tokens/{id}",
    params(
        ("id" = String, Path, description = "Auth token ID")
    ),
    responses(
        (status = 200, description = "Auth token details", body = AuthToken),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Token not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth-tokens",
    security(("bearer_auth" = []))
)]
pub async fn get_auth_token(
    State(state): State<Arc<AppState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<AuthToken>, (StatusCode, Json<ErrorResponse>)> {
    let token_id = Uuid::parse_str(&id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid token ID format".to_string(),
                code: Some("INVALID_ID".to_string()),
            }),
        )
    })?;

    // Parse authenticated user_id
    let user_id = Uuid::parse_str(&auth_user.user_id).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Invalid user ID format".to_string(),
                code: Some("INVALID_USER_ID".to_string()),
            }),
        )
    })?;

    let token = AuthTokenEntity::find_by_id(token_id)
        .one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding token: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Auth token not found".to_string(),
                    code: Some("NOT_FOUND".to_string()),
                }),
            )
        })?;

    // Verify ownership: token must belong to authenticated user
    if token.user_id != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "You don't have permission to access this token".to_string(),
                code: Some("FORBIDDEN".to_string()),
            }),
        ));
    }

    Ok(Json(AuthToken {
        id: token.id.to_string(),
        user_id: token.user_id.to_string(),
        team_id: token.team_id.map(|id| id.to_string()),
        name: token.name,
        description: token.description,
        last_used_at: token.last_used_at,
        expires_at: token.expires_at,
        is_active: token.is_active,
        created_at: token.created_at,
    }))
}

/// Update auth token (name, description, or active status)
#[utoipa::path(
    patch,
    path = "/api/auth-tokens/{id}",
    params(
        ("id" = String, Path, description = "Auth token ID")
    ),
    request_body = UpdateAuthTokenRequest,
    responses(
        (status = 200, description = "Auth token updated", body = AuthToken),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Token not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth-tokens",
    security(("bearer_auth" = []))
)]
pub async fn update_auth_token(
    State(state): State<Arc<AppState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAuthTokenRequest>,
) -> Result<Json<AuthToken>, (StatusCode, Json<ErrorResponse>)> {
    let token_id = Uuid::parse_str(&id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid token ID format".to_string(),
                code: Some("INVALID_ID".to_string()),
            }),
        )
    })?;

    // Parse authenticated user_id
    let user_id = Uuid::parse_str(&auth_user.user_id).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Invalid user ID format".to_string(),
                code: Some("INVALID_USER_ID".to_string()),
            }),
        )
    })?;

    let token = AuthTokenEntity::find_by_id(token_id)
        .one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding token: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Auth token not found".to_string(),
                    code: Some("NOT_FOUND".to_string()),
                }),
            )
        })?;

    // Verify ownership: token must belong to authenticated user
    if token.user_id != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "You don't have permission to modify this token".to_string(),
                code: Some("FORBIDDEN".to_string()),
            }),
        ));
    }

    // Update token
    let mut active_token: auth_token::ActiveModel = token.into();

    if let Some(name) = req.name {
        active_token.name = Set(name);
    }
    if let Some(description) = req.description {
        active_token.description = Set(Some(description));
    }
    if let Some(is_active) = req.is_active {
        active_token.is_active = Set(is_active);
    }

    let updated_token = active_token.update(&state.db).await.map_err(|e| {
        tracing::error!("Database error updating token: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
                code: Some("DB_ERROR".to_string()),
            }),
        )
    })?;

    Ok(Json(AuthToken {
        id: updated_token.id.to_string(),
        user_id: updated_token.user_id.to_string(),
        team_id: updated_token.team_id.map(|id| id.to_string()),
        name: updated_token.name,
        description: updated_token.description,
        last_used_at: updated_token.last_used_at,
        expires_at: updated_token.expires_at,
        is_active: updated_token.is_active,
        created_at: updated_token.created_at,
    }))
}

/// Delete (revoke) an auth token
#[utoipa::path(
    delete,
    path = "/api/auth-tokens/{id}",
    params(
        ("id" = String, Path, description = "Auth token ID")
    ),
    responses(
        (status = 204, description = "Auth token deleted successfully"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Token not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth-tokens",
    security(("bearer_auth" = []))
)]
pub async fn delete_auth_token(
    State(state): State<Arc<AppState>>,
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    let token_id = Uuid::parse_str(&id).map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid token ID format".to_string(),
                code: Some("INVALID_ID".to_string()),
            }),
        )
    })?;

    // Parse authenticated user_id
    let user_id = Uuid::parse_str(&auth_user.user_id).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Invalid user ID format".to_string(),
                code: Some("INVALID_USER_ID".to_string()),
            }),
        )
    })?;

    let token = AuthTokenEntity::find_by_id(token_id)
        .one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error finding token: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "Auth token not found".to_string(),
                    code: Some("NOT_FOUND".to_string()),
                }),
            )
        })?;

    // Verify ownership: token must belong to authenticated user
    if token.user_id != user_id {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "You don't have permission to delete this token".to_string(),
                code: Some("FORBIDDEN".to_string()),
            }),
        ));
    }

    // Delete token
    let active_token: auth_token::ActiveModel = token.into();
    active_token.delete(&state.db).await.map_err(|e| {
        tracing::error!("Database error deleting token: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Internal server error".to_string(),
                code: Some("DB_ERROR".to_string()),
            }),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get available transport protocols (well-known endpoint)
///
/// This endpoint is used by clients to discover which transport protocols
/// are available on this relay (QUIC, WebSocket, HTTP/2).
#[utoipa::path(
    get,
    path = "/.well-known/localup-protocols",
    responses(
        (status = 200, description = "Protocol discovery response", body = ProtocolDiscoveryResponse),
        (status = 204, description = "Protocol discovery not configured")
    ),
    tag = "discovery"
)]
pub async fn protocol_discovery(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match &state.protocol_discovery {
        Some(discovery) => {
            debug!(
                "Protocol discovery request, returning {} transports",
                discovery.transports.len()
            );
            Json(discovery.clone()).into_response()
        }
        None => {
            // Return default QUIC-only response if not configured
            let default_discovery = localup_proto::ProtocolDiscoveryResponse::quic_only(4443);
            Json(default_discovery).into_response()
        }
    }
}
