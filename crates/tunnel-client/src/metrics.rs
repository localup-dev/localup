//! Metrics collection for tunnel client
//!
//! This module provides request/response tracking for HTTP tunnels,
//! including headers, bodies (when JSON), and timing information.

use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, RwLock};
use utoipa::ToSchema;

/// HTTP request metrics entry
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HttpMetric {
    /// Unique ID for this request
    pub id: String,
    /// Stream ID from the tunnel (short GUID)
    pub stream_id: String,
    /// Timestamp when request was received (milliseconds since epoch)
    pub timestamp: u64,
    /// Request method (GET, POST, etc.)
    pub method: String,
    /// Request URI
    pub uri: String,
    /// Request headers
    pub request_headers: Vec<(String, String)>,
    /// Request body (stored only if JSON or text)
    pub request_body: Option<BodyData>,
    /// Response status code
    pub response_status: Option<u16>,
    /// Response headers
    pub response_headers: Option<Vec<(String, String)>>,
    /// Response body (stored only if JSON or text)
    pub response_body: Option<BodyData>,
    /// Duration in milliseconds
    pub duration_ms: Option<u64>,
    /// Error message if request failed
    pub error: Option<String>,
}

/// TCP connection metrics entry
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TcpMetric {
    /// Unique ID for this connection
    pub id: String,
    /// Stream ID from the tunnel (short GUID)
    pub stream_id: String,
    /// Timestamp when connection was established (milliseconds since epoch)
    pub timestamp: u64,
    /// Remote IP address that connected
    pub remote_addr: String,
    /// Local IP address (usually 127.0.0.1)
    pub local_addr: String,
    /// Connection state (active, closed, error)
    pub state: TcpConnectionState,
    /// Bytes received from remote
    pub bytes_received: u64,
    /// Bytes sent to remote
    pub bytes_sent: u64,
    /// Connection duration in milliseconds (None if still active)
    pub duration_ms: Option<u64>,
    /// Timestamp when connection was closed (milliseconds since epoch)
    pub closed_at: Option<u64>,
    /// Error message if connection failed
    pub error: Option<String>,
}

/// TCP connection state
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TcpConnectionState {
    /// Connection is active
    Active,
    /// Connection closed normally
    Closed,
    /// Connection closed with error
    Error,
}

/// Body data with content type information
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct BodyData {
    /// Content-Type header value
    pub content_type: String,
    /// Size in bytes
    pub size: usize,
    /// Parsed data (only for JSON/text)
    pub data: BodyContent,
}

/// Body content based on content type
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", content = "value")]
pub enum BodyContent {
    /// JSON body (parsed)
    Json(serde_json::Value),
    /// Text body
    Text(String),
    /// Binary body (not stored, only metadata)
    Binary { size: usize },
}

/// Metrics update event for SSE broadcasting
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type")]
pub enum MetricsEvent {
    /// New HTTP request recorded
    #[serde(rename = "request")]
    Request { metric: HttpMetric },
    /// Response recorded for existing HTTP request
    #[serde(rename = "response")]
    Response {
        id: String,
        status: u16,
        headers: Vec<(String, String)>,
        body: Option<BodyData>,
        duration_ms: u64,
    },
    /// Error recorded for existing HTTP request
    #[serde(rename = "error")]
    Error {
        id: String,
        error: String,
        duration_ms: u64,
    },
    /// New TCP connection established
    #[serde(rename = "tcp_connection")]
    TcpConnection { metric: TcpMetric },
    /// TCP connection updated (bytes transferred)
    #[serde(rename = "tcp_update")]
    TcpUpdate {
        id: String,
        bytes_received: u64,
        bytes_sent: u64,
    },
    /// TCP connection closed
    #[serde(rename = "tcp_closed")]
    TcpClosed {
        id: String,
        bytes_received: u64,
        bytes_sent: u64,
        duration_ms: u64,
        error: Option<String>,
    },
    /// Stats updated
    #[serde(rename = "stats")]
    Stats { stats: MetricsStats },
}

