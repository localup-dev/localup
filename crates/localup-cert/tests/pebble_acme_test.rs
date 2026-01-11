//! Integration tests for ACME certificate provisioning using Pebble test server
//!
//! These tests use testcontainers to spin up Pebble (Let's Encrypt's test ACME server)
//! and challtestsrv (DNS mock server) to test the full ACME flow including DNS-01
//! challenges for wildcard certificates.
//!
//! To run these tests:
//! ```
//! # Requires Docker to be running
//! cargo test -p localup-cert --test pebble_acme_test -- --ignored
//! ```

use std::sync::Once;
use std::time::Duration;
use testcontainers::{
    core::{ContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage, ImageExt,
};

// Install the Rustls crypto provider once for all tests
static INIT: Once = Once::new();

fn init_crypto_provider() {
    INIT.call_once(|| {
        rustls::crypto::ring::default_provider()
            .install_default()
            .expect("Failed to install Rustls crypto provider");
    });
}

/// Pebble's minica root CA certificate (from Pebble repository)
/// This is used to verify Pebble's TLS server certificate
const PEBBLE_MINICA_ROOT_CA: &str = r#"-----BEGIN CERTIFICATE-----
MIIDPzCCAiegAwIBAgIIU0Xm9UFdQxUwDQYJKoZIhvcNAQELBQAwIDEeMBwGA1UE
AxMVbWluaWNhIHJvb3QgY2EgNTM0NWU2MCAXDTI1MDkwMzIzANDAwNVoYDzIxMjUw
OTAzMjM0MDA1WjAgMR4wHAYDVQQDExVtaW5pY2Egcm9vdCBjYSA1MzQ1ZTYwggEi
MA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQC5WgZNoVJandj43kkLyU50vzCZ
alozvdRo3OFiKoDtmqKPNWRNO2hC9AUNxTDJco51Yc42u/WV3fPbbhSznTiOOVtn
Ajm6iq4I5nZYltGGZetGDOQWr78y2gWY+SG078MuOO2hyDIiKtVc3xiXYA+8Hluu
9F8KbqSS1h55yxZ9b87eKR+B0zu2ahzBCIHKmKWgc6N13l7aDxxY3D6uq8gtJRU0
toumyLbdzGcupVvjbjDP11nl07RESDWBLG1/g3ktJvqIa4BWgU2HMh4rND6y8OD3
Hy3H8MY6CElL+MOCbFJjWqhtOxeFyZZV9q3kYnk9CAuQJKMEGuN4GU6tzhW1AgMB
AAGjezB5MA4GA1UdDwEB/wQEAwIChDATBgNVHSUEDDAKBggrBgEFBQcDATASBgNV
HRMBAf8ECDAGAQH/AgEAMB0GA1UdDgQWBBSu8RGpErgYUoYnQuwCq+/ggTiEjDAf
BgNVHSMEGDAWgBSu8RGpErgYUoYnQuwCq+/ggTiEjDANBgkqhkiG9w0BAQsFAAOC
AQEAXDVYov1+f6EL7S41LhYQkEX/GyNNzsEvqxE9U0+3Iri5JfkcNOiA9O9L6Z+Y
bqcsXV93s3vi4r4WSWuc//wHyJYrVe5+tK4nlFpbJOvfBUtnoBDyKNxXzZCxFJVh
f9uc8UejRfQMFbDbhWY/x83y9BDufJHHq32OjCIN7gp2UR8rnfYvlz7Zg4qkJBsn
DG4dwd+pRTCFWJOVIG0JoNhK3ZmE7oJ1N4H38XkZ31NPcMksKxpsLLIS9+mosZtg
4olL7tMPJklx5ZaeMFaKRDq4Gdxkbw4+O4vRgNm3Z8AXWKknOdfgdpqLUPPhRcP4
v1lhy71EhBuXXwRQJry0lTdF+w==
-----END CERTIFICATE-----"#;

/// Pebble ACME server ports
const PEBBLE_HTTPS_PORT: u16 = 14000;
const PEBBLE_MGMT_PORT: u16 = 15000;

/// Challtestsrv ports for DNS challenge mock
const CHALLTESTSRV_HTTP_PORT: u16 = 8055;
const CHALLTESTSRV_DNS_PORT: u16 = 8053;

/// Docker network name for Pebble and challtestsrv communication
const TEST_NETWORK: &str = "pebble-test-net";
/// Container name for challtestsrv (used as DNS server hostname)
const CHALLTESTSRV_CONTAINER_NAME: &str = "challtestsrv";

/// Add a TXT record to challtestsrv for DNS-01 challenge
/// Note: challtestsrv requires FQDN with trailing period
async fn add_dns_txt_record(mgmt_url: &str, name: &str, value: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    // challtestsrv requires trailing period for FQDN
    let fqdn = if name.ends_with('.') {
        name.to_string()
    } else {
        format!("{}.", name)
    };

    let url = format!("{}/set-txt", mgmt_url);
    let body = serde_json::json!({
        "host": fqdn,
        "value": value
    });

    client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to add DNS TXT record: {}", e))?;

    Ok(())
}

/// Remove a TXT record from challtestsrv
/// Note: challtestsrv requires FQDN with trailing period
async fn clear_dns_txt_record(mgmt_url: &str, name: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| e.to_string())?;

    // challtestsrv requires trailing period for FQDN
    let fqdn = if name.ends_with('.') {
        name.to_string()
    } else {
        format!("{}.", name)
    };

    let url = format!("{}/clear-txt", mgmt_url);
    let body = serde_json::json!({
        "host": fqdn
    });

    client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Failed to clear DNS TXT record: {}", e))?;

    Ok(())
}

