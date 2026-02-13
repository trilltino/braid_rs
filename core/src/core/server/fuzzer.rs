use crate::core::server::{ConflictResolver, ResourceStateManager};
use crate::core::types::{Update, Version};
use rand::prelude::*;
use rand::rngs::SmallRng;
use rand::SeedableRng;
use serde_json::json;

/// Fuzz the state management by applying random concurrent operations.
///
/// This test simulates multiple agents performing random operations (inserts/deletes)
/// on a shared resource to ensure the CRDT and state manager handle multi-author
/// concurrency without panics or inconsistency.
async fn fuzz_state_management_once(seed: u64) {
    let mut rng = SmallRng::seed_from_u64(seed);
    let manager = ResourceStateManager::new();
    let resolver = ConflictResolver::new(manager.clone());
    let resource_id = "fuzz-resource";
    let agents = ["alice", "bob", "charlie"];

    // Initialize resource
    let _ = manager.get_or_create_resource(resource_id, "system", Some("diamond"));

    let mut expected_versions = Vec::new();

    // Apply a sequence of random operations
    for i in 0..100 {
        let agent = agents.choose(&mut rng).unwrap();
        let op_type = rng.random_range(0..2);

        // Get current version to use as parent (sometimes)
        let current_version = resolver.get_resource_version(resource_id);
        let parents = if rng.random_bool(0.7) && current_version.is_some() {
            vec![current_version.unwrap()]
        } else {
            vec![]
        };

        let version_id = format!("v-{}-{}", seed, i);
        let update = match op_type {
            0 => {
                // Insert
                let pos = rng.random_range(0..200); // Intentionally allow "out of bounds" to see how CRDT handles it
                Update {
                    version: vec![Version::new(&version_id)],
                    parents,
                    body: Some(
                        json!({
                            "inserts": [{"pos": pos, "text": format!("x-{}", i)}]
                        })
                        .to_string()
                        .into(),
                    ),
                    merge_type: Some("diamond".to_string()),
                    ..Default::default()
                }
            }
            _ => {
                // Delete
                let start = rng.random_range(0..200);
                Update {
                    version: vec![Version::new(&version_id)],
                    parents,
                    body: Some(
                        json!({
                            "deletes": [{"start": start, "end": start + 1}]
                        })
                        .to_string()
                        .into(),
                    ),
                    merge_type: Some("diamond".to_string()),
                    ..Default::default()
                }
            }
        };

        // Apply via resolver
        match resolver.resolve_update(resource_id, &update, agent).await {
            Ok(_) => {
                expected_versions.push(version_id);
            }
            Err(e) => {
                // Some errors are expected if we generate truly garbage input,
                // but we should check if they are "valid" errors.
                // For now, we just want to ensure it doesn't panic.
                // println!("Caught expected or unexpected error: {}", e);
            }
        }

        // Periodic consistency check
        if i % 10 == 0 {
            if let Some(resource) = manager.get_resource(resource_id) {
                let state = resource.lock();
                state.crdt.dbg_check(true);
            }
        }
    }
}

/// Single-run fuzzer test for state management.
#[tokio::test]
async fn test_state_fuzz_once() {
    fuzz_state_management_once(123).await;
}

/// Test convergence of two independent state managers.
///
/// Generates different sets of updates on two managers and then synchronizes
/// them in both directions to verify that they reach the same final state.
#[tokio::test]
async fn test_state_convergence() {
    let seed = 456;
    let mut _rng = SmallRng::seed_from_u64(seed);

    let manager_a = ResourceStateManager::new();
    let manager_b = ResourceStateManager::new();
    let resolver_a = ConflictResolver::new(manager_a.clone());
    let resolver_b = ConflictResolver::new(manager_b.clone());

    let resource_id = "conv-resource";

    // Initialize both
    let _ = manager_a.get_or_create_resource(resource_id, "sys", Some("diamond"));
    let _ = manager_b.get_or_create_resource(resource_id, "sys", Some("diamond"));

    let mut updates_a = Vec::new();
    let mut updates_b = Vec::new();

    // 1. Generate local updates on A
    let initial_version_a = resolver_a.get_resource_version(resource_id);
    let mut current_parents_a = initial_version_a.map(|v| vec![v]).unwrap_or_default();
    for i in 0..10 {
        let version_id = format!("v-a-{}", i);
        let update = Update {
            version: vec![Version::new(&version_id)],
            parents: current_parents_a.clone(),
            body: Some(
                json!({"inserts": [{"pos": 0, "text": format!("a{}", i)}]})
                    .to_string()
                    .into(),
            ),
            merge_type: Some("diamond".to_string()),
            ..Default::default()
        };
        // We use alice as the agent for these updates
        let _ = resolver_a
            .resolve_update(resource_id, &update, "alice")
            .await
            .unwrap();
        current_parents_a = vec![Version::new(&version_id)];
        updates_a.push(update);
    }

    // 2. Generate local updates on B
    let initial_version_b = resolver_b.get_resource_version(resource_id);
    let mut current_parents_b = initial_version_b.map(|v| vec![v]).unwrap_or_default();
    for i in 0..10 {
        let version_id = format!("v-b-{}", i);
        let update = Update {
            version: vec![Version::new(&version_id)],
            parents: current_parents_b.clone(),
            body: Some(
                json!({"inserts": [{"pos": 0, "text": format!("b{}", i)}]})
                    .to_string()
                    .into(),
            ),
            merge_type: Some("diamond".to_string()),
            ..Default::default()
        };
        // We use bob as the agent for these updates
        let _ = resolver_b
            .resolve_update(resource_id, &update, "bob")
            .await
            .unwrap();
        updates_b.push(update);
        current_parents_b = vec![Version::new(&version_id)];
    }

    // 3. Sync A -> B
    for u in &updates_a {
        resolver_b
            .resolve_update(resource_id, u, "alice")
            .await
            .unwrap();
    }

    // 4. Sync B -> A
    for u in &updates_b {
        resolver_a
            .resolve_update(resource_id, u, "bob")
            .await
            .unwrap();
    }

    // 5. Verify convergence
    let content_a = resolver_a.get_resource_content(resource_id).unwrap();
    let content_b = resolver_b.get_resource_content(resource_id).unwrap();

    println!("Content A: {}", content_a);
    println!("Content B: {}", content_b);

    assert_eq!(content_a, content_b);

    // 6. Consistency checks
    {
        let res_a = manager_a.get_resource(resource_id).unwrap();
        let state_a = res_a.lock();
        state_a.crdt.dbg_check(true);

        let res_b = manager_b.get_resource(resource_id).unwrap();
        let state_b = res_b.lock();
        state_b.crdt.dbg_check(true);
    }
}
