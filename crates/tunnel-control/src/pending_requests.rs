//! Pending requests tracker
//!
//! Tracks HTTP requests sent through tunnels and routes responses back to the original connections.

use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::oneshot;
use tracing::{debug, warn};
use tunnel_proto::TunnelMessage;

/// Tracks pending HTTP requests awaiting responses
#[derive(Clone)]
pub struct PendingRequests {
    /// Maps stream_id -> oneshot sender for the response
    requests: Arc<DashMap<u32, oneshot::Sender<TunnelMessage>>>,
}

impl PendingRequests {
    pub fn new() -> Self {
        Self {
            requests: Arc::new(DashMap::new()),
        }
    }

    /// Register a new pending request
    /// Returns a receiver that will receive the response
    pub fn register(&self, stream_id: u32) -> oneshot::Receiver<TunnelMessage> {
        let (tx, rx) = oneshot::channel();
        self.requests.insert(stream_id, tx);
        debug!("Registered pending request for stream {}", stream_id);
        rx
    }

    /// Send a response for a pending request
    /// Returns true if the response was delivered, false if the request wasn't found
    pub fn respond(&self, stream_id: u32, response: TunnelMessage) -> bool {
        if let Some((_, tx)) = self.requests.remove(&stream_id) {
            debug!("Routing response for stream {}", stream_id);
            if tx.send(response).is_err() {
                warn!(
                    "Failed to send response for stream {} - receiver dropped",
                    stream_id
                );
                return false;
            }
            return true;
        }
        warn!("No pending request found for stream {}", stream_id);
        false
    }

    /// Cancel a pending request (e.g., on timeout or error)
    pub fn cancel(&self, stream_id: u32) {
        if self.requests.remove(&stream_id).is_some() {
            debug!("Cancelled pending request for stream {}", stream_id);
        }
    }

    /// Get count of pending requests
    pub fn count(&self) -> usize {
        self.requests.len()
    }
}

impl Default for PendingRequests {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_and_respond() {
        let tracker = PendingRequests::new();

        let stream_id = 123;
        let rx = tracker.register(stream_id);

        assert_eq!(tracker.count(), 1);

        let response = TunnelMessage::HttpResponse {
            stream_id,
            status: 200,
            headers: vec![],
            body: None,
        };

        let delivered = tracker.respond(stream_id, response.clone());
        assert!(delivered);
        assert_eq!(tracker.count(), 0);

        let received = rx.await.unwrap();
        assert_eq!(received, response);
    }

    #[tokio::test]
    async fn test_cancel() {
        let tracker = PendingRequests::new();

        let stream_id = 456;
        let _rx = tracker.register(stream_id);

        assert_eq!(tracker.count(), 1);

        tracker.cancel(stream_id);
        assert_eq!(tracker.count(), 0);
    }

    #[tokio::test]
    async fn test_respond_not_found() {
        let tracker = PendingRequests::new();

        let response = TunnelMessage::HttpResponse {
            stream_id: 999,
            status: 200,
            headers: vec![],
            body: None,
        };

        let delivered = tracker.respond(999, response);
        assert!(!delivered);
    }

    #[tokio::test]
    async fn test_respond_with_dropped_receiver() {
        let tracker = PendingRequests::new();

        let stream_id = 789;
        let rx = tracker.register(stream_id);

        // Drop the receiver
        drop(rx);

        let response = TunnelMessage::HttpResponse {
            stream_id,
            status: 200,
            headers: vec![],
            body: None,
        };

        // Should return false because receiver was dropped
        let delivered = tracker.respond(stream_id, response);
        assert!(!delivered);
    }

    #[tokio::test]
    async fn test_multiple_pending_requests() {
        let tracker = PendingRequests::new();

        let mut receivers = vec![];
        for i in 1..=5 {
            let rx = tracker.register(i);
            receivers.push((i, rx));
        }

        assert_eq!(tracker.count(), 5);

        // Respond to each request
        for (stream_id, rx) in receivers {
            let response = TunnelMessage::HttpResponse {
                stream_id,
                status: 200,
                headers: vec![],
                body: None,
            };

            tracker.respond(stream_id, response.clone());
            let received = rx.await.unwrap();
            assert_eq!(received, response);
        }

        assert_eq!(tracker.count(), 0);
    }

