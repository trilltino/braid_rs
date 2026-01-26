//! Per-resource state management with thread-safe CRDT synchronization.
//!
//! This module provides centralized state management for collaborative document editing.
//! It maintains an in-memory registry of documents, each with its own CRDT instance and
//! metadata. All access is thread-safe via `Arc<RwLock<>>`.

use crate::core::merge::DiamondCRDT;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

/// The state of a single collaborative resource.
///
/// Each resource maintains its own CRDT instance along with synchronization metadata.
/// State is protected by a Mutex for safe concurrent access from multiple async tasks.
///
/// # Invariants
///
/// - The CRDT is always at the tip of its operation log
/// - `last_sync` is updated whenever operations are applied
/// - State is never invalid or inconsistent
#[derive(Debug, Clone)]
pub struct ResourceState {
    /// The document's CRDT with full operation history
    pub crdt: DiamondCRDT,

    /// When this resource was last modified
    pub last_sync: SystemTime,

    /// Set of version IDs seen for this resource (for idempotence)
    pub seen_versions: std::collections::HashSet<String>,

    /// The merge strategy for this resource (e.g., "diamond", "sync9")
    pub merge_type: String,
}

/// Thread-safe registry of collaborative document resources.
///
/// `ResourceStateManager` maintains the canonical state for all active resources in the
/// system. It provides methods for creating resources, applying edits, and querying state.
/// All operations are atomic and thread-safe.
///
/// # Thread Safety
///
/// Uses `Arc<Mutex<>>` for exclusive access.
///
/// # Resource Lifecycle
///
/// Resources are created lazily on first access and remain in memory. There is currently
/// no automatic cleanup; consider implementing time-based eviction for long-running servers.
///
/// # Examples
///
/// ```ignore
/// use crate::core::server::ResourceStateManager;
///
/// let manager = ResourceStateManager::new();
/// let _ = manager.apply_update("doc1", "hello", "session-1")?;
/// let state = manager.get_resource_state("doc1");
/// assert!(state.is_some());
/// ```
pub struct ResourceStateManager {
    /// Resource ID → Arc<Mutex<ResourceState>>
    /// Using Arc allows multiple concurrent tasks to reference the same resource
    resources: Arc<Mutex<HashMap<String, Arc<Mutex<ResourceState>>>>>,
}

impl ResourceStateManager {
    /// Create a new empty resource manager.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// use crate::core::server::ResourceStateManager;
    ///
    /// let manager = ResourceStateManager::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            resources: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    // ========== Resource Lifecycle ==========

    /// Get or create a resource, initializing its CRDT if needed.
    ///
    /// If the resource doesn't exist, it's created with a new CRDT using the provided
    /// agent ID. This agent ID is only used during initialization; future edits from
    /// any agent are merged into the same CRDT.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Unique resource identifier (e.g., document UUID)
    /// * `initial_agent_id` - Agent ID for the first CRDT initialization (if new)
    ///
    /// # Returns
    ///
    /// An Arc-wrapped Mutex to the resource state, allowing safe concurrent access.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let manager = ResourceStateManager::new();
    /// let resource = manager.get_or_create_resource("doc1", "session-1");
    /// // Now resource can be accessed and modified
    /// ```
    #[must_use]
    pub fn get_or_create_resource(
        &self,
        resource_id: &str,
        initial_agent_id: &str,
        requested_merge_type: Option<&str>,
    ) -> Arc<Mutex<ResourceState>> {
        let mut resources = self.resources.lock();

        resources
            .entry(resource_id.to_string())
            .or_insert_with(|| {
                let merge_type = requested_merge_type
                    .unwrap_or(crate::core::protocol::constants::merge_types::DIAMOND)
                    .to_string();
                Arc::new(Mutex::new(ResourceState {
                    crdt: DiamondCRDT::new(initial_agent_id),
                    last_sync: SystemTime::now(),
                    seen_versions: std::collections::HashSet::new(),
                    merge_type,
                }))
            })
            .clone()
    }

    /// Get an existing resource without creating it.
    ///
    /// Returns `None` if the resource hasn't been created yet.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Unique resource identifier
    ///
    /// # Returns
    ///
    /// `Some(Arc<Mutex<ResourceState>>)` if found, `None` otherwise.
    #[inline]
    #[must_use]
    pub fn get_resource(&self, resource_id: &str) -> Option<Arc<Mutex<ResourceState>>> {
        let resources = self.resources.lock();
        resources.get(resource_id).cloned()
    }

    /// List all resource IDs currently in memory.
    ///
    /// # Returns
    ///
    /// A vector of resource IDs in arbitrary order.
    #[must_use]
    pub fn list_resources(&self) -> Vec<String> {
        let resources = self.resources.lock();
        resources.keys().cloned().collect()
    }

