//! Gossip-backed Braid subscriptions.
//!
//! Maps resource URLs to iroh-gossip topics. When a peer PUTs an update,
//! it gets broadcast to everyone subscribed to that URL's topic.
//! This replaces Braid's traditional long-lived HTTP subscription responses
//! with fully decentralized gossip.

use braid_http_rs::Update;
use bytes::Bytes;
use iroh::EndpointId;
use iroh_gossip::api::{GossipReceiver, GossipSender, GossipTopic};
use iroh_gossip::net::Gossip;
use iroh_gossip::proto::TopicId;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Manages active gossip subscriptions keyed by resource URL.
///
/// Each URL gets a deterministic `TopicId` via blake3 hash, so any peer
/// that knows the URL can join the correct topic without coordination.
pub struct SubscriptionManager {
    gossip: Gossip,
    /// Active topics: URL → sender handle
    topics: Arc<Mutex<HashMap<String, GossipSender>>>,
}

impl SubscriptionManager {
    /// Wrap an existing gossip instance.
    pub fn new(gossip: Gossip) -> Self {
        Self {
            gossip,
            topics: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Derive a deterministic TopicId from a resource URL.
    /// Any peer hashing the same URL gets the same topic.
    pub fn topic_for_url(url: &str) -> TopicId {
        let hash = blake3::hash(url.as_bytes());
        TopicId::from_bytes(*hash.as_bytes())
    }

    /// Subscribe to a resource URL. Joins the gossip topic and returns
    /// a receiver stream of incoming gossip events.
    ///
    /// `bootstrap` should contain at least one known peer so the gossip
    /// protocol can form the initial overlay.
    pub async fn subscribe(
        &self,
        url: &str,
        bootstrap: Vec<EndpointId>,
    ) -> anyhow::Result<(GossipSender, GossipReceiver)> {
        let topic_id = Self::topic_for_url(url);
        let topic: GossipTopic = self.gossip.subscribe(topic_id, bootstrap).await?;
        let (sender, receiver) = topic.split();

        // Stash the sender so we can broadcast later
        self.topics
            .lock()
            .await
            .insert(url.to_string(), sender.clone());

        Ok((sender, receiver))
    }

    /// Broadcast a Braid Update to all peers on a resource's gossip topic.
    /// Serializes the update to JSON bytes before sending.
    pub async fn broadcast(&self, url: &str, update: &Update) -> anyhow::Result<()> {
        let mut topics = self.topics.lock().await;

        // If we don't have a sender for this topic, join it (with no bootstrap peers)
        // This allows us to publish to a topic we haven't explicitly subscribed to
        if !topics.contains_key(url) {
            let topic_id = Self::topic_for_url(url);
            // Join with empty bootstrap peers since we are likely the publisher/origin
            let topic: GossipTopic = self.gossip.subscribe(topic_id, vec![]).await?;
            let (sender, _receiver) = topic.split();

            // We discard the receiver because we don't necessarily want to listen to our own updates
            // (or maybe we do? but for now just enable publishing)
            topics.insert(url.to_string(), sender);
        }

        if let Some(sender) = topics.get(url) {
            let bytes = serde_json::to_vec(update)?;
            sender.broadcast(Bytes::from(bytes)).await?;
        }
        Ok(())
    }

    /// Access the underlying gossip instance (e.g. for shutdown).
    #[allow(dead_code)]
    pub fn gossip(&self) -> &Gossip {
        &self.gossip
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_derivation_deterministic() {
        let url = "/test/resource/123";
        let topic1 = SubscriptionManager::topic_for_url(url);
        let topic2 = SubscriptionManager::topic_for_url(url);

        assert_eq!(topic1, topic2);
    }

    #[test]
    fn test_topic_derivation_unique_per_url() {
        let topic1 = SubscriptionManager::topic_for_url("/resource/a");
        let topic2 = SubscriptionManager::topic_for_url("/resource/b");

        assert_ne!(topic1, topic2);
    }

    #[test]
    fn test_topic_derivation_empty_url() {
        let topic = SubscriptionManager::topic_for_url("");
        let topic_bytes: &[u8; 32] = topic.as_ref();

        // Should produce a valid 32-byte hash (not panic)
        assert_eq!(topic_bytes.len(), 32);
    }

    #[test]
    fn test_topic_derivation_long_url() {
        let long_url = "/a/very/long/url/path".repeat(100);
        let topic = SubscriptionManager::topic_for_url(&long_url);
        let topic_bytes: &[u8; 32] = topic.as_ref();

        // Should still produce a valid 32-byte hash
        assert_eq!(topic_bytes.len(), 32);
    }

    #[test]
    fn test_topic_derivation_special_chars() {
        let url = "/resource/with spaces/and/special/chars/!@#$%";
        let topic1 = SubscriptionManager::topic_for_url(url);
        let topic2 = SubscriptionManager::topic_for_url(url);

        // Should be deterministic even with special characters
        assert_eq!(topic1, topic2);
    }
}
