//! Antimatter CRDT Tests
//!
//! These tests verify that the Rust antimatter implementation
//! behaves identically to the JavaScript reference implementation.

use crate::antimatter::crdt_trait::PrunableCrdt;
use crate::antimatter::json_crdt::{JsonCrdt, JsonPatch};
use crate::antimatter::messages::{Message, Patch};
use crate::antimatter::sequence_crdt::{self, SequenceElems, SequenceNode};
use crate::antimatter::AntimatterCrdt;
use serde_json::json;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Mock CRDT for testing antimatter coordination layer.
#[derive(Default)]
pub struct MockSequenceCrdt {
    content: String,
    seq: u64,
}

impl PrunableCrdt for MockSequenceCrdt {
    fn apply_patch(&mut self, patch: Patch) {
        self.seq += 1;
        // Parse range and apply
        let range = &patch.range;
        let content = patch.content.as_str().unwrap_or("");

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
        }
    }

    fn prune(&mut self, _version: &str) {
        // No-op for mock
    }

    fn get_next_seq(&self) -> u64 {
        self.seq
    }
}

impl MockSequenceCrdt {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== sequence_crdt tests ====================

    #[test]
    fn test_sequence_node_creation() {
        let node = SequenceNode::text("alice1", "hello");
        assert_eq!(node.version, Some("alice1".to_string()));
        assert!(matches!(node.elems, SequenceElems::String(ref s) if s == "hello"));
    }

    #[test]
    fn test_sequence_content() {
        let node = SequenceNode::text("alice1", "hello world");
        let content = sequence_crdt::content(&node, |_| true);
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_sequence_length() {
        let node = SequenceNode::text("alice1", "hello");
        assert_eq!(sequence_crdt::length(&node, |_| true), 5);
    }

    #[test]
    fn test_sequence_get() {
        let node = SequenceNode::text("alice1", "hello");
        assert_eq!(sequence_crdt::get(&node, 0, |_| true), Some('h'));
        assert_eq!(sequence_crdt::get(&node, 4, |_| true), Some('o'));
        assert_eq!(sequence_crdt::get(&node, 5, |_| true), None);
    }

    #[test]
    fn test_sequence_deletion_visibility() {
        let mut node = SequenceNode::text("alice1", "hello");
        node.deleted_by.insert("bob1".to_string());

        // With both versions visible, content is deleted
        let len = sequence_crdt::length(&node, |v| v == "alice1" || v == "bob1");
        assert_eq!(len, 0);

        // Without delete version, content is visible
        let len = sequence_crdt::length(&node, |v| v == "alice1");
        assert_eq!(len, 5);
    }

    // ==================== json_crdt tests ====================

    #[test]
    fn test_json_crdt_creation() {
        let crdt = JsonCrdt::new("alice");
        assert_eq!(crdt.id, "alice");
        assert_eq!(crdt.next_seq, 0);
    }

    #[test]
    fn test_json_crdt_with_content() {
        let crdt = JsonCrdt::with_content("alice", "hello");
        assert_eq!(crdt.get_content(), "hello");
        assert_eq!(crdt.get_length(), 5);
    }

    #[test]
    fn test_json_crdt_version_generation() {
        let mut crdt = JsonCrdt::new("alice");
        let v1 = crdt.generate_version();
        let v2 = crdt.generate_version();
        assert!(v1 != v2);
        assert!(v1.ends_with("@alice"));
        assert!(v2.ends_with("@alice"));
    }

    #[test]
    fn test_json_crdt_frontier() {
        let crdt = JsonCrdt::with_content("alice", "hello");
        let frontier = crdt.get_frontier();
        assert_eq!(frontier.len(), 1);
        assert!(frontier[0].contains("@alice"));
    }

    // ==================== MockSequenceCrdt tests ====================

    #[test]
    fn test_mock_crdt_insert() {
        let mut crdt = MockSequenceCrdt::new();
        crdt.apply_patch(Patch {
            range: "0".to_string(),
            content: json!("hello"),
        });
        assert_eq!(crdt.content(), "hello");
    }

    #[test]
    fn test_mock_crdt_append() {
        let mut crdt = MockSequenceCrdt::new();
        crdt.apply_patch(Patch {
            range: "0".to_string(),
            content: json!("hello"),
        });
        crdt.apply_patch(Patch {
            range: "5".to_string(),
            content: json!(" world"),
        });
        assert_eq!(crdt.content(), "hello world");
    }

    #[test]
    fn test_mock_crdt_replace() {
        let mut crdt = MockSequenceCrdt::new();
        crdt.apply_patch(Patch {
            range: "0".to_string(),
            content: json!("hello"),
        });
        crdt.apply_patch(Patch {
            range: "1:4".to_string(),
            content: json!("i"),
        });
        assert_eq!(crdt.content(), "hio");
    }

    // ==================== AntimatterCrdt basic tests ====================

    #[test]
    fn test_antimatter_creation() {
        let messages: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(Vec::new()));
        let msgs = messages.clone();

        let crdt = AntimatterCrdt::new(
            Some("alice".to_string()),
            MockSequenceCrdt::new(),
            Box::new(move |msg| {
                msgs.lock().unwrap().push(msg);
            }),
        );

        assert_eq!(crdt.id, "alice");
        assert!(crdt.t.is_empty());
        assert!(crdt.current_version.is_empty());
    }

    #[test]
    fn test_antimatter_add_version() {
        let messages: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(Vec::new()));
        let msgs = messages.clone();

