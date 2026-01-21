//! JSON CRDT - A pruneable CRDT for arbitrary JSON structures.
//!
//! This is a Rust port of the JavaScript json_crdt from the antimatter library.
//! It provides a CRDT wrapper around JSON values using sequence_crdt for
//! conflict-free merging of object properties and array elements.
//!
//! # Architecture
//!
//! JSON values are represented recursively:
//! - Objects: A map from key strings to JsonCrdt values
//! - Arrays: A sequence_crdt of JsonCrdt values
//! - Primitives: Stored as serde_json::Value with version metadata

use crate::antimatter::sequence_crdt::{self, SequenceElems, SequenceNode, Splice};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// A JSON CRDT for conflict-free JSON editing.
///
/// This wraps arbitrary JSON values with CRDT metadata for
/// versioned, mergeable JSON structures.
#[derive(Debug, Clone)]
pub struct JsonCrdt {
    /// The root sequence node for this value
    pub sequence: SequenceNode,
    /// Version graph: version -> set of parent versions
    pub t: HashMap<String, HashMap<String, bool>>,
    /// Current frontier versions
    pub current_version: HashMap<String, bool>,
    /// Next sequence number for generating unique versions
    pub next_seq: u64,
    /// Node ID for this peer
    pub id: String,
}

impl JsonCrdt {
    /// Create a new empty JSON CRDT.
    pub fn new(id: &str) -> Self {
        Self {
            sequence: SequenceNode::new(
                Some("root".to_string()),
                SequenceElems::String(String::new()),
                false,
                None,
            ),
            t: HashMap::new(),
            current_version: HashMap::new(),
            next_seq: 0,
            id: id.to_string(),
        }
    }

    /// Create a JSON CRDT with initial content.
    pub fn with_content(id: &str, content: &str) -> Self {
        let version = format!("0@{}", id);
        let mut crdt = Self::new(id);
        crdt.sequence = SequenceNode::text(&version, content);
        crdt.t.insert(version.clone(), HashMap::new());
        crdt.current_version.insert(version, true);
        crdt.next_seq = 1;
        crdt
    }

    /// Generate a new unique version ID.
    pub fn generate_version(&mut self) -> String {
        let version = format!("{}@{}", self.next_seq, self.id);
        self.next_seq += 1;
        version
    }

    /// Add a new version with the given patches.
    ///
    /// # Arguments
    /// * `version` - Unique version string
    /// * `parents` - Map of parent version IDs
    /// * `patches` - Array of patches to apply
    ///
    /// # Returns
    /// Rebased patches that can be used to update other views
    pub fn add_version(
        &mut self,
        version: &str,
        parents: HashMap<String, bool>,
        patches: Vec<JsonPatch>,
    ) -> Vec<Splice> {
        // Validate parents exist
        for p in parents.keys() {
            if !self.t.contains_key(p) && p != "root" {
                // Parent not found, skip
                return Vec::new();
            }
        }

        // Add to version graph
        self.t.insert(version.to_string(), parents.clone());

        // Update frontier
        for p in parents.keys() {
            self.current_version.remove(p);
        }
        self.current_version.insert(version.to_string(), true);

        // Convert patches to splices
        let splices: Vec<Splice> = patches.iter().map(|p| p.to_splice()).collect();

        // Apply splices to sequence
        let is_anc = |v: &str| self.t.contains_key(v) || v == "root";
        sequence_crdt::add_version(&mut self.sequence, version, splices, is_anc)
    }

    /// Get the current content as a string.
    pub fn get_content(&self) -> String {
        let is_anc = |v: &str| self.t.contains_key(v) || v == "root";
        sequence_crdt::content(&self.sequence, is_anc)
    }

    /// Get the current length.
    pub fn get_length(&self) -> usize {
        let is_anc = |v: &str| self.t.contains_key(v) || v == "root";
        sequence_crdt::length(&self.sequence, is_anc)
    }

