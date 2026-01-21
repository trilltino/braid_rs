//! MergeType trait - Pluggable merge algorithm interface for Braid-HTTP.
//!
//! This module defines the core trait for merge algorithms that can be used
//! with Braid-HTTP resources. Multiple implementations can be registered
//! and selected per-resource.
//!
//! # Supported Merge Types
//!
//! | Name | Description |
//! |------|-------------|
//! | `"sync9"` | Simple last-write-wins synchronization |
//! | `"diamond"` | Diamond-types CRDT for text |
//! | `"antimatter"` | Antimatter CRDT with pruning |
//! | Custom | Application-defined algorithms |

use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Debug;

/// A patch representing a change to a resource.
#[derive(Debug, Clone)]
pub struct MergePatch {
    /// Range or path specifier (e.g., "0:5" or ".foo.bar")
    pub range: String,
    /// The content to insert/replace
    pub content: Value,
    /// Version ID that created this patch
    pub version: Option<String>,
    /// Parent versions this patch depends on
    pub parents: Vec<String>,
}

impl MergePatch {
    /// Create a new merge patch.
    pub fn new(range: &str, content: Value) -> Self {
        Self {
            range: range.to_string(),
            content,
            version: None,
            parents: Vec::new(),
        }
    }

    /// Create with version info.
    pub fn with_version(range: &str, content: Value, version: &str, parents: Vec<String>) -> Self {
        Self {
            range: range.to_string(),
            content,
            version: Some(version.to_string()),
            parents,
        }
    }
}

/// Result of a merge operation.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// Whether the merge was successful
    pub success: bool,
    /// Rebased patches that can be sent to other clients
    pub rebased_patches: Vec<MergePatch>,
    /// The new version ID created (if any)
    pub version: Option<String>,
    /// Error message if merge failed
    pub error: Option<String>,
}

impl MergeResult {
    /// Create a successful merge result.
    pub fn success(version: Option<String>, rebased_patches: Vec<MergePatch>) -> Self {
        Self {
            success: true,
            rebased_patches,
            version,
            error: None,
        }
    }

    /// Create a failed merge result.
    pub fn failure(error: &str) -> Self {
        Self {
            success: false,
            rebased_patches: Vec::new(),
            version: None,
            error: Some(error.to_string()),
        }
    }
}

/// Trait for pluggable merge algorithms.
///
/// Implementations of this trait can be registered with the Braid-HTTP server
/// to handle merge operations for resources.
pub trait MergeType: Debug + Send + Sync {
    /// Get the name of this merge type (e.g., "sync9", "diamond", "antimatter").
    fn name(&self) -> &str;

    /// Initialize the merge state with initial content.
    fn initialize(&mut self, content: &str) -> MergeResult;

    /// Apply a patch from a remote client.
    ///
    /// # Arguments
    /// * `patch` - The patch to apply
    ///
    /// # Returns
    /// MergeResult with rebased patches for other clients
    fn apply_patch(&mut self, patch: MergePatch) -> MergeResult;

    /// Apply a local edit and create a new version.
    ///
    /// # Arguments
    /// * `patch` - The local edit to apply
    ///
    /// # Returns
    /// MergeResult with the new version and patches to broadcast
    fn local_edit(&mut self, patch: MergePatch) -> MergeResult;

    /// Get the current content as a string.
    fn get_content(&self) -> String;

    /// Get the current version frontier.
    fn get_version(&self) -> Vec<String>;

    /// Get all known versions (for sync).
    fn get_all_versions(&self) -> HashMap<String, Vec<String>>;

    /// Prune old versions that are no longer needed.
    ///
    /// Called when all peers have acknowledged versions.
    fn prune(&mut self) -> bool;

    /// Check if this merge type supports history compression.
    fn supports_pruning(&self) -> bool {
        false
    }

    /// Clone this merge type instance.
    fn clone_box(&self) -> Box<dyn MergeType>;
}

impl Clone for Box<dyn MergeType> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Simple last-write-wins merge type.
///
/// This is the simplest merge strategy - the last write overwrites previous content.
/// No conflict resolution is performed.
#[derive(Debug, Clone)]
pub struct Sync9MergeType {
    content: String,
    version: String,
    seq: u64,
    id: String,
}

