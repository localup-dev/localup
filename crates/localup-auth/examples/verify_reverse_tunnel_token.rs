//! Example: Verify reverse tunnel JWT tokens
//!
//! This example demonstrates how to generate and validate reverse tunnel tokens.
//!
//! Usage:
//!   cargo run --example verify_reverse_localup_token

use chrono::Duration;
use localup_auth::{JwtClaims, JwtValidator};

fn main() {
    println!("=== Reverse Tunnel JWT Token Examples ===\n");

    let secret = b"example_secret_key_12345";

    // Example 1: Permissive token (all agents and addresses allowed)
    println!("1. Permissive Token (All Access)");
    println!("   -------------------------------");
    let permissive = JwtClaims::new(
        "client-permissive".to_string(),
        "localup-exit-node".to_string(),
        "localup-client".to_string(),
        Duration::hours(24),
    )
    .with_reverse_tunnel(true);

    let token = JwtValidator::encode(secret, &permissive).unwrap();
    println!(
        "   Token: {}...{}",
        &token[..40],
        &token[token.len() - 20..]
    );

    // Decode and verify
    let validator = JwtValidator::new(secret)
        .with_issuer("localup-exit-node".to_string())
        .with_audience("localup-client".to_string());
    let decoded = validator.validate(&token).unwrap();

    println!("   ✅ Decoded successfully");
    println!("   reverse_tunnel: {:?}", decoded.reverse_tunnel);
    println!("   allowed_agents: {:?}", decoded.allowed_agents);
    println!("   allowed_addresses: {:?}", decoded.allowed_addresses);

    // Test validation
    match decoded.validate_reverse_localup_access("any-agent", "any-host:1234") {
        Ok(()) => println!("   ✅ Access validated: any agent/address allowed"),
        Err(e) => println!("   ❌ Access denied: {}", e),
    }
    println!();

    // Example 2: Restrictive token (specific agents only)
    println!("2. Restrictive Token (Specific Agents)");
    println!("   ------------------------------------");
    let agent_restricted = JwtClaims::new(
        "client-agent-restricted".to_string(),
        "localup-exit-node".to_string(),
        "localup-client".to_string(),
        Duration::hours(24),
    )
    .with_reverse_tunnel(true)
    .with_allowed_agents(vec!["agent-1".to_string(), "agent-2".to_string()]);

    let token = JwtValidator::encode(secret, &agent_restricted).unwrap();
    println!(
        "   Token: {}...{}",
        &token[..40],
        &token[token.len() - 20..]
    );

    let decoded = validator.validate(&token).unwrap();
    println!("   ✅ Decoded successfully");
    println!("   reverse_tunnel: {:?}", decoded.reverse_tunnel);
    println!("   allowed_agents: {:?}", decoded.allowed_agents);
    println!("   allowed_addresses: {:?}", decoded.allowed_addresses);

    // Test validation - allowed agent
    match decoded.validate_reverse_localup_access("agent-1", "any-host:1234") {
        Ok(()) => println!("   ✅ Access validated: agent-1 allowed"),
        Err(e) => println!("   ❌ Access denied: {}", e),
    }

    // Test validation - disallowed agent
    match decoded.validate_reverse_localup_access("agent-3", "any-host:1234") {
        Ok(()) => println!("   ✅ Access validated: agent-3 allowed"),
        Err(e) => println!("   ❌ Access denied: {}", e),
    }
    println!();

    // Example 3: Restrictive token (specific addresses only)
    println!("3. Restrictive Token (Specific Addresses)");
    println!("   ---------------------------------------");
    let address_restricted = JwtClaims::new(
        "client-address-restricted".to_string(),
        "localup-exit-node".to_string(),
        "localup-client".to_string(),
        Duration::hours(24),
    )
    .with_reverse_tunnel(true)
    .with_allowed_addresses(vec![
        "192.168.1.100:8080".to_string(),
        "10.0.0.5:22".to_string(),
    ]);

    let token = JwtValidator::encode(secret, &address_restricted).unwrap();
    println!(
        "   Token: {}...{}",
        &token[..40],
        &token[token.len() - 20..]
    );

    let decoded = validator.validate(&token).unwrap();
    println!("   ✅ Decoded successfully");
    println!("   reverse_tunnel: {:?}", decoded.reverse_tunnel);
    println!("   allowed_agents: {:?}", decoded.allowed_agents);
    println!("   allowed_addresses: {:?}", decoded.allowed_addresses);

    // Test validation - allowed address
    match decoded.validate_reverse_localup_access("any-agent", "192.168.1.100:8080") {
        Ok(()) => println!("   ✅ Access validated: 192.168.1.100:8080 allowed"),
        Err(e) => println!("   ❌ Access denied: {}", e),
    }

    // Test validation - disallowed address
    match decoded.validate_reverse_localup_access("any-agent", "10.0.0.99:1234") {
        Ok(()) => println!("   ✅ Access validated: 10.0.0.99:1234 allowed"),
        Err(e) => println!("   ❌ Access denied: {}", e),
    }
    println!();

    // Example 4: Fully restrictive token (specific agents AND addresses)
    println!("4. Fully Restrictive Token");
    println!("   -------------------------");
    let fully_restricted = JwtClaims::new(
        "client-fully-restricted".to_string(),
        "localup-exit-node".to_string(),
        "localup-client".to_string(),
        Duration::hours(1),
    )
    .with_reverse_tunnel(true)
    .with_allowed_agents(vec!["prod-agent-1".to_string()])
    .with_allowed_addresses(vec!["10.0.1.100:5432".to_string()]);

    let token = JwtValidator::encode(secret, &fully_restricted).unwrap();
    println!(
        "   Token: {}...{}",
        &token[..40],
        &token[token.len() - 20..]
    );

    let decoded = validator.validate(&token).unwrap();
    println!("   ✅ Decoded successfully");
    println!("   reverse_tunnel: {:?}", decoded.reverse_tunnel);
    println!("   allowed_agents: {:?}", decoded.allowed_agents);
    println!("   allowed_addresses: {:?}", decoded.allowed_addresses);

    // Test validation matrix
    println!("   Validation matrix:");
    let test_cases = [
        ("prod-agent-1", "10.0.1.100:5432", true),
        ("prod-agent-1", "10.0.1.200:5432", false),
        ("other-agent", "10.0.1.100:5432", false),
        ("other-agent", "10.0.1.200:5432", false),
    ];

    for (agent, addr, expected) in test_cases {
        let result = decoded.validate_reverse_localup_access(agent, addr);
        let icon = if result.is_ok() == expected {
            "✅"
        } else {
            "❌"
        };
        println!(
            "   {} Agent: {:<15} Address: {:<20} → {}",
            icon,
            agent,
            addr,
            if result.is_ok() { "ALLOWED" } else { "DENIED" }
        );
    }
    println!();

    // Example 5: Disabled reverse tunnel
    println!("5. Reverse Tunnel Disabled");
    println!("   -------------------------");
    let disabled = JwtClaims::new(
        "client-no-reverse".to_string(),
        "localup-exit-node".to_string(),
        "localup-client".to_string(),
        Duration::hours(24),
    )
    .with_reverse_tunnel(false);

    let token = JwtValidator::encode(secret, &disabled).unwrap();
    println!(
        "   Token: {}...{}",
        &token[..40],
        &token[token.len() - 20..]
    );

    let decoded = validator.validate(&token).unwrap();
    println!("   ✅ Decoded successfully");
    println!("   reverse_tunnel: {:?}", decoded.reverse_tunnel);

    match decoded.validate_reverse_localup_access("any-agent", "any-host:1234") {
        Ok(()) => println!("   ✅ Access validated"),
        Err(e) => println!("   ❌ Access denied: {}", e),
    }
    println!();

    // Example 6: Backward compatible (no reverse_tunnel claim)
    println!("6. Backward Compatible Token (Old Format)");
    println!("   ----------------------------------------");
    let old_token = JwtClaims::new(
        "client-legacy".to_string(),
        "localup-exit-node".to_string(),
        "localup-client".to_string(),
        Duration::hours(24),
    );

    let token = JwtValidator::encode(secret, &old_token).unwrap();
    println!(
        "   Token: {}...{}",
        &token[..40],
        &token[token.len() - 20..]
    );

    let decoded = validator.validate(&token).unwrap();
    println!("   ✅ Decoded successfully");
    println!("   reverse_tunnel: {:?}", decoded.reverse_tunnel);
    println!("   allowed_agents: {:?}", decoded.allowed_agents);
    println!("   allowed_addresses: {:?}", decoded.allowed_addresses);

    match decoded.validate_reverse_localup_access("any-agent", "any-host:1234") {
        Ok(()) => println!("   ✅ Backward compatible: access allowed (default behavior)"),
        Err(e) => println!("   ❌ Access denied: {}", e),
    }
    println!();

    println!("=== All Examples Complete ===");
}
