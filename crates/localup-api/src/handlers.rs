use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use localup_router::WildcardPattern;
use sea_orm::DatabaseConnection;
use std::sync::Arc;
use tracing::{debug, error, info};

use crate::middleware::AuthUser;
use crate::models::*;
use crate::AppState;

// ============================================================================
// Shared Helper Functions
// ============================================================================

/// Convert a `localup_proto::Endpoint` to our API `TunnelEndpoint` model.
fn proto_endpoint_to_api(e: &localup_proto::Endpoint) -> TunnelEndpoint {
    TunnelEndpoint {
        protocol: match &e.protocol {
            localup_proto::Protocol::Http { subdomain, .. } => TunnelProtocol::Http {
                subdomain: subdomain.clone().unwrap_or_else(|| "unknown".to_string()),
            },
            localup_proto::Protocol::Https { subdomain, .. } => TunnelProtocol::Https {
                subdomain: subdomain.clone().unwrap_or_else(|| "unknown".to_string()),
            },
            localup_proto::Protocol::Tcp { port } => TunnelProtocol::Tcp { port: *port },
            localup_proto::Protocol::Tls {
                port: _,
                sni_patterns,
            } => TunnelProtocol::Tls {
                domains: sni_patterns.clone(),
            },
        },
        public_url: e.public_url.clone(),
        port: e.port,
    }
}

/// Convert a DB `DomainStatus` to our API `CustomDomainStatus` model.
fn db_domain_status_to_api(
    status: localup_relay_db::entities::custom_domain::DomainStatus,
) -> CustomDomainStatus {
    match status {
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
    }
}

/// Convert a DB `custom_domain::Model` to our API `CustomDomain` model.
fn db_domain_to_api(d: localup_relay_db::entities::custom_domain::Model) -> CustomDomain {
    CustomDomain {
        id: d.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        domain: d.domain,
        status: db_domain_status_to_api(d.status),
        provisioned_at: d.provisioned_at,
        expires_at: d.expires_at,
        auto_renew: d.auto_renew,
        error_message: d.error_message,
    }
}

/// Convert a DB `user::Model` to our API `User` model.
fn db_user_to_api(user: &localup_relay_db::entities::user::Model) -> crate::models::User {
    use localup_relay_db::entities::user::UserRole;
    crate::models::User {
        id: user.id.to_string(),
        email: user.email.clone(),
        full_name: user.full_name.clone(),
        role: match user.role {
            UserRole::Admin => crate::models::UserRole::Admin,
            UserRole::User => crate::models::UserRole::User,
        },
        is_active: user.is_active,
        created_at: user.created_at,
        updated_at: user.updated_at,
    }
}

/// Convert a DB `auth_token::Model` to our API `AuthToken` model.
fn db_auth_token_to_api(t: localup_relay_db::entities::auth_token::Model) -> AuthToken {
    AuthToken {
        id: t.id.to_string(),
        user_id: t.user_id.to_string(),
        team_id: t.team_id.map(|id| id.to_string()),
        name: t.name,
        description: t.description,
        last_used_at: t.last_used_at,
        expires_at: t.expires_at,
        is_active: t.is_active,
        created_at: t.created_at,
    }
}

/// Generate a session JWT token and Set-Cookie header for a user.
#[allow(clippy::type_complexity)]
fn generate_session_response(
    state: &AppState,
    user: &localup_relay_db::entities::user::Model,
    req_headers: &HeaderMap,
) -> Result<(String, chrono::DateTime<chrono::Utc>, HeaderMap), (StatusCode, Json<ErrorResponse>)> {
    use chrono::Duration;
    use localup_auth::{JwtClaims, JwtValidator};
    use localup_relay_db::entities::user::UserRole;

    let jwt_secret = state.jwt_secret.as_bytes();
    let now = chrono::Utc::now();
    let user_role_str = match user.role {
        UserRole::Admin => "admin",
        UserRole::User => "user",
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

    let is_secure = is_request_secure(req_headers, state.is_https);
    let cookie = create_session_cookie(&token, is_secure);

    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie.parse().unwrap());

    Ok((token, expires_at, headers))
}

/// Ensure a user has a default auth token, creating one if missing.
/// Returns the JWT string of the default token if newly created, or empty string.
async fn ensure_default_auth_token(state: &AppState, user_id: uuid::Uuid) -> String {
    use localup_auth::{JwtClaims, JwtValidator};
    use localup_relay_db::entities::{auth_token, prelude::AuthToken as AuthTokenEntity};
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    use sha2::{Digest, Sha256};

    let existing = AuthTokenEntity::find()
        .filter(auth_token::Column::UserId.eq(user_id))
        .filter(auth_token::Column::Name.eq("Default"))
        .one(&state.db)
        .await
        .ok()
        .flatten();

    if existing.is_some() {
        return String::new();
    }

    let auth_token_id = uuid::Uuid::new_v4();
    let jwt_secret = state.jwt_secret.as_bytes();
    let now = chrono::Utc::now();

    let auth_claims = JwtClaims::new(
        auth_token_id.to_string(),
        "localup-relay".to_string(),
        "localup-tunnel".to_string(),
        chrono::Duration::days(36500),
    )
    .with_user_id(user_id.to_string())
    .with_token_type("auth".to_string());

    let auth_token_jwt = match JwtValidator::encode(jwt_secret, &auth_claims) {
        Ok(jwt) => jwt,
        Err(_) => return String::new(),
    };

    let mut hasher = Sha256::new();
    hasher.update(auth_token_jwt.as_bytes());
    let auth_token_hash = format!("{:x}", hasher.finalize());

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
        expires_at: Set(None),
        is_active: Set(true),
        created_at: Set(now),
    };

    if let Err(e) = new_auth_token.insert(&state.db).await {
        tracing::warn!(
            "Failed to create default auth token for user {}: {}",
            user_id,
            e
        );
        return String::new();
    }

    tracing::info!("Created default auth token for user {}", user_id);
    auth_token_jwt
}

/// Return an API error response.
fn api_error(
    status: StatusCode,
    error: impl Into<String>,
    code: impl Into<String>,
) -> (StatusCode, Json<ErrorResponse>) {
    (
        status,
        Json(ErrorResponse {
            error: error.into(),
            code: Some(code.into()),
        }),
    )
}

/// Parse a user ID string into a UUID, or return an error response.
fn parse_user_id(user_id: &str) -> Result<uuid::Uuid, (StatusCode, Json<ErrorResponse>)> {
    uuid::Uuid::parse_str(user_id).map_err(|_| {
        api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Invalid user ID format",
            "INVALID_USER_ID",
        )
    })
}