/// Test that we can start Pebble and challtestsrv containers
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_pebble_container_starts() {
    // Start Pebble container
    // Note: with_wait_for must be called on GenericImage BEFORE ImageExt methods
    // Using ghcr.io registry for official Let's Encrypt images
    // Using seconds-based wait since Pebble logs may not be captured by testcontainers
    let pebble = GenericImage::new("ghcr.io/letsencrypt/pebble", "latest")
        .with_wait_for(WaitFor::seconds(3))
        .with_exposed_port(ContainerPort::Tcp(PEBBLE_HTTPS_PORT))
        .with_exposed_port(ContainerPort::Tcp(PEBBLE_MGMT_PORT))
        .with_env_var("PEBBLE_VA_NOSLEEP", "1")
        .with_env_var("PEBBLE_VA_ALWAYS_VALID", "1")
        .start()
        .await
        .expect("Failed to start Pebble container");

    let pebble_port = pebble
        .get_host_port_ipv4(PEBBLE_HTTPS_PORT)
        .await
        .expect("Failed to get Pebble port");

    println!("Pebble started on port {}", pebble_port);

    // Verify Pebble is responding
    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();

    let directory_url = format!("https://localhost:{}/dir", pebble_port);
    let response = client.get(&directory_url).send().await;

    assert!(
        response.is_ok(),
        "Pebble directory should be accessible: {:?}",
        response.err()
    );

    let directory = response.unwrap();
    assert!(
        directory.status().is_success(),
        "Pebble directory should return success status"
    );

    println!("Pebble ACME directory accessible at {}", directory_url);
}

