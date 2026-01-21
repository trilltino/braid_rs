//! Example: Blob Storage with Content Addressing
//!
//! Demonstrates the Braid-Blob service with SHA-256 
//! content-addressable storage.
//!
//! Run with: cargo run --example blob_demo

fn main() {
    println!("=== Braid-Blob Demo ===\n");

    // In real usage:
    // use braid_rs::blob::{BlobStore, BlobMetadata};

    println!("1. Content-Addressable Storage:");
    println!("   - Each blob is hashed with SHA-256");
    println!("   - Duplicate content is automatically deduplicated");
    println!("   - Hash stored in BlobMetadata.content_hash");
    println!();

    // Example metadata
    println!("2. BlobMetadata Example:");
    println!(r#"
    BlobMetadata {{
        key: "images/photo.jpg",
        version: ["1@server"],
        content_type: Some("image/jpeg"),
        parents: [],
        content_hash: Some("a3f2b1c4..."),
        size: Some(1048576),
    }}
    "#);
    println!();

    // HTTP endpoints
    println!("3. HTTP Endpoints:");
    println!("   GET  /blob/{key}  - Download blob");
    println!("   PUT  /blob/{key}  - Upload blob (returns version)");
    println!("   DELETE /blob/{key}  - Delete blob");
    println!();

    // Subscription example
    println!("4. Real-time Subscriptions:");
    println!(r#"
    GET /blob/images/photo.jpg HTTP/1.1
    Subscribe: true
    
    ---
    
    HTTP/1.1 209 Subscription
    Subscribe: true
    
    Version: "1@server"
    Content-Type: image/jpeg
    Content-Length: 1048576
    
    [blob data...]
    
    // Later updates streamed automatically
    "#);
    println!();

    // Deduplication
    println!("5. Deduplication:");
    println!("   Upload same file twice:");
    println!("   - First upload: stores blob, computes hash");
    println!("   - Second upload: detects duplicate, reuses storage");
    println!("   - Storage savings: 50%!");

    println!("\n=== Key Features ===");
    println!("✓ SHA-256 content hashing");
    println!("✓ Automatic deduplication");
    println!("✓ SQLite metadata storage");
    println!("✓ Filesystem blob storage");
    println!("✓ Braid-HTTP subscription support");
}