/// Compute upstream status by analyzing recent requests for a tunnel
/// Returns (upstream_status, recent_502_count, total_recent_count)
async fn compute_upstream_status(
    db: &DatabaseConnection,
    localup_id: &str,
) -> (UpstreamStatus, Option<i64>, Option<i64>) {
    use localup_relay_db::entities::prelude::*;
    use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};

    // Look at requests from the last 60 seconds
    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(60);

    // Count recent 502 errors (Bad Gateway - indicates upstream connection failure)
    let recent_502_count = CapturedRequest::find()
        .filter(localup_relay_db::entities::captured_request::Column::LocalupId.eq(localup_id))
        .filter(localup_relay_db::entities::captured_request::Column::CreatedAt.gt(cutoff))
        .filter(localup_relay_db::entities::captured_request::Column::Status.eq(502))
        .count(db)
        .await
        .unwrap_or(0) as i64;

    // Count total recent requests
    let total_recent_count = CapturedRequest::find()
        .filter(localup_relay_db::entities::captured_request::Column::LocalupId.eq(localup_id))
        .filter(localup_relay_db::entities::captured_request::Column::CreatedAt.gt(cutoff))
        .count(db)
        .await
        .unwrap_or(0) as i64;

    // Also check for pending requests (no status yet)
    let pending_count = CapturedRequest::find()
        .filter(localup_relay_db::entities::captured_request::Column::LocalupId.eq(localup_id))
        .filter(localup_relay_db::entities::captured_request::Column::CreatedAt.gt(cutoff))
        .filter(localup_relay_db::entities::captured_request::Column::Status.is_null())
        .count(db)
        .await
        .unwrap_or(0) as i64;

    // Determine upstream status:
    // - Unknown: No recent requests at all
    // - Down: Recent 502 errors (more than 50% of requests, or all recent requests are 502/pending)
    // - Up: Has recent requests with non-502 responses
    let upstream_status = if total_recent_count == 0 {
        UpstreamStatus::Unknown
    } else if recent_502_count > 0 {
        // Check if the most recent request was a 502 or pending
        let most_recent = CapturedRequest::find()
            .filter(localup_relay_db::entities::captured_request::Column::LocalupId.eq(localup_id))
            .filter(localup_relay_db::entities::captured_request::Column::CreatedAt.gt(cutoff))
            .order_by_desc(localup_relay_db::entities::captured_request::Column::CreatedAt)
            .one(db)
            .await
            .ok()
            .flatten();

        if let Some(req) = most_recent {
            match req.status {
                Some(502) => UpstreamStatus::Down,
                None => UpstreamStatus::Down, // Pending request, likely upstream not responding
                _ => {
                    // Recent 502s but most recent succeeded - might be recovering
                    if recent_502_count * 2 > total_recent_count {
                        UpstreamStatus::Down
                    } else {
                        UpstreamStatus::Up
                    }
                }
            }
        } else {
            UpstreamStatus::Unknown
        }
    } else if pending_count > 0 && pending_count == total_recent_count {
        // All requests are pending - upstream might be slow or down
        UpstreamStatus::Down
    } else {
        UpstreamStatus::Up
    };

    (
        upstream_status,
        if total_recent_count > 0 {
            Some(recent_502_count)
        } else {
            None
        },
        if total_recent_count > 0 {
            Some(total_recent_count)
        } else {
            None
        },
    )
}

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
    Extension(auth_user): Extension<AuthUser>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<TunnelList>, (StatusCode, Json<ErrorResponse>)> {
    let include_inactive = query
        .get("include_inactive")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    let is_admin = auth_user.role == "admin";

    // scope=all shows all tunnels (admin only), scope=mine shows only user's tunnels
    // Default: admin sees all, non-admin sees mine
    let scope = query
        .get("scope")
        .map(|s| s.as_str())
        .unwrap_or(if is_admin { "all" } else { "mine" });
    let show_all = is_admin && scope == "all";

    debug!(
        "Listing tunnels (include_inactive={}, user={}, admin={}, scope={})",
        include_inactive, auth_user.user_id, is_admin, scope
    );

    // Get active tunnel IDs (filtered by user unless admin viewing all)
    let active_localup_ids = if show_all {
        state.localup_manager.list_tunnels().await
    } else {
        state
            .localup_manager
            .list_tunnels_for_user(&auth_user.user_id)
            .await
    };
    let mut tunnels = Vec::new();

    // Add active tunnels
    for localup_id in &active_localup_ids {
        if let Some(endpoints) = state.localup_manager.get_endpoints(localup_id).await {
            let (upstream_status, recent_upstream_errors, recent_request_count) =
                compute_upstream_status(&state.db, localup_id).await;

            let tunnel_user_id = state.localup_manager.get_user_id(localup_id).await;

            let tunnel = Tunnel {
                id: localup_id.clone(),
                user_id: tunnel_user_id,
                endpoints: endpoints.iter().map(proto_endpoint_to_api).collect(),
                status: TunnelStatus::Connected,
                upstream_status,
                region: "us-east-1".to_string(), // TODO: Get from config
                connected_at: chrono::Utc::now(), // TODO: Track actual connection time
                local_addr: None,                // Client-side information
                recent_upstream_errors,
                recent_request_count,
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
                user_id: None,     // Unknown for historical tunnels
                endpoints: vec![], // No endpoints for inactive tunnels
                status: TunnelStatus::Disconnected,
                upstream_status: UpstreamStatus::Unknown, // Disconnected tunnels have unknown upstream
                region: "unknown".to_string(),
                connected_at: chrono::Utc::now(), // Use current time as placeholder
                local_addr: None,
                recent_upstream_errors: None,
                recent_request_count: None,
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
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<Json<Tunnel>, (StatusCode, Json<ErrorResponse>)> {
    debug!("Getting tunnel: {} (user: {})", id, auth_user.user_id);

    // First, check if tunnel is active in memory
    if let Some(endpoints) = state.localup_manager.get_endpoints(&id).await {
        // Check ownership (non-admin can only see their own tunnels)
        let tunnel_user_id = state.localup_manager.get_user_id(&id).await;
        if auth_user.role != "admin" && tunnel_user_id.as_deref() != Some(&auth_user.user_id) {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Tunnel '{}' not found", id),
                    code: Some("TUNNEL_NOT_FOUND".to_string()),
                }),
            ));
        }

        let (upstream_status, recent_upstream_errors, recent_request_count) =
            compute_upstream_status(&state.db, &id).await;

        let tunnel = Tunnel {
            id: id.clone(),
            user_id: tunnel_user_id,
            endpoints: endpoints.iter().map(proto_endpoint_to_api).collect(),
            status: TunnelStatus::Connected,
            upstream_status,
            region: "us-east-1".to_string(),
            connected_at: chrono::Utc::now(),
            local_addr: None,
            recent_upstream_errors,
            recent_request_count,
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
            user_id: None, // Unknown for historical tunnels
            endpoints,
            status: TunnelStatus::Disconnected,
            upstream_status: UpstreamStatus::Unknown, // Disconnected tunnels have unknown upstream
            region: "unknown".to_string(),
            connected_at: chrono::Utc::now(), // TODO: Get actual connected_at from DB
            local_addr: None,
            recent_upstream_errors: None,
            recent_request_count: None,
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
    Extension(auth_user): Extension<AuthUser>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    info!("Deleting tunnel: {} (user: {})", id, auth_user.user_id);

    // Check ownership (non-admin can only delete their own tunnels)
    if auth_user.role != "admin" {
        let tunnel_user_id = state.localup_manager.get_user_id(&id).await;
        if tunnel_user_id.as_deref() != Some(&auth_user.user_id) {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Tunnel '{}' not found", id),
                    code: Some("TUNNEL_NOT_FOUND".to_string()),
                }),
            ));
        }
    }

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
    Extension(auth_user): Extension<AuthUser>,
    Query(query): Query<crate::models::CapturedRequestQuery>,
) -> Result<Json<CapturedRequestList>, (StatusCode, Json<ErrorResponse>)> {
    debug!(
        "Listing captured requests with filters: {:?} (user: {}, admin: {})",
        query,
        auth_user.user_id,
        auth_user.role == "admin"
    );

    use localup_relay_db::entities::captured_request::Column;
    use localup_relay_db::entities::prelude::*;
    use sea_orm::{ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};

    // Build query with filters
    let mut query_builder = CapturedRequest::find();

    // Apply filters
    let mut condition = Condition::all();

    // Scope filtering: non-admin always sees own tunnels only;
    // admin sees all by default, or own tunnels if scope=mine
    let is_admin = auth_user.role == "admin";
    let show_all = is_admin && query.scope.as_deref() != Some("mine");

    if !show_all {
        let user_tunnel_ids = state
            .localup_manager
            .list_tunnels_for_user(&auth_user.user_id)
            .await;
        if user_tunnel_ids.is_empty() {
            // User has no tunnels — return empty
            return Ok(Json(CapturedRequestList {
                requests: vec![],
                total: 0,
                offset: query.offset.unwrap_or(0),
                limit: query.limit.unwrap_or(100).min(1000),
            }));
        }
        condition = condition.add(Column::LocalupId.is_in(user_tunnel_ids));
    }

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
    Extension(auth_user): Extension<AuthUser>,
    Query(query): Query<crate::models::CapturedTcpConnectionQuery>,
) -> Result<Json<crate::models::CapturedTcpConnectionList>, (StatusCode, Json<ErrorResponse>)> {
    debug!(
        "Listing TCP connections with filters: {:?} (user: {})",
        query, auth_user.user_id
    );

    use localup_relay_db::entities::captured_tcp_connection::Column;
    use localup_relay_db::entities::prelude::*;
    use sea_orm::{ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder};

    // Build query with filters
    let mut query_builder = CapturedTcpConnection::find();
    let mut condition = Condition::all();

    // Scope filtering: non-admin always sees own tunnels only;
    // admin sees all by default, or own tunnels if scope=mine
    let is_admin = auth_user.role == "admin";
    let show_all = is_admin && query.scope.as_deref() != Some("mine");

    if !show_all {
        let user_tunnel_ids = state
            .localup_manager
            .list_tunnels_for_user(&auth_user.user_id)
            .await;
        if user_tunnel_ids.is_empty() {
            return Ok(Json(crate::models::CapturedTcpConnectionList {
                connections: vec![],
                total: 0,
                offset: query.offset.unwrap_or(0),
                limit: query.limit.unwrap_or(100).min(1000),
            }));
        }
        condition = condition.add(Column::LocalupId.is_in(user_tunnel_ids));
    }

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

    // Convert PEM content to strings for database storage
    let cert_pem_str = String::from_utf8_lossy(&cert_pem).to_string();
    let key_pem_str = String::from_utf8_lossy(&key_pem).to_string();

    // Detect if this is a wildcard domain
    let is_wildcard = WildcardPattern::is_wildcard_pattern(&req.domain);

    // Save to database (including PEM content for direct loading)
    let domain_model = custom_domain::ActiveModel {
        domain: Set(req.domain.clone()),
        id: Set(Some(uuid::Uuid::new_v4().to_string())),
        cert_path: Set(Some(cert_path.to_string_lossy().to_string())),
        key_path: Set(Some(key_path.to_string_lossy().to_string())),
        status: Set(localup_relay_db::entities::custom_domain::DomainStatus::Active),
        provisioned_at: Set(Utc::now()),
        expires_at: Set(Some(expires_at)),
        auto_renew: Set(req.auto_renew),
        error_message: Set(None),
        cert_pem: Set(Some(cert_pem_str)),
        key_pem: Set(Some(key_pem_str)),
        is_wildcard: Set(is_wildcard),
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
    let domains = domains.into_iter().map(db_domain_to_api).collect();

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

    Ok(Json(db_domain_to_api(domain_model)))
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
        (status = 503, description = "ACME not configured", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn initiate_challenge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InitiateChallengeRequest>,
) -> Result<Json<InitiateChallengeResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!(
        "Initiating {} challenge for domain: {}",
        req.challenge_type, req.domain
    );

    // Check if we have an ACME client configured
    let acme_client = state.acme_client.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ACME client not configured. Use --acme-email flag or set ACME_EMAIL environment variable to enable Let's Encrypt.".to_string(),
                code: Some("ACME_NOT_CONFIGURED".to_string()),
            }),
        )
    })?;

    // Determine challenge type
    let challenge_type = if req.challenge_type == "dns-01" {
        localup_cert::AcmeChallengeType::Dns01
    } else {
        localup_cert::AcmeChallengeType::Http01
    };

    // Initiate the ACME order
    let client = acme_client.read().await;
    let challenge_state = client
        .initiate_order(&req.domain, challenge_type)
        .await
        .map_err(|e| {
            error!("ACME order initiation failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Failed to initiate certificate request: {}", e),
                    code: Some("ACME_INIT_FAILED".to_string()),
                }),
            )
        })?;
    drop(client);

    // Store challenge for HTTP-01 serving
    if let Some(ref http01) = challenge_state.http01 {
        let mut challenges = state.acme_challenges.write().await;
        challenges.insert(http01.token.clone(), http01.key_authorization.clone());
        info!("Stored ACME HTTP-01 challenge for token: {}", http01.token);
    }

    // Build response and database model based on challenge type
    let (challenge, token_or_record_name, key_auth_or_record_value, db_challenge_type) =
        match challenge_type {
            localup_cert::AcmeChallengeType::Http01 => {
                let http01 = challenge_state.http01.ok_or_else(|| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "HTTP-01 challenge data not available".to_string(),
                            code: Some("CHALLENGE_DATA_MISSING".to_string()),
                        }),
                    )
                })?;
                let challenge_info = ChallengeInfo::Http01 {
                    domain: req.domain.clone(),
                    token: http01.token.clone(),
                    key_authorization: http01.key_authorization.clone(),
                    file_path: format!("http://{}{}", req.domain, http01.url_path),
                    instructions: vec![
                        "1. Ensure your domain DNS points to this server".to_string(),
                        "2. The challenge response is automatically served by this relay"
                            .to_string(),
                        format!("3. Verify URL: http://{}{}", req.domain, http01.url_path),
                        "4. Call POST /api/domains/challenge/complete to complete verification"
                            .to_string(),
                    ],
                };
                (
                    challenge_info,
                    Some(http01.token.clone()),
                    Some(http01.key_authorization.clone()),
                    localup_relay_db::entities::domain_challenge::ChallengeType::Http01,
                )
            }
            localup_cert::AcmeChallengeType::Dns01 => {
                let dns01 = challenge_state.dns01.ok_or_else(|| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse {
                            error: "DNS-01 challenge data not available".to_string(),
                            code: Some("CHALLENGE_DATA_MISSING".to_string()),
                        }),
                    )
                })?;
                let challenge_info = ChallengeInfo::Dns01 {
                    domain: req.domain.clone(),
                    record_name: dns01.record_name.clone(),
                    record_value: dns01.record_value.clone(),
                    instructions: vec![
                        "1. Add a TXT record to your DNS:".to_string(),
                        format!("   Name: {}", dns01.record_name),
                        format!("   Type: {}", dns01.record_type),
                        format!("   Value: {}", dns01.record_value),
                        "2. Wait for DNS propagation (can take up to 48 hours)".to_string(),
                        "3. Call POST /api/domains/challenge/complete with the challenge_id"
                            .to_string(),
                    ],
                };
                (
                    challenge_info,
                    Some(dns01.record_name.clone()),
                    Some(dns01.record_value.clone()),
                    localup_relay_db::entities::domain_challenge::ChallengeType::Dns01,
                )
            }
        };

    // Persist custom domain with pending status
    use localup_relay_db::entities::{custom_domain, domain_challenge};
    use sea_orm::sea_query::OnConflict;
    use sea_orm::ActiveValue::Set;

    let domain_id = uuid::Uuid::new_v4().to_string();
    // Detect if this is a wildcard domain
    let is_wildcard = WildcardPattern::is_wildcard_pattern(&req.domain);
    let domain_model = custom_domain::ActiveModel {
        domain: Set(req.domain.clone()),
        id: Set(Some(domain_id)),
        cert_path: Set(None),
        key_path: Set(None),
        status: Set(custom_domain::DomainStatus::Pending),
        provisioned_at: Set(chrono::Utc::now()),
        expires_at: Set(Some(challenge_state.expires_at)),
        auto_renew: Set(true),
        error_message: Set(None),
        cert_pem: Set(None),
        key_pem: Set(None),
        is_wildcard: Set(is_wildcard),
    };

    use sea_orm::EntityTrait;
    // Insert or update custom domain to pending status
    custom_domain::Entity::insert(domain_model)
        .on_conflict(
            OnConflict::column(custom_domain::Column::Domain)
                .update_columns([
                    custom_domain::Column::Status,
                    custom_domain::Column::ExpiresAt,
                    custom_domain::Column::ErrorMessage,
                ])
                .to_owned(),
        )
        .exec(&state.db)
        .await
        .map_err(|e| {
            error!("Failed to persist custom domain: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to save domain: {}", e),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;

    // Persist challenge details
    let challenge_model = domain_challenge::ActiveModel {
        id: Set(challenge_state.order_id.clone()),
        domain: Set(req.domain.clone()),
        challenge_type: Set(db_challenge_type),
        status: Set(domain_challenge::ChallengeStatus::Pending),
        token_or_record_name: Set(token_or_record_name),
        key_auth_or_record_value: Set(key_auth_or_record_value),
        order_url: Set(Some(challenge_state.order_id.clone())),
        created_at: Set(chrono::Utc::now()),
        expires_at: Set(challenge_state.expires_at),
        error_message: Set(None),
    };

    domain_challenge::Entity::insert(challenge_model)
        .exec(&state.db)
        .await
        .map_err(|e| {
            error!("Failed to persist challenge: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to save challenge: {}", e),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;

    info!(
        "Domain {} and challenge {} persisted to database",
        req.domain, challenge_state.order_id
    );

    Ok(Json(InitiateChallengeResponse {
        domain: req.domain,
        challenge,
        challenge_id: challenge_state.order_id,
        expires_at: challenge_state.expires_at,
    }))
}

/// Complete/verify ACME challenge
#[utoipa::path(
    post,
    path = "/api/domains/challenge/complete",
    request_body = CompleteChallengeRequest,
    responses(
        (status = 200, description = "Challenge completed, certificate issued", body = UploadCustomDomainResponse),
        (status = 503, description = "ACME not configured", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn complete_challenge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompleteChallengeRequest>,
) -> Result<Json<UploadCustomDomainResponse>, (StatusCode, Json<ErrorResponse>)> {
    info!(
        "Completing ACME challenge {} for domain: {}",
        req.challenge_id, req.domain
    );

    // Check if we have an ACME client configured
    let acme_client = state.acme_client.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "ACME client not configured. Use --acme-email flag or set ACME_EMAIL environment variable to enable Let's Encrypt.".to_string(),
                code: Some("ACME_NOT_CONFIGURED".to_string()),
            }),
        )
    })?;

    // Complete the ACME order using the challenge_id (order_id)
    let client = acme_client.read().await;
    let _cert = client
        .complete_order(&req.challenge_id)
        .await
        .map_err(|e| {
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

    // Read certificate content from files for database storage
    let cert_pem_str = tokio::fs::read_to_string(&cert_path).await.map_err(|e| {
        error!("Failed to read certificate file: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to read certificate: {}", e),
                code: Some("CERT_READ_FAILED".to_string()),
            }),
        )
    })?;

    let key_pem_str = tokio::fs::read_to_string(&key_path).await.map_err(|e| {
        error!("Failed to read key file: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to read key: {}", e),
                code: Some("KEY_READ_FAILED".to_string()),
            }),
        )
    })?;

    // Calculate expiration (Let's Encrypt certs are valid for 90 days)
    let expires_at = Utc::now() + chrono::Duration::days(90);

    // Save to database (keep existing id if updating, include PEM content)
    use sea_orm::ActiveValue::NotSet;
    // Detect if this is a wildcard domain
    let is_wildcard = WildcardPattern::is_wildcard_pattern(&req.domain);
    let domain_model = custom_domain::ActiveModel {
        domain: Set(req.domain.clone()),
        id: NotSet, // Preserve existing ID on update
        cert_path: Set(Some(cert_path)),
        key_path: Set(Some(key_path)),
        status: Set(localup_relay_db::entities::custom_domain::DomainStatus::Active),
        provisioned_at: Set(Utc::now()),
        expires_at: Set(Some(expires_at)),
        auto_renew: Set(true),
        error_message: Set(None),
        cert_pem: Set(Some(cert_pem_str)),
        key_pem: Set(Some(key_pem_str)),
        is_wildcard: Set(is_wildcard),
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
                    custom_domain::Column::CertPem,
                    custom_domain::Column::KeyPem,
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

    // Update challenge status in database to completed
    use localup_relay_db::entities::domain_challenge;
    if let Ok(Some(challenge)) = domain_challenge::Entity::find_by_id(&req.challenge_id)
        .one(&state.db)
        .await
    {
        let mut challenge_active: domain_challenge::ActiveModel = challenge.into();
        challenge_active.status = Set(domain_challenge::ChallengeStatus::Completed);
        let _ = challenge_active.update(&state.db).await;
    }

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

/// Get pending challenges for a domain
///
/// Returns any pending ACME challenges for the specified domain.
#[utoipa::path(
    get,
    path = "/api/domains/{domain}/challenges",
    params(
        ("domain" = String, Path, description = "Domain name")
    ),
    responses(
        (status = 200, description = "List of pending challenges", body = Vec<InitiateChallengeResponse>),
        (status = 404, description = "No challenges found")
    ),
    tag = "domains"
)]
pub async fn get_domain_challenges(
    State(state): State<Arc<AppState>>,
    Path(domain): Path<String>,
) -> Result<Json<Vec<InitiateChallengeResponse>>, (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::domain_challenge;
    use sea_orm::{ColumnTrait, QueryFilter};

    let challenges = domain_challenge::Entity::find()
        .filter(domain_challenge::Column::Domain.eq(&domain))
        .filter(domain_challenge::Column::Status.eq(domain_challenge::ChallengeStatus::Pending))
        .all(&state.db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;

    let responses: Vec<InitiateChallengeResponse> = challenges
        .into_iter()
        .map(|c| {
            let challenge = match c.challenge_type {
                domain_challenge::ChallengeType::Http01 => ChallengeInfo::Http01 {
                    domain: c.domain.clone(),
                    token: c.token_or_record_name.clone().unwrap_or_default(),
                    key_authorization: c.key_auth_or_record_value.clone().unwrap_or_default(),
                    file_path: format!(
                        "http://{}/.well-known/acme-challenge/{}",
                        c.domain,
                        c.token_or_record_name.as_deref().unwrap_or("")
                    ),
                    instructions: vec![
                        "1. Ensure your domain DNS points to this server".to_string(),
                        "2. The challenge response is automatically served by this relay"
                            .to_string(),
                    ],
                },
                domain_challenge::ChallengeType::Dns01 => ChallengeInfo::Dns01 {
                    domain: c.domain.clone(),
                    record_name: c.token_or_record_name.clone().unwrap_or_default(),
                    record_value: c.key_auth_or_record_value.clone().unwrap_or_default(),
                    instructions: vec![
                        "1. Add a TXT record to your DNS".to_string(),
                        "2. Wait for DNS propagation".to_string(),
                    ],
                },
            };

            InitiateChallengeResponse {
                domain: c.domain,
                challenge,
                challenge_id: c.id,
                expires_at: c.expires_at,
            }
        })
        .collect();

    Ok(Json(responses))
}

/// Get a custom domain by ID
#[utoipa::path(
    get,
    path = "/api/domains/by-id/{id}",
    params(
        ("id" = String, Path, description = "Domain ID")
    ),
    responses(
        (status = 200, description = "Domain found", body = CustomDomain),
        (status = 404, description = "Domain not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn get_domain_by_id(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<crate::models::CustomDomain>, (StatusCode, Json<ErrorResponse>)> {
    let domain_model = custom_domain::Entity::find()
        .filter(custom_domain::Column::Id.eq(&id))
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                    code: Some("DB_QUERY_FAILED".to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Domain not found with ID: {}", id),
                    code: Some("DOMAIN_NOT_FOUND".to_string()),
                }),
            )
        })?;

    Ok(Json(db_domain_to_api(domain_model)))
}

