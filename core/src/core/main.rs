//! Example Braid-HTTP client usage.

use crate::core::{BraidClient, BraidRequest, Version};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _client = BraidClient::new()?;

    println!("Braid HTTP Client Example");
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
    let _request = BraidRequest::new().with_patches(vec![patch]);
    println!("   client.fetch(url, request).await\n");

    Ok(())
}
