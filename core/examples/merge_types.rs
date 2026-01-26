//! Example: MergeType System
//!
//! Demonstrates the pluggable merge type system that allows
//! different merge algorithms for different resources.
//!
//! Run with: cargo run --example merge_types

use serde_json::json;

fn main() {
    println!("=== MergeType System Demo ===\n");

    // In real usage:
    // use braid_rs::core::merge::{MergeType, MergeTypeRegistry, MergePatch, Sync9MergeType};

    println!("1. Available Merge Types:");
    println!("   - sync9: Simple last-write-wins");
    println!("   - antimatter: Full CRDT with history compression");
    println!("   - diamond: High-performance text CRDT (native only)");
    println!();

    // Demonstrate Sync9 (last-write-wins)
    println!("2. Sync9 Example (Last-Write-Wins):");
    println!("   Initial: 'Hello World'");
    println!("   Edit 1: 'Hello' -> 'Hi' (version 1@alice)");
    println!("   Edit 2: 'Hi' -> 'Hey' (version 2@bob)");
    println!("   Result: 'Hey World' (bob's edit wins)");
    println!();

    // Demonstrate Antimatter
    println!("3. Antimatter Example (CRDT Merge):");
    println!("   Initial: 'Hello World'");
    println!("   Concurrent edit 1: 'Hello' -> 'Hi' (1@alice)");
    println!("   Concurrent edit 2: 'World' -> 'Earth' (1@bob)");
    println!("   Result: 'Hi Earth' (both edits preserved!)");
    println!();

    // Registry usage
    println!("4. MergeTypeRegistry Usage:");
    println!(
        r#"
    let mut registry = MergeTypeRegistry::new();
    
    // Register custom merge type
    registry.register("custom", |id| Box::new(MyMergeType::new(id)));
    
    // Create instance by name
    let merge = registry.create("antimatter", "peer1").unwrap();
    
    // Use the merge type
    merge.initialize("Hello World");
    let result = merge.local_edit(MergePatch::new("0:5", json!("Hi")));
    println!("Version: {{:?}}", result.version);
    "#
    );
    println!();

    // Axum integration example
    println!("5. Axum Server Integration:");
    println!(
        r#"
    // Set merge type via header
    PUT /resource HTTP/1.1
    Merge-Type: antimatter
    Version: "1@alice"
    Parents: "0@alice"
    Content-Type: application/json

    {{"range": "0:5", "content": "Hi"}}
    "#
    );

    println!("\n=== Key Benefits ===");
    println!("✓ Pluggable architecture");
    println!("✓ Runtime algorithm selection");
    println!("✓ Factory pattern via registry");
    println!("✓ Easy custom implementations");
}