/// Get certificate details for a domain
#[utoipa::path(
    get,
    path = "/api/domains/{domain}/certificate-details",
    params(
        ("domain" = String, Path, description = "Domain name")
    ),
    responses(
        (status = 200, description = "Certificate details", body = CertificateDetails),
        (status = 404, description = "Domain or certificate not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn get_certificate_details(
    State(state): State<Arc<AppState>>,
    Path(domain): Path<String>,
) -> Result<Json<CertificateDetails>, (StatusCode, Json<ErrorResponse>)> {
    use x509_parser::prelude::*;

    // Find the domain
    let domain_model = custom_domain::Entity::find()
        .filter(custom_domain::Column::Domain.eq(&domain))
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
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

    // Get cert path
    let cert_path = domain_model.cert_path.ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Certificate not found for this domain".to_string(),
                code: Some("CERT_NOT_FOUND".to_string()),
            }),
        )
    })?;

    // Read certificate file
    let cert_pem = tokio::fs::read_to_string(&cert_path).await.map_err(|e| {
        error!("Failed to read certificate file: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: format!("Failed to read certificate: {}", e),
                code: Some("CERT_READ_FAILED".to_string()),
            }),
        )
    })?;

    // Parse PEM
    let (_, pem) = parse_x509_pem(cert_pem.as_bytes()).map_err(|e| {
        error!("Failed to parse PEM: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to parse certificate PEM".to_string(),
                code: Some("CERT_PARSE_FAILED".to_string()),
            }),
        )
    })?;

    // Parse X.509 certificate
    let (_, cert) = X509Certificate::from_der(&pem.contents).map_err(|e| {
        error!("Failed to parse X.509: {:?}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to parse X.509 certificate".to_string(),
                code: Some("CERT_PARSE_FAILED".to_string()),
            }),
        )
    })?;

    // Extract subject
    let subject = cert.subject().to_string();

    // Extract issuer
    let issuer = cert.issuer().to_string();

    // Extract serial number
    let serial_number = cert
        .serial
        .to_bytes_be()
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(":");

    // Extract validity dates
    let not_before =
        chrono::DateTime::<chrono::Utc>::from_timestamp(cert.validity().not_before.timestamp(), 0)
            .unwrap_or_default();

    let not_after =
        chrono::DateTime::<chrono::Utc>::from_timestamp(cert.validity().not_after.timestamp(), 0)
            .unwrap_or_default();

    // Extract SANs
    let san = cert
        .subject_alternative_name()
        .ok()
        .flatten()
        .map(|san| {
            san.value
                .general_names
                .iter()
                .filter_map(|name| match name {
                    x509_parser::extensions::GeneralName::DNSName(dns) => Some(dns.to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    // Extract signature algorithm
    let signature_algorithm = cert.signature_algorithm.algorithm.to_string();

    // Extract public key algorithm
    let public_key_algorithm = cert.public_key().algorithm.algorithm.to_string();

    // Calculate SHA-256 fingerprint
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&pem.contents);
    let fingerprint = hasher.finalize();
    let fingerprint_sha256 = fingerprint
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(":");

    Ok(Json(CertificateDetails {
        domain,
        subject,
        issuer,
        serial_number,
        not_before,
        not_after,
        san,
        signature_algorithm,
        public_key_algorithm,
        fingerprint_sha256,
        pem: cert_pem,
    }))
}

/// Cancel a pending ACME challenge
#[utoipa::path(
    post,
    path = "/api/domains/{domain}/challenge/cancel",
    params(
        ("domain" = String, Path, description = "Domain name")
    ),
    responses(
        (status = 200, description = "Challenge cancelled"),
        (status = 404, description = "No pending challenge found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn cancel_challenge(
    State(state): State<Arc<AppState>>,
    Path(domain): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::domain_challenge;

    // Find pending challenges for this domain
    let challenges = domain_challenge::Entity::find()
        .filter(domain_challenge::Column::Domain.eq(&domain))
        .filter(domain_challenge::Column::Status.eq(domain_challenge::ChallengeStatus::Pending))
        .all(&state.db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;

    if challenges.is_empty() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("No pending challenges found for domain: {}", domain),
                code: Some("NO_PENDING_CHALLENGE".to_string()),
            }),
        ));
    }

    // Delete all pending challenges for this domain
    for challenge in &challenges {
        // Remove from in-memory challenge store if HTTP-01
        if challenge.challenge_type == domain_challenge::ChallengeType::Http01 {
            if let Some(token) = &challenge.token_or_record_name {
                let mut acme_challenges = state.acme_challenges.write().await;
                acme_challenges.remove(token);
            }
        }
    }

    // Delete from database
    domain_challenge::Entity::delete_many()
        .filter(domain_challenge::Column::Domain.eq(&domain))
        .filter(domain_challenge::Column::Status.eq(domain_challenge::ChallengeStatus::Pending))
        .exec(&state.db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Failed to delete challenges: {}", e),
                    code: Some("DB_DELETE_FAILED".to_string()),
                }),
            )
        })?;

    // Update domain status to failed
    use sea_orm::ActiveValue::NotSet;
    let domain_model = custom_domain::ActiveModel {
        domain: Set(domain.clone()),
        id: NotSet,
        cert_path: NotSet,
        key_path: NotSet,
        status: Set(localup_relay_db::entities::custom_domain::DomainStatus::Failed),
        provisioned_at: NotSet,
        expires_at: NotSet,
        auto_renew: NotSet,
        error_message: Set(Some("Challenge cancelled by user".to_string())),
        cert_pem: NotSet,
        key_pem: NotSet,
        is_wildcard: NotSet, // Preserve existing value
    };
    domain_model.update(&state.db).await.ok();

    info!(
        "Cancelled {} challenge(s) for domain: {}",
        challenges.len(),
        domain
    );

    Ok(Json(serde_json::json!({
        "message": format!("Cancelled {} challenge(s) for domain {}", challenges.len(), domain),
        "cancelled_count": challenges.len()
    })))
}

