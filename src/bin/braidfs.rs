//! # BraidFS CLI
//!
//! This binary is the Rust port of the Node.js BraidFS implementation.
//! It serves as the entry point for the daemon and control commands.
//!
//! ## Mapping to Original JS
//! - **JS Entry**: `index.js` (manual argv parsing)
//! - **Rust Entry**: Uses `clap` for structured subcommands.
//! - **IPC**: Commands `sync`/`unsync` communicate with the running daemon
//!   via the internal API (default port 45678), similar to how the JS CLI
//!   talks to its local server.
//!
//! See `porting_guide.md` in artifacts for detailed architectural comparison.

use braid_rs::fs;
use clap::{Parser, Subcommand};
use reqwest::Client;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "braidfs")]
#[command(about = "BraidFS: Braid your Filesystem with the Web")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the BraidFS daemon
    Run {
        #[arg(short, long, default_value = "45678")]
        port: u16,
    },
    /// Sync a URL to a local file
    Sync {
        url: String,
        #[arg(short, long, default_value = "45678")]
        port: u16,
    },
    /// Unsync a URL
    Unsync {
        url: String,
        #[arg(short, long, default_value = "45678")]
        port: u16,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run { port } => {
            println!("Starting BraidFS daemon on port {}", port);
            fs::run_daemon(port).await?;
        }
        Commands::Sync { url, port } => {
            let client = Client::new();
            let api_url = format!("http://127.0.0.1:{}/api/sync", port);
            println!("Syncing URL via {}: {}", api_url, url);

            let res = client
                .put(&api_url)
                .json(&serde_json::json!({ "url": url }))
                .send()
                .await?;

            if res.status().is_success() {
                println!("Successfully scheduled sync for {}", url);
            } else {
                eprintln!("Failed to sync: {}", res.status());
            }
        }
        Commands::Unsync { url, port } => {
            let client = Client::new();
            let api_url = format!("http://127.0.0.1:{}/api/sync", port);
            println!("Unsyncing URL via {}: {}", api_url, url);

            // DELETE request with body is allowed but sometimes tricky.
            // reqwest supports it.
            let res = client
                .delete(&api_url)
                .json(&serde_json::json!({ "url": url }))
                .send()
                .await?;

            if res.status().is_success() {
                println!("Successfully unsynced {}", url);
            } else {
                eprintln!("Failed to unsync: {}", res.status());
            }
        }
    }

    Ok(())
}