    /// Check if a resource has already seen a specific version.
    #[must_use]
    pub fn has_version(&self, resource_id: &str, version_id: &str) -> bool {
        if let Some(resource) = self.get_resource(resource_id) {
            let state = resource.lock();
            state.seen_versions.contains(version_id)
        } else {
            false
        }
    }

    // ========== Edit Operations ==========

    /// Apply a full document update (replacement).
    ///
    /// Inserts text at position 0. This is typically used for initial document loads
    /// or complete replacements. The agent ID identifies the origin of this edit.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Resource to update
    /// * `content` - Content to insert
    /// * `agent_id` - Origin agent (for operation log tracking)
    ///
    /// # Returns
    ///
    /// Current resource state exported as JSON, or error if the operation failed.
    ///
    /// # Note
    ///
    /// Errors currently never occur (Result wrapper is for future extensibility).
    pub fn apply_update(
        &self,
        resource_id: &str,
        content: &str,
        agent_id: &str,
        version_id: Option<&str>,
        requested_merge_type: Option<&str>,
    ) -> Result<Value, String> {
        let resource = self.get_or_create_resource(resource_id, agent_id, requested_merge_type);
        let mut state = resource.lock();

        if let Some(req_mt) = requested_merge_type {
            if state.merge_type != req_mt {
                return Err(format!(
                    "Merge-type mismatch: resource is {}, requested {}",
                    state.merge_type, req_mt
                ));
            }
        }

        if let Some(vid) = version_id {
            if state.seen_versions.contains(vid) {
                return Ok(state.crdt.export_operations());
            }
            state.seen_versions.insert(vid.to_string());
        }

        state.crdt.add_insert(0, content);
        state.last_sync = SystemTime::now();

        Ok(state.crdt.export_operations())
    }

    /// Apply a remote insertion operation.
    ///
    /// Merges a text insertion from a peer into the resource's CRDT.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Resource to modify
    /// * `agent_id` - Peer's agent ID
    /// * `pos` - Position to insert at
    /// * `text` - Text to insert
    ///
    /// # Returns
    ///
    /// Current resource state after merge.
    pub fn apply_remote_insert(
        &self,
        resource_id: &str,
        agent_id: &str,
        pos: usize,
        text: &str,
        version_id: Option<&str>,
        requested_merge_type: Option<&str>,
    ) -> Result<Value, String> {
        let resource = self.get_or_create_resource(resource_id, agent_id, requested_merge_type);
        let mut state = resource.lock();

        if let Some(req_mt) = requested_merge_type {
            if state.merge_type != req_mt {
                return Err(format!(
                    "Merge-type mismatch: resource is {}, requested {}",
                    state.merge_type, req_mt
                ));
            }
        }

        if let Some(vid) = version_id {
            if state.seen_versions.contains(vid) {
                return Ok(state.crdt.export_operations());
            }
            state.seen_versions.insert(vid.to_string());
        }

        state.crdt.add_insert_remote(agent_id, pos, text);
        state.last_sync = SystemTime::now();

        Ok(state.crdt.export_operations())
    }

    /// Apply a remote deletion operation.
    ///
    /// Merges a deletion from a peer into the resource's CRDT.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Resource to modify
    /// * `agent_id` - Peer's agent ID
    /// * `start` - Delete range start (inclusive)
    /// * `end` - Delete range end (exclusive)
    ///
    /// # Returns
    ///
    /// Current resource state after merge.
    pub fn apply_remote_delete(
        &self,
        resource_id: &str,
        agent_id: &str,
        start: usize,
        end: usize,
        version_id: Option<&str>,
        requested_merge_type: Option<&str>,
    ) -> Result<Value, String> {
        let resource = self.get_or_create_resource(resource_id, agent_id, requested_merge_type);
        let mut state = resource.lock();

        if let Some(req_mt) = requested_merge_type {
            if state.merge_type != req_mt {
                return Err(format!(
                    "Merge-type mismatch: resource is {}, requested {}",
                    state.merge_type, req_mt
                ));
            }
        }

        if let Some(vid) = version_id {
            if state.seen_versions.contains(vid) {
                return Ok(state.crdt.export_operations());
            }
            state.seen_versions.insert(vid.to_string());
        }

        state.crdt.add_delete_remote(agent_id, start..end);
        state.last_sync = SystemTime::now();

        Ok(state.crdt.export_operations())
    }

    // ========== Query Methods ==========