    #[tokio::test]
    async fn test_cancel_multiple_requests() {
        let tracker = PendingRequests::new();

        for i in 1..=10 {
            tracker.register(i);
        }

        assert_eq!(tracker.count(), 10);

        // Cancel odd numbered requests
        for i in (1..=10).step_by(2) {
            tracker.cancel(i);
        }

        assert_eq!(tracker.count(), 5);

        // Cancel even numbered requests
        for i in (2..=10).step_by(2) {
            tracker.cancel(i);
        }

        assert_eq!(tracker.count(), 0);
    }

    #[tokio::test]
    async fn test_cancel_nonexistent_request() {
        let tracker = PendingRequests::new();

        // Canceling a non-existent request should not panic
        tracker.cancel(999);
        assert_eq!(tracker.count(), 0);
    }

    #[tokio::test]
    async fn test_concurrent_register_and_respond() {
        let tracker = Arc::new(PendingRequests::new());

        let mut handles = vec![];

        // Spawn tasks to register and respond concurrently
        for i in 1..=20 {
            let tracker = tracker.clone();
            let handle = tokio::spawn(async move {
                let rx = tracker.register(i);

                // Simulate some work
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;

                let response = TunnelMessage::HttpResponse {
                    stream_id: i,
                    status: 200,
                    headers: vec![],
                    body: None,
                };

                tracker.respond(i, response.clone());
                rx.await.unwrap()
            });
            handles.push(handle);
        }

        // Wait for all tasks
        for handle in handles {
            handle.await.unwrap();
        }

        assert_eq!(tracker.count(), 0);
    }

    #[tokio::test]
    async fn test_respond_with_different_message_types() {
        let tracker = PendingRequests::new();

        // Test HttpResponse
        let stream_id_1 = 1;
        let rx1 = tracker.register(stream_id_1);
        let response1 = TunnelMessage::HttpResponse {
            stream_id: stream_id_1,
            status: 404,
            headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
            body: Some(b"Not Found".to_vec()),
        };
        tracker.respond(stream_id_1, response1.clone());
        assert_eq!(rx1.await.unwrap(), response1);

        // Test HttpChunk (can be used as response)
        let stream_id_2 = 2;
        let rx2 = tracker.register(stream_id_2);
        let response2 = TunnelMessage::HttpChunk {
            stream_id: stream_id_2,
            chunk: vec![1, 2, 3, 4, 5],
            is_final: true,
        };
        tracker.respond(stream_id_2, response2.clone());
        assert_eq!(rx2.await.unwrap(), response2);
    }

    #[tokio::test]
    async fn test_double_respond_same_stream() {
        let tracker = PendingRequests::new();

        let stream_id = 100;
        let rx = tracker.register(stream_id);

        let response1 = TunnelMessage::HttpResponse {
            stream_id,
            status: 200,
            headers: vec![],
            body: None,
        };

        let response2 = TunnelMessage::HttpResponse {
            stream_id,
            status: 500,
            headers: vec![],
            body: None,
        };

        // First response succeeds
        assert!(tracker.respond(stream_id, response1.clone()));
        assert_eq!(rx.await.unwrap(), response1);

        // Second response fails (request already removed)
        assert!(!tracker.respond(stream_id, response2));
    }

    #[tokio::test]
    async fn test_register_after_cancel() {
        let tracker = PendingRequests::new();

        let stream_id = 42;

        // Register and cancel
        let rx1 = tracker.register(stream_id);
        tracker.cancel(stream_id);

        // rx1 should never receive anything
        assert!(rx1.await.is_err());

        // Can register same stream ID again
        let rx2 = tracker.register(stream_id);

        let response = TunnelMessage::HttpResponse {
            stream_id,
            status: 200,
            headers: vec![],
            body: None,
        };

        tracker.respond(stream_id, response.clone());
        assert_eq!(rx2.await.unwrap(), response);
    }

    #[tokio::test]
    async fn test_clone_tracker() {
        let tracker = PendingRequests::new();
        let tracker_clone = tracker.clone();

        let stream_id = 123;
        let rx = tracker.register(stream_id);

        assert_eq!(tracker_clone.count(), 1);

        let response = TunnelMessage::HttpResponse {
            stream_id,
            status: 200,
            headers: vec![],
            body: None,
        };

        // Respond through clone
        tracker_clone.respond(stream_id, response.clone());

        // Receive through original receiver
        assert_eq!(rx.await.unwrap(), response);
        assert_eq!(tracker.count(), 0);
        assert_eq!(tracker_clone.count(), 0);
    }
}