/// Restart ACME challenge for a domain (cancel existing and start new)
#[utoipa::path(
    post,
    path = "/api/domains/{domain}/challenge/restart",
    params(
        ("domain" = String, Path, description = "Domain name")
    ),
    request_body = InitiateChallengeRequest,
    responses(
        (status = 200, description = "New challenge initiated", body = InitiateChallengeResponse),
        (status = 503, description = "ACME not configured", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn restart_challenge(
    State(state): State<Arc<AppState>>,
    Path(domain): Path<String>,
    Json(req): Json<InitiateChallengeRequest>,
) -> Result<Json<InitiateChallengeResponse>, (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::domain_challenge;

    info!("Restarting challenge for domain: {}", domain);

    // First, cancel any existing challenges
    let existing_challenges = domain_challenge::Entity::find()
        .filter(domain_challenge::Column::Domain.eq(&domain))
        .filter(domain_challenge::Column::Status.eq(domain_challenge::ChallengeStatus::Pending))
        .all(&state.db)
        .await
        .unwrap_or_default();

    // Remove from in-memory store
    for challenge in &existing_challenges {
        if challenge.challenge_type == domain_challenge::ChallengeType::Http01 {
            if let Some(token) = &challenge.token_or_record_name {
                let mut acme_challenges = state.acme_challenges.write().await;
                acme_challenges.remove(token);
            }
        }
    }

    // Delete existing pending challenges
    domain_challenge::Entity::delete_many()
        .filter(domain_challenge::Column::Domain.eq(&domain))
        .filter(domain_challenge::Column::Status.eq(domain_challenge::ChallengeStatus::Pending))
        .exec(&state.db)
        .await
        .ok();

    // Now initiate a new challenge using the existing initiate_challenge logic
    let new_req = InitiateChallengeRequest {
        domain: domain.clone(),
        challenge_type: req.challenge_type,
    };

    // Call the existing initiate_challenge handler
    initiate_challenge(State(state), Json(new_req)).await
}

/// Pre-validate a challenge before submitting to Let's Encrypt
///
/// This endpoint simulates what Let's Encrypt will do when validating your challenge:
/// - For HTTP-01: Makes an HTTP request to http://{domain}/.well-known/acme-challenge/{token}
/// - For DNS-01: Performs a DNS TXT lookup for _acme-challenge.{domain}
///
/// Use this to verify your setup is correct before submitting the challenge.
#[utoipa::path(
    post,
    path = "/api/domains/challenge/pre-validate",
    request_body = PreValidateChallengeRequest,
    responses(
        (status = 200, description = "Pre-validation result", body = PreValidateChallengeResponse),
        (status = 404, description = "Challenge not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "domains"
)]
pub async fn pre_validate_challenge(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PreValidateChallengeRequest>,
) -> Result<Json<PreValidateChallengeResponse>, (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::domain_challenge;

    info!(
        "Pre-validating challenge {} for domain: {}",
        req.challenge_id, req.domain
    );

    // Find the challenge
    let challenge = domain_challenge::Entity::find_by_id(&req.challenge_id)
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: format!("Database error: {}", e),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: format!("Challenge not found: {}", req.challenge_id),
                    code: Some("CHALLENGE_NOT_FOUND".to_string()),
                }),
            )
        })?;

    // Validate that the challenge belongs to the requested domain
    if challenge.domain != req.domain {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Challenge does not belong to the specified domain".to_string(),
                code: Some("DOMAIN_MISMATCH".to_string()),
            }),
        ));
    }

    // Perform the appropriate validation based on challenge type
    let response = match challenge.challenge_type {
        domain_challenge::ChallengeType::Http01 => {
            pre_validate_http01_challenge(&req.domain, &challenge).await
        }
        domain_challenge::ChallengeType::Dns01 => {
            pre_validate_dns01_challenge(&req.domain, &challenge).await
        }
    };

    Ok(Json(response))
}

/// Pre-validate HTTP-01 challenge by making HTTP request like Let's Encrypt does
async fn pre_validate_http01_challenge(
    domain: &str,
    challenge: &localup_relay_db::entities::domain_challenge::Model,
) -> PreValidateChallengeResponse {
    let token = challenge.token_or_record_name.as_deref().unwrap_or("");
    let expected_key_auth = challenge.key_auth_or_record_value.as_deref().unwrap_or("");

    let check_url = format!("http://{}/.well-known/acme-challenge/{}", domain, token);

    info!("Pre-validating HTTP-01 challenge at: {}", check_url);

    // Create HTTP client with timeout (Let's Encrypt uses about 10 seconds)
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return PreValidateChallengeResponse {
                ready: false,
                challenge_type: "http-01".to_string(),
                checked: check_url,
                expected: expected_key_auth.to_string(),
                found: None,
                error: Some(format!("Failed to create HTTP client: {}", e)),
                details: None,
            };
        }
    };

    // Make the request
    match client.get(&check_url).send().await {
        Ok(response) => {
            let status = response.status();
            if !status.is_success() {
                return PreValidateChallengeResponse {
                    ready: false,
                    challenge_type: "http-01".to_string(),
                    checked: check_url,
                    expected: expected_key_auth.to_string(),
                    found: None,
                    error: Some(format!("HTTP request returned status: {}", status)),
                    details: Some(format!(
                        "Let's Encrypt expects HTTP 200. Make sure your domain {} points to this server and port 80 is accessible.",
                        domain
                    )),
                };
            }

            // Read the response body
            match response.text().await {
                Ok(body) => {
                    let body_trimmed = body.trim();
                    let ready = body_trimmed == expected_key_auth;

                    PreValidateChallengeResponse {
                        ready,
                        challenge_type: "http-01".to_string(),
                        checked: check_url,
                        expected: expected_key_auth.to_string(),
                        found: Some(body_trimmed.to_string()),
                        error: if ready {
                            None
                        } else {
                            Some("Response does not match expected key authorization".to_string())
                        },
                        details: if ready {
                            Some("✅ HTTP-01 challenge is properly configured. You can now submit the challenge to Let's Encrypt.".to_string())
                        } else {
                            Some("The server returned a response, but it doesn't match the expected key authorization. Make sure the challenge token is being served correctly.".to_string())
                        },
                    }
                }
                Err(e) => PreValidateChallengeResponse {
                    ready: false,
                    challenge_type: "http-01".to_string(),
                    checked: check_url,
                    expected: expected_key_auth.to_string(),
                    found: None,
                    error: Some(format!("Failed to read response body: {}", e)),
                    details: None,
                },
            }
        }
        Err(e) => {
            let error_details = if e.is_connect() {
                format!(
                    "Connection failed. Make sure:\n1. Domain {} has DNS pointing to this server\n2. Port 80 is open and accessible\n3. No firewall blocking HTTP traffic",
                    domain
                )
            } else if e.is_timeout() {
                format!(
                    "Request timed out. Make sure:\n1. The server is responding on port 80\n2. Network latency is not too high\n3. Domain {} resolves correctly",
                    domain
                )
            } else {
                format!("HTTP request failed: {}", e)
            };

            PreValidateChallengeResponse {
                ready: false,
                challenge_type: "http-01".to_string(),
                checked: check_url,
                expected: expected_key_auth.to_string(),
                found: None,
                error: Some(format!("HTTP request failed: {}", e)),
                details: Some(error_details),
            }
        }
    }
}

