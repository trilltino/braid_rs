//! Per-resource state management.

use crate::core::merge::DiamondCRDT;
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::SystemTime;

/// The state of a single collaborative resource.
#[derive(Debug, Clone)]
pub struct ResourceState {
    pub crdt: DiamondCRDT,
    pub last_sync: SystemTime,
    pub seen_versions: HashSet<String>,
    pub merge_type: String,
}

/// Thread-safe registry of collaborative document resources.
pub struct ResourceStateManager {
    resources: Arc<Mutex<HashMap<String, Arc<Mutex<ResourceState>>>>>,
}

impl ResourceStateManager {
    #[must_use]
    pub fn new() -> Self {
        Self {
            resources: Arc::new(Mutex::new(HashMap::new())),
        }
    }

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
                    seen_versions: HashSet::new(),
                    merge_type,
                }))
            })
            .clone()
    }

    #[inline]
    #[must_use]
    pub fn get_resource(&self, resource_id: &str) -> Option<Arc<Mutex<ResourceState>>> {
        self.resources.lock().get(resource_id).cloned()
    }

    #[must_use]
    pub fn list_resources(&self) -> Vec<String> {
        self.resources.lock().keys().cloned().collect()
    }

    #[must_use]
    pub fn has_version(&self, resource_id: &str, version_id: &str) -> bool {
        self.get_resource(resource_id)
            .is_some_and(|r| r.lock().seen_versions.contains(version_id))
    }

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
                    "Merge-type mismatch: {} vs {}",
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
        if let Some(vid) = version_id {
            let frontier = state.crdt.get_local_frontier();
            state
                .crdt
                .register_version_mapping(vid.to_string(), frontier);
        }
        state.last_sync = SystemTime::now();
        Ok(state.crdt.export_operations())
    }

    pub fn apply_remote_insert_versioned(
        &self,
        resource_id: &str,
        agent_id: &str,
        parents: &[&str],
        pos: usize,
        text: &str,
        version_id: Option<&str>,
        requested_merge_type: Option<&str>,
    ) -> Result<Value, String> {
        let resource = self.get_or_create_resource(resource_id, agent_id, requested_merge_type);
        let mut state = resource.lock();
        if let Some(vid) = version_id {
            if state.seen_versions.contains(vid) {
                return Ok(state.crdt.export_operations());
            }
            state.seen_versions.insert(vid.to_string());
        }
        state
            .crdt
            .add_insert_remote_versioned(agent_id, parents, pos, text, version_id);
        state.last_sync = SystemTime::now();
        Ok(state.crdt.export_operations())
    }

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
                    "Merge-type mismatch: {} vs {}",
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

    pub fn apply_remote_delete_versioned(
        &self,
        resource_id: &str,
        agent_id: &str,
        parents: &[&str],
        range: std::ops::Range<usize>,
        version_id: Option<&str>,
        requested_merge_type: Option<&str>,
    ) -> Result<Value, String> {
        let resource = self.get_or_create_resource(resource_id, agent_id, requested_merge_type);
        let mut state = resource.lock();
        if let Some(vid) = version_id {
            if state.seen_versions.contains(vid) {
                return Ok(state.crdt.export_operations());
            }
            state.seen_versions.insert(vid.to_string());
        }
        state
            .crdt
            .add_delete_remote_versioned(agent_id, parents, range, version_id);
        state.last_sync = SystemTime::now();
        Ok(state.crdt.export_operations())
    }

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
                    "Merge-type mismatch: {} vs {}",
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

    #[inline]
    #[must_use]
    pub fn get_resource_state(&self, resource_id: &str) -> Option<Value> {
        self.get_resource(resource_id)
            .map(|r| r.lock().crdt.checkpoint())
    }

    #[inline]
    #[must_use]
    pub fn get_merge_quality(&self, resource_id: &str) -> Option<u32> {
        self.get_resource(resource_id)
            .map(|r| r.lock().crdt.merge_quality())
    }

    pub fn register_version_mapping(
        &self,
        resource_id: &str,
        version: String,
        frontier: crate::vendor::diamond_types::Frontier,
    ) {
        if let Some(resource) = self.get_resource(resource_id) {
            resource
                .lock()
                .crdt
                .register_version_mapping(version, frontier);
        }
    }

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
            frontiers.push(
                state
                    .crdt
                    .resolve_version(v)
                    .ok_or_else(|| format!("Version not found/pruned: {}", v))?
                    .clone(),
            );
        }
        Ok(state.crdt.get_ops_since(&frontiers))
    }
}

impl Clone for ResourceStateManager {
    fn clone(&self) -> Self {
        Self {
            resources: Arc::clone(&self.resources),
        }
    }
}

impl Default for ResourceStateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_resource_manager() {
        let manager = ResourceStateManager::new();
        manager
            .apply_update("doc1", "hello", "alice", None, None)
            .unwrap();
        assert_eq!(
            manager.get_resource_state("doc1").unwrap()["content"],
            "hello"
        );
    }
}