/// Test that challtestsrv container starts and accepts DNS records
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_challtestsrv_container_starts() {
    // Start challtestsrv container
    // Using ghcr.io registry for official Let's Encrypt images
    // Using seconds-based wait since logs may not be captured by testcontainers
    let challtestsrv = GenericImage::new("ghcr.io/letsencrypt/pebble-challtestsrv", "latest")
        .with_wait_for(WaitFor::seconds(3))
        .with_exposed_port(ContainerPort::Tcp(CHALLTESTSRV_HTTP_PORT))
        .start()
        .await
        .expect("Failed to start challtestsrv container");

    let mgmt_port = challtestsrv
        .get_host_port_ipv4(CHALLTESTSRV_HTTP_PORT)
        .await
        .expect("Failed to get challtestsrv port");

    let mgmt_url = format!("http://localhost:{}", mgmt_port);
    println!("Challtestsrv management API at {}", mgmt_url);

    // Test adding a TXT record
    let result = add_dns_txt_record(
        &mgmt_url,
        "_acme-challenge.test.example.com",
        "test-challenge-value",
    )
    .await;

    assert!(
        result.is_ok(),
        "Should be able to add DNS TXT record: {:?}",
        result.err()
    );

    println!("Successfully added DNS TXT record via challtestsrv");

    // Clean up
    let _ = clear_dns_txt_record(&mgmt_url, "_acme-challenge.test.example.com").await;
}

/// Helper to create Docker network (idempotent - ignores if exists)
async fn ensure_docker_network(name: &str) {
    let output = std::process::Command::new("docker")
        .args(["network", "create", name])
        .output()
        .expect("Failed to execute docker network create");

    // Ignore error if network already exists
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("already exists") {
            panic!("Failed to create Docker network: {}", stderr);
        }
    }
}

