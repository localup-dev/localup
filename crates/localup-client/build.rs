use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn build_webapp(workspace_root: &Path, webapp_name: &str) {
    let webapp_dir = workspace_root.join("webapps").join(webapp_name);

    println!("cargo:rerun-if-changed={}/src", webapp_dir.display());
    println!(
        "cargo:rerun-if-changed={}/package.json",
        webapp_dir.display()
    );
    println!(
        "cargo:rerun-if-changed={}/vite.config.ts",
        webapp_dir.display()
    );
    println!("cargo:rerun-if-changed={}/bun.lock", webapp_dir.display());

    println!("cargo:warning=Building {} web application...", webapp_name);

    // Install dependencies
    println!("cargo:warning=Installing {} dependencies...", webapp_name);
    let install_status = Command::new("bun")
        .arg("install")
        .current_dir(&webapp_dir)
        .status()
        .unwrap_or_else(|_| panic!("Failed to run bun install for {}", webapp_name));

    if !install_status.success() {
        eprintln!("Failed to install {} dependencies", webapp_name);
        std::process::exit(1);
    }

    // Build the webapp
    println!("cargo:warning=Building {} assets...", webapp_name);
    let build_status = Command::new("bun")
        .arg("run")
        .arg("build")
        .current_dir(&webapp_dir)
        .status()
        .unwrap_or_else(|_| panic!("Failed to run bun build for {}", webapp_name));

    if !build_status.success() {
        eprintln!("Failed to build {}", webapp_name);
        std::process::exit(1);
    }

    println!("cargo:warning={} build complete!", webapp_name);
}

fn setup_relay_config(workspace_root: &Path) {
    // Get relay config path from environment variable or use default
    let relay_config_path = env::var("LOCALUP_RELAYS_CONFIG").unwrap_or_else(|_| {
        // Default to workspace root relays.yaml
        workspace_root.join("relays.yaml").display().to_string()
    });

    // Verify the file exists
    let config_path = PathBuf::from(&relay_config_path);
    if !config_path.exists() {
        eprintln!(
            "\n❌ ERROR: Relay configuration file not found at: {}",
            relay_config_path
        );
        eprintln!("Set LOCALUP_RELAYS_CONFIG environment variable to specify a custom path.");
        eprintln!("Example: LOCALUP_RELAYS_CONFIG=/path/to/custom-relays.yaml cargo build");
        std::process::exit(1);
    }

    // Pass the path to the compiler as an environment variable
    println!("cargo:rustc-env=RELAY_CONFIG_PATH={}", relay_config_path);

    // Rebuild if the relay config file changes
    println!("cargo:rerun-if-changed={}", relay_config_path);

    // Rebuild if the env variable changes
    println!("cargo:rerun-if-env-changed=LOCALUP_RELAYS_CONFIG");

    // Print info message (only visible during build)
    println!(
        "cargo:warning=📡 Using relay configuration from: {}",
        relay_config_path
    );
}

fn main() {
    // Get the workspace root (two levels up from crates/tunnel-client)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    // Setup relay configuration
    setup_relay_config(&workspace_root);

    // Check if bun is available
    let bun_check = Command::new("bun").arg("--version").output();

    if bun_check.is_err() {
        eprintln!("\n❌ ERROR: Bun is not installed or not in PATH");
        eprintln!("Please install Bun: https://bun.sh/docs/installation");
        eprintln!("\nAlternatively, build the webapps manually:");
        eprintln!("  cd webapps/dashboard");
        eprintln!("  bun install && bun run build");
        eprintln!("  cd ../exit-node-portal");
        eprintln!("  bun install && bun run build\n");
        std::process::exit(1);
    }

    // Build both webapps
    build_webapp(&workspace_root, "dashboard");
    build_webapp(&workspace_root, "exit-node-portal");

    println!("cargo:warning=All web applications built successfully!");
}
