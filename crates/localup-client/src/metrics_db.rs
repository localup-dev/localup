//! Database-backed metrics storage
//!
//! This module provides persistent storage for metrics using SQLite,
//! allowing metrics to survive application restarts.

#[cfg(feature = "db-metrics")]
use crate::metrics::{BodyContent, BodyData, HttpMetric, TcpConnectionState, TcpMetric};
#[cfg(feature = "db-metrics")]
use localup_relay_db::entities::{captured_request, captured_tcp_connection};
#[cfg(feature = "db-metrics")]
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
#[cfg(feature = "db-metrics")]
use std::sync::Arc;
#[cfg(feature = "db-metrics")]
use tokio::sync::RwLock;

/// Database-backed metrics storage
#[cfg(feature = "db-metrics")]
#[derive(Clone)]
pub struct DbMetricsStore {
    db: Arc<DatabaseConnection>,
    localup_id: Arc<RwLock<String>>,
}

#[cfg(feature = "db-metrics")]
impl DbMetricsStore {
    /// Create a new database-backed metrics store
    pub async fn new(db: DatabaseConnection, localup_id: String) -> Result<Self, sea_orm::DbErr> {
        Ok(Self {
            db: Arc::new(db),
            localup_id: Arc::new(RwLock::new(localup_id)),
        })
    }

    /// Update the tunnel ID (when reconnecting)
    pub async fn set_localup_id(&self, localup_id: String) {
        let mut tid = self.localup_id.write().await;
        *tid = localup_id;
    }

    /// Save HTTP request to database
    pub async fn save_http_metric(&self, metric: &HttpMetric) -> Result<(), sea_orm::DbErr> {
        let localup_id = self.localup_id.read().await.clone();

        let model = captured_request::ActiveModel {
            id: Set(metric.id.clone()),
            localup_id: Set(localup_id),
            method: Set(metric.method.clone()),
            path: Set(metric.uri.clone()),
            host: Set(None), // Could extract from headers
            headers: Set(serde_json::to_string(&metric.request_headers).unwrap_or_default()),
            body: Set(metric.request_body.as_ref().map(|b| match &b.data {
                BodyContent::Json(v) => serde_json::to_string(v).unwrap_or_default(),
                BodyContent::Text(t) => t.clone(),
                BodyContent::Binary { .. } => String::new(),
            })),
            status: Set(metric.response_status.map(|s| s as i32)),
            response_headers: Set(metric
                .response_headers
                .as_ref()
                .map(|h| serde_json::to_string(h).unwrap_or_default())),
            response_body: Set(metric.response_body.as_ref().map(|b| match &b.data {
                BodyContent::Json(v) => serde_json::to_string(v).unwrap_or_default(),
                BodyContent::Text(t) => t.clone(),
                BodyContent::Binary { .. } => String::new(),
            })),
            created_at: Set(
                chrono::DateTime::from_timestamp_millis(metric.timestamp as i64)
                    .unwrap_or(chrono::Utc::now()),
            ),
            responded_at: Set(metric.duration_ms.and_then(|d| {
                chrono::DateTime::from_timestamp_millis((metric.timestamp + d) as i64)
            })),
            latency_ms: Set(metric.duration_ms.map(|d| d as i32)),
        };

        model.insert(self.db.as_ref()).await?;
        Ok(())
    }

    /// Save TCP connection to database
    pub async fn save_tcp_metric(&self, metric: &TcpMetric) -> Result<(), sea_orm::DbErr> {
        let localup_id = self.localup_id.read().await.clone();

        let model = captured_tcp_connection::ActiveModel {
            id: Set(metric.id.clone()),
            localup_id: Set(localup_id),
            client_addr: Set(metric.remote_addr.clone()),
            target_port: Set(0), // Could extract from context
            bytes_received: Set(metric.bytes_received as i64),
            bytes_sent: Set(metric.bytes_sent as i64),
            connected_at: Set(sea_orm::prelude::DateTimeWithTimeZone::from(
                chrono::DateTime::from_timestamp_millis(metric.timestamp as i64)
                    .unwrap_or(chrono::Utc::now()),
            )),
            disconnected_at: Set(metric.closed_at.and_then(|ts| {
                chrono::DateTime::from_timestamp_millis(ts as i64)
                    .map(sea_orm::prelude::DateTimeWithTimeZone::from)
            })),
            duration_ms: Set(metric.duration_ms.map(|d| d as i32)),
            disconnect_reason: Set(metric.error.clone()),
        };

        model.insert(self.db.as_ref()).await?;
        Ok(())
    }

