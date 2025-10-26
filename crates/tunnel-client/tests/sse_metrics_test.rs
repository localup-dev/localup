//! Integration tests for Server-Sent Events (SSE) metrics streaming
//!
//! This test verifies that the metrics server correctly streams real-time
//! updates via SSE when new requests are recorded.

use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tunnel_client::metrics::MetricsStore;
use tunnel_client::metrics_server::MetricsServer;

/// Helper to start a metrics server on a random port
async fn start_test_server() -> (MetricsStore, tokio::task::JoinHandle<()>, u16) {
    let metrics = MetricsStore::new(100);
    let metrics_clone = metrics.clone();

    // Bind to port 0 to get a random available port
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 0));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");
    let bound_addr = listener.local_addr().expect("Failed to get local addr");
    let port = bound_addr.port();

    let handle = tokio::spawn(async move {
        let server = MetricsServer::new(
            bound_addr,
            metrics_clone,
            vec![],
            "http://localhost:3000".to_string(),
        );
        drop(listener); // Close the listener we created
        let _ = server.run().await;
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(200)).await;

    (metrics, handle, port)
}

/// Parse SSE event from stream
async fn read_sse_event(
    reader: &mut BufReader<TcpStream>,
) -> Result<Option<serde_json::Value>, Box<dyn std::error::Error>> {
    let mut data = String::new();

    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            return Ok(None); // Connection closed
        }

        // SSE format: "data: {json}\n\n"
        if line.starts_with("data: ") {
            data = line.trim_start_matches("data: ").trim().to_string();
        } else if line == "\n" || line == "\r\n" {
            // End of event
            if !data.is_empty() {
                let json: serde_json::Value = serde_json::from_str(&data)?;
                return Ok(Some(json));
            }
        }
    }
}

#[tokio::test]
async fn test_sse_stream_initial_stats() {
    let (metrics, _handle, port) = start_test_server().await;

    // Connect to SSE endpoint
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect");

    // Send SSE request
    let request = format!(
        "GET /api/metrics/stream HTTP/1.1\r\n\
         Host: localhost:{}\r\n\
         Connection: keep-alive\r\n\
         \r\n",
        port
    );

    stream
        .write_all(request.as_bytes())
        .await
        .expect("Failed to write request");
    stream.flush().await.expect("Failed to flush");

    // Read response headers
    let mut reader = BufReader::new(stream);
    let mut headers_complete = false;

    while !headers_complete {
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .await
            .expect("Failed to read line");

        if line == "\r\n" || line == "\n" {
            headers_complete = true;
        } else if line.starts_with("HTTP/1.1") {
            assert!(line.contains("200 OK"), "Expected 200 OK response");
        } else if line.to_lowercase().starts_with("content-type") {
            assert!(
                line.contains("text/event-stream"),
                "Expected SSE content type"
            );
        }
    }

    // Read initial stats event (should be sent immediately)
    let result = timeout(Duration::from_secs(2), read_sse_event(&mut reader)).await;

    assert!(
        result.is_ok(),
        "Should receive initial stats within timeout"
    );
    let event = result
        .unwrap()
        .expect("Should parse SSE event")
        .expect("Should have event data");

    // Verify it's a stats event
    assert_eq!(
        event["type"].as_str().unwrap(),
        "stats",
        "First event should be stats"
    );
    assert!(event["stats"].is_object(), "Should contain stats object");

    // Add a new request to metrics
    let _id = metrics
        .record_request(
            "abc123".to_string(),
            "GET".to_string(),
            "/test".to_string(),
            vec![],
            None,
        )
        .await;

    // Read the request event via SSE
    let result = timeout(Duration::from_secs(2), read_sse_event(&mut reader)).await;

    assert!(
        result.is_ok(),
        "Should receive request event within timeout"
    );
    let event = result
        .unwrap()
        .expect("Should parse SSE event")
        .expect("Should have event data");

    // Verify it's a request event
    assert_eq!(
        event["type"].as_str().unwrap(),
        "request",
        "Second event should be request"
    );
    assert!(event["metric"].is_object(), "Should contain metric object");

    let metric = &event["metric"];
    assert_eq!(
        metric["method"].as_str().unwrap(),
        "GET",
        "Should have correct method"
    );
    assert_eq!(
        metric["uri"].as_str().unwrap(),
        "/test",
        "Should have correct URI"
    );
}

