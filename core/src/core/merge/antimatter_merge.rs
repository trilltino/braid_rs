//! Antimatter implementation of MergeType trait.
//!
//! This module provides the Antimatter CRDT as a pluggable merge type
//! for Braid-HTTP resources.

use super::merge_type::{MergePatch, MergeResult, MergeType};
use crate::antimatter::crdt_trait::PrunableCrdt;
use crate::antimatter::messages::{Message, Patch};
use crate::antimatter::AntimatterCrdt;
use crate::core::traits::BraidRuntime;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Simple CRDT implementation for Antimatter merge type.
#[derive(Debug, Default, Clone)]
struct SimpleCrdt {
    content: String,
    seq: u64,
}

impl PrunableCrdt for SimpleCrdt {
    fn apply_patch(&mut self, patch: Patch) {
        self.seq += 1;
        let content = patch.content.as_str().unwrap_or("");
        let range = &patch.range;

        if range.contains(':') {
            let parts: Vec<&str> = range.split(':').collect();
            if parts.len() == 2 {
                if let (Ok(start), Ok(end)) = (parts[0].parse::<usize>(), parts[1].parse::<usize>())
                {
                    let start = start.min(self.content.len());
                    let end = end.min(self.content.len());
                    self.content = format!(
                        "{}{}{}",
                        &self.content[..start],
                        content,
                        &self.content[end..]
                    );
                }
            }
        } else if let Ok(pos) = range.parse::<usize>() {
            let pos = pos.min(self.content.len());
            self.content = format!(
                "{}{}{}",
                &self.content[..pos],
                content,
                &self.content[pos..]
            );
        } else {
            // Replace all
            self.content = content.to_string();
        }
    }

    fn prune(&mut self, _version: &str) {
        // No-op for simple crdt
    }

    fn get_next_seq(&self) -> u64 {
        self.seq
    }

    fn generate_braid(
        &self,
        known_versions: &std::collections::HashMap<String, bool>,
    ) -> Vec<(String, std::collections::HashMap<String, bool>, Vec<Patch>)> {
        // For simple CRDT, we just send the whole content as one update if they don't have it
        if known_versions.is_empty() {
            vec![(
                "initial".to_string(),
                HashMap::new(),
                vec![Patch {
                    range: "".to_string(),
                    content: Value::String(self.content.clone()),
                }],
            )]
        } else {
            Vec::new()
        }
    }
}

/// Antimatter CRDT merge type.
///
/// This implements the MergeType trait using the Antimatter algorithm,
/// providing history compression and peer acknowledgment for efficient
/// syncing.
pub struct AntimatterMergeType {
    crdt: AntimatterCrdt<SimpleCrdt>,
    messages: Arc<Mutex<Vec<Message>>>,
    runtime: Arc<dyn BraidRuntime>,
}

impl AntimatterMergeType {
    /// Create a new Antimatter merge type with the given peer ID and runtime.
    pub fn new(peer_id: &str, runtime: Arc<dyn BraidRuntime>) -> Self {
        let messages: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(Vec::new()));
        let msgs = messages.clone();

        let crdt = AntimatterCrdt::new(
            Some(peer_id.to_string()),
            SimpleCrdt::default(),
            Arc::new(move |msg| {
                if let Ok(mut queue) = msgs.lock() {
                    queue.push(msg);
                }
            }),
            runtime.clone(),
        );

        Self {
            crdt,
            messages,
            runtime,
        }
    }

    /// Create a new Antimatter merge type using the native runtime.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new_native(peer_id: &str) -> Self {
        Self::new(peer_id, Arc::new(crate::core::traits::NativeRuntime))
    }

    /// Get pending messages to send to peers.
    pub fn drain_messages(&self) -> Vec<Message> {
        if let Ok(mut queue) = self.messages.lock() {
            queue.drain(..).collect()
        } else {
            Vec::new()
        }
    }

    fn patch_to_internal(&self, patch: &MergePatch) -> Patch {
        Patch {
            range: patch.range.clone(),
            content: patch.content.clone(),
        }
    }
}

impl std::fmt::Debug for AntimatterMergeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AntimatterMergeType")
            .field("id", &self.crdt.id)
            .field("versions", &self.crdt.current_version.len())
            .finish()
    }
}

impl MergeType for AntimatterMergeType {
    fn name(&self) -> &str {
        "antimatter"
    }

    fn initialize(&mut self, content: &str) -> MergeResult {
        self.crdt.crdt.content = content.to_string();

        // Create initial version
        let patch = Patch {
            range: "".to_string(),
            content: Value::String(content.to_string()),
        };
        let version = self.crdt.update(vec![patch]);

        MergeResult::success(Some(version), Vec::new())
    }

    fn apply_patch(&mut self, patch: MergePatch) -> MergeResult {
        let internal_patch = self.patch_to_internal(&patch);
        let version = patch.version.clone();
        let parents: HashMap<String, bool> =
            patch.parents.iter().map(|p| (p.clone(), true)).collect();

        if let Some(v) = &version {
            let rebased = self
                .crdt
                .add_version(v.clone(), parents, vec![internal_patch]);

            let rebased_patches: Vec<MergePatch> = rebased
                .into_iter()
                .map(|p| MergePatch::new(&p.range, p.content))
                .collect();

            MergeResult::success(Some(v.clone()), rebased_patches)
        } else {
            MergeResult::failure("No version provided for remote patch")
        }
    }

    fn local_edit(&mut self, patch: MergePatch) -> MergeResult {
        if let serde_json::Value::String(new_content) = &patch.content {
            if new_content == &self.crdt.crdt.content && patch.range.is_empty() {
                return MergeResult::success(None, Vec::new());
            }
        }

        let internal_patch = self.patch_to_internal(&patch);
        let version = self.crdt.update(vec![internal_patch.clone()]);

        let out_patch =
            MergePatch::with_version(&patch.range, patch.content, &version, self.get_version());

        MergeResult::success(Some(version), vec![out_patch])
    }

    fn get_content(&self) -> String {
        self.crdt.crdt.content.clone()
    }

    fn get_version(&self) -> Vec<String> {
        self.crdt.current_version.keys().cloned().collect()
    }

    fn get_all_versions(&self) -> HashMap<String, Vec<String>> {
        self.crdt
            .t
            .iter()
            .map(|(k, v)| (k.clone(), v.iter().cloned().collect()))
            .collect()
    }

    fn prune(&mut self) -> bool {
        self.crdt.prune(false)
    }

    fn supports_pruning(&self) -> bool {
        true
    }

    fn clone_box(&self) -> Box<dyn MergeType> {
        Box::new(Self {
            crdt: self.crdt.clone(),
            messages: self.messages.clone(),
            runtime: self.runtime.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_antimatter_creation() {
        let merge = AntimatterMergeType::new_native("alice");
        assert_eq!(merge.name(), "antimatter");
    }

    #[test]
    fn test_antimatter_initialize() {
        let mut merge = AntimatterMergeType::new_native("alice");
        let result = merge.initialize("hello");

        assert!(result.success);
        assert_eq!(merge.get_content(), "hello");
    }

    #[test]
    fn test_antimatter_local_edit() {
        let mut merge = AntimatterMergeType::new_native("alice");
        merge.initialize("");

        let patch = MergePatch::new("0", json!("hello"));
        let result = merge.local_edit(patch);

        assert!(result.success);
        assert!(result.version.is_some());
        assert!(!merge.get_version().is_empty());
    }

    #[test]
    fn test_antimatter_supports_pruning() {
        let merge = AntimatterMergeType::new_native("alice");
        assert!(merge.supports_pruning());
    }
}