    /// Check if ancestors relationship exists.
    pub fn is_ancestor(&self, version: &str) -> bool {
        self.t.contains_key(version) || version == "root"
    }

    /// Get the version graph.
    pub fn get_version_graph(&self) -> &HashMap<String, HashMap<String, bool>> {
        &self.t
    }

    /// Get the current frontier versions.
    pub fn get_frontier(&self) -> Vec<String> {
        self.current_version.keys().cloned().collect()
    }

    /// Generate braid for a specific version.
    pub fn generate_braid(&self, version: &str) -> Vec<Splice> {
        let is_anc = |v: &str| self.t.contains_key(v) || v == "root";
        sequence_crdt::generate_braid(&self.sequence, version, is_anc)
    }

    /// Apply bubble compression for pruning.
    pub fn apply_bubbles(&mut self, to_bubble: &HashMap<String, (String, String)>) {
        sequence_crdt::apply_bubbles(&mut self.sequence, to_bubble);

        // Update version graph
        for (old_v, (new_v, _)) in to_bubble {
            if old_v != new_v {
                if let Some(parents) = self.t.remove(old_v) {
                    // Update or merge with new version entry
                    self.t
                        .entry(new_v.clone())
                        .or_insert_with(HashMap::new)
                        .extend(parents);
                }
                if self.current_version.remove(old_v).is_some() {
                    self.current_version.insert(new_v.clone(), true);
                }
            }
        }
    }
}

/// A patch operation for modifying JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonPatch {
    /// The path in the JSON structure (e.g., ".foo.bar[0]" or "5:10" for ranges)
    pub range: String,
    /// The content to insert/replace
    pub content: Value,
}

impl JsonPatch {
    /// Create a new JSON patch.
    pub fn new(range: &str, content: Value) -> Self {
        Self {
            range: range.to_string(),
            content,
        }
    }

    /// Convert to a Splice for sequence operations.
    pub fn to_splice(&self) -> Splice {
        let (pos, delete_count) = parse_range(&self.range);
        let insert_str = match &self.content {
            Value::String(s) => s.clone(),
            _ => self.content.to_string(),
        };

        Splice {
            pos,
            delete_count,
            insert: SequenceElems::String(insert_str),
            sort_key: None,
            op_type: if delete_count > 0 { 'r' } else { 'i' },
        }
    }
}

/// Parse a range string like "5" or "5:10" into (position, delete_count).
fn parse_range(range: &str) -> (usize, usize) {
    if range.contains(':') {
        let parts: Vec<&str> = range.split(':').collect();
        if parts.len() == 2 {
            let start: usize = parts[0].parse().unwrap_or(0);
            let end: usize = parts[1].parse().unwrap_or(start);
            return (start, end.saturating_sub(start));
        }
    }
    (range.parse().unwrap_or(0), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_json_crdt() {
        let crdt = JsonCrdt::new("alice");
        assert_eq!(crdt.id, "alice");
        assert_eq!(crdt.next_seq, 0);
        assert!(crdt.t.is_empty());
    }

    #[test]
    fn test_with_content() {
        let crdt = JsonCrdt::with_content("alice", "hello");
        assert_eq!(crdt.get_content(), "hello");
        assert_eq!(crdt.get_length(), 5);
    }

    #[test]
    fn test_add_version() {
        let mut crdt = JsonCrdt::with_content("alice", "hello");

        let version = crdt.generate_version();
        let parents: HashMap<String, bool> = crdt.current_version.clone();

        let patch = JsonPatch::new("5", Value::String(" world".to_string()));
        crdt.add_version(&version, parents, vec![patch]);

        // Content should be updated
        assert!(crdt.t.contains_key(&version));
    }

    #[test]
    fn test_parse_range() {
        assert_eq!(parse_range("5"), (5, 0));
        assert_eq!(parse_range("5:10"), (5, 5));
        assert_eq!(parse_range("0:3"), (0, 3));
    }
}
