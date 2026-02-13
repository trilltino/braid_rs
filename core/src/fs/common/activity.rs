//! Activity tracking and pending write management
//!
//! Tracks which URLs are active and manages paths that should be ignored
//! during file system events (to prevent echo loops).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Tracks pending writes to prevent echo loops
#[derive(Clone)]
pub struct PendingWrites {
    // Map path -> Expiration Time (when we stop ignoring it)
    paths: Arc<Mutex<HashMap<String, Instant>>>,
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
        let expiry = Instant::now() + Duration::from_millis(100);
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
            if Instant::now() < expiry {
                return true; // Still within ignore window
            } else {
                paths.remove(&key); // Expired, cleanup
                return false;
            }
        }
        false
    }
}

impl Default for PendingWrites {
    fn default() -> Self {
        Self::new()
    }
}

/// Tracks activity for URLs to determine if they're "active"
#[derive(Clone)]
pub struct ActivityTracker {
    // Map URL -> Last Activity Time
    activity: Arc<Mutex<HashMap<String, Instant>>>,
}

impl ActivityTracker {
    pub fn new() -> Self {
        Self {
            activity: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn mark(&self, url: &str) {
        let mut activity = self.activity.lock().unwrap();
        activity.insert(url.to_string(), Instant::now());
    }

    pub fn is_active(&self, url: &str) -> bool {
        // Log is active if there was activity in the last 10 minutes
        let activity = self.activity.lock().unwrap();
        if let Some(&last_time) = activity.get(url) {
            Instant::now().duration_since(last_time) < Duration::from_secs(600)
        } else {
            false
        }
    }
}

impl Default for ActivityTracker {
    fn default() -> Self {
        Self::new()
    }
}
