//! Service layer for metrics operations
//!
//! This module contains the business logic for metrics operations,
//! separated from HTTP concerns.

use crate::metrics::{HttpMetric, MetricsStats, MetricsStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tunnel_proto::Endpoint;
use utoipa::ToSchema;

/// Service errors that can occur during metrics operations
#[derive(Debug, Error)]
pub enum MetricsServiceError {
    /// Metric not found by ID
    #[error("Metric with ID '{0}' not found")]
    MetricNotFound(String),

    /// Replay request failed
    #[error("Failed to replay request: {0}")]
    ReplayFailed(String),

    /// Invalid request parameters
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

/// Replay request structure
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct ReplayRequest {
    /// HTTP method (GET, POST, PUT, DELETE, PATCH)
    pub method: String,
    /// Request URI path
    pub uri: String,
    /// HTTP headers as key-value pairs
    pub headers: Vec<(String, String)>,
    /// Optional request body
    pub body: Option<serde_json::Value>,
}

/// Replay response structure
#[derive(Debug, Serialize, ToSchema)]
pub struct ReplayResponse {
    /// HTTP status code if successful
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    /// Response body content
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Error message if request failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Metrics service containing business logic
#[derive(Clone)]
pub struct MetricsService {
    metrics: MetricsStore,
    endpoints: Arc<Vec<Endpoint>>,
    local_upstream: Arc<String>,
}

impl MetricsService {
    /// Create a new metrics service
    pub fn new(metrics: MetricsStore, endpoints: Vec<Endpoint>, local_upstream: String) -> Self {
        Self {
            metrics,
            endpoints: Arc::new(endpoints),
            local_upstream: Arc::new(local_upstream),
        }
    }

    /// Get tunnel endpoint information
    pub fn get_info(&self) -> Vec<Endpoint> {
        (*self.endpoints).clone()
    }

    /// Get metrics with pagination
    pub async fn get_metrics(&self, offset: usize, limit: usize) -> Vec<HttpMetric> {
        self.metrics.get_paginated(offset, limit).await
    }

    /// Get aggregated statistics
    pub async fn get_stats(&self) -> MetricsStats {
        self.metrics.get_stats().await
    }

    /// Get a specific metric by ID
    pub async fn get_metric_by_id(&self, id: &str) -> Result<HttpMetric, MetricsServiceError> {
        self.metrics
            .get_by_id(id)
            .await
            .ok_or_else(|| MetricsServiceError::MetricNotFound(id.to_string()))
    }

    /// Clear all metrics
    pub async fn clear_metrics(&self) {
        self.metrics.clear().await;
    }

    /// Replay a request to the local upstream server
    pub async fn replay_request(
        &self,
        replay_req: ReplayRequest,
    ) -> Result<ReplayResponse, MetricsServiceError> {
        // Validate request
        if replay_req.method.is_empty() {
            return Err(MetricsServiceError::InvalidRequest(
                "Method cannot be empty".to_string(),
            ));
        }
        if replay_req.uri.is_empty() {
            return Err(MetricsServiceError::InvalidRequest(
                "URI cannot be empty".to_string(),
            ));
        }

        // Build target URL
        let target_url = format!("{}{}", self.local_upstream, replay_req.uri);

        // Build HTTP client request
        let client = reqwest::Client::new();
        let mut request = match replay_req.method.as_str() {
            "GET" => client.get(&target_url),
            "POST" => client.post(&target_url),
            "PUT" => client.put(&target_url),
            "DELETE" => client.delete(&target_url),
            "PATCH" => client.patch(&target_url),
            _ => client.get(&target_url),
        };

        // Add headers
        for (name, value) in &replay_req.headers {
            let name_lower = name.to_lowercase();
            if !["host", "connection", "content-length"].contains(&name_lower.as_str()) {
                request = request.header(name, value);
            }
        }

        // Add body if present
        if let Some(body_data) = &replay_req.body {
            if let Some(data_obj) = body_data.as_object() {
                if let Some(data_type) = data_obj.get("type").and_then(|t| t.as_str()) {
                    match data_type {
                        "Json" => {
                            if let Some(json_value) = data_obj.get("value") {
                                if let Ok(body_str) = serde_json::to_string(json_value) {
                                    request = request
                                        .header("Content-Type", "application/json")
                                        .body(body_str);
                                }
                            }
                        }
                        "Text" => {
                            if let Some(text) = data_obj.get("value").and_then(|v| v.as_str()) {
                                request = request.body(text.to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        // Make the request
        match request.send().await {
            Ok(response) => {
                let status = response.status().as_u16();
                let content_type = response
                    .headers()
                    .get("content-type")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_lowercase();

                let content_encoding = response
                    .headers()
                    .get("content-encoding")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_lowercase();

                // Get the response body as bytes
                let body_bytes = response
                    .bytes()
                    .await
                    .map_err(|e| MetricsServiceError::ReplayFailed(e.to_string()))?;

                // Check if response is compressed/encoded
                let is_compressed = content_encoding.contains("gzip")
                    || content_encoding.contains("deflate")
                    || content_encoding.contains("br")
                    || content_encoding.contains("compress");

                // Try to decode as readable text
                let body = if is_compressed {
                    // Don't try to display compressed data
                    let size = body_bytes.len();
                    format!(
                        "[Compressed response: {} bytes, encoding: {}, content-type: {}]",
                        size, content_encoding, content_type
                    )
                } else if content_type.contains("json")
                    || content_type.contains("text")
                    || content_type.contains("xml")
                    || content_type.contains("javascript")
                    || content_type.contains("html")
                {
                    // Try to decode as UTF-8 text
                    String::from_utf8_lossy(&body_bytes).to_string()
                } else {
                    // For other binary data, show summary instead
                    let size = body_bytes.len();
                    if size > 0 {
                        format!(
                            "[Binary response: {} bytes, content-type: {}]",
                            size, content_type
                        )
                    } else {
                        "[Empty response]".to_string()
                    }
                };

                Ok(ReplayResponse {
                    status: Some(status),
                    body: Some(body),
                    error: None,
                })
            }
            Err(e) => Ok(ReplayResponse {
                status: None,
                body: None,
                error: Some(format!("Failed to replay request: {}", e)),
            }),
        }
    }
}
