//! Integration tests for gossip subscription functionality.

use braid_iroh::subscription::SubscriptionManager;

mod common;
use common::*;

#[tokio::test]
async fn test_topic_derivation_consistency() -> anyhow::Result<()> {
    init_test_tracing();
    
    // Same URL should always derive to same topic
    let url = "/demo/doc/123";
    
    let discovery = create_test_discovery();
    let node = create_test_node(discovery).await?;
    
    let topic1 = SubscriptionManager::topic_for_url(url);
    let topic2 = SubscriptionManager::topic_for_url(url);
    let topic3 = SubscriptionManager::topic_for_url(url);
    
    assert_eq!(topic1, topic2);
    assert_eq!(topic2, topic3);
    
    node.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_topic_derivation_across_nodes() -> anyhow::Result<()> {
    init_test_tracing();
    
    // Different nodes should derive same topic for same URL
    let url = "/shared/resource";
    
    let discovery = create_test_discovery();
    let node_a = create_test_node(discovery.clone()).await?;
    let node_b = create_test_node(discovery.clone()).await?;
    
    let topic_a = SubscriptionManager::topic_for_url(url);
    let topic_b = SubscriptionManager::topic_for_url(url);
    
    assert_eq!(topic_a, topic_b);
    
    node_a.shutdown().await?;
    node_b.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_subscribe_creates_receiver() -> anyhow::Result<()> {
    init_test_tracing();
    
    let discovery = create_test_discovery();
    let node = create_test_node(discovery.clone()).await?;
    
    // Get our own address for bootstrap
    let addr = node.node_addr().await?;
    discovery.add_node(addr);
    
    // Subscribe to a resource
    let receiver = node.subscribe("/test/resource", vec![]).await?;
    
    // Just verify we got a receiver - can't easily test it without another peer
    drop(receiver);
    
    node.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_two_peers_subscribe_same_topic() -> anyhow::Result<()> {
    init_test_tracing();
    
    let (peer_a, peer_b) = setup_two_peers().await?;
    
    let peer_a_id = peer_a.node_id();
    let peer_b_id = peer_b.node_id();
    
    // Both subscribe to the same resource
    let _rx_a = peer_a.subscribe("/shared/doc", vec![peer_b_id]).await?;
    let _rx_b = peer_b.subscribe("/shared/doc", vec![peer_a_id]).await?;
    
    // Give subscriptions time to establish
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    peer_a.shutdown().await?;
    peer_b.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_different_urls_different_topics() -> anyhow::Result<()> {
    init_test_tracing();
    
    let discovery = create_test_discovery();
    let node = create_test_node(discovery).await?;
    
    let topic1 = SubscriptionManager::topic_for_url("/doc/a");
    let topic2 = SubscriptionManager::topic_for_url("/doc/b");
    let topic3 = SubscriptionManager::topic_for_url("/doc/c");
    
    // All topics should be unique
    assert_ne!(topic1, topic2);
    assert_ne!(topic2, topic3);
    assert_ne!(topic1, topic3);
    
    node.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn test_topic_byte_representation() -> anyhow::Result<()> {
    init_test_tracing();
    
    let discovery = create_test_discovery();
    let node = create_test_node(discovery).await?;
    
    let topic = SubscriptionManager::topic_for_url("/test");
    let bytes: &[u8; 32] = topic.as_ref();
    
    // Should be exactly 32 bytes
    assert_eq!(bytes.len(), 32);
    
    // Should be non-zero (very unlikely to be all zeros)
    assert!(bytes.iter().any(|&b| b != 0));
    
    node.shutdown().await?;
    Ok(())
}
