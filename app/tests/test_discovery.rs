//! Integration tests for peer discovery functionality.

use braid_iroh::DiscoveryConfig;

mod common;
use common::*;

#[tokio::test]
async fn test_mock_discovery_basic() -> anyhow::Result<()> {
    init_test_tracing();
    
    let discovery = create_test_discovery();
    let node = create_test_node(discovery.clone()).await?;
    
    // Register the node's address
    let addr = node.node_addr().await?;
    discovery.add_node(addr);
    
    // Verify the node has a valid ID
    let node_id = node.node_id();
    assert!(!format!("{}", node_id).is_empty());
    
    node.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_multiple_peers_discovery() -> anyhow::Result<()> {
    init_test_tracing();
    
    let discovery = create_test_discovery();
    
    // Create multiple peers with the same discovery map
    let peer1 = create_test_node(discovery.clone()).await?;
    let peer2 = create_test_node(discovery.clone()).await?;
    let peer3 = create_test_node(discovery.clone()).await?;
    
    // Register all addresses
    discovery.add_node(peer1.node_addr().await?);
    discovery.add_node(peer2.node_addr().await?);
    discovery.add_node(peer3.node_addr().await?);
    
    // Verify all have unique IDs
    let id1 = peer1.node_id();
    let id2 = peer2.node_id();
    let id3 = peer3.node_id();
    
    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);
    
    // Cleanup
    peer1.shutdown().await?;
    peer2.shutdown().await?;
    peer3.shutdown().await?;
    
    Ok(())
}

#[tokio::test]
async fn test_discovery_config_clone() -> anyhow::Result<()> {
    let discovery1 = create_test_discovery();
    let discovery2 = discovery1.clone();
    
    // Create nodes with cloned discovery
    let peer1 = create_test_node(discovery1).await?;
    let peer2 = create_test_node(discovery2).await?;
    
    // They should be able to find each other
    peer1.node_addr().await?;
    peer2.node_addr().await?;
    
    peer1.shutdown().await?;
    peer2.shutdown().await?;
    
    Ok(())
}

#[test]
fn test_discovery_config_variants() {
    // Test that we can create both config variants
    let _mock = DiscoveryConfig::mock();
    let _default = DiscoveryConfig::Default;
    
    // Just verify they compile and create successfully
}
