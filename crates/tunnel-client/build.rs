use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Get the workspace root (two levels up from crates/tunnel-client)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let workspace_root = PathBuf::from(&manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();

    let dashboard_dir = workspace_root.join("webapps").join("dashboard");

    println!("cargo:rerun-if-changed={}/src", dashboard_dir.display());
    println!(
        "cargo:rerun-if-changed={}/package.json",
        dashboard_dir.display()
    );
    println!(
        "cargo:rerun-if-changed={}/vite.config.ts",
        dashboard_dir.display()
    );
    println!(
        "cargo:rerun-if-changed={}/bun.lock",
        dashboard_dir.display()
    );

    println!("cargo:warning=Building dashboard web application...");

    // Check if bun is available
    let bun_check = Command::new("bun").arg("--version").output();

    if bun_check.is_err() {
        eprintln!("\n❌ ERROR: Bun is not installed or not in PATH");
        eprintln!("Please install Bun: https://bun.sh/docs/installation");
        eprintln!("\nAlternatively, build the dashboard manually:");
        eprintln!("  cd webapps/dashboard");
        eprintln!("  bun install");
        eprintln!("  bun run build\n");
        std::process::exit(1);
    }

    // Install dependencies
    println!("cargo:warning=Installing dashboard dependencies...");
    let install_status = Command::new("bun")
        .arg("install")
        .current_dir(&dashboard_dir)
        .status()
        .expect("Failed to run bun install");

    if !install_status.success() {
        eprintln!("Failed to install dashboard dependencies");
        std::process::exit(1);
    }

    // Build the dashboard
    println!("cargo:warning=Building dashboard assets...");
    let build_status = Command::new("bun")
        .arg("run")
        .arg("build")
        .current_dir(&dashboard_dir)
        .status()
        .expect("Failed to run bun build");

    if !build_status.success() {
        eprintln!("Failed to build dashboard");
        std::process::exit(1);
    }

    println!("cargo:warning=Dashboard build complete!");
}
