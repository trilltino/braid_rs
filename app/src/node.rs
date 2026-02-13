//! BraidIrohNode — the main entry point for a Braid-over-Iroh peer.
//!
//! Orchestrates an iroh `Endpoint`, a gossip `Gossip` instance, and the
//! Braid protocol handler. This is the type you create to participate
//! in the P2P Braid network.

use braid_http_rs::Update;

use iroh::{protocol::Router, Endpoint, EndpointAddr, EndpointId, SecretKey};
use iroh_gossip::api::GossipReceiver;
use iroh_gossip::net::Gossip;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::discovery::{DiscoveryConfig, MockDiscoveryMap};
use crate::protocol::{self, BraidAppState};
use crate::subscription::SubscriptionManager;

/// ALPN protocol identifier for Braid-over-H3.
/// Peers negotiate this during the QUIC handshake.
pub const BRAID_H3_ALPN: &[u8] = b"braid-h3/0";

/// Configuration for spawning a BraidIrohNode.
#[derive(Clone)]
pub struct BraidIrohConfig {
    /// Discovery configuration.
    pub discovery: DiscoveryConfig,

    /// Optional pre-generated secret key for a stable identity.
    /// If `None`, a random identity is generated.
    pub secret_key: Option<SecretKey>,
}

impl Default for BraidIrohConfig {
    fn default() -> Self {
        Self {
            discovery: DiscoveryConfig::Mock(MockDiscoveryMap::new()),
            secret_key: None,
        }
    }
}

/// A Braid-capable P2P peer. Holds the iroh endpoint, gossip, and
/// subscription state. Create one per peer identity.
pub struct BraidIrohNode {
    endpoint: Endpoint,
    #[allow(dead_code)]
    router: Router,
    subscription_mgr: Arc<SubscriptionManager>,
    resources: Arc<RwLock<HashMap<String, Vec<Update>>>>,
}

impl BraidIrohNode {
    /// Spawn a new Braid-over-Iroh node.
    ///
    /// This sets up the iroh endpoint, starts the gossip protocol,
    /// mounts the Braid-HTTP Axum routes via `IrohAxum`, and begins
    /// accepting incoming connections.
    pub async fn spawn(config: BraidIrohConfig) -> anyhow::Result<Self> {
        // 1. Build the iroh endpoint with discovery + Braid ALPN
        let mut builder = Endpoint::builder().alpns(vec![
            BRAID_H3_ALPN.to_vec(),
            iroh_gossip::net::GOSSIP_ALPN.to_vec(),
        ]);

        // Apply optional secret key
        if let Some(key) = config.secret_key {
            builder = builder.secret_key(key);
        }

        // Apply discovery logic
        match config.discovery {
            DiscoveryConfig::Mock(map) => {
                builder = builder.address_lookup(map);
            }
        }

        let endpoint = builder.bind().await?;

        tracing::info!(id = %endpoint.id(), "iroh endpoint bound");

        // 2. Start the gossip protocol on this endpoint
        // In iroh 0.96, spawning gossip is synchronous and returns the Gossip handle directly
        let gossip = Gossip::builder().spawn(endpoint.clone());

        // 3. Build shared state
        let resources: Arc<RwLock<HashMap<String, Vec<Update>>>> =
            Arc::new(RwLock::new(HashMap::new()));

        let subscription_mgr = Arc::new(SubscriptionManager::new(gossip.clone()));

        let app_state = BraidAppState {
            subscriptions: subscription_mgr.clone(),
            resources: resources.clone(),
        };

        // 4. Mount the Braid protocol handler on the iroh router
        let braid_handler = protocol::build_protocol_handler(app_state);

        let router = Router::builder(endpoint.clone())
            .accept(BRAID_H3_ALPN.to_vec(), braid_handler)
            .accept(iroh_gossip::net::GOSSIP_ALPN.to_vec(), gossip.clone())
            .spawn();

        Ok(Self {
            endpoint,
            router,
            subscription_mgr,
            resources,
        })
    }

    /// This node's public identity (EndpointId / NodeId).
    pub fn node_id(&self) -> EndpointId {
        self.endpoint.id()
    }

    /// Full address info for this node (id + addresses + relay).
    pub async fn node_addr(&self) -> anyhow::Result<EndpointAddr> {
        Ok(self.endpoint.addr())
    }

    /// Subscribe to a resource URL on the gossip network.
    /// Returns a receiver stream of gossip events for this resource.
    pub async fn subscribe(
        &self,
        url: &str,
        bootstrap: Vec<EndpointId>,
    ) -> anyhow::Result<GossipReceiver> {
        let (_sender, receiver) = self.subscription_mgr.subscribe(url, bootstrap).await?;
        Ok(receiver)
    }

    /// PUT a Braid Update to a resource. Stores it locally and broadcasts
    /// to all gossip subscribers.
    pub async fn put(&self, url: &str, update: Update) -> anyhow::Result<()> {
        // Debug logging for Braid format
        println!("\nOUTGOING BRAID PUT:");
        println!("PUT {} HTTP/3", url);
        println!("Version: {:?}", update.version);
        if !update.parents.is_empty() {
            println!("Parents: {:?}", update.parents);
        }
        if let Some(body) = &update.body {
            println!("Content-Length: {}", body.len());
            println!("");
            println!("{}", String::from_utf8_lossy(body));
        }
        println!("----------------------------------------\n");

        self.resources
            .write()
            .await
            .entry(url.to_string())
            .or_insert_with(Vec::new)
            .push(update.clone());
        self.subscription_mgr.broadcast(url, &update).await?;
        Ok(())
    }

    /// Store an update locally without broadcasting (for received gossip).
    pub async fn store_update(&self, url: &str, update: Update) {
        self.resources
            .write()
            .await
            .entry(url.to_string())
            .or_insert_with(Vec::new)
            .push(update);
    }

    /// GET the latest state of a resource from local storage.
    #[allow(dead_code)]
    pub async fn get(&self, url: &str) -> Option<Update> {
        self.resources
            .read()
            .await
            .get(url)
            .and_then(|h| h.last().cloned())
    }

    /// GET a specific version of a resource.
    pub async fn get_version(&self, url: &str, version_id: &str) -> Option<Update> {
        self.resources.read().await.get(url).and_then(|history| {
            history
                .iter()
                .find(|u| u.version.iter().any(|v| v.to_string().contains(version_id)))
                .cloned()
        })
    }

    /// GET all version IDs for a resource (latest last).
    pub async fn get_history(&self, url: &str) -> Vec<String> {
        if let Some(history) = self.resources.read().await.get(url) {
            history
                .iter()
                .map(|u| {
                    u.version
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Shut down the node gracefully.
    #[allow(dead_code)]
    pub async fn shutdown(self) -> anyhow::Result<()> {
        self.router.shutdown().await?;
        Ok(())
    }

    /// Access the subscription manager (for advanced usage).
    #[allow(dead_code)]
    pub fn subscriptions(&self) -> &Arc<SubscriptionManager> {
        &self.subscription_mgr
    }

    /// Access the iroh endpoint (for advanced usage).
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }
}
