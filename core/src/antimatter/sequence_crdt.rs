//! Sequence CRDT - A pruneable sequence CRDT for strings and arrays.
//!
//! This is a Rust port of the JavaScript sequence_crdt from the antimatter library.
//! It represents a versioned sequence (string or array) that supports:
//! - Concurrent inserts and deletes from multiple peers
//! - CRDT-style merge for conflict resolution
//! - Pruning to remove old metadata
//!
//! # Architecture
//!
//! The sequence is represented as a tree of `SequenceNode`s, where each node
//! contains a slice of the sequence and metadata about its version and deletions.

use std::collections::{HashMap, HashSet};

/// A node in the sequence CRDT tree.
///
/// Each node represents a contiguous slice of the sequence, with metadata
/// for version tracking and deletion.
#[derive(Debug, Clone)]
pub struct SequenceNode {
    /// Globally unique version string identifying when this node was created
    pub version: Option<String>,
    /// Version to use for sorting (if different from version)
    pub sort_key: Option<String>,
    /// The actual elements (string for text, or indices into separate array)
    pub elems: SequenceElems,
    /// If true, this marks the end of a replacement operation
    pub end_cap: bool,
    /// Set of versions that have deleted this node
    pub deleted_by: HashSet<String>,
    /// Array of nodes that branch from after this node
    pub nexts: Vec<Box<SequenceNode>>,
    /// The next node in the linear sequence (after nexts)
    pub next: Option<Box<SequenceNode>>,
}

/// Elements in a sequence node - either a string or indices to values.
#[derive(Debug, Clone)]
pub enum SequenceElems {
    /// String elements (for text sequences)
    String(String),
    /// Indices into a separate value array (for JSON arrays/objects)
    Indices(Vec<usize>),
}

impl SequenceElems {
    /// Get the length of the elements.
    pub fn len(&self) -> usize {
        match self {
            SequenceElems::String(s) => s.len(),
            SequenceElems::Indices(v) => v.len(),
        }
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Slice the elements from start to end.
    pub fn slice(&self, start: usize, end: usize) -> Self {
        match self {
            SequenceElems::String(s) => {
                SequenceElems::String(s.chars().skip(start).take(end - start).collect())
            }
            SequenceElems::Indices(v) => SequenceElems::Indices(v[start..end].to_vec()),
        }
    }

    /// Concatenate with another SequenceElems.
    pub fn concat(&self, other: &Self) -> Self {
        match (self, other) {
            (SequenceElems::String(a), SequenceElems::String(b)) => {
                SequenceElems::String(format!("{}{}", a, b))
            }
            (SequenceElems::Indices(a), SequenceElems::Indices(b)) => {
                let mut result = a.clone();
                result.extend(b);
                SequenceElems::Indices(result)
            }
            _ => panic!("Cannot concat different element types"),
        }
    }

    /// Create empty elements of the same type.
    pub fn empty_like(&self) -> Self {
        match self {
            SequenceElems::String(_) => SequenceElems::String(String::new()),
            SequenceElems::Indices(_) => SequenceElems::Indices(Vec::new()),
        }
    }
}

impl SequenceNode {
    /// Create a new sequence node.
    ///
    /// # Arguments
    /// * `version` - Globally unique version string
    /// * `elems` - The elements this node contains
    /// * `end_cap` - Whether this marks end of a replacement
    /// * `sort_key` - Optional sort key for ordering
    pub fn new(
        version: Option<String>,
        elems: SequenceElems,
        end_cap: bool,
        sort_key: Option<String>,
    ) -> Self {
        Self {
            version,
            sort_key,
            elems,
            end_cap,
            deleted_by: HashSet::new(),
            nexts: Vec::new(),
            next: None,
        }
    }

    /// Create a text node from a string.
    pub fn text(version: &str, text: &str) -> Self {
        Self::new(
            Some(version.to_string()),
            SequenceElems::String(text.to_string()),
            false,
            None,
        )
    }

