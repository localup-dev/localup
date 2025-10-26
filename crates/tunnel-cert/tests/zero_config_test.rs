//! Tests for zero-config certificate generation

use tunnel_cert::generate_self_signed_cert;

#[test]
fn test_self_signed_cert_generation() {
    // Should not panic or error
    let cert = generate_self_signed_cert().expect("Failed to generate self-signed cert");

    // Verify we got valid data
    assert!(
        !cert.cert_der.is_empty(),
        "Certificate DER should not be empty"
    );
    assert!(
        !cert.pem_cert.is_empty(),
        "Certificate PEM should not be empty"
    );
    assert!(
        !cert.pem_key.is_empty(),
        "Private key PEM should not be empty"
    );

    // Verify PEM format
    assert!(cert.pem_cert.contains("-----BEGIN CERTIFICATE-----"));
    assert!(cert.pem_cert.contains("-----END CERTIFICATE-----"));
    assert!(
        cert.pem_key.contains("-----BEGIN PRIVATE KEY-----")
            || cert.pem_key.contains("-----BEGIN RSA PRIVATE KEY-----")
    );

    println!("✅ Self-signed certificate generated successfully!");
}

#[test]
fn test_cert_can_be_saved() {
    let cert = generate_self_signed_cert().unwrap();

    let temp_dir = std::env::temp_dir();
    let cert_path = temp_dir.join("test-cert.pem");
    let key_path = temp_dir.join("test-key.pem");

    // Save files
    cert.save_to_files(cert_path.to_str().unwrap(), key_path.to_str().unwrap())
        .expect("Failed to save files");

    // Verify files exist
    assert!(cert_path.exists(), "Certificate file should exist");
    assert!(key_path.exists(), "Key file should exist");

    // Verify content
    let saved_cert = std::fs::read_to_string(&cert_path).unwrap();
    let saved_key = std::fs::read_to_string(&key_path).unwrap();

    assert_eq!(saved_cert, cert.pem_cert);
    assert_eq!(saved_key, cert.pem_key);

    // Cleanup
    std::fs::remove_file(cert_path).ok();
    std::fs::remove_file(key_path).ok();

    println!("✅ Certificate can be saved and loaded!");
}

#[test]
fn test_cert_generation_is_fast() {
    use std::time::Instant;

    let start = Instant::now();
    let _cert = generate_self_signed_cert().unwrap();
    let duration = start.elapsed();

    println!("Certificate generation took: {:?}", duration);

    // Should be reasonably fast (< 200ms on modern hardware)
    assert!(
        duration.as_millis() < 500,
        "Cert generation should be fast, took {:?}",
        duration
    );

    println!("✅ Certificate generation is fast enough!");
}

#[test]
fn test_multiple_certs_are_unique() {
    let cert1 = generate_self_signed_cert().unwrap();
    let cert2 = generate_self_signed_cert().unwrap();

    // Different certs should have different serial numbers and keys
    assert_ne!(cert1.pem_cert, cert2.pem_cert, "Each cert should be unique");
    assert_ne!(cert1.pem_key, cert2.pem_key, "Each key should be unique");

    println!("✅ Each certificate generation is unique!");
}
