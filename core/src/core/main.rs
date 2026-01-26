//! Example Braid-HTTP client usage.
//!
//! This is a demonstration program showing how to use the Braid-HTTP client
//! library. It demonstrates:
//!
//! 1. Creating a Braid client
//! 2. Making simple GET requests
//! 3. Making requests with version tracking
//! 4. Setting up subscriptions
//! 5. Sending patches
//!
//! # Running the Example
//!
//! ```bash
//! cargo run --example main
//! ```
//!
//! # Note
//!
//! This example demonstrates the API but doesn't actually make HTTP requests.
//! For a complete working example, you would need a Braid-HTTP server running.

use crate::core::{BraidClient, BraidRequest, Version};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Braid HTTP Client Example");
    println!("========================\n");

    let _client = BraidClient::new();

    println!("1. Simple GET request:");
    println!("   client.get(url).await\n");

    println!("2. With version tracking:");
    let _request = BraidRequest::new()
        .with_version(Version::from("v1"))
        .with_parent(Version::from("v0"));
    println!("   client.fetch(url, request).await\n");

    println!("3. Subscription to updates:");
    println!("   let mut sub = client.subscribe(url, BraidRequest::new().subscribe()).await?;\n");

    println!("4. With patches:");
    use crate::core::Patch;
    let patch = Patch::json(".field", "new_value");
    let _request = BraidRequest::new()
        .with_patches(vec![patch]);
    println!("   client.fetch(url, request).await\n");

    Ok(())
}
