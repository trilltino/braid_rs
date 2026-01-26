use crate::antimatter::messages::Patch;

/// Trait for the underlying CRDT that Antimatter manages.
/// Antimatter handles the pruning and networking/metadata, while this trait
/// handles the actual data application and storage.
pub trait PrunableCrdt {
    /// Apply a patch to the CRDT.
    /// In the JS version, this is hidden inside `json_crdt`, but here we verify
    /// and apply the operation.
    fn apply_patch(&mut self, patch: Patch);

    /// Prune metadata associated with a version.
    /// This is the core "antimatter" operation.
    fn prune(&mut self, version: &str);

    /// Get the current sequence number or internal state identifier (optional).
    fn get_next_seq(&self) -> u64;

    /// Generate a braid (list of updates) to sync a peer that knows `known_versions`.
    ///
    /// # Arguments
    /// * `known_versions` - A map of version ID -> true for versions the peer already has.
    ///
    /// # Returns
    /// An array of updates (Version, Parents, Patches) needed to bring the peer up to date.
    /// Each tuple is (Version, Parents, Patches).
    fn generate_braid(
        &self,
        known_versions: &std::collections::HashMap<String, bool>,
    ) -> Vec<(String, std::collections::HashMap<String, bool>, Vec<Patch>)>;
}

/// A simple mock implementation for testing/compilation
pub struct MockCrdt;
impl PrunableCrdt for MockCrdt {
    fn apply_patch(&mut self, _patch: Patch) {}
    fn prune(&mut self, _version: &str) {}
    fn get_next_seq(&self) -> u64 {
        0
    }
    fn generate_braid(
        &self,
        _known_versions: &std::collections::HashMap<String, bool>,
    ) -> Vec<(String, std::collections::HashMap<String, bool>, Vec<Patch>)> {
        Vec::new()
    }
}
