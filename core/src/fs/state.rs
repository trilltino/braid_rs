use crate::core::merge::{MergeType, MergeTypeRegistry};
use crate::core::BraidClient;
use crate::fs::common::{ActivityTracker, Config, PendingWrites};
use crate::fs::local::VersionStore;
use crate::fs::sync::BinarySyncManager;
use parking_lot::Mutex as PMutex;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub enum Command {
    Sync { url: String },
    Unsync { url: String },
    SetCookie { domain: String, value: String },
    SetIdentity { domain: String, email: String },
    Mount { port: u16 },
    Unmount,
}

/// Unified state for the BraidFS daemon.
#[derive(Clone)]
pub struct DaemonState {
    pub config: Arc<RwLock<Config>>,
    pub content_cache: Arc<RwLock<HashMap<String, String>>>,
    pub version_store: Arc<RwLock<VersionStore>>,
    pub tracker: ActivityTracker,
    pub merge_registry: Arc<MergeTypeRegistry>,
    pub active_merges: Arc<RwLock<HashMap<String, Box<dyn MergeType>>>>,
    pub pending: PendingWrites,
    pub client: BraidClient,
    pub failed_syncs: Arc<RwLock<HashMap<String, (u16, std::time::Instant)>>>,
    pub binary_sync: Arc<BinarySyncManager>,
    /// Inode database for NFS persistence
    pub inode_db: Arc<PMutex<Connection>>,
    pub tx_cmd: async_channel::Sender<Command>,
}