/// Pre-validate DNS-01 challenge by performing DNS TXT lookup like Let's Encrypt does
async fn pre_validate_dns01_challenge(
    domain: &str,
    challenge: &localup_relay_db::entities::domain_challenge::Model,
) -> PreValidateChallengeResponse {
    let default_record_name = format!("_acme-challenge.{}", domain);
    let record_name = challenge
        .token_or_record_name
        .as_deref()
        .unwrap_or(&default_record_name);
    let expected_value = challenge.key_auth_or_record_value.as_deref().unwrap_or("");

    info!("Pre-validating DNS-01 challenge for: {}", record_name);

    // Create DNS resolver
    let resolver = match hickory_resolver::TokioAsyncResolver::tokio_from_system_conf() {
        Ok(r) => r,
        Err(e) => {
            return PreValidateChallengeResponse {
                ready: false,
                challenge_type: "dns-01".to_string(),
                checked: format!("TXT {}", record_name),
                expected: expected_value.to_string(),
                found: None,
                error: Some(format!("Failed to create DNS resolver: {}", e)),
                details: None,
            };
        }
    };

    // Perform TXT lookup
    match resolver.txt_lookup(record_name).await {
        Ok(response) => {
            let txt_records: Vec<String> = response
                .iter()
                .map(|txt| {
                    txt.iter()
                        .map(|data| String::from_utf8_lossy(data).to_string())
                        .collect::<Vec<_>>()
                        .join("")
                })
                .collect();

            let found_str = txt_records.join(", ");
            let ready = txt_records.iter().any(|r| r == expected_value);

            PreValidateChallengeResponse {
                ready,
                challenge_type: "dns-01".to_string(),
                checked: format!("TXT {}", record_name),
                expected: expected_value.to_string(),
                found: Some(found_str.clone()),
                error: if ready {
                    None
                } else if txt_records.is_empty() {
                    Some("No TXT records found".to_string())
                } else {
                    Some("TXT record value does not match expected value".to_string())
                },
                details: if ready {
                    Some("✅ DNS-01 challenge is properly configured. You can now submit the challenge to Let's Encrypt.".to_string())
                } else if txt_records.is_empty() {
                    Some(format!(
                        "No TXT records found for {}. Add the TXT record and wait for DNS propagation (can take minutes to hours).",
                        record_name
                    ))
                } else {
                    Some(format!(
                        "Found TXT record(s) but value doesn't match. Found: [{}]. Expected: {}",
                        found_str, expected_value
                    ))
                },
            }
        }
        Err(e) => {
            let error_str = e.to_string();
            let is_nxdomain = error_str.contains("no records found")
                || error_str.contains("NXDOMAIN")
                || error_str.contains("NoRecordsFound");

            PreValidateChallengeResponse {
                ready: false,
                challenge_type: "dns-01".to_string(),
                checked: format!("TXT {}", record_name),
                expected: expected_value.to_string(),
                found: None,
                error: Some(format!("DNS lookup failed: {}", e)),
                details: Some(if is_nxdomain {
                    format!(
                        "No TXT record found for {}. Please add the following TXT record to your DNS:\n\nName: {}\nType: TXT\nValue: {}\n\nNote: DNS propagation can take minutes to hours.",
                        record_name, record_name, expected_value
                    )
                } else {
                    "DNS lookup failed. This could be due to:\n1. DNS propagation not complete\n2. Network issues\n3. Invalid domain name\n\nTry again in a few minutes.".to_string()
                }),
            }
        }
    }
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
                error: "ACME client not configured. Use --acme-email flag or set ACME_EMAIL environment variable to enable Let's Encrypt.".to_string(),
                code: Some("ACME_NOT_CONFIGURED".to_string()),
            }),
        )
    })?;

    // Initiate the ACME order with HTTP-01 challenge (default for this endpoint)
    let client = acme_client.read().await;
    let challenge_state = client
        .initiate_order(&domain, localup_cert::AcmeChallengeType::Http01)
        .await
        .map_err(|e| {
            error!("ACME order initiation failed: {}", e);
            (
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Failed to initiate certificate request: {}", e),
                    code: Some("ACME_INIT_FAILED".to_string()),
                }),
            )
        })?;
    drop(client);

    // Get HTTP-01 challenge details
    let http01 = challenge_state.http01.ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "HTTP-01 challenge data not available".to_string(),
                code: Some("CHALLENGE_DATA_MISSING".to_string()),
            }),
        )
    })?;

    // Store the challenge for serving
    {
        let mut challenges = state.acme_challenges.write().await;
        challenges.insert(http01.token.clone(), http01.key_authorization.clone());
        info!("Stored ACME challenge for token: {}", http01.token);
    }

    // Create the challenge info for response
    let challenge = ChallengeInfo::Http01 {
        domain: domain.clone(),
        token: http01.token.clone(),
        key_authorization: http01.key_authorization.clone(),
        file_path: format!("http://{}{}", domain, http01.url_path),
        instructions: vec![
            "1. Ensure your domain DNS points to this server".to_string(),
            "2. The challenge response is automatically served at the URL above".to_string(),
            "3. Call POST /api/domains/challenge/complete to complete the verification".to_string(),
        ],
    };

    Ok(Json(InitiateChallengeResponse {
        domain,
        challenge,
        challenge_id: challenge_state.order_id,
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
    let mut social_providers = Vec::new();
    if state.social_auth.google.is_some() {
        social_providers.push(crate::models::SocialAuthProvider {
            id: "google".to_string(),
            name: "Google".to_string(),
        });
    }
    if state.social_auth.github.is_some() {
        social_providers.push(crate::models::SocialAuthProvider {
            id: "github".to_string(),
            name: "GitHub".to_string(),
        });
    }

    Json(crate::models::AuthConfig {
        signup_enabled: state.allow_signup,
        relay: state.relay_config.clone(),
        social_providers,
        magic_link_enabled: state.smtp.is_some(),
        device_auth_enabled: !state.oauth_clients.is_empty(),
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
        oauth_provider: Set(None),
        oauth_provider_id: Set(None),
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
    let auth_token_jwt = ensure_default_auth_token(&state, user_id).await;

    // Generate session token and cookie
    let (token, expires_at, headers) = generate_session_response(&state, &user, &req_headers)?;

    let response = Json(RegisterResponse {
        user: db_user_to_api(&user),
        token,
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
    ensure_default_auth_token(&state, user.id).await;

    // Generate session token and cookie
    let (token, expires_at, headers) = generate_session_response(&state, &user, &req_headers)?;

    let response = Json(LoginResponse {
        user: db_user_to_api(&user),
        token,
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

    let user_id = parse_user_id(&auth_user.user_id)?;

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
    let user_id = parse_user_id(&auth_user.user_id)?;

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
    let user_id = parse_user_id(&auth_user.user_id)?;

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

    let tokens: Vec<AuthToken> = token_records
        .into_iter()
        .map(db_auth_token_to_api)
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
    let user_id = parse_user_id(&auth_user.user_id)?;

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

    Ok(Json(db_auth_token_to_api(token)))
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
    let user_id = parse_user_id(&auth_user.user_id)?;

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

    Ok(Json(db_auth_token_to_api(updated_token)))
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
    let user_id = parse_user_id(&auth_user.user_id)?;

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

// ============================================================================
// Social Login (OAuth) Endpoints
// ============================================================================

/// OAuth user info returned by providers
#[derive(Debug, serde::Deserialize)]
struct OAuthUserInfo {
    email: String,
    name: Option<String>,
    provider_id: String,
}

/// Google token response
#[derive(Debug, serde::Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
}

/// Google user info response
#[derive(Debug, serde::Deserialize)]
struct GoogleUserInfo {
    #[serde(rename = "sub")]
    id: String,
    email: String,
    name: Option<String>,
    #[allow(dead_code)]
    email_verified: Option<bool>,
}

/// GitHub token response
#[derive(Debug, serde::Deserialize)]
struct GitHubTokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
}

/// GitHub user info response
#[derive(Debug, serde::Deserialize)]
struct GitHubUserInfo {
    id: i64,
    #[allow(dead_code)]
    login: String,
    name: Option<String>,
    email: Option<String>,
}

/// GitHub email response (for users with private emails)
#[derive(Debug, serde::Deserialize)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

/// Get OAuth authorization URL for a provider
///
/// Returns the URL the frontend should redirect the user to for OAuth login.
/// The frontend must provide a `redirect_uri` query parameter that the OAuth
/// provider will redirect back to after authentication.
#[utoipa::path(
    get,
    path = "/api/auth/oauth/{provider}/url",
    params(
        ("provider" = String, Path, description = "OAuth provider (google or github)"),
        ("redirect_uri" = String, Query, description = "Frontend callback URL the provider should redirect to"),
    ),
    responses(
        (status = 200, description = "OAuth authorization URL", body = OAuthUrlResponse),
        (status = 400, description = "Unknown provider or provider not configured", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn oauth_get_url(
    State(state): State<Arc<AppState>>,
    Path(provider): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<crate::models::OAuthUrlResponse>, (StatusCode, Json<ErrorResponse>)> {
    let redirect_uri = params.get("redirect_uri").ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Missing redirect_uri query parameter".to_string(),
                code: Some("MISSING_REDIRECT_URI".to_string()),
            }),
        )
    })?;

    // Generate a random state for CSRF protection
    let state_token = uuid::Uuid::new_v4().to_string();

    let url = match provider.as_str() {
        "google" => {
            let config = state.social_auth.google.as_ref().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "Google OAuth is not configured".to_string(),
                        code: Some("PROVIDER_NOT_CONFIGURED".to_string()),
                    }),
                )
            })?;

            format!(
                "https://accounts.google.com/o/oauth2/v2/auth?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&state={}&access_type=offline",
                urlencoding::encode(&config.client_id),
                urlencoding::encode(redirect_uri),
                urlencoding::encode(&state_token),
            )
        }
        "github" => {
            let config = state.social_auth.github.as_ref().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "GitHub OAuth is not configured".to_string(),
                        code: Some("PROVIDER_NOT_CONFIGURED".to_string()),
                    }),
                )
            })?;

            format!(
                "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope=user:email&state={}",
                urlencoding::encode(&config.client_id),
                urlencoding::encode(redirect_uri),
                urlencoding::encode(&state_token),
            )
        }
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Unknown OAuth provider: {}", provider),
                    code: Some("UNKNOWN_PROVIDER".to_string()),
                }),
            ));
        }
    };

    Ok(Json(crate::models::OAuthUrlResponse {
        url,
        state: state_token,
    }))
}