#[tokio::test]
async fn test_sse_stream_request_and_response() {
    let (metrics, _handle, port) = start_test_server().await;

    // Connect to SSE endpoint
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect");

    let request = format!(
        "GET /api/metrics/stream HTTP/1.1\r\n\
         Host: localhost:{}\r\n\
         Connection: keep-alive\r\n\
         \r\n",
        port
    );

    stream.write_all(request.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();

    let mut reader = BufReader::new(stream);

    // Skip headers
    let mut headers_complete = false;
    while !headers_complete {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        if line == "\r\n" || line == "\n" {
            headers_complete = true;
        }
    }

    // Skip initial stats event
    let _ = timeout(Duration::from_secs(2), read_sse_event(&mut reader))
        .await
        .unwrap();

    // Record a request
    let metric_id = metrics
        .record_request(
            "stream1".to_string(),
            "POST".to_string(),
            "/api/data".to_string(),
            vec![("Content-Type".to_string(), "application/json".to_string())],
            Some(b"{\"test\":true}".to_vec()),
        )
        .await;

    // Read request event
    let result = timeout(Duration::from_secs(2), read_sse_event(&mut reader)).await;
    let event = result.unwrap().unwrap().unwrap();
    assert_eq!(event["type"].as_str().unwrap(), "request");

    // Record response
    metrics
        .record_response(
            &metric_id,
            200,
            vec![("Content-Type".to_string(), "application/json".to_string())],
            Some(b"{\"success\":true}".to_vec()),
            42,
        )
        .await;

    // Read next event (might be stats or response due to debouncing)
    let mut event = timeout(Duration::from_secs(2), read_sse_event(&mut reader))
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    // Skip stats event if it comes first (due to debouncing)
    if event["type"].as_str().unwrap() == "stats" {
        event = timeout(Duration::from_secs(2), read_sse_event(&mut reader))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    assert_eq!(
        event["type"].as_str().unwrap(),
        "response",
        "Should be response event"
    );
    assert_eq!(
        event["id"].as_str().unwrap(),
        metric_id,
        "Should have correct metric ID"
    );
    assert_eq!(
        event["status"].as_u64().unwrap(),
        200,
        "Should have correct status"
    );
    assert_eq!(
        event["duration_ms"].as_u64().unwrap(),
        42,
        "Should have correct duration"
    );
}

#[tokio::test]
async fn test_sse_stream_error_event() {
    let (metrics, _handle, port) = start_test_server().await;

    // Connect to SSE endpoint
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect");

    let request = format!(
        "GET /api/metrics/stream HTTP/1.1\r\n\
         Host: localhost:{}\r\n\
         Connection: keep-alive\r\n\
         \r\n",
        port
    );

    stream.write_all(request.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();

    let mut reader = BufReader::new(stream);

    // Skip headers
    let mut headers_complete = false;
    while !headers_complete {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        if line == "\r\n" || line == "\n" {
            headers_complete = true;
        }
    }

    // Skip initial stats
    let _ = timeout(Duration::from_secs(2), read_sse_event(&mut reader))
        .await
        .unwrap();

    // Record a request
    let metric_id = metrics
        .record_request(
            "stream2".to_string(),
            "GET".to_string(),
            "/failing".to_string(),
            vec![],
            None,
        )
        .await;

    // Skip request event (and possibly stats)
    let _ = timeout(Duration::from_secs(2), read_sse_event(&mut reader))
        .await
        .unwrap();

    // Record an error
    metrics
        .record_error(&metric_id, "Connection timeout".to_string(), 5000)
        .await;

    // Read next event (might be stats or error due to debouncing)
    let mut event = timeout(Duration::from_secs(2), read_sse_event(&mut reader))
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    // Skip stats event if it comes first (due to debouncing)
    if event["type"].as_str().unwrap() == "stats" {
        event = timeout(Duration::from_secs(2), read_sse_event(&mut reader))
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    assert_eq!(
        event["type"].as_str().unwrap(),
        "error",
        "Should be error event"
    );
    assert_eq!(
        event["id"].as_str().unwrap(),
        metric_id,
        "Should have correct metric ID"
    );
    assert_eq!(
        event["error"].as_str().unwrap(),
        "Connection timeout",
        "Should have correct error message"
    );
    assert_eq!(
        event["duration_ms"].as_u64().unwrap(),
        5000,
        "Should have correct duration"
    );
}

#[tokio::test]
async fn test_sse_multiple_clients() {
    let (metrics, _handle, port) = start_test_server().await;

    // Connect two SSE clients
    let mut stream1 = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect client 1");
    let mut stream2 = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .expect("Failed to connect client 2");

    let request = format!(
        "GET /api/metrics/stream HTTP/1.1\r\n\
         Host: localhost:{}\r\n\
         Connection: keep-alive\r\n\
         \r\n",
        port
    );

    stream1.write_all(request.as_bytes()).await.unwrap();
    stream1.flush().await.unwrap();

    stream2.write_all(request.as_bytes()).await.unwrap();
    stream2.flush().await.unwrap();

    let mut reader1 = BufReader::new(stream1);
    let mut reader2 = BufReader::new(stream2);

    // Skip headers for both clients
    for reader in [&mut reader1, &mut reader2] {
        let mut headers_complete = false;
        while !headers_complete {
            let mut line = String::new();
            reader.read_line(&mut line).await.unwrap();
            if line == "\r\n" || line == "\n" {
                headers_complete = true;
            }
        }
        // Skip initial stats
        let _ = timeout(Duration::from_secs(2), read_sse_event(reader))
            .await
            .unwrap();
    }

    // Record a request - both clients should receive it
    let _id = metrics
        .record_request(
            "multi1".to_string(),
            "GET".to_string(),
            "/broadcast".to_string(),
            vec![],
            None,
        )
        .await;

    // Both clients should receive the event
    let event1 = timeout(Duration::from_secs(2), read_sse_event(&mut reader1))
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let event2 = timeout(Duration::from_secs(2), read_sse_event(&mut reader2))
        .await
        .unwrap()
        .unwrap()
        .unwrap();

    assert_eq!(event1["type"].as_str().unwrap(), "request");
    assert_eq!(event2["type"].as_str().unwrap(), "request");

    assert_eq!(event1["metric"]["uri"].as_str().unwrap(), "/broadcast");
    assert_eq!(event2["metric"]["uri"].as_str().unwrap(), "/broadcast");
}
