//! Example: Antimatter CRDT Usage
//!
//! Demonstrates how to use the Antimatter CRDT for text synchronization
//! between multiple peers.
//!
//! Run with: cargo run --example antimatter_demo

use serde_json::json;
use std::collections::HashMap;

// Mock CRDT for demonstration
#[derive(Debug, Default, Clone)]
struct TextCrdt {
    content: String,
}

impl TextCrdt {
    fn new() -> Self {
        Self::default()
    }

    fn get_content(&self) -> &str {
        &self.content
    }
}

// Note: In real usage, you would use:
// use braid_rs::antimatter::{AntimatterCrdt, PrunableCrdt};

fn main() {
    println!("=== Antimatter CRDT Demo ===\n");

    // Simulating two peers
    let peer1_id = "alice";
    let peer2_id = "bob";

    println!("Creating two peers: {} and {}", peer1_id, peer2_id);

    // In a real application:
    // let mut peer1 = AntimatterCrdt::new(
    //     Some(peer1_id.to_string()),
    //     TextCrdt::new(),
    //     Box::new(|msg| { send_to_network(msg) }),
    // );

    // Simulating operations
    println!("\n1. Peer1 creates initial content: 'Hello'");
    println!("   Version: 0@alice");

    println!("\n2. Peer2 appends ' World'");
    println!("   Version: 0@bob, Parents: [0@alice]");

    println!("\n3. Both peers have concurrent edits...");
    println!("   Peer1: 'Hello World!' (adds exclamation)");
    println!("   Peer2: 'Hello World?' (adds question mark)");

    println!("\n4. After merge, both converge to same content");
    println!("   Final content: 'Hello World!?' (deterministic ordering)");

    println!("\n5. Pruning occurs when all peers ack");
    println!("   Old versions compressed via bubble logic");

    // Demonstrate version graph
    println!("\n=== Version Graph ===");
    println!(
        r#"
           0@alice
              |
           0@bob
           /    \
      1@alice  1@bob
           \    /
          (merged)
    "#
    );

    // Key features
    println!("\n=== Key Antimatter Features ===");
    println!("✓ Conflict-free merging");
    println!("✓ Automatic history compression (bubbles)");
    println!("✓ Peer acknowledgment protocol");
    println!("✓ Fissure handling for network partitions");
    println!("✓ JS-compatible implementation");
}