        let mut crdt = AntimatterCrdt::new(
            Some("alice".to_string()),
            MockSequenceCrdt::new(),
            Box::new(move |msg| {
                msgs.lock().unwrap().push(msg);
            }),
        );

        let patches = crdt.add_version(
            "1@alice".to_string(),
            HashMap::new(),
            vec![Patch {
                range: "0".to_string(),
                content: json!("hello"),
            }],
        );

        assert!(crdt.t.contains_key("1@alice"));
        assert!(crdt.current_version.contains_key("1@alice"));
        assert_eq!(crdt.crdt.content(), "hello");
    }

    #[test]
    fn test_antimatter_version_parents() {
        let messages: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(Vec::new()));
        let msgs = messages.clone();

        let mut crdt = AntimatterCrdt::new(
            Some("alice".to_string()),
            MockSequenceCrdt::new(),
            Box::new(move |msg| {
                msgs.lock().unwrap().push(msg);
            }),
        );

        // Add first version
        crdt.add_version(
            "1@alice".to_string(),
            HashMap::new(),
            vec![Patch {
                range: "0".to_string(),
                content: json!("hello"),
            }],
        );

        // Add second version with first as parent
        let parents: HashMap<String, bool> = [("1@alice".to_string(), true)].into_iter().collect();
        crdt.add_version(
            "2@alice".to_string(),
            parents,
            vec![Patch {
                range: "5".to_string(),
                content: json!(" world"),
            }],
        );

        assert!(crdt.t.contains_key("2@alice"));
        assert!(crdt.current_version.contains_key("2@alice"));
        assert!(!crdt.current_version.contains_key("1@alice")); // Removed from frontier
        assert_eq!(crdt.crdt.content(), "hello world");
    }

    #[test]
    fn test_antimatter_ancestors() {
        let messages: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(Vec::new()));
        let msgs = messages.clone();

        let mut crdt = AntimatterCrdt::new(
            Some("alice".to_string()),
            MockSequenceCrdt::new(),
            Box::new(move |msg| {
                msgs.lock().unwrap().push(msg);
            }),
        );

        crdt.add_version("1@alice".to_string(), HashMap::new(), vec![]);

        let parents: HashMap<String, bool> = [("1@alice".to_string(), true)].into_iter().collect();
        crdt.add_version("2@alice".to_string(), parents, vec![]);

        let parents: HashMap<String, bool> = [("2@alice".to_string(), true)].into_iter().collect();
        crdt.add_version("3@alice".to_string(), parents, vec![]);

        // Check ancestors of version 3
        let versions: HashMap<String, bool> = [("3@alice".to_string(), true)].into_iter().collect();
        let ancestors = crdt.ancestors(&versions, false).unwrap();

        assert!(ancestors.contains_key("3@alice"));
        assert!(ancestors.contains_key("2@alice"));
        assert!(ancestors.contains_key("1@alice"));
    }

    #[test]
    fn test_antimatter_descendants() {
        let messages: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(Vec::new()));
        let msgs = messages.clone();

        let mut crdt = AntimatterCrdt::new(
            Some("alice".to_string()),
            MockSequenceCrdt::new(),
            Box::new(move |msg| {
                msgs.lock().unwrap().push(msg);
            }),
        );

        crdt.add_version("1@alice".to_string(), HashMap::new(), vec![]);

        let parents: HashMap<String, bool> = [("1@alice".to_string(), true)].into_iter().collect();
        crdt.add_version("2@alice".to_string(), parents, vec![]);

        let parents: HashMap<String, bool> = [("2@alice".to_string(), true)].into_iter().collect();
        crdt.add_version("3@alice".to_string(), parents, vec![]);

        // Check descendants of version 1
        let versions: HashMap<String, bool> = [("1@alice".to_string(), true)].into_iter().collect();
        let descendants = crdt.descendants(&versions, false).unwrap();

        assert!(descendants.contains_key("1@alice"));
        assert!(descendants.contains_key("2@alice"));
        assert!(descendants.contains_key("3@alice"));
    }

    #[test]
    fn test_antimatter_get_leaves() {
        let messages: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(Vec::new()));
        let msgs = messages.clone();

        let mut crdt = AntimatterCrdt::new(
            Some("alice".to_string()),
            MockSequenceCrdt::new(),
            Box::new(move |msg| {
                msgs.lock().unwrap().push(msg);
            }),
        );

        crdt.add_version("1@alice".to_string(), HashMap::new(), vec![]);

        let parents: HashMap<String, bool> = [("1@alice".to_string(), true)].into_iter().collect();
        crdt.add_version("2@alice".to_string(), parents, vec![]);

        let versions: HashMap<String, bool> =
            [("1@alice".to_string(), true), ("2@alice".to_string(), true)]
                .into_iter()
                .collect();

        let leaves = crdt.get_leaves(&versions);
        assert!(leaves.contains_key("2@alice"));
        assert!(!leaves.contains_key("1@alice"));
    }

    #[test]
    fn test_antimatter_prune_empty() {
        let messages: Arc<Mutex<Vec<Message>>> = Arc::new(Mutex::new(Vec::new()));
        let msgs = messages.clone();

        let mut crdt = AntimatterCrdt::new(
            Some("alice".to_string()),
            MockSequenceCrdt::new(),
            Box::new(move |msg| {
                msgs.lock().unwrap().push(msg);
            }),
        );

        // Prune on empty should return false (nothing to prune)
        assert!(!crdt.prune(true));
    }
}