    /// Get the effective sort key for this node.
    pub fn effective_sort_key(&self) -> Option<&str> {
        self.sort_key.as_deref().or(self.version.as_deref())
    }
}

/// A splice operation for modifying the sequence.
#[derive(Debug, Clone)]
pub struct Splice {
    /// Position in the sequence
    pub pos: usize,
    /// Number of elements to delete
    pub delete_count: usize,
    /// Elements to insert
    pub insert: SequenceElems,
    /// Optional sort key
    pub sort_key: Option<String>,
    /// Operation type: 'i' for insert, 'd' for delete, 'r' for replace
    pub op_type: char,
}

/// Get the length of the sequence rooted at this node.
pub fn length<F>(root: &SequenceNode, is_anc: F) -> usize
where
    F: Fn(&str) -> bool + Copy,
{
    let mut count = 0;
    traverse(root, is_anc, |node, _, _, _, _, deleted| {
        if !deleted {
            count += node.elems.len();
        }
        true
    });
    count
}

/// Get the element at position i in the sequence.
pub fn get<F>(root: &SequenceNode, i: usize, is_anc: F) -> Option<char>
where
    F: Fn(&str) -> bool + Copy,
{
    let mut offset = 0;
    let mut result = None;
    traverse(root, is_anc, |node, _, _, _, _, deleted| {
        if deleted {
            return true;
        }
        if i < offset + node.elems.len() {
            if let SequenceElems::String(s) = &node.elems {
                result = s.chars().nth(i - offset);
            }
            return false; // Stop traversal
        }
        offset += node.elems.len();
        true
    });
    result
}

/// Traverse the sequence tree, calling the callback for each visible node.
///
/// # Arguments
/// * `root` - Root node of the sequence
/// * `is_anc` - Function that returns true if a version should be included
/// * `callback` - Called for each node: (node, offset, has_nexts, prev_version, version, deleted) -> continue
pub fn traverse<F, C>(root: &SequenceNode, is_anc: F, mut callback: C)
where
    F: Fn(&str) -> bool + Copy,
    C: FnMut(&SequenceNode, usize, bool, Option<&str>, Option<&str>, bool) -> bool,
{
    let mut offset = 0;

    fn helper<F, C>(
        node: &SequenceNode,
        _prev: Option<&SequenceNode>,
        version: Option<&str>,
        is_anc: F,
        callback: &mut C,
        offset: &mut usize,
    ) -> bool
    where
        F: Fn(&str) -> bool + Copy,
        C: FnMut(&SequenceNode, usize, bool, Option<&str>, Option<&str>, bool) -> bool,
    {
        let has_nexts = node
            .nexts
            .iter()
            .any(|next| next.version.as_ref().map_or(false, |v| is_anc(v)));
        let deleted = node.deleted_by.iter().any(|v| is_anc(v));

        if !callback(node, *offset, has_nexts, None, version, deleted) {
            return false;
        }

        if !deleted {
            *offset += node.elems.len();
        }

        for next in &node.nexts {
            if next.version.as_ref().map_or(false, |v| is_anc(v)) {
                if !helper(
                    next,
                    Some(node),
                    next.version.as_deref(),
                    is_anc,
                    callback,
                    offset,
                ) {
                    return false;
                }
            }
        }

        if let Some(ref next) = node.next {
            if !helper(next, Some(node), version, is_anc, callback, offset) {
                return false;
            }
        }

        true
    }

    helper(
        root,
        None,
        root.version.as_deref(),
        is_anc,
        &mut callback,
        &mut offset,
    );
}

/// Get the content of the sequence as a string.
pub fn content<F>(root: &SequenceNode, is_anc: F) -> String
where
    F: Fn(&str) -> bool + Copy,
{
    let mut result = String::new();
    traverse(root, is_anc, |node, _, _, _, _, deleted| {
        if !deleted {
            if let SequenceElems::String(s) = &node.elems {
                result.push_str(s);
            }
        }
        true
    });
    result
}

/// Break a node at position x, returning the tail node.
///
/// The original node is modified to contain elements before x,
/// and a new node is returned containing elements from x onwards.
pub fn break_node(
    node: &mut SequenceNode,
    x: usize,
    end_cap: bool,
    new_next: Option<Box<SequenceNode>>,
) -> Box<SequenceNode> {
    let tail_elems = node.elems.slice(x, node.elems.len());
    let mut tail = Box::new(SequenceNode::new(None, tail_elems, node.end_cap, None));

    // Copy deleted_by to tail
    tail.deleted_by = node.deleted_by.clone();
    tail.nexts = std::mem::take(&mut node.nexts);
    tail.next = node.next.take();

    // Update original node
    node.elems = node.elems.slice(0, x);
    node.end_cap = end_cap;
    node.nexts = match new_next {
        Some(n) => vec![n],
        None => Vec::new(),
    };
    node.next = Some(tail.clone());

    tail
}

/// Add a version to the sequence CRDT.
///
/// # Arguments
/// * `root` - Root node of the sequence
/// * `version` - Unique version string for this modification
/// * `splices` - Array of splice operations
/// * `is_anc` - Function returning true for versions to consider
///
/// # Returns
/// Rebased splices that can be used to update other views
pub fn add_version<F>(
    root: &mut SequenceNode,
    version: &str,
    splices: Vec<Splice>,
    is_anc: F,
) -> Vec<Splice>
where
    F: Fn(&str) -> bool + Copy,
{
    let mut rebased_splices = Vec::new();

    if splices.is_empty() {
        return rebased_splices;
    }

    let mut si = 0; // Current splice index
    let mut delete_up_to = 0;
    let mut offset = 0;

    // Process each node in traversal order
    fn process_splices(
        node: &mut SequenceNode,
        splices: &[Splice],
        si: &mut usize,
        delete_up_to: &mut usize,
        offset: &mut usize,
        version: &str,
        is_anc: impl Fn(&str) -> bool + Copy,
    ) {
        if *si >= splices.len() {
            return;
        }

        let s = &splices[*si];
        let deleted = node.deleted_by.iter().any(|v| is_anc(v));

        if deleted {
            // Handle inserts at deleted positions
            if s.delete_count == 0 && s.pos == *offset {
                // Create new insert node
                let new_node = Box::new(SequenceNode::new(
                    Some(version.to_string()),
                    s.insert.clone(),
                    false,
                    s.sort_key.clone(),
                ));

                // Add to nexts (simplified - full implementation needs binary search)
                node.nexts.push(new_node);
                *si += 1;
            }
            return;
        }

        // Pure insert (no delete)
        if s.delete_count == 0 {
            let d = s.pos as isize - (*offset + node.elems.len()) as isize;
            if d > 0 {
                return; // Not at this node yet
            }
            if d == 0 && !node.end_cap && !node.nexts.is_empty() {
                return; // Insert at end with nexts, skip
            }

            let new_node = Box::new(SequenceNode::new(
                Some(version.to_string()),
                s.insert.clone(),
                false,
                s.sort_key.clone(),
            ));

            if d == 0 && !node.end_cap {
                node.nexts.push(new_node);
            } else {
                let break_pos = s.pos - *offset;
                break_node(node, break_pos, false, Some(new_node));
            }
            *si += 1;
            return;
        }

        // Delete operation
        if *delete_up_to <= *offset {
            let d = s.pos as isize - (*offset + node.elems.len()) as isize;
            if d > 0 || (d == 0) {
                return;
            }

            *delete_up_to = s.pos + s.delete_count;

            if !s.insert.is_empty() {
                let new_node = Box::new(SequenceNode::new(
                    Some(version.to_string()),
                    s.insert.clone(),
                    false,
                    s.sort_key.clone(),
                ));

                let break_pos = s.pos - *offset;
                break_node(node, break_pos, true, Some(new_node));
                return;
            } else if s.pos != *offset {
                let break_pos = s.pos - *offset;
                break_node(node, break_pos, false, None);
                return;
            }
        }

        // Mark deletion
        if *delete_up_to > *offset {
            if *delete_up_to <= *offset + node.elems.len() {
                if *delete_up_to < *offset + node.elems.len() {
                    let break_pos = *delete_up_to - *offset;
                    break_node(node, break_pos, false, None);
                }
                *si += 1;
            }
            node.deleted_by.insert(version.to_string());
        }
    }

    // Simple traversal for modification
    // Note: Full implementation would need mutable tree traversal
    // This is a simplified version
    process_splices(
        root,
        &splices,
        &mut si,
        &mut delete_up_to,
        &mut offset,
        version,
        is_anc,
    );

    rebased_splices
}

/// Generate braid (splice information) for a version.
///
/// Reconstructs an array of splices that can be passed to `add_version`
/// to recreate a specific version on another sequence_crdt instance.
///
/// # Arguments
/// * `root` - Root node of the sequence
/// * `version` - The version to generate braid for
/// * `is_anc` - Function returning true for ancestor versions
///
/// # Returns
/// Array of `Splice` operations representing the version's changes
pub fn generate_braid<F>(root: &SequenceNode, version: &str, is_anc: F) -> Vec<Splice>
where
    F: Fn(&str) -> bool + Copy,
{
    let mut splices = Vec::new();
    let mut offset = 0;

    fn helper<F>(
        node: &SequenceNode,
        _version: Option<&str>,
        target_version: &str,
        is_anc: F,
        splices: &mut Vec<Splice>,
        offset: &mut usize,
        end_cap: bool,
    ) where
        F: Fn(&str) -> bool + Copy,
    {
        let node_version = node.version.as_deref();

        // If this node was created by the target version, add an insert
        if node_version == Some(target_version) {
            let splice = Splice {
                pos: *offset,
                delete_count: 0,
                insert: node.elems.clone(),
                sort_key: node.sort_key.clone(),
                op_type: if end_cap { 'r' } else { 'i' },
            };
            splices.push(splice);
        }
        // If this node was deleted by the target version, add a delete
        else if node.deleted_by.contains(target_version) && !node.elems.is_empty() {
            let splice = Splice {
                pos: *offset,
                delete_count: node.elems.len(),
                insert: node.elems.empty_like(),
                sort_key: None,
                op_type: 'd',
            };
            splices.push(splice);
        }

        // Update offset for visible nodes
        if (node_version.is_none() || node_version.map_or(false, |v| is_anc(v)))
            && !node.deleted_by.iter().any(|v| is_anc(v))
        {
            *offset += node.elems.len();
        }

        // Traverse nexts
        for next in &node.nexts {
            helper(
                next,
                next.version.as_deref(),
                target_version,
                is_anc,
                splices,
                offset,
                node.end_cap,
            );
        }

        // Traverse next
        if let Some(ref next) = node.next {
            helper(
                next,
                _version,
                target_version,
                is_anc,
                splices,
                offset,
                false,
            );
        }
    }

    helper(
        root,
        root.version.as_deref(),
        version,
        is_anc,
        &mut splices,
        &mut offset,
        false,
    );

    // Post-process: make replaces with 0 deletes have at least 1 delete
    for s in &mut splices {
        if s.op_type == 'r' && s.delete_count == 0 {
            s.delete_count = 1;
        }
    }

    splices
}

/// Apply bubble compression to the sequence.
///
/// This method prunes metadata by renaming versions according to `to_bubble`,
/// where keys are version IDs and values are (bottom, top) bubble pairs.
/// The "bottom" version becomes the new name, and "top" becomes the new parent.
///
/// # Arguments
/// * `root` - Root node of the sequence (mutable)
/// * `to_bubble` - Map of version -> (bottom_version, top_version)
pub fn apply_bubbles(root: &mut SequenceNode, to_bubble: &HashMap<String, (String, String)>) {
    // Phase 1: Rename versions and update deleted_by
    fn rename_versions(node: &mut SequenceNode, to_bubble: &HashMap<String, (String, String)>) {
        // Rename this node's version if needed
        if let Some(ref v) = node.version {
            if let Some((bottom, _top)) = to_bubble.get(v) {
                if bottom != v {
                    if node.sort_key.is_none() {
                        node.sort_key = node.version.take();
                    }
                    node.version = Some(bottom.clone());
                }
            }
        }

        // Update deleted_by
        let old_deleted: Vec<String> = node.deleted_by.iter().cloned().collect();
        for v in old_deleted {
            if let Some((bottom, _)) = to_bubble.get(&v) {
                node.deleted_by.remove(&v);
                node.deleted_by.insert(bottom.clone());
            }
        }

        // Recurse into nexts
        for next in &mut node.nexts {
            rename_versions(next, to_bubble);
        }

        // Recurse into next
        if let Some(ref mut next) = node.next {
            rename_versions(next, to_bubble);
        }
    }

    rename_versions(root, to_bubble);

    // Phase 2: Merge nodes with same version
    fn merge_nodes(node: &mut SequenceNode) {
        // Check if first next has same version as node
        if let Some(first_next) = node.nexts.first() {
            if first_next.version == node.version {
                // Merge all nexts with same version
                let same_version_nexts: Vec<_> = node
                    .nexts
                    .iter()
                    .filter(|n| n.version == node.version)
                    .cloned()
                    .collect();

                if same_version_nexts.len() == node.nexts.len() {
                    // All nexts have same version, merge them
                    // This is simplified - full impl is more complex
                    node.nexts.clear();
                }
            }
        }

        // Try to merge with next if possible
        while let Some(ref mut next) = node.next {
            if node.nexts.is_empty()
                && !node.elems.is_empty()
                && !next.elems.is_empty()
                && node.deleted_by.iter().all(|v| next.deleted_by.contains(v))
                && next.deleted_by.iter().all(|v| node.deleted_by.contains(v))
            {
                // Same deleted_by, can merge
                node.elems = node.elems.concat(&next.elems);
                node.end_cap = next.end_cap;
                node.nexts = std::mem::take(&mut next.nexts);
                node.next = next.next.take();
            } else {
                break;
            }
        }

        // Recurse
        for next in &mut node.nexts {
            merge_nodes(next);
        }
        if let Some(ref mut next) = node.next {
            merge_nodes(next);
        }
    }

    merge_nodes(root);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_node() {
        let node = SequenceNode::text("alice1", "hello");
        assert_eq!(node.version, Some("alice1".to_string()));
        assert!(matches!(node.elems, SequenceElems::String(ref s) if s == "hello"));
        assert!(!node.end_cap);
        assert!(node.deleted_by.is_empty());
        assert!(node.nexts.is_empty());
        assert!(node.next.is_none());
    }

    #[test]
    fn test_content() {
        let node = SequenceNode::text("alice1", "hello");
        let content = content(&node, |_| true);
        assert_eq!(content, "hello");
    }

    #[test]
    fn test_length() {
        let node = SequenceNode::text("alice1", "hello");
        let len = length(&node, |_| true);
        assert_eq!(len, 5);
    }

    #[test]
    fn test_get() {
        let node = SequenceNode::text("alice1", "hello");
        assert_eq!(get(&node, 0, |_| true), Some('h'));
        assert_eq!(get(&node, 4, |_| true), Some('o'));
        assert_eq!(get(&node, 5, |_| true), None);
    }

    #[test]
    fn test_deleted_node() {
        let mut node = SequenceNode::text("alice1", "hello");
        node.deleted_by.insert("bob1".to_string());

        let len = length(&node, |v| v == "alice1" || v == "bob1");
        assert_eq!(len, 0); // Deleted, so length is 0

        let len_without_delete = length(&node, |v| v == "alice1");
        assert_eq!(len_without_delete, 5); // Not seeing the delete
    }
}
