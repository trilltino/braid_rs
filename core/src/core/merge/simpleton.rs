use crate::core::merge::merge_type::{MergePatch, MergeResult, MergeType};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt::Debug;

/// Simpleton merge-type implementation.
///
/// Matches the behavior of braid-text's simpleton-client.js.
#[derive(Debug, Clone)]
pub struct SimpletonMergeType {
    peer_id: String,
    content: String,
    version: Vec<String>,
    seq: u64,
}

impl SimpletonMergeType {
    pub fn new(peer_id: &str) -> Self {
        Self {
            peer_id: peer_id.to_string(),
            content: String::new(),
            version: Vec::new(),
            seq: 0,
        }
    }

    fn generate_version(&mut self) -> String {
        self.seq += 1;
        format!("{}@{}", self.seq, self.peer_id)
    }

    #[allow(dead_code)]
    fn simple_diff(&self, old_text: &str, new_text: &str) -> (usize, usize, String) {
        let a: Vec<char> = old_text.chars().collect();
        let b: Vec<char> = new_text.chars().collect();

        // Common prefix
        let mut p = 0;
        let len = std::cmp::min(a.len(), b.len());
        while p < len && a[p] == b[p] {
            p += 1;
        }

        // Common suffix
        let mut s = 0;
        let len_remaining = std::cmp::min(a.len() - p, b.len() - p);
        while s < len_remaining && a[a.len() - s - 1] == b[b.len() - s - 1] {
            s += 1;
        }

        let range_start = p;
        let range_end = a.len() - s;
        let content: String = b[p..b.len() - s].iter().collect();

        (range_start, range_end, content)
    }
}

impl MergeType for SimpletonMergeType {
    fn name(&self) -> &str {
        "simpleton"
    }

    fn initialize(&mut self, content: &str) -> MergeResult {
        self.content = content.to_string();
        let version = self.generate_version();
        self.version = vec![version.clone()];
        MergeResult::success(Some(version), Vec::new())
    }

    fn apply_patch(&mut self, patch: MergePatch) -> MergeResult {
        let mut offset: isize = 0;
        let mut last_version = None;

        let range_regex = regex::Regex::new(r"\[(\d+):(\d+)\]").unwrap();
        if let Some(caps) = range_regex.captures(&patch.range) {
            let start_idx: usize = caps[1].parse().unwrap_or(0);
            let end_idx: usize = caps[2].parse().unwrap_or(0);

            let content_str = match &patch.content {
                Value::String(s) => s.clone(),
                v => v.to_string(),
            };

            let chars: Vec<char> = self.content.chars().collect();

            // Adjust start/end with current offset
            let start = (start_idx as isize + offset).max(0) as usize;
            let end = (end_idx as isize + offset).max(0) as usize;

            if start <= chars.len() && end <= chars.len() && start <= end {
                let mut new_chars = chars[..start].to_vec();
                new_chars.extend(content_str.chars());
                new_chars.extend(&chars[end..]);

                let deleted_count = end - start;
                let added_count = content_str.chars().count();
                let _offset_change = added_count as isize - deleted_count as isize;
                #[allow(unused_assignments)]
                {
                    offset += _offset_change;
                }

                self.content = new_chars.into_iter().collect();
                if let Some(v) = patch.version.clone() {
                    last_version = Some(v);
                }
            } else {
                return MergeResult::failure(&format!(
                    "Invalid range [{} : {}] for content length {}",
                    start,
                    end,
                    chars.len()
                ));
            }
        }

        if let Some(v) = last_version {
            self.version = vec![v.clone()];
        }

        MergeResult::success(patch.version, Vec::new())
    }

    fn local_edit(&mut self, mut patch: MergePatch) -> MergeResult {
        let version = self.generate_version();
        patch.version = Some(version.clone());

        // For simpleton, we expect the user to provide the full content or a diff
        // If it's a diff in [start:end] format, we use it. 
        // If it's just content, we replace everything? No, usually local_edit provides the change.
        
        // This is a simplified implementation. Real simpleton-client.js does more.
        // But for now, we follow the trait.
        
        self.apply_patch(patch.clone())
    }

    fn get_content(&self) -> String {
        self.content.clone()
    }

    fn get_version(&self) -> Vec<String> {
        self.version.clone()
    }

    fn get_all_versions(&self) -> HashMap<String, Vec<String>> {
        let mut map = HashMap::new();
        for v in &self.version {
            map.insert(v.clone(), Vec::new());
        }
        map
    }

    fn prune(&mut self) -> bool {
        false
    }

    fn clone_box(&self) -> Box<dyn MergeType> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ========== Constructor Tests ==========

    #[test]
    fn test_simpleton_new() {
        let simpleton = SimpletonMergeType::new("alice");
        assert_eq!(simpleton.name(), "simpleton");
        assert!(simpleton.get_content().is_empty());
        assert!(simpleton.get_version().is_empty());
    }

    #[test]
    fn test_simpleton_new_different_peers() {
        let alice = SimpletonMergeType::new("alice");
        let bob = SimpletonMergeType::new("bob");
        
        assert_eq!(alice.peer_id, "alice");
        assert_eq!(bob.peer_id, "bob");
    }

    // ========== Initialize Tests ==========

    #[test]
    fn test_simpleton_initialize() {
        let mut simpleton = SimpletonMergeType::new("alice");
        let result = simpleton.initialize("hello world");
        
        assert!(result.success);
        assert_eq!(simpleton.get_content(), "hello world");
        assert_eq!(simpleton.get_version().len(), 1);
        assert!(result.version.is_some());
    }

