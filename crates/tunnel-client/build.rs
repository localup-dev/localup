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

fn main() {
    // Get the workspace root (two levels up from crates/tunnel-client)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

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