/// Handle OAuth callback - exchange code for tokens and create/login user
///
/// The frontend posts the authorization code received from the OAuth provider.
/// The backend exchanges it for an access token, fetches user info, and
/// creates or logs in the user.
#[utoipa::path(
    post,
    path = "/api/auth/oauth/{provider}/callback",
    params(
        ("provider" = String, Path, description = "OAuth provider (google or github)"),
    ),
    request_body = OAuthCallbackRequest,
    responses(
        (status = 200, description = "Login successful", body = LoginResponse),
        (status = 201, description = "User registered and logged in", body = RegisterResponse),
        (status = 400, description = "Invalid request or provider not configured", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn oauth_callback(
    State(state): State<Arc<AppState>>,
    req_headers: HeaderMap,
    Path(provider): Path<String>,
    Json(req): Json<crate::models::OAuthCallbackRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Exchange the authorization code for user info based on provider
    let user_info = match provider.as_str() {
        "google" => exchange_google_code(&state, &req.code, &req.redirect_uri).await?,
        "github" => exchange_github_code(&state, &req.code, &req.redirect_uri).await?,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: format!("Unknown OAuth provider: {}", provider),
                    code: Some("UNKNOWN_PROVIDER".to_string()),
                }),
            ));
        }
    };

    // Look up user by OAuth provider + provider ID
    use localup_relay_db::entities::user;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let existing_user = UserEntity::find()
        .filter(user::Column::OauthProvider.eq(&provider))
        .filter(user::Column::OauthProviderId.eq(&user_info.provider_id))
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Database error looking up OAuth user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;

    // Also check if a user with this email already exists (link accounts)
    let existing_email_user = if existing_user.is_none() {
        UserEntity::find()
            .filter(user::Column::Email.eq(&user_info.email))
            .one(&state.db)
            .await
            .map_err(|e| {
                error!("Database error checking email: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Internal server error".to_string(),
                        code: Some("DB_ERROR".to_string()),
                    }),
                )
            })?
    } else {
        None
    };

    let (user_model, is_new_user) = if let Some(user) = existing_user {
        // Existing OAuth user - just log them in
        (user, false)
    } else if let Some(user) = existing_email_user {
        // User exists with same email but different auth method
        // Link the OAuth provider to existing account
        use sea_orm::ActiveModelTrait;
        let mut active_user: user::ActiveModel = user.into();
        active_user.oauth_provider = sea_orm::Set(Some(provider.clone()));
        active_user.oauth_provider_id = sea_orm::Set(Some(user_info.provider_id.clone()));
        active_user.updated_at = sea_orm::Set(chrono::Utc::now());

        let updated = active_user.update(&state.db).await.map_err(|e| {
            error!("Database error linking OAuth: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;
        (updated, false)
    } else {
        // New user - check if signup is allowed
        if !state.allow_signup {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "Public registration is disabled. Please contact your administrator."
                        .to_string(),
                    code: Some("SIGNUP_DISABLED".to_string()),
                }),
            ));
        }

        // Create new user
        let user_id = Uuid::new_v4();
        let now = chrono::Utc::now();

        let new_user = user::ActiveModel {
            id: sea_orm::Set(user_id),
            email: sea_orm::Set(user_info.email.clone()),
            password_hash: sea_orm::Set(String::new()), // No password for OAuth users
            full_name: sea_orm::Set(user_info.name.clone()),
            role: sea_orm::Set(user::UserRole::User),
            is_active: sea_orm::Set(true),
            created_at: sea_orm::Set(now),
            updated_at: sea_orm::Set(now),
            oauth_provider: sea_orm::Set(Some(provider.clone())),
            oauth_provider_id: sea_orm::Set(Some(user_info.provider_id.clone())),
        };

        let user = new_user.insert(&state.db).await.map_err(|e| {
            error!("Database error creating OAuth user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;

        info!(
            "New user registered via {}: {} ({})",
            provider, user.email, user.id
        );

        (user, true)
    };

    // Check if account is active
    if !user_model.is_active {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Account is disabled".to_string(),
                code: Some("ACCOUNT_DISABLED".to_string()),
            }),
        ));
    }

    // Auto-create default auth token if user doesn't have one
    let auth_token_jwt = ensure_default_auth_token(&state, user_model.id).await;

    // Generate session token and cookie
    let (token, expires_at, headers) =
        generate_session_response(&state, &user_model, &req_headers)?;

    if is_new_user {
        let response = Json(crate::models::RegisterResponse {
            user: db_user_to_api(&user_model),
            token,
            expires_at,
            auth_token: auth_token_jwt,
        });

        Ok((StatusCode::CREATED, headers, response).into_response())
    } else {
        let response = Json(crate::models::LoginResponse {
            user: db_user_to_api(&user_model),
            token,
            expires_at,
        });

        Ok((headers, response).into_response())
    }
}

/// Exchange a Google authorization code for user info
async fn exchange_google_code(
    state: &AppState,
    code: &str,
    redirect_uri: &str,
) -> Result<OAuthUserInfo, (StatusCode, Json<ErrorResponse>)> {
    let config = state.social_auth.google.as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Google OAuth is not configured".to_string(),
                code: Some("PROVIDER_NOT_CONFIGURED".to_string()),
            }),
        )
    })?;

    let http_client = reqwest::Client::new();

    // Exchange code for access token
    let token_resp = http_client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code),
            ("client_id", &config.client_id),
            ("client_secret", &config.client_secret),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| {
            error!("Google token exchange error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to exchange authorization code with Google".to_string(),
                    code: Some("OAUTH_EXCHANGE_ERROR".to_string()),
                }),
            )
        })?;

    if !token_resp.status().is_success() {
        let status = token_resp.status();
        let body = token_resp.text().await.unwrap_or_default();
        error!("Google token exchange failed: {} - {}", status, body);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Failed to authenticate with Google. Please try again.".to_string(),
                code: Some("OAUTH_EXCHANGE_FAILED".to_string()),
            }),
        ));
    }

    let token_data: GoogleTokenResponse = token_resp.json().await.map_err(|e| {
        error!("Google token response parse error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Invalid response from Google".to_string(),
                code: Some("OAUTH_PARSE_ERROR".to_string()),
            }),
        )
    })?;

    // Fetch user info
    let user_resp = http_client
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(&token_data.access_token)
        .send()
        .await
        .map_err(|e| {
            error!("Google userinfo error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to fetch user info from Google".to_string(),
                    code: Some("OAUTH_USERINFO_ERROR".to_string()),
                }),
            )
        })?;

    let google_user: GoogleUserInfo = user_resp.json().await.map_err(|e| {
        error!("Google userinfo parse error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Invalid user info response from Google".to_string(),
                code: Some("OAUTH_PARSE_ERROR".to_string()),
            }),
        )
    })?;

    Ok(OAuthUserInfo {
        email: google_user.email,
        name: google_user.name,
        provider_id: google_user.id,
    })
}

/// Exchange a GitHub authorization code for user info
async fn exchange_github_code(
    state: &AppState,
    code: &str,
    redirect_uri: &str,
) -> Result<OAuthUserInfo, (StatusCode, Json<ErrorResponse>)> {
    let config = state.social_auth.github.as_ref().ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "GitHub OAuth is not configured".to_string(),
                code: Some("PROVIDER_NOT_CONFIGURED".to_string()),
            }),
        )
    })?;

    let http_client = reqwest::Client::new();

    // Exchange code for access token
    let token_resp = http_client
        .post("https://github.com/login/oauth/access_token")
        .header("Accept", "application/json")
        .form(&[
            ("code", code),
            ("client_id", &config.client_id as &str),
            ("client_secret", &config.client_secret as &str),
            ("redirect_uri", redirect_uri),
        ])
        .send()
        .await
        .map_err(|e| {
            error!("GitHub token exchange error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to exchange authorization code with GitHub".to_string(),
                    code: Some("OAUTH_EXCHANGE_ERROR".to_string()),
                }),
            )
        })?;

    if !token_resp.status().is_success() {
        let status = token_resp.status();
        let body = token_resp.text().await.unwrap_or_default();
        error!("GitHub token exchange failed: {} - {}", status, body);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Failed to authenticate with GitHub. Please try again.".to_string(),
                code: Some("OAUTH_EXCHANGE_FAILED".to_string()),
            }),
        ));
    }

    let token_data: GitHubTokenResponse = token_resp.json().await.map_err(|e| {
        error!("GitHub token response parse error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Invalid response from GitHub".to_string(),
                code: Some("OAUTH_PARSE_ERROR".to_string()),
            }),
        )
    })?;

    // Fetch user info
    let user_resp = http_client
        .get("https://api.github.com/user")
        .header("User-Agent", "localup-relay")
        .bearer_auth(&token_data.access_token)
        .send()
        .await
        .map_err(|e| {
            error!("GitHub user info error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to fetch user info from GitHub".to_string(),
                    code: Some("OAUTH_USERINFO_ERROR".to_string()),
                }),
            )
        })?;

    let github_user: GitHubUserInfo = user_resp.json().await.map_err(|e| {
        error!("GitHub user info parse error: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Invalid user info response from GitHub".to_string(),
                code: Some("OAUTH_PARSE_ERROR".to_string()),
            }),
        )
    })?;

    // GitHub may not return email if user has it private - fetch emails separately
    let email = if let Some(email) = github_user.email {
        email
    } else {
        let emails_resp = http_client
            .get("https://api.github.com/user/emails")
            .header("User-Agent", "localup-relay")
            .bearer_auth(&token_data.access_token)
            .send()
            .await
            .map_err(|e| {
                error!("GitHub emails error: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "Failed to fetch email from GitHub".to_string(),
                        code: Some("OAUTH_USERINFO_ERROR".to_string()),
                    }),
                )
            })?;

        let emails: Vec<GitHubEmail> = emails_resp.json().await.map_err(|e| {
            error!("GitHub emails parse error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Invalid email response from GitHub".to_string(),
                    code: Some("OAUTH_PARSE_ERROR".to_string()),
                }),
            )
        })?;

        emails
            .into_iter()
            .find(|e| e.primary && e.verified)
            .map(|e| e.email)
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: "No verified primary email found on GitHub account".to_string(),
                        code: Some("NO_EMAIL".to_string()),
                    }),
                )
            })?
    };

    Ok(OAuthUserInfo {
        email,
        name: github_user.name,
        provider_id: github_user.id.to_string(),
    })
}

// ============================================================================
// Magic Link (Passwordless Email) Endpoints
// ============================================================================

/// Send a magic link to the given email address
///
/// If SMTP is configured, generates a one-time token (15 min expiry),
/// stores it in the database, and sends an email with a verification link.
/// The link points directly to `GET /api/auth/magic-link/verify?token=...`
/// which sets a session cookie and redirects to the frontend.
///
/// Always returns success to prevent email enumeration attacks.
#[utoipa::path(
    post,
    path = "/api/auth/magic-link/send",
    request_body = MagicLinkRequest,
    responses(
        (status = 200, description = "Magic link sent (or not, to prevent enumeration)", body = MagicLinkResponse),
        (status = 503, description = "Magic link not configured", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn magic_link_send(
    State(state): State<Arc<AppState>>,
    req_headers: HeaderMap,
    Json(req): Json<crate::models::MagicLinkRequest>,
) -> Result<Json<crate::models::MagicLinkResponse>, (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::magic_link_token;
    use sea_orm::ActiveModelTrait;

    let smtp_config = state.smtp.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "Magic link login is not configured".to_string(),
                code: Some("MAGIC_LINK_NOT_CONFIGURED".to_string()),
            }),
        )
    })?;

    // Always return the same message regardless of whether the email exists
    // to prevent email enumeration
    let success_msg = crate::models::MagicLinkResponse {
        message: "If that email is registered (or registration is open), a magic link has been sent. Check your inbox.".to_string(),
    };

    // Validate email format (basic)
    if !req.email.contains('@') {
        // Still return success to prevent enumeration
        return Ok(Json(success_msg));
    }

    // Check if user exists, or if signup is enabled (we'll create on verify)
    let user_exists = UserEntity::find()
        .filter(user::Column::Email.eq(&req.email))
        .one(&state.db)
        .await
        .ok()
        .flatten()
        .is_some();

    if !user_exists && !state.allow_signup {
        // Don't reveal that the user doesn't exist
        return Ok(Json(success_msg));
    }

    // Generate a secure random token
    let token_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let expires_at = now + Duration::minutes(15);

    // Store token in database
    let token_model = magic_link_token::ActiveModel {
        id: Set(token_id.clone()),
        email: Set(req.email.clone()),
        expires_at: Set(expires_at),
        used_at: Set(None),
        created_at: Set(now),
    };

    if let Err(e) = token_model.insert(&state.db).await {
        error!("Failed to store magic link token: {}", e);
        // Still return success to not leak info
        return Ok(Json(success_msg));
    }

    // Build the verification URL
    // Determine the base URL from the request headers
    let base_url = determine_base_url(&req_headers, state.is_https);
    let verify_url = format!(
        "{}/api/auth/magic-link/verify?token={}",
        base_url,
        urlencoding::encode(&token_id)
    );

    // Send the email
    if let Err(e) = send_magic_link_email(smtp_config, &req.email, &verify_url).await {
        error!("Failed to send magic link email to {}: {}", req.email, e);
        // Still return success to not leak info
    } else {
        info!("Magic link sent to {}", req.email);
    }

    Ok(Json(success_msg))
}

