//! Version tracking example
//!
//! Demonstrates how to use version and parent tracking
//! in Braid protocol requests.
//!
//! Run with: cargo run --example versioning

use braid_rs::{BraidClient, BraidRequest, Version};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Version Tracking Example");
    println!("========================\n");

    let client = BraidClient::new();

    println!("Creating request with version tracking...\n");

    let request = BraidRequest::new()
        .with_version(Version::from("v2"))
        .with_parent(Version::from("v1"))
        .with_parent(Version::from("v0"));

    println!("Request configuration:");
    println!("  - Version: v2");
    println!("  - Parents: v1, v0\n");

    match client.fetch("http://example.com/data", request).await {
        Ok(response) => {
            println!("Response Status: {}", response.status);

            if let Some(versions) = response.get_version() {
                println!("Response Versions: {:?}", versions);
            } else {
                println!("No version information in response");
            }

            if let Some(parents) = response.get_parents() {
                println!("Response Parents: {:?}", parents);
            } else {
                println!("No parent information in response");
            }

            println!("\nBraid Version DAG Example:");
            println!("       v0 (initial)");
            println!("      /");
            println!("     v1 (derived from v0)");
            println!("    /");
            println!("   v2 (derived from v1)");
        }
        Err(e) => {
            eprintln!("Request failed: {}", e);
        }
    }

    Ok(())
}