/// Test DNS-01 challenge flow for wildcard certificate
///
/// This test demonstrates the full flow:
/// 1. Start Pebble (ACME server)
/// 2. Start challtestsrv (DNS mock)
/// 3. Create ACME account
/// 4. Order wildcard certificate
/// 5. Complete DNS-01 challenge via challtestsrv
/// 6. Finalize order and download certificate
#[tokio::test]
#[ignore = "Requires Docker - full ACME flow test"]
async fn test_wildcard_certificate_dns01_flow() {
    use instant_acme::{Account, ChallengeType, Identifier, NewOrder, OrderStatus, RetryPolicy};

    // Initialize Rustls crypto provider
    init_crypto_provider();

    // Create shared Docker network for Pebble <-> challtestsrv communication
    ensure_docker_network(TEST_NETWORK).await;

    // Start challtestsrv FIRST with a known container name
    // Pebble will query this container for DNS lookups
    // NOTE: Method order matters - GenericImage methods (with_wait_for, with_exposed_port)
    // must come BEFORE ImageExt methods (with_network, with_container_name) that return ContainerRequest
    let challtestsrv = GenericImage::new("ghcr.io/letsencrypt/pebble-challtestsrv", "latest")
        .with_wait_for(WaitFor::seconds(3))
        .with_exposed_port(ContainerPort::Tcp(CHALLTESTSRV_HTTP_PORT))
        .with_exposed_port(ContainerPort::Tcp(CHALLTESTSRV_DNS_PORT))
        .with_network(TEST_NETWORK)
        .with_container_name(CHALLTESTSRV_CONTAINER_NAME)
        .start()
        .await
        .expect("Failed to start challtestsrv");

    // Wait for challtestsrv to be ready before starting Pebble
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Start Pebble with -dnsserver pointing to challtestsrv container
    // The DNS server format is: container_name:port (on shared network)
    let dns_server = format!("{}:{}", CHALLTESTSRV_CONTAINER_NAME, CHALLTESTSRV_DNS_PORT);

    // Pebble image entrypoint is /app (the pebble binary), so cmd is just arguments
    let pebble = GenericImage::new("ghcr.io/letsencrypt/pebble", "latest")
        .with_wait_for(WaitFor::seconds(3))
        .with_exposed_port(ContainerPort::Tcp(PEBBLE_HTTPS_PORT))
        .with_exposed_port(ContainerPort::Tcp(PEBBLE_MGMT_PORT))
        .with_network(TEST_NETWORK)
        .with_env_var("PEBBLE_VA_NOSLEEP", "1")
        .with_cmd(vec![
            "-config",
            "/test/config/pebble-config.json",
            "-dnsserver",
            &dns_server,
        ])
        .start()
        .await
        .expect("Failed to start Pebble");

    let pebble_port = pebble.get_host_port_ipv4(PEBBLE_HTTPS_PORT).await.unwrap();
    let challtestsrv_port = challtestsrv
        .get_host_port_ipv4(CHALLTESTSRV_HTTP_PORT)
        .await
        .unwrap();

    let directory_url = format!("https://localhost:{}/dir", pebble_port);
    let mgmt_url = format!("http://localhost:{}", challtestsrv_port);

    println!("Pebble directory: {}", directory_url);
    println!("Challtestsrv mgmt: {}", mgmt_url);

    // Wait for Pebble to be ready
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Write Pebble's minica root CA to temp file for instant-acme
    // This is the CA that signs Pebble's TLS server certificate
    let temp_dir = std::env::temp_dir();
    let root_ca_path = temp_dir.join("pebble_minica_root_ca.pem");
    std::fs::write(&root_ca_path, PEBBLE_MINICA_ROOT_CA).expect("Failed to write root CA file");

    println!("Root CA saved to: {:?}", root_ca_path);

    // Step 1: Create ACME account using builder with custom root CA (instant-acme 0.8+)
    let (account, _creds) = Account::builder_with_root(&root_ca_path)
        .expect("Failed to create account builder with root CA")
        .create(
            &instant_acme::NewAccount {
                contact: &["mailto:test@example.com"],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            directory_url.clone(),
            None,
        )
        .await
        .expect("Failed to create ACME account");

    println!("Created ACME account");

    // Step 2: Create order for wildcard certificate
    let wildcard_domain = "*.example.com";
    let identifiers = [Identifier::Dns(wildcard_domain.to_string())];

    let mut order = account
        .new_order(&NewOrder::new(&identifiers))
        .await
        .expect("Failed to create order");

    println!("Created order for {}", wildcard_domain);

    // Step 3: Get DNS-01 challenge
    // authorizations() returns an iterator, not a future
    let mut authorizations = order.authorizations();
    let mut authz_handle = authorizations
        .next()
        .await
        .expect("Should have at least one authorization")
        .expect("Authorization should be valid");

    // Find DNS-01 challenge
    let mut dns_challenge = authz_handle
        .challenge(ChallengeType::Dns01)
        .expect("Should have DNS-01 challenge for wildcard domain");

    // Get the DNS record name and value from challenge_handle
    let dns_record_name = format!(
        "_acme-challenge.{}",
        wildcard_domain.trim_start_matches("*.")
    );
    let key_authorization = dns_challenge.key_authorization();
    let dns_value = key_authorization.dns_value();

    println!("DNS-01 challenge: {} TXT {}", dns_record_name, dns_value);

    // Step 4: Add DNS TXT record via challtestsrv
    add_dns_txt_record(&mgmt_url, &dns_record_name, &dns_value)
        .await
        .expect("Failed to add DNS record");

    println!("Added DNS TXT record");

    // Step 5: Notify ACME server that challenge is ready
    dns_challenge
        .set_ready()
        .await
        .expect("Failed to set challenge ready");

    println!("Notified ACME server challenge is ready");

    // Step 6: Poll for order to be ready
    let retry_policy = RetryPolicy::new()
        .timeout(Duration::from_secs(60))
        .initial_delay(Duration::from_secs(2));

    let status = order
        .poll_ready(&retry_policy)
        .await
        .expect("Failed to poll order status");

    match status {
        OrderStatus::Ready => {
            println!("Order is ready for finalization");
        }
        OrderStatus::Valid => {
            println!("Order is already valid");
        }
        other => {
            panic!("Unexpected order status: {:?}", other);
        }
    }

    // Step 7: Finalize order (CSR is generated internally by instant-acme)
    let _private_key = order.finalize().await.expect("Failed to finalize order");

    println!("Order finalized");

    // Step 8: Download certificate
    let cert_chain = order
        .poll_certificate(&retry_policy)
        .await
        .expect("Failed to download certificate");

    println!("Downloaded certificate chain ({} bytes)", cert_chain.len());

    // Verify certificate contains PEM format
    assert!(
        cert_chain.contains("-----BEGIN CERTIFICATE-----"),
        "Should contain PEM certificate"
    );

    println!("Wildcard certificate successfully obtained!");

    // Cleanup DNS record
    let _ = clear_dns_txt_record(&mgmt_url, &dns_record_name).await;
}

/// Test that wildcard domains require DNS-01 challenge (not HTTP-01)
#[tokio::test]
#[ignore = "Requires Docker"]
async fn test_wildcard_requires_dns01() {
    use instant_acme::{Account, ChallengeType, Identifier, NewOrder};

    // Initialize Rustls crypto provider
    init_crypto_provider();

    // Start Pebble
    // Using ghcr.io registry for official Let's Encrypt images
    // Using seconds-based wait since logs may not be captured by testcontainers
    let pebble = GenericImage::new("ghcr.io/letsencrypt/pebble", "latest")
        .with_wait_for(WaitFor::seconds(3))
        .with_exposed_port(ContainerPort::Tcp(PEBBLE_HTTPS_PORT))
        .with_exposed_port(ContainerPort::Tcp(PEBBLE_MGMT_PORT))
        .with_env_var("PEBBLE_VA_NOSLEEP", "1")
        .start()
        .await
        .expect("Failed to start Pebble");

    let pebble_port = pebble.get_host_port_ipv4(PEBBLE_HTTPS_PORT).await.unwrap();
    let directory_url = format!("https://localhost:{}/dir", pebble_port);

    // Wait for Pebble
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Write Pebble's minica root CA to temp file for instant-acme
    let temp_dir = std::env::temp_dir();
    let root_ca_path = temp_dir.join("pebble_minica_root_ca_dns01.pem");
    std::fs::write(&root_ca_path, PEBBLE_MINICA_ROOT_CA).expect("Failed to write root CA file");

    // Create account using builder with custom root CA (instant-acme 0.8+)
    let (account, _creds) = Account::builder_with_root(&root_ca_path)
        .expect("Failed to create account builder with root CA")
        .create(
            &instant_acme::NewAccount {
                contact: &["mailto:test@example.com"],
                terms_of_service_agreed: true,
                only_return_existing: false,
            },
            directory_url.clone(),
            None,
        )
        .await
        .expect("Failed to create account");

    // Order wildcard certificate
    let wildcard_domain = "*.wildcard-test.example.com";
    let identifiers = [Identifier::Dns(wildcard_domain.to_string())];

    let mut order = account
        .new_order(&NewOrder::new(&identifiers))
        .await
        .expect("Failed to create order");

    // Get authorizations (iterator, not async)
    let mut authorizations = order.authorizations();
    let mut authz_handle = authorizations
        .next()
        .await
        .expect("Should have at least one authorization")
        .expect("Authorization should be valid");

    // Verify DNS-01 is available (drop result to release borrow)
    let has_dns01 = authz_handle.challenge(ChallengeType::Dns01).is_some();

    assert!(
        has_dns01,
        "Wildcard domain authorization should include DNS-01 challenge"
    );

    // Note: HTTP-01 is typically NOT available for wildcard domains
    // as per ACME specification
    // Get authz again to avoid borrow issues
    let mut authorizations2 = order.authorizations();
    let mut authz_handle2 = authorizations2
        .next()
        .await
        .expect("Should have authorization")
        .expect("Authorization should be valid");
    let has_http01 = authz_handle2.challenge(ChallengeType::Http01).is_some();

    println!(
        "Wildcard domain challenges: DNS-01={}, HTTP-01={}",
        has_dns01, has_http01
    );

    // Per RFC 8555, HTTP-01 should NOT be available for wildcards
    // Pebble follows this specification
    assert!(
        !has_http01,
        "HTTP-01 should NOT be available for wildcard domains (RFC 8555)"
    );

    println!("Verified: Wildcard domains only support DNS-01 challenge");
}
