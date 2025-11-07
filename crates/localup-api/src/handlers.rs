use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use tracing::{debug, info};

use crate::models::*;
use crate::AppState;

/// List all active tunnels
#[utoipa::path(
    get,
    path = "/api/tunnels",
    responses(
        (status = 200, description = "List of tunnels", body = TunnelList),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "tunnels"
)]
pub async fn list_tunnels(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TunnelList>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Listing tunnels");

    // Get active tunnel IDs
    let localup_ids = state.localup_manager.list_tunnels().await;

    let mut tunnels = Vec::new();

    for localup_id in localup_ids {
        if let Some(endpoints) = state.localup_manager.get_endpoints(&localup_id).await {
            let tunnel = Tunnel {
                id: localup_id.clone(),
                endpoints: endpoints
                    .iter()
                    .map(|e| TunnelEndpoint {
                        protocol: match &e.protocol {
                            localup_proto::Protocol::Http { subdomain } => TunnelProtocol::Http {
                                subdomain: subdomain
                                    .clone()
                                    .unwrap_or_else(|| "unknown".to_string()),
                            },
                            localup_proto::Protocol::Https { subdomain } => TunnelProtocol::Https {
                                subdomain: subdomain
                                    .clone()
                                    .unwrap_or_else(|| "unknown".to_string()),
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
                region: "us-east-1".to_string(), // TODO: Get from config
                connected_at: chrono::Utc::now(), // TODO: Track actual connection time
                local_addr: None,                // Client-side information
            };
            tunnels.push(tunnel);
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

    if let Some(endpoints) = state.localup_manager.get_endpoints(&id).await {
        let tunnel = Tunnel {
            id: id.clone(),
            endpoints: endpoints
                .iter()
                .map(|e| TunnelEndpoint {
                    protocol: match &e.protocol {
                        localup_proto::Protocol::Http { subdomain } => TunnelProtocol::Http {
                            subdomain: subdomain.clone().unwrap_or_else(|| "unknown".to_string()),
                        },
                        localup_proto::Protocol::Https { subdomain } => TunnelProtocol::Https {
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

        Ok(Json(tunnel))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Tunnel '{}' not found", id),
                code: Some("TUNNEL_NOT_FOUND".to_string()),
            }),
        ))
    }
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