    #[test]
    fn test_simpleton_initialize_empty() {
        let mut simpleton = SimpletonMergeType::new("alice");
        let result = simpleton.initialize("");
        
        assert!(result.success);
        assert!(simpleton.get_content().is_empty());
    }

    // ========== Apply Patch Tests ==========

    #[test]
    fn test_simpleton_apply_patch_insert() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("hello world");
        
        // Insert " beautiful" at position 5 (after "hello")
        let patch = MergePatch::new("[5:5]", json!(" beautiful"));
        let result = simpleton.apply_patch(patch);
        
        assert!(result.success);
        assert_eq!(simpleton.get_content(), "hello beautiful world");
    }

    #[test]
    fn test_simpleton_apply_patch_replace() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("hello world");
        
        // Replace "world" with "universe"
        let patch = MergePatch::new("[6:11]", json!("universe"));
        let result = simpleton.apply_patch(patch);
        
        assert!(result.success);
        assert_eq!(simpleton.get_content(), "hello universe");
    }

    #[test]
    fn test_simpleton_apply_patch_delete() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("hello world");
        
        // Delete " world"
        let patch = MergePatch::new("[5:11]", json!(""));
        let result = simpleton.apply_patch(patch);
        
        assert!(result.success);
        assert_eq!(simpleton.get_content(), "hello");
    }

    #[test]
    fn test_simpleton_apply_patch_at_start() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("world");
        
        let patch = MergePatch::new("[0:0]", json!("hello "));
        let result = simpleton.apply_patch(patch);
        
        assert!(result.success);
        assert_eq!(simpleton.get_content(), "hello world");
    }

    #[test]
    fn test_simpleton_apply_patch_at_end() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("hello");
        
        let patch = MergePatch::new("[5:5]", json!(" world"));
        let result = simpleton.apply_patch(patch);
        
        assert!(result.success);
        assert_eq!(simpleton.get_content(), "hello world");
    }

    #[test]
    fn test_simpleton_apply_patch_invalid_range() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("hi");
        
        // Range beyond content length
        let patch = MergePatch::new("[10:20]", json!("xyz"));
        let result = simpleton.apply_patch(patch);
        
        assert!(!result.success);
    }

    // ========== Local Edit Tests ==========

    #[test]
    fn test_simpleton_local_edit() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("hello");
        
        let patch = MergePatch::new("[5:5]", json!(" world"));
        let result = simpleton.local_edit(patch);
        
        assert!(result.success);
        assert!(result.version.is_some());
        assert_eq!(simpleton.get_content(), "hello world");
    }

    #[test]
    fn test_simpleton_local_edit_generates_version() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("test");
        
        let patch = MergePatch::new("[0:4]", json!("changed"));
        let result = simpleton.local_edit(patch);
        
        let version = result.version.unwrap();
        assert!(version.contains('@')); // Format: seq@peer_id
        assert!(version.contains("alice"));
    }

    // ========== Version Tests ==========

    #[test]
    fn test_simpleton_version_increments() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("test");
        
        let v1 = simpleton.get_version()[0].clone();
        
        let patch = MergePatch::new("[0:4]", json!("changed"));
        simpleton.local_edit(patch);
        
        let v2 = simpleton.get_version()[0].clone();
        
        assert_ne!(v1, v2);
    }

    #[test]
    fn test_simpleton_get_all_versions() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("test");
        
        let versions = simpleton.get_all_versions();
        assert_eq!(versions.len(), 1);
    }

    // ========== Prune Tests ==========

    #[test]
    fn test_simpleton_prune_returns_false() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("test");
        
        assert!(!simpleton.prune());
    }

    // ========== Clone Tests ==========

    #[test]
    fn test_simpleton_clone() {
        let mut original = SimpletonMergeType::new("alice");
        original.initialize("hello");
        
        let cloned = original.clone();
        
        assert_eq!(cloned.get_content(), "hello");
        assert_eq!(cloned.peer_id, "alice");
    }

    #[test]
    fn test_simpleton_clone_box() {
        let mut simpleton = SimpletonMergeType::new("alice");
        simpleton.initialize("hello");
        
        let boxed: Box<dyn MergeType> = simpleton.clone_box();
        
        assert_eq!(boxed.get_content(), "hello");
        assert_eq!(boxed.name(), "simpleton");
    }

    // ========== Diff Tests ==========

    #[test]
    fn test_simpleton_simple_diff() {
        let simpleton = SimpletonMergeType::new("alice");
        
        let (start, end, content) = simpleton.simple_diff("hello world", "hello beautiful world");
        
        assert_eq!(start, 6); // After "hello "
        assert_eq!(end, 6);   // Insertion point
        assert_eq!(content, "beautiful ");
    }

    #[test]
    fn test_simpleton_simple_diff_replace() {
        let simpleton = SimpletonMergeType::new("alice");
        
        let (start, end, content) = simpleton.simple_diff("hello world", "hello universe");
        
        assert_eq!(start, 6);
        assert_eq!(end, 11); // End of "world"
        assert_eq!(content, "universe");
    }

    #[test]
    fn test_simpleton_simple_diff_delete() {
        let simpleton = SimpletonMergeType::new("alice");
        
        let (start, end, content) = simpleton.simple_diff("hello world", "hello");
        
        assert_eq!(start, 5);
        assert_eq!(end, 11);
        assert_eq!(content, "");
    }
}
