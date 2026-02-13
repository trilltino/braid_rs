//! Test utilities and common fixtures for braid_iroh integration tests.

#![allow(unused_imports)]

use anyhow::Result;
use braid_iroh::{BraidIrohConfig, BraidIrohNode, DiscoveryConfig};
use iroh::SecretKey;
use std::time::Duration;
use tokio::time::timeout;

/// Default timeout for test operations.
pub const TEST_TIMEOUT_MS: u64 = 30000;

/// Create a fresh mock discovery configuration for isolated tests.
pub fn create_test_discovery() -> DiscoveryConfig {
    DiscoveryConfig::mock()
}

/// Create a test node with the given discovery config.
pub async fn create_test_node(discovery: DiscoveryConfig) -> Result<BraidIrohNode> {
    let config = BraidIrohConfig {
        discovery,
        secret_key: None,
    };
    BraidIrohNode::spawn(config).await
}

/// Create a test node with a specific secret key (for persistent identity).
pub async fn create_test_node_with_key(
    discovery: DiscoveryConfig,
    key: SecretKey,
) -> Result<BraidIrohNode> {
    let config = BraidIrohConfig {
        discovery,
        secret_key: Some(key),
    };
    BraidIrohNode::spawn(config).await
}

/// Generate random content for testing.
pub fn random_content() -> String {
    format!("test-content-{}", uuid::Uuid::new_v4())
}

/// Wait for a condition to become true, with timeout.
pub async fn wait_for_condition<F>(timeout_ms: u64, mut condition: F) -> bool
where
    F: FnMut() -> bool,
{
    let start = std::time::Instant::now();
    let duration = Duration::from_millis(timeout_ms);
    let check_interval = Duration::from_millis(10);

    while start.elapsed() < duration {
        if condition() {
            return true;
        }
        tokio::time::sleep(check_interval).await;
    }
    false
}

/// Wait for an async condition to become true, with timeout.
pub async fn wait_for_condition_async<F, Fut>(timeout_ms: u64, mut condition: F) -> bool
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let start = std::time::Instant::now();
    let duration = Duration::from_millis(timeout_ms);
    let check_interval = Duration::from_millis(10);

    while start.elapsed() < duration {
        if condition().await {
            return true;
        }
        tokio::time::sleep(check_interval).await;
    }
    false
}

/// Setup tracing for tests.
pub fn init_test_tracing() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info,braid_iroh=debug")
        .try_init();
}

/// Helper to setup two connected peers for tests.
pub async fn setup_two_peers() -> Result<(BraidIrohNode, BraidIrohNode)> {
    let discovery = create_test_discovery();

    let peer_a = create_test_node(discovery.clone()).await?;
    let peer_b = create_test_node(discovery.clone()).await?;

    // Register each peer's address with the discovery map
    let addr_a = peer_a.node_addr().await?;
    let addr_b = peer_b.node_addr().await?;
    discovery.add_node(addr_a);
    discovery.add_node(addr_b);

    Ok((peer_a, peer_b))
}

/// Helper to setup two peers subscribed to the same resource.
pub async fn setup_two_peers_subscribed(
    resource_url: &str,
) -> Result<(BraidIrohNode, BraidIrohNode)> {
    let (peer_a, peer_b) = setup_two_peers().await?;

    let peer_a_id = peer_a.node_id();
    let peer_b_id = peer_b.node_id();

    // Both subscribe to the resource
    peer_a.subscribe(resource_url, vec![peer_b_id]).await?;
    peer_b.subscribe(resource_url, vec![peer_a_id]).await?;

    Ok((peer_a, peer_b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_util_create_node() -> Result<()> {
        init_test_tracing();
        let discovery = create_test_discovery();
        let node = create_test_node(discovery).await?;
        assert!(!format!("{}", node.node_id()).is_empty());
        node.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_util_two_peers() -> Result<()> {
        init_test_tracing();
        let (peer_a, peer_b) = setup_two_peers().await?;
        
        assert_ne!(peer_a.node_id(), peer_b.node_id());
        
        peer_a.shutdown().await?;
        peer_b.shutdown().await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_wait_for_condition() {
        let mut counter = 0u32;
        
        let result = wait_for_condition(1000, || {
            counter += 1;
            counter >= 5
        }).await;
        
        assert!(result);
        assert!(counter >= 5);
    }

    #[tokio::test]
    async fn test_wait_for_condition_timeout() {
        let result = wait_for_condition(50, || false).await;
        assert!(!result);
    }
}
