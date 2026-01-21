//! Simple GET request example
//!
//! Demonstrates basic HTTP GET request using the Braid client.
//!
//! Run with: cargo run --example simple_get

use braid_rs::BraidClient;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Simple GET Request Example");
    println!("==========================\n");

    let client = BraidClient::new();

    println!("Making GET request to example.com...\n");

    match client.get("http://example.com").await {
        Ok(response) => {
            println!("Response Status: {}", response.status);
            println!("Headers: {:#?}", response.headers);
            println!("Body length: {} bytes", response.body.len());

            if let Some(versions) = response.get_version() {
                println!("Versions: {:?}", versions);
            }

            if let Some(parents) = response.get_parents() {
                println!("Parents: {:?}", parents);
            }
        }
        Err(e) => {
            eprintln!("Request failed: {}", e);
        }
    }

    Ok(())
}