/// Metrics storage and query interface
#[derive(Clone)]
pub struct MetricsStore {
    metrics: Arc<RwLock<Vec<HttpMetric>>>,
    tcp_connections: Arc<RwLock<Vec<TcpMetric>>>,
    max_entries: usize,
    /// Broadcast channel for real-time updates
    update_tx: broadcast::Sender<MetricsEvent>,
    /// Cached stats (updated incrementally)
    cached_stats: Arc<RwLock<MetricsStats>>,
    /// Histogram for duration percentiles (values in milliseconds, max 60 seconds)
    duration_histogram: Arc<RwLock<Histogram<u64>>>,
    /// Last time stats were broadcast (for debouncing)
    last_stats_broadcast: Arc<RwLock<Option<Instant>>>,
}

impl MetricsStore {
    /// Create a new metrics store
    pub fn new(max_entries: usize) -> Self {
        // Create broadcast channel with capacity for 100 events
        let (update_tx, _) = broadcast::channel(100);

        // Create histogram for tracking request durations
        // Tracks values from 1ms to 3,600,000ms (1 hour) with 3 significant figures
        let histogram =
            Histogram::<u64>::new_with_bounds(1, 3_600_000, 3).expect("Failed to create histogram");

        Self {
            metrics: Arc::new(RwLock::new(Vec::new())),
            tcp_connections: Arc::new(RwLock::new(Vec::new())),
            max_entries,
            update_tx,
            cached_stats: Arc::new(RwLock::new(MetricsStats {
                total_requests: 0,
                successful_requests: 0,
                failed_requests: 0,
                avg_duration_ms: None,
                percentiles: None,
                methods: HashMap::new(),
                status_codes: HashMap::new(),
            })),
            duration_histogram: Arc::new(RwLock::new(histogram)),
            last_stats_broadcast: Arc::new(RwLock::new(None)),
        }
    }

    /// Subscribe to metrics updates for SSE
    pub fn subscribe(&self) -> broadcast::Receiver<MetricsEvent> {
        self.update_tx.subscribe()
    }

    /// Broadcast stats update with 1-second debouncing
    async fn broadcast_stats_debounced(&self) {
        const DEBOUNCE_DURATION: Duration = Duration::from_secs(1);

        let mut last_broadcast = self.last_stats_broadcast.write().await;
        let now = Instant::now();

        // Check if we should broadcast (first time or 1 second elapsed)
        let should_broadcast = match *last_broadcast {
            None => true,
            Some(last_time) => now.duration_since(last_time) >= DEBOUNCE_DURATION,
        };

        if should_broadcast {
            *last_broadcast = Some(now);
            drop(last_broadcast); // Release lock

            // Use cached stats instead of recalculating
            let stats = self.cached_stats.read().await.clone();
            let _ = self.update_tx.send(MetricsEvent::Stats { stats });
        }
    }

    /// Record a new HTTP request
    pub async fn record_request(
        &self,
        stream_id: String,
        method: String,
        uri: String,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        // Parse request body if it's JSON or text
        let request_body = body.as_ref().and_then(|b| {
            let content_type = headers
                .iter()
                .find(|(k, _)| k.to_lowercase() == "content-type")
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| "application/octet-stream".to_string());

            Self::parse_body(b, &content_type)
        });

        let metric = HttpMetric {
            id: id.clone(),
            stream_id,
            timestamp,
            method,
            uri,
            request_headers: headers,
            request_body,
            response_status: None,
            response_headers: None,
            response_body: None,
            duration_ms: None,
            error: None,
        };

        let mut metrics = self.metrics.write().await;
        metrics.push(metric.clone());

        // Enforce max entries (keep most recent)
        if metrics.len() > self.max_entries {
            let overflow = metrics.len() - self.max_entries;
            metrics.drain(0..overflow);
        }

        // Update cached stats
        let mut stats = self.cached_stats.write().await;
        stats.total_requests += 1;
        *stats.methods.entry(metric.method.clone()).or_insert(0) += 1;
        drop(stats);

        drop(metrics); // Release lock

        // Broadcast the new request event
        let _ = self.update_tx.send(MetricsEvent::Request { metric });

        // Broadcast stats update (debounced)
        self.broadcast_stats_debounced().await;

