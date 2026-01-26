use crate::core::merge::{MergeType, MergeTypeRegistry};
use crate::core::BraidClient;
use crate::fs::binary_sync::BinarySyncManager;
use crate::fs::config::Config;
use crate::fs::versions::VersionStore;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct PendingWrites {
    // Map path -> Expiration Time (when we stop ignoring it)
    paths: Arc<Mutex<HashMap<String, std::time::Instant>>>,
}

impl PendingWrites {
    pub fn new() -> Self {
        Self {
            paths: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn normalize(path: &std::path::Path) -> String {
        path.to_string_lossy().to_lowercase().replace('\\', "/")
    }

    pub fn add(&self, path: PathBuf) {
        // Ignore events for this path for 100ms
        let expiry = std::time::Instant::now() + Duration::from_millis(100);
        self.paths
            .lock()
            .unwrap()
            .insert(Self::normalize(&path), expiry);
    }

    pub fn remove(&self, path: &PathBuf) {
        self.paths.lock().unwrap().remove(&Self::normalize(path));
    }

    pub fn should_ignore(&self, path: &PathBuf) -> bool {
        let mut paths = self.paths.lock().unwrap();
        let key = Self::normalize(path);

        if let Some(&expiry) = paths.get(&key) {
            if std::time::Instant::now() < expiry {
                return true; // Still within ignore window
            } else {
                paths.remove(&key); // Expired, cleanup
                return false;
            }
        }
        false
    }
}

#[derive(Clone)]
pub struct ActivityTracker {
    // Map URL -> Last Activity Time
    activity: Arc<Mutex<HashMap<String, std::time::Instant>>>,
}

impl ActivityTracker {
    pub fn new() -> Self {
        Self {
            activity: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn mark(&self, url: &str) {
        let mut activity = self.activity.lock().unwrap();
        activity.insert(url.to_string(), std::time::Instant::now());
    }

    pub fn is_active(&self, url: &str) -> bool {
        // Log is active if there was activity in the last 10 minutes
        let activity = self.activity.lock().unwrap();
        if let Some(&last_time) = activity.get(url) {
            std::time::Instant::now().duration_since(last_time) < Duration::from_secs(600)
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub enum Command {
    Sync { url: String },
    Unsync { url: String },
    SetCookie { domain: String, value: String },
    SetIdentity { domain: String, email: String },
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
    pub tx_cmd: async_channel::Sender<Command>,
}
