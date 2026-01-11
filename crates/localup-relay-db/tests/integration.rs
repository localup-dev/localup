//! Integration tests for tunnel-relay-db
//!
//! Tests database operations with real SQLite in-memory database

use chrono::Utc;
use localup_relay_db::{connect, entities::captured_request, migrate};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, ModelTrait, PaginatorTrait,
    QueryFilter, Set,
};

/// Helper to create a test database
async fn setup_test_db() -> sea_orm::DatabaseConnection {
    let db = connect("sqlite::memory:")
        .await
        .expect("Failed to connect to in-memory database");

    migrate(&db).await.expect("Failed to run migrations");

    db
}

#[tokio::test]
async fn test_database_connection() {
    let db = connect("sqlite::memory:").await.expect("Failed to connect");

    let backend = db.get_database_backend();
    assert!(matches!(backend, sea_orm::DatabaseBackend::Sqlite));
}

#[tokio::test]
async fn test_migrations_run_successfully() {
    let db = connect("sqlite::memory:").await.expect("Failed to connect");

    let result = migrate(&db).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_create_captured_request() {
    let db = setup_test_db().await;

    let request = captured_request::ActiveModel {
        id: Set("req-123".to_string()),
        localup_id: Set("localup-1".to_string()),
        method: Set("GET".to_string()),
        path: Set("/api/users".to_string()),
        host: Set(Some("example.com".to_string())),
        headers: Set(r#"[["Content-Type","application/json"]]"#.to_string()),
        body: Set(None),
        status: Set(None),
        response_headers: Set(None),
        response_body: Set(None),
        created_at: Set(Utc::now()),
        responded_at: Set(None),
        latency_ms: Set(None),
    };

    let result = request.insert(&db).await;
    assert!(result.is_ok());

    let inserted = result.unwrap();
    assert_eq!(inserted.id, "req-123");
    assert_eq!(inserted.method, "GET");
    assert_eq!(inserted.path, "/api/users");
}

#[tokio::test]
async fn test_read_captured_request() {
    let db = setup_test_db().await;

    // Insert a request
    let request = captured_request::ActiveModel {
        id: Set("req-456".to_string()),
        localup_id: Set("localup-2".to_string()),
        method: Set("POST".to_string()),
        path: Set("/api/data".to_string()),
        host: Set(Some("api.example.com".to_string())),
        headers: Set("[]".to_string()),
        body: Set(Some(r#"{"key":"value"}"#.to_string())),
        status: Set(Some(200)),
        response_headers: Set(Some("[]".to_string())),
        response_body: Set(Some(r#"{"result":"success"}"#.to_string())),
        created_at: Set(Utc::now()),
        responded_at: Set(Some(Utc::now())),
        latency_ms: Set(Some(150)),
    };

    request.insert(&db).await.expect("Failed to insert");

    // Read it back
    let found = captured_request::Entity::find_by_id("req-456")
        .one(&db)
        .await
        .expect("Failed to query")
        .expect("Request not found");

    assert_eq!(found.id, "req-456");
    assert_eq!(found.method, "POST");
    assert_eq!(found.status, Some(200));
    assert_eq!(found.latency_ms, Some(150));
}

#[tokio::test]
async fn test_update_captured_request_with_response() {
    let db = setup_test_db().await;

    // Insert a request without response
    let request = captured_request::ActiveModel {
        id: Set("req-789".to_string()),
        localup_id: Set("localup-3".to_string()),
        method: Set("PUT".to_string()),
        path: Set("/api/update".to_string()),
        host: Set(None),
        headers: Set("[]".to_string()),
        body: Set(None),
        status: Set(None),
        response_headers: Set(None),
        response_body: Set(None),
        created_at: Set(Utc::now()),
        responded_at: Set(None),
        latency_ms: Set(None),
    };

    request.insert(&db).await.expect("Failed to insert");

    // Update with response
    let found = captured_request::Entity::find_by_id("req-789")
        .one(&db)
        .await
        .expect("Failed to query")
        .expect("Request not found");

    let mut active: captured_request::ActiveModel = found.into();
    active.status = Set(Some(201));
    active.response_body = Set(Some("Created".to_string()));
    active.responded_at = Set(Some(Utc::now()));
    active.latency_ms = Set(Some(75));

    let updated = active.update(&db).await.expect("Failed to update");

    assert_eq!(updated.status, Some(201));
    assert_eq!(updated.latency_ms, Some(75));
    assert!(updated.responded_at.is_some());
}

#[tokio::test]
async fn test_delete_captured_request() {
    let db = setup_test_db().await;

    // Insert a request
    let request = captured_request::ActiveModel {
        id: Set("req-delete".to_string()),
        localup_id: Set("localup-4".to_string()),
        method: Set("DELETE".to_string()),
        path: Set("/api/resource/1".to_string()),
        host: Set(None),
        headers: Set("[]".to_string()),
        body: Set(None),
        status: Set(Some(204)),
        response_headers: Set(None),
        response_body: Set(None),
        created_at: Set(Utc::now()),
        responded_at: Set(Some(Utc::now())),
        latency_ms: Set(Some(25)),
    };

    let inserted = request.insert(&db).await.expect("Failed to insert");

    // Delete it
    let result = inserted.delete(&db).await;
    assert!(result.is_ok());

    // Verify it's gone
    let found = captured_request::Entity::find_by_id("req-delete")
        .one(&db)
        .await
        .expect("Failed to query");

    assert!(found.is_none());
}

#[tokio::test]
async fn test_query_by_localup_id() {
    let db = setup_test_db().await;

    // Insert multiple requests for the same tunnel
    for i in 1..=3 {
        let request = captured_request::ActiveModel {
            id: Set(format!("req-tunnel-{}", i)),
            localup_id: Set("localup-query-test".to_string()),
            method: Set("GET".to_string()),
            path: Set(format!("/api/item/{}", i)),
            host: Set(None),
            headers: Set("[]".to_string()),
            body: Set(None),
            status: Set(Some(200)),
            response_headers: Set(None),
            response_body: Set(None),
            created_at: Set(Utc::now()),
            responded_at: Set(None),
            latency_ms: Set(None),
        };

        request.insert(&db).await.expect("Failed to insert");
    }

    // Insert a request for a different tunnel
    let other_request = captured_request::ActiveModel {
        id: Set("req-other-tunnel".to_string()),
        localup_id: Set("other-tunnel".to_string()),
        method: Set("GET".to_string()),
        path: Set("/api/other".to_string()),
        host: Set(None),
        headers: Set("[]".to_string()),
        body: Set(None),
        status: Set(None),
        response_headers: Set(None),
        response_body: Set(None),
        created_at: Set(Utc::now()),
        responded_at: Set(None),
        latency_ms: Set(None),
    };

    other_request.insert(&db).await.expect("Failed to insert");

    // Query by localup_id
    let requests = captured_request::Entity::find()
        .filter(captured_request::Column::LocalupId.eq("localup-query-test"))
        .all(&db)
        .await
        .expect("Failed to query");

    assert_eq!(requests.len(), 3);
    assert!(requests
        .iter()
        .all(|r| r.localup_id == "localup-query-test"));
}

#[tokio::test]
async fn test_query_by_status_code() {
    let db = setup_test_db().await;

    // Insert requests with different status codes
    let statuses = [200, 404, 500, 200, 201];
    for (i, status) in statuses.iter().enumerate() {
        let request = captured_request::ActiveModel {
            id: Set(format!("req-status-{}", i)),
            localup_id: Set("localup-status-test".to_string()),
            method: Set("GET".to_string()),
            path: Set("/".to_string()),
            host: Set(None),
            headers: Set("[]".to_string()),
            body: Set(None),
            status: Set(Some(*status)),
            response_headers: Set(None),
            response_body: Set(None),
            created_at: Set(Utc::now()),
            responded_at: Set(Some(Utc::now())),
            latency_ms: Set(None),
        };

        request.insert(&db).await.expect("Failed to insert");
    }

    // Query for 200 status codes
    let successful_requests = captured_request::Entity::find()
        .filter(captured_request::Column::Status.eq(200))
        .all(&db)
        .await
        .expect("Failed to query");

    assert_eq!(successful_requests.len(), 2);
}

#[tokio::test]
async fn test_request_with_large_body() {
    let db = setup_test_db().await;

    let large_body = "x".repeat(10000);

    let request = captured_request::ActiveModel {
        id: Set("req-large-body".to_string()),
        localup_id: Set("localup-5".to_string()),
        method: Set("POST".to_string()),
        path: Set("/api/upload".to_string()),
        host: Set(None),
        headers: Set("[]".to_string()),
        body: Set(Some(large_body.clone())),
        status: Set(None),
        response_headers: Set(None),
        response_body: Set(None),
        created_at: Set(Utc::now()),
        responded_at: Set(None),
        latency_ms: Set(None),
    };

    let inserted = request
        .insert(&db)
        .await
        .expect("Failed to insert large body");

    assert_eq!(inserted.body.as_ref().unwrap().len(), 10000);
}

#[tokio::test]
async fn test_concurrent_inserts() {
    let db = setup_test_db().await;

    let mut handles = vec![];

    // Spawn 10 concurrent insert tasks
    for i in 0..10 {
        let db_clone = db.clone();
        let handle = tokio::spawn(async move {
            let request = captured_request::ActiveModel {
                id: Set(format!("req-concurrent-{}", i)),
                localup_id: Set("localup-concurrent".to_string()),
                method: Set("GET".to_string()),
                path: Set(format!("/api/item/{}", i)),
                host: Set(None),
                headers: Set("[]".to_string()),
                body: Set(None),
                status: Set(Some(200)),
                response_headers: Set(None),
                response_body: Set(None),
                created_at: Set(Utc::now()),
                responded_at: Set(None),
                latency_ms: Set(None),
            };

            request.insert(&db_clone).await
        });

        handles.push(handle);
    }

    // Wait for all inserts to complete
    for handle in handles {
        let result = handle.await.expect("Task panicked");
        assert!(result.is_ok());
    }

    // Verify all 10 were inserted
    let count = captured_request::Entity::find()
        .filter(captured_request::Column::LocalupId.eq("localup-concurrent"))
        .count(&db)
        .await
        .expect("Failed to count");

    assert_eq!(count, 10);
}

#[tokio::test]
async fn test_latency_calculation() {
    let db = setup_test_db().await;

    let created = Utc::now();
    let responded = created + chrono::Duration::milliseconds(250);

    let request = captured_request::ActiveModel {
        id: Set("req-latency".to_string()),
        localup_id: Set("localup-6".to_string()),
        method: Set("GET".to_string()),
        path: Set("/api/slow".to_string()),
        host: Set(None),
        headers: Set("[]".to_string()),
        body: Set(None),
        status: Set(Some(200)),
        response_headers: Set(None),
        response_body: Set(None),
        created_at: Set(created),
        responded_at: Set(Some(responded)),
        latency_ms: Set(Some(250)),
    };

    let inserted = request.insert(&db).await.expect("Failed to insert");

    assert_eq!(inserted.latency_ms, Some(250));

    let duration = inserted
        .responded_at
        .unwrap()
        .signed_duration_since(inserted.created_at);
    assert!(duration.num_milliseconds() >= 250);
}

#[tokio::test]
async fn test_headers_json_encoding() {
    let db = setup_test_db().await;

    let headers_json =
        r#"[["Content-Type","application/json"],["Authorization","Bearer token123"]]"#;

    let request = captured_request::ActiveModel {
        id: Set("req-headers".to_string()),
        localup_id: Set("localup-7".to_string()),
        method: Set("POST".to_string()),
        path: Set("/api/secure".to_string()),
        host: Set(None),
        headers: Set(headers_json.to_string()),
        body: Set(None),
        status: Set(None),
        response_headers: Set(None),
        response_body: Set(None),
        created_at: Set(Utc::now()),
        responded_at: Set(None),
        latency_ms: Set(None),
    };

    let inserted = request.insert(&db).await.expect("Failed to insert");

    // Verify headers can be deserialized
    let parsed: Result<Vec<(String, String)>, _> = serde_json::from_str(&inserted.headers);
    assert!(parsed.is_ok());

    let headers = parsed.unwrap();
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0].0, "Content-Type");
    assert_eq!(headers[1].0, "Authorization");
}

#[tokio::test]
async fn test_nullable_fields() {
    let db = setup_test_db().await;

    // Create request with minimal fields (many nulls)
    let request = captured_request::ActiveModel {
        id: Set("req-minimal".to_string()),
        localup_id: Set("localup-8".to_string()),
        method: Set("GET".to_string()),
        path: Set("/".to_string()),
        host: Set(None),
        headers: Set("[]".to_string()),
        body: Set(None),
        status: Set(None),
        response_headers: Set(None),
        response_body: Set(None),
        created_at: Set(Utc::now()),
        responded_at: Set(None),
        latency_ms: Set(None),
    };

    let inserted = request.insert(&db).await.expect("Failed to insert");

    assert!(inserted.host.is_none());
    assert!(inserted.body.is_none());
    assert!(inserted.status.is_none());
    assert!(inserted.response_headers.is_none());
    assert!(inserted.response_body.is_none());
    assert!(inserted.responded_at.is_none());
    assert!(inserted.latency_ms.is_none());
}