/// Verify a magic link token (GET endpoint)
///
/// This is the URL the user clicks in their email. It validates the token,
/// creates/logs in the user, sets a session cookie, and redirects to the frontend.
#[utoipa::path(
    get,
    path = "/api/auth/magic-link/verify",
    params(
        ("token" = String, Query, description = "Magic link token"),
    ),
    responses(
        (status = 302, description = "Redirect to frontend dashboard with session cookie"),
        (status = 400, description = "Invalid or expired token", body = ErrorResponse)
    ),
    tag = "auth"
)]
pub async fn magic_link_verify(
    State(state): State<Arc<AppState>>,
    req_headers: HeaderMap,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::magic_link_token;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter};

    let token_id = params.get("token").ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Missing token parameter".to_string(),
                code: Some("MISSING_TOKEN".to_string()),
            }),
        )
    })?;

    // Look up the token
    let token_record = magic_link_token::Entity::find_by_id(token_id)
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Database error looking up magic link token: {}", e);
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
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid or expired magic link".to_string(),
                    code: Some("INVALID_TOKEN".to_string()),
                }),
            )
        })?;

    // Check if already used
    if token_record.used_at.is_some() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "This magic link has already been used".to_string(),
                code: Some("TOKEN_USED".to_string()),
            }),
        ));
    }

    // Check expiration
    let now = chrono::Utc::now();
    if now > token_record.expires_at {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "This magic link has expired. Please request a new one.".to_string(),
                code: Some("TOKEN_EXPIRED".to_string()),
            }),
        ));
    }

    // Mark token as used
    let mut active_token: magic_link_token::ActiveModel = token_record.clone().into();
    active_token.used_at = Set(Some(now));
    if let Err(e) = active_token.update(&state.db).await {
        error!("Failed to mark magic link token as used: {}", e);
    }

    // Find or create user
    let existing_user = UserEntity::find()
        .filter(user::Column::Email.eq(&token_record.email))
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Database error finding user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;

    let (user_model, is_new_user) = if let Some(user) = existing_user {
        (user, false)
    } else {
        // Create new user (signup must be enabled - checked in send handler)
        if !state.allow_signup {
            return Err((
                StatusCode::FORBIDDEN,
                Json(ErrorResponse {
                    error: "Public registration is disabled".to_string(),
                    code: Some("SIGNUP_DISABLED".to_string()),
                }),
            ));
        }

        let user_id = Uuid::new_v4();
        let new_user = user::ActiveModel {
            id: Set(user_id),
            email: Set(token_record.email.clone()),
            password_hash: Set(String::new()), // No password for magic link users
            full_name: Set(None),
            role: Set(user::UserRole::User),
            is_active: Set(true),
            created_at: Set(now),
            updated_at: Set(now),
            oauth_provider: Set(None),
            oauth_provider_id: Set(None),
        };

        let user = new_user.insert(&state.db).await.map_err(|e| {
            error!("Database error creating magic link user: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("DB_ERROR".to_string()),
                }),
            )
        })?;

        info!(
            "New user registered via magic link: {} ({})",
            user.email, user.id
        );

        (user, true)
    };

    // Check if account is active
    if !user_model.is_active {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "Account is disabled".to_string(),
                code: Some("ACCOUNT_DISABLED".to_string()),
            }),
        ));
    }

    // Auto-create default auth token if user doesn't have one
    if is_new_user {
        ensure_default_auth_token(&state, user_model.id).await;
    }

    // Generate session token and cookie
    let (_token, _expires_at, mut headers) =
        generate_session_response(&state, &user_model, &req_headers)?;

    // Redirect to the frontend dashboard
    headers.insert(header::LOCATION, "/dashboard".parse().unwrap());

    Ok((StatusCode::FOUND, headers))
}

/// Determine the base URL from request headers (for constructing magic link URLs)
fn determine_base_url(headers: &HeaderMap, server_is_https: bool) -> String {
    // Try X-Forwarded-Host + X-Forwarded-Proto first (reverse proxy)
    if let (Some(host), Some(proto)) = (
        headers
            .get("x-forwarded-host")
            .and_then(|v| v.to_str().ok()),
        headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok()),
    ) {
        return format!("{}://{}", proto, host);
    }

    // Try Host header
    if let Some(host) = headers.get("host").and_then(|v| v.to_str().ok()) {
        let proto = if server_is_https { "https" } else { "http" };
        return format!("{}://{}", proto, host);
    }

    // Fallback
    if server_is_https {
        "https://localhost".to_string()
    } else {
        "http://localhost".to_string()
    }
}

/// Send a magic link email via SMTP
async fn send_magic_link_email(
    smtp_config: &crate::SmtpConfig,
    to_email: &str,
    verify_url: &str,
) -> Result<(), String> {
    use lettre::message::header::ContentType;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

    let email = Message::builder()
        .from(
            smtp_config
                .from
                .parse()
                .map_err(|e| format!("Invalid from address: {}", e))?,
        )
        .to(to_email
            .parse()
            .map_err(|e| format!("Invalid to address: {}", e))?)
        .subject("Sign in to your account")
        .header(ContentType::TEXT_HTML)
        .body(format!(
            r#"<!DOCTYPE html>
<html>
<head><meta charset="UTF-8"></head>
<body style="font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; max-width: 600px; margin: 0 auto; padding: 20px;">
  <h2 style="color: #1a1a1a;">Sign in to your account</h2>
  <p style="color: #4a4a4a; line-height: 1.6;">
    Click the button below to sign in. This link expires in 15 minutes.
  </p>
  <p style="margin: 30px 0;">
    <a href="{url}" style="background-color: #0070f3; color: white; padding: 12px 24px; text-decoration: none; border-radius: 6px; display: inline-block; font-weight: 500;">
      Sign in
    </a>
  </p>
  <p style="color: #888; font-size: 14px; line-height: 1.5;">
    If the button doesn't work, copy and paste this link into your browser:<br>
    <a href="{url}" style="color: #0070f3; word-break: break-all;">{url}</a>
  </p>
  <hr style="border: none; border-top: 1px solid #eaeaea; margin: 30px 0;">
  <p style="color: #aaa; font-size: 12px;">
    If you didn't request this email, you can safely ignore it.
  </p>
</body>
</html>"#,
            url = verify_url,
        ))
        .map_err(|e| format!("Failed to build email: {}", e))?;

    // Determine TLS mode: explicit setting or auto-detect from port
    let effective_mode = match &smtp_config.tls_mode {
        crate::SmtpTlsMode::Auto => match smtp_config.port {
            465 => &crate::SmtpTlsMode::Tls,
            587 => &crate::SmtpTlsMode::StartTls,
            _ => &crate::SmtpTlsMode::Plain,
        },
        mode => mode,
    };

    let mailer = match effective_mode {
        crate::SmtpTlsMode::StartTls => {
            let creds =
                Credentials::new(smtp_config.username.clone(), smtp_config.password.clone());
            AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&smtp_config.host)
                .map_err(|e| format!("Failed to create SMTP STARTTLS transport: {}", e))?
                .port(smtp_config.port)
                .credentials(creds)
                .build()
        }
        crate::SmtpTlsMode::Tls => {
            let creds =
                Credentials::new(smtp_config.username.clone(), smtp_config.password.clone());
            AsyncSmtpTransport::<Tokio1Executor>::relay(&smtp_config.host)
                .map_err(|e| format!("Failed to create SMTP TLS transport: {}", e))?
                .port(smtp_config.port)
                .credentials(creds)
                .build()
        }
        crate::SmtpTlsMode::Plain | crate::SmtpTlsMode::Auto => {
            // Plain mode: no auth (local dev servers like Mailpit don't support it)
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&smtp_config.host)
                .port(smtp_config.port)
                .build()
        }
    };

    mailer
        .send(email)
        .await
        .map_err(|e| format!("Failed to send email: {}", e))?;

    Ok(())
}

// ============================================================================
// Device Authorization Grant (RFC 8628) Handlers
// ============================================================================

/// Generate a cryptographically random device code (opaque, 40 chars hex)
fn generate_device_code() -> String {
    use rand::Rng;
    let bytes: [u8; 20] = rand::rng().random();
    hex::encode(bytes)
}

/// Generate a user-friendly code (e.g., "ABCD-1234")
/// Uses uppercase letters (no O/I to avoid confusion) and digits (no 0/1)
fn generate_user_code() -> String {
    use rand::Rng;
    // Unambiguous charset: no 0, O, 1, I, L
    const CHARSET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let mut rng = rand::rng();
    let part1: String = (0..4)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect();
    let part2: String = (0..4)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect();
    format!("{}-{}", part1, part2)
}

/// Initiate device authorization (RFC 8628 Section 3.1)
///
/// Third-party applications call this endpoint to start the device flow.
/// Returns a device_code, user_code, and verification_uri.
#[utoipa::path(
    post,
    path = "/api/device/authorize",
    request_body = DeviceAuthorizationRequest,
    responses(
        (status = 200, description = "Device authorization initiated", body = DeviceAuthorizationResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "device-auth"
)]
pub async fn device_authorize(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeviceAuthorizationRequest>,
) -> Result<Json<DeviceAuthorizationResponse>, (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::device_authorization;
    use sea_orm::{ActiveModelTrait, Set};

    if req.client_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "client_id is required".to_string(),
                code: Some("invalid_request".to_string()),
            }),
        ));
    }

    // Validate client_id against registered OAuth clients
    if state.oauth_clients.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Device authorization is not configured. No OAuth clients registered."
                    .to_string(),
                code: Some("invalid_client".to_string()),
            }),
        ));
    }

    let registered_client = state
        .oauth_clients
        .iter()
        .find(|c| c.client_id == req.client_id);

    if registered_client.is_none() {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: format!("Unknown client_id '{}'", req.client_id),
                code: Some("invalid_client".to_string()),
            }),
        ));
    }

    let now = chrono::Utc::now();
    let expires_in_secs: i64 = 600; // 10 minutes
    let expires_at = now + chrono::Duration::seconds(expires_in_secs);
    let interval = 5;

    let id = uuid::Uuid::new_v4().to_string();
    let device_code = generate_device_code();
    let user_code = generate_user_code();

    // Store in DB
    let active_model = device_authorization::ActiveModel {
        id: Set(id),
        device_code: Set(device_code.clone()),
        user_code: Set(user_code.clone()),
        client_id: Set(req.client_id.clone()),
        scope: Set(req.scope.clone()),
        status: Set("pending".to_string()),
        user_id: Set(None),
        polling_interval: Set(interval),
        last_polled_at: Set(None),
        expires_at: Set(expires_at),
        created_at: Set(now),
    };

    active_model.insert(&state.db).await.map_err(|e| {
        error!("Failed to create device authorization: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to create device authorization".to_string(),
                code: Some("server_error".to_string()),
            }),
        )
    })?;

    // Build verification URI
    // Use the relay config domain if available, otherwise fall back to a relative path
    let base_url = if let Some(ref relay_config) = state.relay_config {
        let scheme = if state.is_https { "https" } else { "http" };
        format!("{}://{}", scheme, relay_config.domain)
    } else {
        String::new() // relative URL
    };
    let verification_uri = format!("{}/device", base_url);
    let verification_uri_complete = format!("{}/device?code={}", base_url, user_code);

    info!(
        "Device authorization created: client_id={}, user_code={}, expires_in={}s",
        req.client_id, user_code, expires_in_secs
    );

    Ok(Json(DeviceAuthorizationResponse {
        device_code,
        user_code,
        verification_uri,
        verification_uri_complete: Some(verification_uri_complete),
        expires_in: expires_in_secs,
        interval,
    }))
}