    /// Get a snapshot of a resource's current state.
    ///
    /// Returns a JSON checkpoint containing content, version, agent ID, and operation count.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Resource to query
    ///
    /// # Returns
    ///
    /// `Some(Value)` if the resource exists, `None` otherwise.
    #[inline]
    #[must_use]
    pub fn get_resource_state(&self, resource_id: &str) -> Option<Value> {
        self.get_resource(resource_id).map(|resource| {
            let state = resource.lock();
            state.crdt.checkpoint()
        })
    }

    /// Get the merge quality score for a resource.
    ///
    /// Returns a heuristic score (0-100) indicating how well concurrent edits have converged.
    ///
    /// # Arguments
    ///
    /// * `resource_id` - Resource to query
    ///
    /// # Returns
    ///
    /// `Some(u32)` if the resource exists, `None` otherwise.
    #[inline]
    #[must_use]
    pub fn get_merge_quality(&self, resource_id: &str) -> Option<u32> {
        self.get_resource(resource_id).map(|resource| {
            let state = resource.lock();
            state.crdt.merge_quality()
        })
    }

    /// Register a version mapping for a resource.
    pub fn register_version_mapping(
        &self,
        resource_id: &str,
        version: String,
        frontier: crate::vendor::diamond_types::Frontier,
    ) {
        if let Some(resource) = self.get_resource(resource_id) {
            let mut state = resource.lock();
            state.crdt.register_version_mapping(version, frontier);
        }
    }

    /// Get history for a resource since a set of versions.
    pub fn get_history(
        &self,
        resource_id: &str,
        since_versions: &[&str],
    ) -> Result<Vec<crate::vendor::diamond_types::SerializedOpsOwned>, String> {
        let resource = self
            .get_resource(resource_id)
            .ok_or_else(|| format!("Resource not found: {}", resource_id))?;
        let state = resource.lock();

        let mut frontiers = Vec::new();
        for v in since_versions {
            if let Some(f) = state.crdt.resolve_version(v) {
                frontiers.push(f.clone());
            } else {
                return Err(format!("Version not found/pruned: {}", v));
            }
        }

        Ok(state.crdt.get_ops_since(&frontiers))
    }
}

impl Clone for ResourceStateManager {
    /// Clone a reference to the same resource registry.
    ///
    /// Cloning creates a new handle to the same underlying resources. Multiple
    /// clones can be distributed to different tasks and will all see the same
    /// document state.
    fn clone(&self) -> Self {
        Self {
            resources: Arc::clone(&self.resources),
        }
    }
}

impl Default for ResourceStateManager {
    /// Create a new resource manager (equivalent to `new()`)
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_resource() {
        let manager = ResourceStateManager::new();
        let resource = manager.get_or_create_resource("doc1", "alice", None);
        assert!(resource.lock().crdt.is_empty());
    }

    #[test]
    fn test_get_nonexistent_resource() {
        let manager = ResourceStateManager::new();
        let resource = manager.get_resource("nonexistent");
        assert!(resource.is_none());
    }

    #[test]
    fn test_apply_update() {
        let manager = ResourceStateManager::new();
        let result = manager.apply_update("doc1", "hello", "alice", None, None);
        assert!(result.is_ok());

        let state = manager.get_resource_state("doc1");
        assert!(state.is_some());
        assert_eq!(state.unwrap()["content"], "hello");
    }

    #[test]
    fn test_concurrent_updates() {
        let manager = ResourceStateManager::new();
        let _ = manager.apply_remote_insert("doc1", "alice", 0, "hello", None, None);
        let _ = manager.apply_remote_insert("doc1", "bob", 5, " world", None, None);

        let state = manager.get_resource_state("doc1");
        assert!(state.is_some());
        assert_eq!(state.unwrap()["content"], "hello world");
    }

    #[test]
    fn test_merge_quality() {
        let manager = ResourceStateManager::new();
        let quality = manager.get_merge_quality("doc1");
        assert!(quality.is_none());

        let _ = manager.apply_update("doc1", "text", "alice", None, None);
        let quality = manager.get_merge_quality("doc1");
        assert_eq!(quality, Some(100));
    }

    #[test]
    fn test_list_resources() {
        let manager = ResourceStateManager::new();
        let _ = manager.apply_update("doc1", "text", "alice", None, None);
        let _ = manager.apply_update("doc2", "text", "bob", None, None);

        let resources = manager.list_resources();
        assert_eq!(resources.len(), 2);
    }

    #[test]
    fn test_clone_shares_state() {
        let manager1 = ResourceStateManager::new();
        let _ = manager1.apply_update("doc1", "original", "alice", None, None);

        let manager2 = manager1.clone();
        let state = manager2.get_resource_state("doc1");
        assert_eq!(state.unwrap()["content"], "original");
    }
}