        id
    }

    /// Update a metric with response data
    pub async fn record_response(
        &self,
        id: &str,
        status: u16,
        headers: Vec<(String, String)>,
        body: Option<Vec<u8>>,
        duration_ms: u64,
    ) {
        let mut metrics = self.metrics.write().await;

        if let Some(metric) = metrics.iter_mut().find(|m| m.id == id) {
            // Parse response body if it's JSON or text
            let response_body = body.as_ref().and_then(|b| {
                let content_type = headers
                    .iter()
                    .find(|(k, _)| k.to_lowercase() == "content-type")
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| "application/octet-stream".to_string());

                Self::parse_body(b, &content_type)
            });

            metric.response_status = Some(status);
            metric.response_headers = Some(headers.clone());
            metric.response_body = response_body.clone();
            metric.duration_ms = Some(duration_ms);

            // Record duration in histogram
            let mut histogram = self.duration_histogram.write().await;
            if let Err(e) = histogram.record(duration_ms) {
                tracing::warn!("Failed to record duration in histogram: {}", e);
            }
            drop(histogram);

            // Update cached stats
            let mut stats = self.cached_stats.write().await;
            if status < 400 {
                stats.successful_requests += 1;
            } else {
                stats.failed_requests += 1;
            }
            *stats.status_codes.entry(status).or_insert(0) += 1;

            // Recalculate average duration and percentiles
            let histogram = self.duration_histogram.read().await;
            if !histogram.is_empty() {
                stats.avg_duration_ms = Some(histogram.mean() as u64);
                stats.percentiles = Some(DurationPercentiles {
                    min: histogram.min(),
                    p50: histogram.value_at_quantile(0.50),
                    p90: histogram.value_at_quantile(0.90),
                    p95: histogram.value_at_quantile(0.95),
                    p99: histogram.value_at_quantile(0.99),
                    p999: histogram.value_at_quantile(0.999),
                    max: histogram.max(),
                });
            }
            drop(histogram);
            drop(stats);
            drop(metrics);

            // Broadcast the response event with full data
            let _ = self.update_tx.send(MetricsEvent::Response {
                id: id.to_string(),
                status,
                headers,
                body: response_body,
                duration_ms,
            });

            // Broadcast stats update (debounced)
            self.broadcast_stats_debounced().await;
        }
    }

    /// Record an error for a request
    pub async fn record_error(&self, id: &str, error: String, duration_ms: u64) {
        let mut metrics = self.metrics.write().await;

        if let Some(metric) = metrics.iter_mut().find(|m| m.id == id) {
            metric.error = Some(error.clone());
            metric.duration_ms = Some(duration_ms);

            // Record duration in histogram
            let mut histogram = self.duration_histogram.write().await;
            if let Err(e) = histogram.record(duration_ms) {
                tracing::warn!("Failed to record duration in histogram: {}", e);
            }
            drop(histogram);

            // Update cached stats
            let mut stats = self.cached_stats.write().await;
            stats.failed_requests += 1;

            // Recalculate average duration and percentiles
            let histogram = self.duration_histogram.read().await;
            if !histogram.is_empty() {
                stats.avg_duration_ms = Some(histogram.mean() as u64);
                stats.percentiles = Some(DurationPercentiles {
                    min: histogram.min(),
                    p50: histogram.value_at_quantile(0.50),
                    p90: histogram.value_at_quantile(0.90),
                    p95: histogram.value_at_quantile(0.95),
                    p99: histogram.value_at_quantile(0.99),
                    p999: histogram.value_at_quantile(0.999),
                    max: histogram.max(),
                });
            }
            drop(histogram);
            drop(stats);

            drop(metrics);

            // Broadcast the error event
            let _ = self.update_tx.send(MetricsEvent::Error {
                id: id.to_string(),
                error,
                duration_ms,
            });

            // Broadcast stats update (debounced)
            self.broadcast_stats_debounced().await;
        }
    }

    /// Get all metrics
    pub async fn get_all(&self) -> Vec<HttpMetric> {
        self.metrics.read().await.clone()
    }

    /// Get metrics with pagination
    pub async fn get_paginated(&self, offset: usize, limit: usize) -> Vec<HttpMetric> {
        let metrics = self.metrics.read().await;
        metrics
            .iter()
            .rev() // Most recent first
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get a specific metric by ID
    pub async fn get_by_id(&self, id: &str) -> Option<HttpMetric> {
        self.metrics
            .read()
            .await
            .iter()
            .find(|m| m.id == id)
            .cloned()
    }

    /// Get metrics count
    pub async fn count(&self) -> usize {
        self.metrics.read().await.len()
    }

    /// Get metrics summary statistics
    pub async fn get_stats(&self) -> MetricsStats {
        let metrics = self.metrics.read().await;

        let total_requests = metrics.len();
        let successful_requests = metrics
            .iter()
            .filter(|m| m.response_status.map(|s| s < 400).unwrap_or(false))
            .count();

        let failed_requests = metrics
            .iter()
            .filter(|m| m.response_status.map(|s| s >= 400).unwrap_or(false) || m.error.is_some())
            .count();

        let avg_duration = if !metrics.is_empty() {
            let total: u64 = metrics.iter().filter_map(|m| m.duration_ms).sum();
            let count = metrics.iter().filter(|m| m.duration_ms.is_some()).count();
            if count > 0 {
                Some(total / count as u64)
            } else {
                None
            }
        } else {
            None
        };

        // Method counts
        let mut methods: HashMap<String, usize> = HashMap::new();
        for metric in metrics.iter() {
            *methods.entry(metric.method.clone()).or_insert(0) += 1;
        }

        // Status code counts
        let mut status_codes: HashMap<u16, usize> = HashMap::new();
        for metric in metrics.iter() {
            if let Some(status) = metric.response_status {
                *status_codes.entry(status).or_insert(0) += 1;
            }
        }

        drop(metrics);

        // Calculate percentiles from histogram
        let histogram = self.duration_histogram.read().await;
        let percentiles = if !histogram.is_empty() {
            Some(DurationPercentiles {
                min: histogram.min(),
                p50: histogram.value_at_quantile(0.50),
                p90: histogram.value_at_quantile(0.90),
                p95: histogram.value_at_quantile(0.95),
                p99: histogram.value_at_quantile(0.99),
                p999: histogram.value_at_quantile(0.999),
                max: histogram.max(),
            })
        } else {
            None
        };

        MetricsStats {
            total_requests,
            successful_requests,
            failed_requests,
            avg_duration_ms: avg_duration,
            percentiles,
            methods,
            status_codes,
        }
    }

    // ========== TCP Connection Tracking ==========

    /// Record a new TCP connection
    pub async fn record_tcp_connection(
        &self,
        stream_id: String,
        remote_addr: String,
        local_addr: String,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let metric = TcpMetric {
            id: id.clone(),
            stream_id,
            timestamp,
            remote_addr,
            local_addr,
            state: TcpConnectionState::Active,
            bytes_received: 0,
            bytes_sent: 0,
            duration_ms: None,
            closed_at: None,
            error: None,
        };

        let mut connections = self.tcp_connections.write().await;
        connections.push(metric.clone());

        // Enforce max entries (keep most recent)
        if connections.len() > self.max_entries {
            let overflow = connections.len() - self.max_entries;
            connections.drain(0..overflow);
        }

        drop(connections);

        // Broadcast the new connection event
        let _ = self.update_tx.send(MetricsEvent::TcpConnection { metric });

        id
    }

    /// Update TCP connection with transferred bytes
    pub async fn update_tcp_connection(&self, id: &str, bytes_received: u64, bytes_sent: u64) {
        let mut connections = self.tcp_connections.write().await;

        if let Some(conn) = connections.iter_mut().find(|c| c.id == id) {
            conn.bytes_received = bytes_received;
            conn.bytes_sent = bytes_sent;

            // Broadcast update (throttled for performance)
            drop(connections);
            let _ = self.update_tx.send(MetricsEvent::TcpUpdate {
                id: id.to_string(),
                bytes_received,
                bytes_sent,
            });
        }
    }

    /// Close a TCP connection
    pub async fn close_tcp_connection(&self, id: &str, error: Option<String>) {
        let mut connections = self.tcp_connections.write().await;

        if let Some(conn) = connections.iter_mut().find(|c| c.id == id) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            let duration_ms = now - conn.timestamp;
            conn.duration_ms = Some(duration_ms);
            conn.closed_at = Some(now);
            conn.state = if error.is_some() {
                TcpConnectionState::Error
            } else {
                TcpConnectionState::Closed
            };
            conn.error = error.clone();

            let bytes_received = conn.bytes_received;
            let bytes_sent = conn.bytes_sent;

            drop(connections);

            // Broadcast close event
            let _ = self.update_tx.send(MetricsEvent::TcpClosed {
                id: id.to_string(),
                bytes_received,
                bytes_sent,
                duration_ms,
                error,
            });
        }
    }

    /// Get all TCP connections
    pub async fn get_all_tcp_connections(&self) -> Vec<TcpMetric> {
        self.tcp_connections.read().await.clone()
    }

    /// Get TCP connections with pagination
    pub async fn get_tcp_connections_paginated(
        &self,
        offset: usize,
        limit: usize,
    ) -> Vec<TcpMetric> {
        let connections = self.tcp_connections.read().await;
        connections
            .iter()
            .rev() // Most recent first
            .skip(offset)
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get a specific TCP connection by ID
    pub async fn get_tcp_connection_by_id(&self, id: &str) -> Option<TcpMetric> {
        self.tcp_connections
            .read()
            .await
            .iter()
            .find(|c| c.id == id)
            .cloned()
    }

    /// Get TCP connections count
    pub async fn tcp_connections_count(&self) -> usize {
        self.tcp_connections.read().await.len()
    }

    /// Get active TCP connections count
    pub async fn active_tcp_connections_count(&self) -> usize {
        self.tcp_connections
            .read()
            .await
            .iter()
            .filter(|c| c.state == TcpConnectionState::Active)
            .count()
    }

    // ========== General Methods ==========

    /// Clear all metrics
    pub async fn clear(&self) {
        self.metrics.write().await.clear();
        self.tcp_connections.write().await.clear();

        // Reset histogram
        let mut histogram = self.duration_histogram.write().await;
        histogram.clear();
        drop(histogram);

        // Reset cached stats
        let mut stats = self.cached_stats.write().await;
        *stats = MetricsStats {
            total_requests: 0,
            successful_requests: 0,
            failed_requests: 0,
            avg_duration_ms: None,
            percentiles: None,
            methods: HashMap::new(),
            status_codes: HashMap::new(),
        };
    }

    /// Parse body data based on content type
    fn parse_body(body: &[u8], content_type: &str) -> Option<BodyData> {
        let content_type_lower = content_type.to_lowercase();

        // Try to parse as JSON first (for any content-type if body is small and looks like JSON)
        if content_type_lower.contains("application/json") || body.len() < 1_000_000 {
            if let Ok(json) = serde_json::from_slice::<serde_json::Value>(body) {
                return Some(BodyData {
                    content_type: content_type.to_string(),
                    size: body.len(),
                    data: BodyContent::Json(json),
                });
            }
        }

        // Check if text (fallback if JSON parsing failed)
        if content_type_lower.contains("text/")
            || content_type_lower.contains("application/xml")
            || content_type_lower.contains("application/x-www-form-urlencoded")
            || body.len() < 1_000_000
        // Try to parse as text for small bodies
        {
            if let Ok(text) = String::from_utf8(body.to_vec()) {
                return Some(BodyData {
                    content_type: content_type.to_string(),
                    size: body.len(),
                    data: BodyContent::Text(text),
                });
            }
        }

        // Binary data - just store metadata
        Some(BodyData {
            content_type: content_type.to_string(),
            size: body.len(),
            data: BodyContent::Binary { size: body.len() },
        })
    }
}

/// Duration percentiles
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DurationPercentiles {
    pub min: u64,
    pub p50: u64, // median
    pub p90: u64,
    pub p95: u64,
    pub p99: u64,
    pub p999: u64, // p99.9
    pub max: u64,
}

/// Metrics statistics summary
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MetricsStats {
    pub total_requests: usize,
    pub successful_requests: usize,
    pub failed_requests: usize,
    pub avg_duration_ms: Option<u64>,
    pub percentiles: Option<DurationPercentiles>,
    pub methods: HashMap<String, usize>,
    pub status_codes: HashMap<u16, usize>,
}

impl Default for MetricsStore {
    fn default() -> Self {
        Self::new(1000) // Default to storing last 1000 requests
    }
}
