//! Generate a JWT token for tunnel authentication
//!
//! Usage:
//!   cargo run --example generate_token -- --secret "your-secret-key"
//!   cargo run --example generate_token -- --secret "your-secret-key" --tunnel-id "my-tunnel"

use chrono::Duration;
use clap::Parser;
use localup_auth::{JwtClaims, JwtValidator};

#[derive(Parser, Debug)]
#[command(name = "generate_token")]
#[command(about = "Generate a JWT token for tunnel authentication", long_about = None)]
struct Args {
    /// JWT secret (must match the exit node's secret)
    #[arg(long, env = "TUNNEL_JWT_SECRET")]
    secret: String,

    /// Tunnel ID (optional, defaults to "client")
    #[arg(long, default_value = "client")]
    localup_id: String,

    /// Issuer (optional)
    #[arg(long, default_value = "localup-relay")]
    issuer: String,

    /// Audience (optional)
    #[arg(long, default_value = "localup-client")]
    audience: String,

    /// Token validity in hours (default: 24)
    #[arg(long, default_value = "24")]
    hours: i64,
}

fn main() {
    let args = Args::parse();

    // Create JWT claims
    let claims = JwtClaims::new(
        args.localup_id.clone(),
        args.issuer,
        args.audience,
        Duration::hours(args.hours),
    );

    // Encode the token
    match JwtValidator::encode(args.secret.as_bytes(), &claims) {
        Ok(token) => {
            println!("\n✅ JWT Token generated successfully!\n");
            println!("Tunnel ID: {}", args.localup_id);
            println!("Valid for:  {} hours", args.hours);
            println!("\nToken:");
            println!("{}\n", token);
            println!("Usage:");
            println!("  export TUNNEL_AUTH_TOKEN=\"{}\"", token);
            println!("  ./target/release/tunnel --local-port 3000 --subdomain myapp --relay localhost:4443\n");
        }
        Err(e) => {
            eprintln!("❌ Failed to generate token: {}", e);
            std::process::exit(1);
        }
    }
}