    /// Get paginated HTTP metrics
    pub async fn get_http_metrics(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<HttpMetric>, sea_orm::DbErr> {
        let localup_id = self.localup_id.read().await.clone();

        let models = captured_request::Entity::find()
            .filter(captured_request::Column::LocalupId.eq(localup_id))
            .order_by_desc(captured_request::Column::CreatedAt)
            .offset(offset as u64)
            .limit(limit as u64)
            .all(self.db.as_ref())
            .await?;

        Ok(models.into_iter().map(Self::to_http_metric).collect())
    }

    /// Get paginated TCP metrics
    pub async fn get_tcp_metrics(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<TcpMetric>, sea_orm::DbErr> {
        let localup_id = self.localup_id.read().await.clone();

        let models = captured_tcp_connection::Entity::find()
            .filter(captured_tcp_connection::Column::LocalupId.eq(localup_id))
            .order_by_desc(captured_tcp_connection::Column::ConnectedAt)
            .offset(offset as u64)
            .limit(limit as u64)
            .all(self.db.as_ref())
            .await?;

        Ok(models.into_iter().map(Self::to_tcp_metric).collect())
    }

    /// Count total HTTP metrics
    pub async fn count_http_metrics(&self) -> Result<u64, sea_orm::DbErr> {
        let localup_id = self.localup_id.read().await.clone();

        captured_request::Entity::find()
            .filter(captured_request::Column::LocalupId.eq(localup_id))
            .count(self.db.as_ref())
            .await
    }

    /// Clear all metrics for this tunnel
    pub async fn clear_metrics(&self) -> Result<(), sea_orm::DbErr> {
        let localup_id = self.localup_id.read().await.clone();

        // Delete HTTP metrics
        captured_request::Entity::delete_many()
            .filter(captured_request::Column::LocalupId.eq(localup_id.clone()))
            .exec(self.db.as_ref())
            .await?;

        // Delete TCP metrics
        captured_tcp_connection::Entity::delete_many()
            .filter(captured_tcp_connection::Column::LocalupId.eq(localup_id))
            .exec(self.db.as_ref())
            .await?;

        Ok(())
    }

    /// Convert database model to HttpMetric
    fn to_http_metric(model: captured_request::Model) -> HttpMetric {
        let request_headers: Vec<(String, String)> =
            serde_json::from_str(&model.headers).unwrap_or_default();

        let response_headers: Option<Vec<(String, String)>> = model
            .response_headers
            .as_ref()
            .and_then(|h| serde_json::from_str(h).ok());

        HttpMetric {
            id: model.id,
            stream_id: String::new(), // Not stored in DB
            timestamp: model.created_at.timestamp_millis() as u64,
            method: model.method,
            uri: model.path,
            request_headers,
            request_body: model.body.map(|b| BodyData {
                content_type: "application/octet-stream".to_string(),
                size: b.len(),
                data: BodyContent::Text(b),
            }),
            response_status: model.status.map(|s| s as u16),
            response_headers,
            response_body: model.response_body.map(|b| BodyData {
                content_type: "application/octet-stream".to_string(),
                size: b.len(),
                data: BodyContent::Text(b),
            }),
            duration_ms: model.latency_ms.map(|l| l as u64),
            error: None,
        }
    }

    /// Convert database model to TcpMetric
    fn to_tcp_metric(model: captured_tcp_connection::Model) -> TcpMetric {
        let state = if model.disconnected_at.is_some() {
            if model.disconnect_reason.is_some() {
                TcpConnectionState::Error
            } else {
                TcpConnectionState::Closed
            }
        } else {
            TcpConnectionState::Active
        };

        TcpMetric {
            id: model.id,
            stream_id: String::new(), // Not stored in DB
            timestamp: model.connected_at.timestamp_millis() as u64,
            remote_addr: model.client_addr,
            local_addr: "127.0.0.1".to_string(), // Not stored in DB
            state,
            bytes_received: model.bytes_received as u64,
            bytes_sent: model.bytes_sent as u64,
            duration_ms: model.duration_ms.map(|d| d as u64),
            closed_at: model.disconnected_at.map(|dt| dt.timestamp_millis() as u64),
            error: model.disconnect_reason,
        }
    }
}
