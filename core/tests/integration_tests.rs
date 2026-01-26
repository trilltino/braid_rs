use braid_rs::{BraidLayer, BraidState, Update, Version};
use std::sync::Arc;

#[test]
fn test_braid_layer_creation() {
    let layer = BraidLayer::new();
    assert!(Arc::strong_count(&layer.resource_manager) > 0);
}

#[test]
fn test_resource_state_creation() {
    let layer = BraidLayer::new();
    let resource = layer
        .resource_manager
        .get_or_create_resource("doc1", "alice", None);

    let state = resource.lock();
    assert!(state.crdt.is_empty());
    assert_eq!(state.crdt.agent_id(), "alice");
}

#[test]
fn test_diamond_merge_basic() {
    let layer = BraidLayer::new();

    let _ = layer
        .resource_manager
        .apply_update("doc1", "hello", "alice", None, None);

    let state = layer.resource_manager.get_resource_state("doc1");
    assert!(state.is_some());

    let state_json = state.unwrap();
    assert_eq!(state_json["content"], "hello");
}

#[test]
fn test_concurrent_edits_merge() {
    let layer = BraidLayer::new();

    let _ = layer
        .resource_manager
        .apply_remote_insert("doc1", "alice", 0, "hello", None, None);
    let _ = layer
        .resource_manager
        .apply_remote_insert("doc1", "bob", 5, " world", None, None);

    let state = layer.resource_manager.get_resource_state("doc1");
    assert!(state.is_some());
}

#[test]
fn test_delete_operations() {
    let layer = BraidLayer::new();

    let _ = layer
        .resource_manager
        .apply_update("doc1", "hello world", "alice", None, None);
    let _ = layer
        .resource_manager
        .apply_remote_delete("doc1", "bob", 5, 6, None, None);

    let state = layer.resource_manager.get_resource_state("doc1");
    assert!(state.is_some());
}

#[test]
fn test_merge_quality_tracking() {
    let layer = BraidLayer::new();

    let _ = layer
        .resource_manager
        .apply_update("doc1", "text", "alice", None, None);
    let quality = layer.resource_manager.get_merge_quality("doc1");

    assert_eq!(quality, Some(100));
}

#[test]
fn test_multiple_resources() {
    let layer = BraidLayer::new();

    let _ = layer
        .resource_manager
        .apply_update("doc1", "text1", "alice", None, None);
    let _ = layer
        .resource_manager
        .apply_update("doc2", "text2", "bob", None, None);

    let resources = layer.resource_manager.list_resources();
    assert_eq!(resources.len(), 2);
    assert!(resources.contains(&"doc1".to_string()));
    assert!(resources.contains(&"doc2".to_string()));
}

#[test]
fn test_braid_state_extraction() {
    use axum::http::HeaderMap;

    let mut headers = HeaderMap::new();
    headers.insert("version", "v1".parse().unwrap());
    headers.insert("subscribe", "true".parse().unwrap());

    let state = BraidState::from_headers(&headers);
    assert!(state.subscribe);
    assert!(state.version.is_some());
}

#[test]
fn test_update_snapshot_creation() {
    let update = Update::snapshot(Version::new("v1"), "test content").with_merge_type("diamond");

    assert_eq!(update.version.len(), 1);
    assert_eq!(update.merge_type, Some("diamond".to_string()));
    assert!(update.body.is_some());
}

#[test]
fn test_update_with_parent() {
    let update =
        Update::snapshot(Version::new("v2"), "new content").with_parent(Version::new("v1"));

    assert_eq!(update.parents.len(), 1);
}

#[tokio::test]
async fn test_async_conflict_resolution() {
    use braid_rs::server::ConflictResolver;

    let layer = BraidLayer::new();
    let resolver = ConflictResolver::new((*layer.resource_manager).clone());

    let update = Update::snapshot(Version::new("v1"), "content").with_merge_type("diamond");

    let result = resolver.resolve_update("doc1", &update, "alice").await;
    assert!(result.is_ok());

    let resolved = result.unwrap();
    assert_eq!(resolved.merge_type, Some("diamond".to_string()));
}

#[test]
fn test_crdt_concurrent_convergence() {
    let layer = BraidLayer::new();

    let _ = layer
        .resource_manager
        .apply_remote_insert("doc", "alice", 0, "a", None, None);
    let _ = layer
        .resource_manager
        .apply_remote_insert("doc", "bob", 1, "b", None, None);
    let _ = layer
        .resource_manager
        .apply_remote_insert("doc", "alice", 2, "c", None, None);
    let _ = layer
        .resource_manager
        .apply_remote_insert("doc", "bob", 0, "x", None, None);

    let state = layer.resource_manager.get_resource_state("doc");
    assert!(state.is_some());
}

#[test]
fn test_resource_state_after_operations() {
    let layer = BraidLayer::new();

    let _ = layer
        .resource_manager
        .apply_update("doc1", "hello world", "alice", None, None);

    let state = layer.resource_manager.get_resource_state("doc1");
    assert!(state.is_some());

    let json = state.unwrap();
    assert!(json.get("content").is_some());
    assert!(json.get("version").is_some());
    assert!(json.get("agent_id").is_some());
}

#[test]
fn test_diamond_crdt_wrapper() {
    use braid_rs::merge::DiamondCRDT;

    let mut crdt = DiamondCRDT::new("alice");
    assert!(crdt.is_empty());

    crdt.add_insert(0, "hello");
    assert!(!crdt.is_empty());
    assert_eq!(crdt.content(), "hello");

    let version = crdt.get_version();
    assert!(version.contains("alice"));

    let checkpoint = crdt.checkpoint();
    assert_eq!(checkpoint["content"], "hello");
}

#[test]
fn test_diamond_crdt_concurrent() {
    use braid_rs::merge::DiamondCRDT;

    let mut crdt = DiamondCRDT::new("alice");
    crdt.add_insert(0, "a");
    crdt.add_insert_remote("bob", 1, "b");

    assert_eq!(crdt.content(), "ab");
}