/// Poll for device access token (RFC 8628 Section 3.4)
///
/// Applications poll this endpoint with the device_code until the user approves.
/// Returns authorization_pending, slow_down, access_denied, expired_token, or the token.
#[utoipa::path(
    post,
    path = "/api/device/token",
    request_body = DeviceTokenRequest,
    responses(
        (status = 200, description = "Access token granted", body = DeviceTokenResponse),
        (status = 400, description = "Authorization pending or error", body = DeviceTokenErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "device-auth"
)]
pub async fn device_token(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DeviceTokenRequest>,
) -> Result<Json<DeviceTokenResponse>, (StatusCode, Json<serde_json::Value>)> {
    use localup_relay_db::entities::device_authorization;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    // Validate grant_type per RFC 8628
    if req.grant_type != "urn:ietf:params:oauth:grant-type:device_code" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "unsupported_grant_type",
                "error_description": "grant_type must be urn:ietf:params:oauth:grant-type:device_code"
            })),
        ));
    }

    // Look up the device authorization
    let auth = device_authorization::Entity::find()
        .filter(device_authorization::Column::DeviceCode.eq(&req.device_code))
        .filter(device_authorization::Column::ClientId.eq(&req.client_id))
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Failed to query device authorization: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "server_error",
                    "error_description": "Internal server error"
                })),
            )
        })?;

    let auth = match auth {
        Some(a) => a,
        None => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "invalid_grant",
                    "error_description": "Invalid device_code or client_id"
                })),
            ));
        }
    };

    let now = chrono::Utc::now();

    // Check if expired
    if now > auth.expires_at {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "expired_token",
                "error_description": "The device_code has expired. Please start a new authorization request."
            })),
        ));
    }

    // Check slow_down: if client is polling faster than the interval
    if let Some(last_polled) = auth.last_polled_at {
        let elapsed = (now - last_polled).num_seconds();
        if elapsed < auth.polling_interval as i64 {
            // Update last_polled_at and increase interval by 5 seconds per RFC
            let mut active: device_authorization::ActiveModel = auth.clone().into();
            active.last_polled_at = Set(Some(now));
            active.polling_interval = Set(auth.polling_interval + 5);
            let _ = active.update(&state.db).await;

            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "slow_down",
                    "error_description": format!("Please wait at least {} seconds between requests", auth.polling_interval + 5)
                })),
            ));
        }
    }

    // Update last_polled_at
    {
        let mut active: device_authorization::ActiveModel = auth.clone().into();
        active.last_polled_at = Set(Some(now));
        let _ = active.update(&state.db).await;
    }

    match auth.status.as_str() {
        "pending" => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "authorization_pending",
                "error_description": "The user has not yet authorized this device. Continue polling."
            })),
        )),
        "denied" => Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "access_denied",
                "error_description": "The user denied the authorization request."
            })),
        )),
        "approved" => {
            let user_id = auth
                .user_id
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            // Generate an auth token (JWT) for the approved user
            let token_duration = chrono::Duration::hours(24);
            let claims = localup_auth::JwtClaims::new(
                req.client_id.clone(),
                "localup-relay".to_string(),
                "localup-client".to_string(),
                token_duration,
            )
            .with_user_id(user_id.clone());

            let token = localup_auth::JwtValidator::encode(state.jwt_secret.as_bytes(), &claims)
                .map_err(|e| {
                    error!("Failed to encode JWT for device auth: {}", e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({
                            "error": "server_error",
                            "error_description": "Failed to generate access token"
                        })),
                    )
                })?;

            // Mark as consumed (delete or update status)
            let mut active: device_authorization::ActiveModel = auth.into();
            active.status = Set("consumed".to_string());
            let _ = active.update(&state.db).await;

            info!(
                "Device authorization approved: client_id={}, user_id={}",
                req.client_id, user_id
            );

            Ok(Json(DeviceTokenResponse {
                access_token: token,
                token_type: "Bearer".to_string(),
                expires_in: token_duration.num_seconds(),
                scope: None,
            }))
        }
        other => {
            error!("Unknown device authorization status: {}", other);
            Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": "server_error",
                    "error_description": "Unknown authorization status"
                })),
            ))
        }
    }
}

/// Look up a device authorization by user_code (for the verification page)
///
/// The verification page calls this to show the user what app is requesting access.
#[utoipa::path(
    get,
    path = "/api/device/info",
    params(
        ("user_code" = String, Query, description = "The user code displayed on the device")
    ),
    responses(
        (status = 200, description = "Device authorization info", body = DeviceAuthorizationInfo),
    ),
    tag = "device-auth"
)]
pub async fn device_info(
    State(state): State<Arc<AppState>>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Json<DeviceAuthorizationInfo> {
    use localup_relay_db::entities::device_authorization;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

    let user_code = params.get("user_code").cloned().unwrap_or_default();
    // Normalize: uppercase, remove spaces/dashes for lookup
    let normalized = user_code.to_uppercase().replace(['-', ' '], "");

    if normalized.len() != 8 {
        return Json(DeviceAuthorizationInfo {
            valid: false,
            client_id: None,
            client_name: None,
            scope: None,
            message: "Invalid code format".to_string(),
        });
    }

    // Re-insert dash for DB lookup (stored as "XXXX-YYYY")
    let db_code = format!("{}-{}", &normalized[..4], &normalized[4..]);

    let now = chrono::Utc::now();

    let auth = device_authorization::Entity::find()
        .filter(device_authorization::Column::UserCode.eq(&db_code))
        .filter(device_authorization::Column::Status.eq("pending"))
        .one(&state.db)
        .await
        .ok()
        .flatten();

    match auth {
        Some(a) if a.expires_at > now => {
            // Look up the human-readable display name for this client_id
            let client_name = state
                .oauth_clients
                .iter()
                .find(|c| c.client_id == a.client_id)
                .map(|c| c.display_name.clone());
            Json(DeviceAuthorizationInfo {
                valid: true,
                client_id: Some(a.client_id),
                client_name,
                scope: a.scope,
                message: "Authorization request found. Approve to grant access.".to_string(),
            })
        }
        _ => Json(DeviceAuthorizationInfo {
            valid: false,
            client_id: None,
            client_name: None,
            scope: None,
            message: "Invalid or expired code".to_string(),
        }),
    }
}

/// Verify (approve) a device authorization
///
/// Called by the logged-in user from the verification page to approve the device.
/// Requires session authentication.
#[utoipa::path(
    post,
    path = "/api/device/verify",
    request_body = DeviceVerifyRequest,
    responses(
        (status = 200, description = "Device authorization approved", body = DeviceVerifyResponse),
        (status = 400, description = "Invalid or expired code", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "device-auth"
)]
pub async fn device_verify(
    State(state): State<Arc<AppState>>,
    Extension(auth_user): Extension<AuthUser>,
    Json(req): Json<DeviceVerifyRequest>,
) -> Result<Json<DeviceVerifyResponse>, (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::device_authorization;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    // Normalize user_code
    let normalized = req.user_code.to_uppercase().replace(['-', ' '], "");

    if normalized.len() != 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid code format. Expected format: XXXX-YYYY".to_string(),
                code: Some("invalid_request".to_string()),
            }),
        ));
    }

    let db_code = format!("{}-{}", &normalized[..4], &normalized[4..]);
    let now = chrono::Utc::now();

    let auth = device_authorization::Entity::find()
        .filter(device_authorization::Column::UserCode.eq(&db_code))
        .filter(device_authorization::Column::Status.eq("pending"))
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Failed to query device authorization: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("server_error".to_string()),
                }),
            )
        })?;

    let auth = match auth {
        Some(a) if a.expires_at > now => a,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid or expired code".to_string(),
                    code: Some("invalid_grant".to_string()),
                }),
            ));
        }
    };

    let client_id = auth.client_id.clone();
    let scope = auth.scope.clone();

    // Approve the authorization
    let mut active: device_authorization::ActiveModel = auth.into();
    active.status = Set("approved".to_string());
    active.user_id = Set(Some(auth_user.user_id.clone()));
    active.update(&state.db).await.map_err(|e| {
        error!("Failed to approve device authorization: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to approve authorization".to_string(),
                code: Some("server_error".to_string()),
            }),
        )
    })?;

    info!(
        "Device authorization approved: user_code={}, client_id={}, user_id={}",
        db_code, client_id, auth_user.user_id
    );

    Ok(Json(DeviceVerifyResponse {
        approved: true,
        client_id,
        scope,
        message: "Device authorized successfully".to_string(),
    }))
}

/// Deny a device authorization
///
/// Called by the logged-in user from the verification page to deny the device.
/// Requires session authentication.
#[utoipa::path(
    post,
    path = "/api/device/deny",
    request_body = DeviceVerifyRequest,
    responses(
        (status = 200, description = "Device authorization denied", body = DeviceVerifyResponse),
        (status = 400, description = "Invalid or expired code", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "device-auth"
)]
pub async fn device_deny(
    State(state): State<Arc<AppState>>,
    Extension(auth_user): Extension<AuthUser>,
    Json(req): Json<DeviceVerifyRequest>,
) -> Result<Json<DeviceVerifyResponse>, (StatusCode, Json<ErrorResponse>)> {
    use localup_relay_db::entities::device_authorization;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let normalized = req.user_code.to_uppercase().replace(['-', ' '], "");

    if normalized.len() != 8 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Invalid code format".to_string(),
                code: Some("invalid_request".to_string()),
            }),
        ));
    }

    let db_code = format!("{}-{}", &normalized[..4], &normalized[4..]);
    let now = chrono::Utc::now();

    let auth = device_authorization::Entity::find()
        .filter(device_authorization::Column::UserCode.eq(&db_code))
        .filter(device_authorization::Column::Status.eq("pending"))
        .one(&state.db)
        .await
        .map_err(|e| {
            error!("Failed to query device authorization: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Internal server error".to_string(),
                    code: Some("server_error".to_string()),
                }),
            )
        })?;

    let auth = match auth {
        Some(a) if a.expires_at > now => a,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "Invalid or expired code".to_string(),
                    code: Some("invalid_grant".to_string()),
                }),
            ));
        }
    };

    let client_id = auth.client_id.clone();
    let scope = auth.scope.clone();

    let mut active: device_authorization::ActiveModel = auth.into();
    active.status = Set("denied".to_string());
    active.user_id = Set(Some(auth_user.user_id.clone()));
    active.update(&state.db).await.map_err(|e| {
        error!("Failed to deny device authorization: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to deny authorization".to_string(),
                code: Some("server_error".to_string()),
            }),
        )
    })?;

    info!(
        "Device authorization denied: user_code={}, client_id={}, user_id={}",
        db_code, client_id, auth_user.user_id
    );

    Ok(Json(DeviceVerifyResponse {
        approved: false,
        client_id,
        scope,
        message: "Device authorization denied".to_string(),
    }))
}
