//! Two-peers debug demo
//!
//! This example spawns two peers and shows detailed debug output
//! of the P2P synchronization process. Run with:
//!
//! ```bash
//! RUST_LOG=two_peers_debug=info,braid_iroh=debug,iroh_gossip=debug cargo run --example two_peers_debug
//! ```

use anyhow::Result;
use braid_http_rs::{Update, Version};
use braid_iroh::{BraidIrohConfig, BraidIrohNode, DiscoveryConfig};
use bytes::Bytes;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing for pretty console output
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "two_peers_debug=info,braid_iroh=debug,iroh_gossip=debug".to_string()),
        )
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .init();

    info!("╔══════════════════════════════════════════════════════════════════╗");
    info!("║     BRAID-IROH: TWO-PEER DEBUG DEMO                              ║");
    info!("╚══════════════════════════════════════════════════════════════════╝");
    info!("");
    info!("This demo shows:");
    info!("  🔵 Peer discovery and connection setup");
    info!("  🟣 Gossip topic subscription");
    info!("  🟢 PUT operations (broadcasting updates)");
    info!("  🟡 GET operations (retrieving state)");
    info!("  📊 Watch the colored output to understand the P2P flow");
    info!("");

    // Create shared discovery (peers can find each other)
    info!("[Setup] Creating shared discovery map...");
    let discovery = DiscoveryConfig::mock();

    // ═════════════════════════════════════════════════════════════════
    // PHASE 1: Spawn Peer A
    // ═════════════════════════════════════════════════════════════════
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE 1: Spawning Peer A");
    info!("═══════════════════════════════════════════════════════════════════");
    
    let peer_a = BraidIrohNode::spawn(BraidIrohConfig {
        discovery: discovery.clone(),
        secret_key: None,
    }).await?;
    
    let peer_a_id = peer_a.node_id();
    info!("🔵 Peer A spawned");
    info!("   NodeId: {}", peer_a_id);
    info!("   Addr: {:?}", peer_a.node_addr().await?);
    info!("");

    // Register Peer A in discovery
    discovery.add_node(peer_a.node_addr().await?);

    // ═════════════════════════════════════════════════════════════════
    // PHASE 2: Spawn Peer B
    // ═════════════════════════════════════════════════════════════════
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE 2: Spawning Peer B");
    info!("═══════════════════════════════════════════════════════════════════");
    
    let peer_b = BraidIrohNode::spawn(BraidIrohConfig {
        discovery: discovery.clone(),
        secret_key: None,
    }).await?;
    
    let peer_b_id = peer_b.node_id();
    info!("🔵 Peer B spawned");
    info!("   NodeId: {}", peer_b_id);
    info!("   Addr: {:?}", peer_b.node_addr().await?);
    info!("");

    // Register Peer B in discovery
    discovery.add_node(peer_b.node_addr().await?);

    // ═════════════════════════════════════════════════════════════════
    // PHASE 3: Subscribe to Gossip Topic
    // ═════════════════════════════════════════════════════════════════
    let resource_url = "/demo-chat";
    
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE 3: Subscribing to Gossip Topic");
    info!("═══════════════════════════════════════════════════════════════════");
    info!("Resource URL: {}", resource_url);
    
    // Derive topic ID (deterministic from URL)
    let topic_id = braid_iroh::subscription::SubscriptionManager::topic_for_url(resource_url);
    info!("🟣 Derived TopicId: {:?}", topic_id);
    
    // Peer A subscribes (bootstraps with Peer B)
    info!("🟣 Peer A subscribing...");
    let _rx_a = peer_a.subscribe(resource_url, vec![peer_b_id]).await?;
    info!("   ✓ Subscribed");
    
    // Peer B subscribes (bootstraps with Peer A)
    info!("🟣 Peer B subscribing...");
    let _rx_b = peer_b.subscribe(resource_url, vec![peer_a_id]).await?;
    info!("   ✓ Subscribed");
    info!("");

    // Give time for gossip overlay to form
    sleep(Duration::from_millis(500)).await;

    // ═════════════════════════════════════════════════════════════════
    // PHASE 4: Peer A Puts Updates
    // ═════════════════════════════════════════════════════════════════
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE 4: Peer A Sending Updates");
    info!("═══════════════════════════════════════════════════════════════════");
    
    for i in 1..=3 {
        let content = format!("Hello from Peer A - Message {}", i);
        let version = format!("v{}", i);
        
        info!("🟢 Peer A: PUT {}", resource_url);
        info!("   Version: {}", version);
        info!("   Content: '{}'", content);
        
        let update = Update::snapshot(
            Version::String(version),
            Bytes::from(content),
        );
        
        peer_a.put(resource_url, update).await?;
        info!("   ✓ Stored and broadcast");
        
        sleep(Duration::from_millis(300)).await;
    }
    info!("");

    // Give time for gossip propagation
    sleep(Duration::from_millis(500)).await;

    // ═════════════════════════════════════════════════════════════════
    // PHASE 5: Peer B Puts Updates
    // ═════════════════════════════════════════════════════════════════
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE 5: Peer B Sending Updates");
    info!("═══════════════════════════════════════════════════════════════════");
    
    for i in 1..=2 {
        let content = format!("Response from Peer B - Reply {}", i);
        let version = format!("v{}-b", i);
        
        info!("🟢 Peer B: PUT {}", resource_url);
        info!("   Version: {}", version);
        info!("   Content: '{}'", content);
        
        let update = Update::snapshot(
            Version::String(version),
            Bytes::from(content),
        );
        
        peer_b.put(resource_url, update).await?;
        info!("   ✓ Stored and broadcast");
        
        sleep(Duration::from_millis(300)).await;
    }
    info!("");

    // Give time for gossip propagation
    sleep(Duration::from_millis(500)).await;

    // ═════════════════════════════════════════════════════════════════
    // PHASE 6: Retrieve State
    // ═════════════════════════════════════════════════════════════════
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE 6: Retrieving Local State");
    info!("═══════════════════════════════════════════════════════════════════");
    
    info!("🟡 Peer A: GET {}", resource_url);
    match peer_a.get(resource_url).await {
        Some(update) => {
            info!("   ✓ Found:");
            info!("     Version: {:?}", update.version);
            info!("     Body: {:?}", update.body.as_ref().map(|b| String::from_utf8_lossy(b)));
        }
        None => {
            warn!("   ✗ Not found");
        }
    }
    
    info!("🟡 Peer B: GET {}", resource_url);
    match peer_b.get(resource_url).await {
        Some(update) => {
            info!("   ✓ Found:");
            info!("     Version: {:?}", update.version);
            info!("     Body: {:?}", update.body.as_ref().map(|b| String::from_utf8_lossy(b)));
        }
        None => {
            warn!("   ✗ Not found");
        }
    }
    info!("");

    // ═════════════════════════════════════════════════════════════════
    // PHASE 7: Additional Resources
    // ═════════════════════════════════════════════════════════════════
    info!("═══════════════════════════════════════════════════════════════════");
    info!("PHASE 7: Multiple Resources");
    info!("═══════════════════════════════════════════════════════════════════");
    
    let urls = ["/doc/a", "/doc/b", "/doc/c"];
    
    for (i, url) in urls.iter().enumerate() {
        let content = format!("Document {} content", i + 1);
        info!("🟢 Peer A: PUT {}", url);
        
        let update = Update::snapshot(
            Version::String(format!("v1-{}", i)),
            Bytes::from(content),
        );
        
        peer_a.put(url, update).await?;
    }
    info!("");

    // Show all resources on Peer A
    info!("🟡 Peer A: Listing all resources");
    for url in &urls {
        match peer_a.get(url).await {
            Some(_) => info!("   ✓ {} - exists", url),
            None => info!("   ✗ {} - not found", url),
        }
    }
    info!("");

    // ═════════════════════════════════════════════════════════════════
    // CLEANUP
    // ═════════════════════════════════════════════════════════════════
    info!("═══════════════════════════════════════════════════════════════════");
    info!("CLEANUP: Shutting down peers");
    info!("═══════════════════════════════════════════════════════════════════");
    
    info!("🔵 Shutting down Peer A...");
    peer_a.shutdown().await?;
    info!("   ✓ Peer A stopped");
    
    info!("🔵 Shutting down Peer B...");
    peer_b.shutdown().await?;
    info!("   ✓ Peer B stopped");
    info!("");

    info!("╔══════════════════════════════════════════════════════════════════╗");
    info!("║                    DEMO COMPLETE ✓                               ║");
    info!("╚══════════════════════════════════════════════════════════════════╝");
    info!("");
    info!("Summary:");
    info!("  • 2 peers spawned with unique NodeIds");
    info!("  • Gossip topic joined for resource '/demo-chat'");
    info!("  • 3 updates sent from Peer A");
    info!("  • 2 updates sent from Peer B");
    info!("  • 3 additional resources created");
    info!("  • All peers shutdown cleanly");
    info!("");
    info!("Look at the colored output above to understand the P2P flow:");
    info!("  🔵 = Connection/network events");
    info!("  🟣 = Gossip/subscription events");
    info!("  🟢 = PUT/update operations");
    info!("  🟡 = GET/retrieve operations");
    info!("  ⚠️  = Warnings");

    Ok(())
}