impl Sync9MergeType {
    /// Create a new Sync9 merge type with the given peer ID.
    pub fn new(id: &str) -> Self {
        Self {
            content: String::new(),
            version: String::new(),
            seq: 0,
            id: id.to_string(),
        }
    }

    fn generate_version(&mut self) -> String {
        self.seq += 1;
        format!("{}@{}", self.seq, self.id)
    }
}

impl MergeType for Sync9MergeType {
    fn name(&self) -> &str {
        "sync9"
    }

    fn initialize(&mut self, content: &str) -> MergeResult {
        self.content = content.to_string();
        self.version = self.generate_version();
        MergeResult::success(Some(self.version.clone()), Vec::new())
    }

    fn apply_patch(&mut self, patch: MergePatch) -> MergeResult {
        // Simple: just overwrite
        if let Some(version) = &patch.version {
            self.version = version.clone();
        }

        if let Value::String(s) = &patch.content {
            self.content = s.clone();
        } else {
            self.content = patch.content.to_string();
        }

        MergeResult::success(patch.version, Vec::new())
    }

    fn local_edit(&mut self, mut patch: MergePatch) -> MergeResult {
        let version = self.generate_version();
        patch.version = Some(version.clone());

        if let Value::String(s) = &patch.content {
            self.content = s.clone();
        } else {
            self.content = patch.content.to_string();
        }
        self.version = version.clone();

        MergeResult::success(Some(version), vec![patch])
    }

    fn get_content(&self) -> String {
        self.content.clone()
    }

    fn get_version(&self) -> Vec<String> {
        vec![self.version.clone()]
    }

    fn get_all_versions(&self) -> HashMap<String, Vec<String>> {
        let mut map = HashMap::new();
        map.insert(self.version.clone(), Vec::new());
        map
    }

    fn prune(&mut self) -> bool {
        false // Sync9 doesn't need pruning
    }

    fn clone_box(&self) -> Box<dyn MergeType> {
        Box::new(self.clone())
    }
}

/// Registry for available merge types.
pub struct MergeTypeRegistry {
    factories: HashMap<String, Box<dyn Fn(&str) -> Box<dyn MergeType> + Send + Sync>>,
}

impl std::fmt::Debug for MergeTypeRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MergeTypeRegistry")
            .field(
                "registered_types",
                &self.factories.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Default for MergeTypeRegistry {
    fn default() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }
}

impl MergeTypeRegistry {
    /// Create a new registry with default merge types.
    pub fn new() -> Self {
        let mut registry = Self::default();

        // Register built-in types
        registry.register("sync9", |id| Box::new(Sync9MergeType::new(id)));

        registry
    }

    /// Register a merge type factory.
    pub fn register<F>(&mut self, name: &str, factory: F)
    where
        F: Fn(&str) -> Box<dyn MergeType> + Send + Sync + 'static,
    {
        self.factories.insert(name.to_string(), Box::new(factory));
    }

    /// Create an instance of a merge type by name.
    pub fn create(&self, name: &str, peer_id: &str) -> Option<Box<dyn MergeType>> {
        self.factories.get(name).map(|f| f(peer_id))
    }

    /// List available merge types.
    pub fn list(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_sync9_initialize() {
        let mut merge = Sync9MergeType::new("alice");
        let result = merge.initialize("hello");

        assert!(result.success);
        assert_eq!(merge.get_content(), "hello");
    }

    #[test]
    fn test_sync9_local_edit() {
        let mut merge = Sync9MergeType::new("alice");
        merge.initialize("hello");

        let patch = MergePatch::new("", json!("world"));
        let result = merge.local_edit(patch);

        assert!(result.success);
        assert_eq!(merge.get_content(), "world");
        assert!(result.version.is_some());
    }

    #[test]
    fn test_registry() {
        let registry = MergeTypeRegistry::new();

        let merge = registry.create("sync9", "alice");
        assert!(merge.is_some());
        assert_eq!(merge.unwrap().name(), "sync9");
    }
}
