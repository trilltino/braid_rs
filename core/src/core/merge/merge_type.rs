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
//! | `"simpleton"` | Simple text-based merge with operational transform |
//! | `"diamond"` | Diamond-types CRDT for text |
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
    /// Get the name of this merge type (e.g., "simpleton", "diamond").
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
    /// Create a new empty registry.
    /// 
    /// Note: Use `register` to add merge types.
    pub fn new() -> Self {
        Self::default()
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
    fn test_merge_patch_new() {
        let patch = MergePatch::new("0:5", json!("hello"));
        assert_eq!(patch.range, "0:5");
        assert_eq!(patch.content, json!("hello"));
        assert!(patch.version.is_none());
    }

    #[test]
    fn test_merge_patch_with_version() {
        let patch = MergePatch::with_version("0:5", json!("hello"), "v1", vec!["v0".to_string()]);
        assert_eq!(patch.version, Some("v1".to_string()));
        assert_eq!(patch.parents, vec!["v0".to_string()]);
    }

    #[test]
    fn test_merge_result_success() {
        let result = MergeResult::success(Some("v1".to_string()), vec![]);
        assert!(result.success);
        assert_eq!(result.version, Some("v1".to_string()));
    }

    #[test]
    fn test_merge_result_failure() {
        let result = MergeResult::failure("something went wrong");
        assert!(!result.success);
        assert_eq!(result.error, Some("something went wrong".to_string()));
    }

    #[test]
    fn test_registry_new_empty() {
        let registry = MergeTypeRegistry::new();
        assert!(registry.list().is_empty());
    }

    #[test]
    fn test_registry_register_and_create() {
        let mut registry = MergeTypeRegistry::new();
        
        // Register a simple mock merge type
        registry.register("mock", |id| {
            Box::new(MockMergeType::new(id)) as Box<dyn MergeType>
        });
        
        assert_eq!(registry.list(), vec!["mock"]);
        
        let merge = registry.create("mock", "alice");
        assert!(merge.is_some());
        assert_eq!(merge.unwrap().name(), "mock");
    }

    #[test]
    fn test_registry_create_nonexistent() {
        let registry = MergeTypeRegistry::new();
        assert!(registry.create("nonexistent", "alice").is_none());
    }

    // Mock merge type for testing
    #[derive(Debug, Clone)]
    struct MockMergeType {
        id: String,
    }

    impl MockMergeType {
        fn new(id: &str) -> Self {
            Self { id: id.to_string() }
        }
    }

    impl MergeType for MockMergeType {
        fn name(&self) -> &str {
            "mock"
        }

        fn initialize(&mut self, _content: &str) -> MergeResult {
            MergeResult::success(None, vec![])
        }

        fn apply_patch(&mut self, _patch: MergePatch) -> MergeResult {
            MergeResult::success(None, vec![])
        }

        fn local_edit(&mut self, _patch: MergePatch) -> MergeResult {
            MergeResult::success(None, vec![])
        }

        fn get_content(&self) -> String {
            String::new()
        }

        fn get_version(&self) -> Vec<String> {
            vec![]
        }

        fn get_all_versions(&self) -> HashMap<String, Vec<String>> {
            HashMap::new()
        }

        fn prune(&mut self) -> bool {
            false
        }

        fn clone_box(&self) -> Box<dyn MergeType> {
            Box::new(self.clone())
        }
    }
}
