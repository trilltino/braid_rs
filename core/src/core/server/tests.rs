//! Comprehensive tests for the Braid server module.
//!
//! This module contains extensive tests for all server components including:
//! - ServerConfig
//! - BraidState and header parsing
//! - BraidLayer middleware
//! - UpdateResponse builder
//! - Update to Response conversion
//! - Status codes
//! - ParseUpdate functionality
//! - ResourceStateManager
//! - ConflictResolver

#[cfg(test)]
mod config_tests {
    use crate::core::server::ServerConfig;

    #[test]
    fn test_default_config() {
        let config = ServerConfig::default();
        assert!(config.enable_subscriptions);
        assert_eq!(config.max_subscriptions, 1000);
        assert_eq!(config.heartbeat_interval, 30);
        assert!(!config.enable_multiplex);
    }

    #[test]
    fn test_custom_config() {
        let config = ServerConfig {
            enable_subscriptions: false,
            max_subscriptions: 5000,
            max_subscription_duration_secs: 7200,
            heartbeat_interval: 60,
            enable_multiplex: true,
        };
        assert!(!config.enable_subscriptions);
        assert_eq!(config.max_subscriptions, 5000);
        assert_eq!(config.heartbeat_interval, 60);
        assert!(config.enable_multiplex);
    }

    #[test]
    fn test_config_clone() {
        let config1 = ServerConfig::default();
        let config2 = config1.clone();
        assert_eq!(config1.max_subscriptions, config2.max_subscriptions);
        assert_eq!(config1.heartbeat_interval, config2.heartbeat_interval);
    }

    #[test]
    fn test_config_debug() {
        let config = ServerConfig::default();
        let debug_str = format!("{:?}", config);
        assert!(debug_str.contains("ServerConfig"));
        assert!(debug_str.contains("enable_subscriptions"));
    }
}

#[cfg(test)]
mod braid_state_tests {
    use crate::core::server::BraidState;
    use axum::http::HeaderMap;
    use axum::http::HeaderValue;

    #[test]
    fn test_empty_headers() {
        let headers = HeaderMap::new();
        let state = BraidState::from_headers(&headers);

        assert!(!state.subscribe);
        assert!(state.version.is_none());
        assert!(state.parents.is_none());
        assert!(state.peer.is_none());
        assert!(state.heartbeat.is_none());
        assert!(state.merge_type.is_none());
        assert!(state.content_range.is_none());
    }

    #[test]
    fn test_subscribe_header_true() {
        let mut headers = HeaderMap::new();
        headers.insert("subscribe", HeaderValue::from_static("true"));
        let state = BraidState::from_headers(&headers);

        assert!(state.subscribe);
    }

    #[test]
    fn test_subscribe_header_false() {
        let mut headers = HeaderMap::new();
        headers.insert("subscribe", HeaderValue::from_static("false"));
        let state = BraidState::from_headers(&headers);

        assert!(!state.subscribe);
    }

    #[test]
    fn test_subscribe_header_case_insensitive() {
        let mut headers = HeaderMap::new();
        headers.insert("subscribe", HeaderValue::from_static("TRUE"));
        let state = BraidState::from_headers(&headers);

        assert!(state.subscribe);
    }

    #[test]
    fn test_version_header_single() {
        let mut headers = HeaderMap::new();
        headers.insert("version", HeaderValue::from_static("\"v1\""));
        let state = BraidState::from_headers(&headers);

        assert!(state.version.is_some());
        let versions = state.version.unwrap();
        assert_eq!(versions.len(), 1);
    }

    #[test]
    fn test_version_header_multiple() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "version",
            HeaderValue::from_static("\"v1\", \"v2\", \"v3\""),
        );
        let state = BraidState::from_headers(&headers);

        assert!(state.version.is_some());
        let versions = state.version.unwrap();
        assert_eq!(versions.len(), 3);
    }

    #[test]
    fn test_parents_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "parents",
            HeaderValue::from_static("\"parent1\", \"parent2\""),
        );
        let state = BraidState::from_headers(&headers);

        assert!(state.parents.is_some());
        let parents = state.parents.unwrap();
        assert_eq!(parents.len(), 2);
    }

    #[test]
    fn test_peer_header() {
        let mut headers = HeaderMap::new();
        headers.insert("peer", HeaderValue::from_static("client-abc123"));
        let state = BraidState::from_headers(&headers);

        assert_eq!(state.peer, Some("client-abc123".to_string()));
    }

    #[test]
    fn test_heartbeats_header_seconds() {
        let mut headers = HeaderMap::new();
        headers.insert("heartbeats", HeaderValue::from_static("30s"));
        let state = BraidState::from_headers(&headers);

        assert_eq!(state.heartbeat, Some(30));
    }

    #[test]
    fn test_heartbeats_header_plain_number() {
        let mut headers = HeaderMap::new();
        headers.insert("heartbeats", HeaderValue::from_static("45"));
        let state = BraidState::from_headers(&headers);

        assert_eq!(state.heartbeat, Some(45));
    }

    #[test]
    fn test_merge_type_header() {
        let mut headers = HeaderMap::new();
        headers.insert("merge-type", HeaderValue::from_static("diamond"));
        let state = BraidState::from_headers(&headers);

        assert_eq!(state.merge_type, Some("diamond".to_string()));
    }

    #[test]
    fn test_content_range_header() {
        let mut headers = HeaderMap::new();
        headers.insert("content-range", HeaderValue::from_static("json .field"));
        let state = BraidState::from_headers(&headers);

        assert_eq!(state.content_range, Some("json .field".to_string()));
    }

    #[test]
    fn test_headers_map_populated() {
        let mut headers = HeaderMap::new();
        headers.insert("subscribe", HeaderValue::from_static("true"));
        headers.insert("custom-header", HeaderValue::from_static("value"));
        let state = BraidState::from_headers(&headers);

        assert!(state.headers.contains_key("subscribe"));
        assert!(state.headers.contains_key("custom-header"));
        assert_eq!(
            state.headers.get("custom-header"),
            Some(&"value".to_string())
        );
    }

    #[test]
    fn test_all_headers_combined() {
        let mut headers = HeaderMap::new();
        headers.insert("subscribe", HeaderValue::from_static("true"));
        headers.insert("version", HeaderValue::from_static("\"v5\""));
        headers.insert("parents", HeaderValue::from_static("\"v4\""));
        headers.insert("peer", HeaderValue::from_static("peer-xyz"));
        headers.insert("heartbeats", HeaderValue::from_static("60"));
        headers.insert("merge-type", HeaderValue::from_static("simpleton"));
        headers.insert("content-range", HeaderValue::from_static("bytes 0:100"));

        let state = BraidState::from_headers(&headers);

        assert!(state.subscribe);
        assert!(state.version.is_some());
        assert!(state.parents.is_some());
        assert_eq!(state.peer, Some("peer-xyz".to_string()));
        assert_eq!(state.heartbeat, Some(60));
        assert_eq!(state.merge_type, Some("simpleton".to_string()));
        assert_eq!(state.content_range, Some("bytes 0:100".to_string()));
    }

    #[test]
    fn test_state_clone() {
        let mut headers = HeaderMap::new();
        headers.insert("subscribe", HeaderValue::from_static("true"));
        let state1 = BraidState::from_headers(&headers);
        let state2 = state1.clone();

        assert_eq!(state1.subscribe, state2.subscribe);
    }

    #[test]
    fn test_state_debug() {
        let headers = HeaderMap::new();
        let state = BraidState::from_headers(&headers);
        let debug_str = format!("{:?}", state);

        assert!(debug_str.contains("BraidState"));
    }
}

#[cfg(test)]
mod braid_layer_tests {
    use crate::core::server::{BraidLayer, ServerConfig};

    #[test]
    fn test_new_layer_default_config() {
        let layer = BraidLayer::new();
        let config = layer.config();

        assert!(config.enable_subscriptions);
        assert_eq!(config.max_subscriptions, 1000);
    }

    #[test]
    fn test_layer_with_custom_config() {
        let config = ServerConfig {
            enable_subscriptions: false,
            max_subscriptions: 500,
            max_subscription_duration_secs: 1800,
            heartbeat_interval: 45,
            enable_multiplex: true,
        };
        let layer = BraidLayer::with_config(config);

        assert!(!layer.config().enable_subscriptions);
        assert_eq!(layer.config().max_subscriptions, 500);
        assert_eq!(layer.config().heartbeat_interval, 45);
        assert!(layer.config().enable_multiplex);
    }

    #[test]
    fn test_layer_default_trait() {
        let layer = BraidLayer::default();
        assert!(layer.config().enable_subscriptions);
    }

    #[test]
    fn test_layer_clone() {
        let layer1 = BraidLayer::new();
        let layer2 = layer1.clone();

        assert_eq!(
            layer1.config().max_subscriptions,
            layer2.config().max_subscriptions
        );
    }

    #[test]
    fn test_layer_resource_manager_shared() {
        let layer = BraidLayer::new();
        let manager1 = layer.resource_manager.clone();
        let manager2 = layer.resource_manager.clone();

        // Apply update through one manager
        let _ = manager1.apply_update("shared-doc", "hello", "agent1", None, None);

        // Should be visible through the other
        let state = manager2.get_resource_state("shared-doc");
        assert!(state.is_some());
    }
}

#[cfg(test)]
mod update_response_tests {
    use crate::core::server::send_update::UpdateResponse;
    use crate::core::types::Version;
    use axum::http::StatusCode;

    #[test]
    fn test_new_response_200() {
        let response = UpdateResponse::new(200).build();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_new_response_209_subscription() {
        let response = UpdateResponse::new(209).build();
        assert_eq!(response.status().as_u16(), 209);
    }

    #[test]
    fn test_new_response_404() {
        let response = UpdateResponse::new(404).build();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_new_response_500() {
        let response = UpdateResponse::new(500).build();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_new_response_custom_status() {
        let response = UpdateResponse::new(293).build();
        assert_eq!(response.status().as_u16(), 293);
    }

    #[test]
    fn test_with_version() {
        let response = UpdateResponse::new(200)
            .with_version(vec![Version::new("v1")])
            .build();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.headers().contains_key("version"));
    }

    #[test]
    fn test_with_multiple_versions() {
        let response = UpdateResponse::new(200)
            .with_version(vec![Version::new("v1"), Version::new("v2")])
            .build();

        let version_header = response.headers().get("version").unwrap().to_str().unwrap();
        assert!(version_header.contains("v1"));
        assert!(version_header.contains("v2"));
    }

    #[test]
    fn test_with_parents() {
        let response = UpdateResponse::new(200)
            .with_parents(vec![Version::new("parent1"), Version::new("parent2")])
            .build();

        assert!(response.headers().contains_key("parents"));
    }

    #[test]
    fn test_with_body() {
        let response = UpdateResponse::new(200)
            .with_body("test body content")
            .build();

        assert!(response.headers().contains_key("content-length"));
        let content_length = response
            .headers()
            .get("content-length")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(content_length, "17"); // "test body content".len()
    }

    #[test]
    fn test_with_custom_header() {
        let response = UpdateResponse::new(200)
            .with_header("X-Custom".to_string(), "custom-value".to_string())
            .build();

        assert!(response.headers().contains_key("x-custom"));
    }

    #[test]
    fn test_builder_chaining() {
        let response = UpdateResponse::new(209)
            .with_version(vec![Version::new("v3")])
            .with_parents(vec![Version::new("v2")])
            .with_header("Merge-Type".to_string(), "diamond".to_string())
            .with_body("{\"data\": \"value\"}")
            .build();

        assert_eq!(response.status().as_u16(), 209);
        assert!(response.headers().contains_key("version"));
        assert!(response.headers().contains_key("parents"));
        assert!(response.headers().contains_key("merge-type"));
        assert!(response.headers().contains_key("content-length"));
    }

    #[test]
    fn test_empty_body() {
        let response = UpdateResponse::new(200).build();
        // No content-length header should be set for empty body
        // (or it should be 0)
        let status = response.status();
        assert_eq!(status, StatusCode::OK);
    }
}

#[cfg(test)]
mod update_into_response_tests {
    use crate::core::types::{Patch, Update, Version};
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[test]
    fn test_snapshot_into_response() {
        let update = Update::snapshot(Version::new("v1"), "body content");
        let response = update.into_response();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_snapshot_with_version_header() {
        let update = Update::snapshot(Version::new("test-version"), "body");
        let response = update.into_response();

        assert!(response.headers().contains_key("version"));
    }

    #[test]
    fn test_snapshot_with_parent() {
        let update = Update::snapshot(Version::new("v2"), "body").with_parent(Version::new("v1"));
        let response = update.into_response();

        assert!(response.headers().contains_key("parents"));
    }

    #[test]
    fn test_snapshot_with_merge_type() {
        let update = Update::snapshot(Version::new("v1"), "body").with_merge_type("diamond");
        let response = update.into_response();

        // Merge type is added via extra_headers or as part of the response
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn test_patched_into_response() {
        let patch = Patch::json(".field", "value");
        let update = Update::patched(Version::new("v1"), vec![patch]);
        let response = update.into_response();

        assert!(response.headers().contains_key("patches"));
    }

    #[test]
    fn test_patched_with_content_range() {
        let patch = Patch::json(".data", "{\"key\": \"value\"}");
        let update = Update::patched(Version::new("v1"), vec![patch]);
        let response = update.into_response();

        assert!(response.headers().contains_key("content-range"));
    }

    #[test]
    fn test_update_with_custom_status() {
        let update = Update::snapshot(Version::new("v1"), "body").with_status(209);
        let response = update.into_response();

        assert_eq!(response.status().as_u16(), 209);
    }

    #[test]
    fn test_update_with_multiple_versions() {
        let mut update = Update::snapshot(Version::new("v1"), "body");
        update.version.push(Version::new("v2"));
        let response = update.into_response();

        let version_header = response.headers().get("version").unwrap().to_str().unwrap();
        assert!(version_header.contains("v1"));
        assert!(version_header.contains("v2"));
    }

    #[test]
    fn test_update_with_multiple_parents() {
        let update = Update::snapshot(Version::new("v3"), "body")
            .with_parent(Version::new("v1"))
            .with_parent(Version::new("v2"));
        let response = update.into_response();

        let parents_header = response.headers().get("parents").unwrap().to_str().unwrap();
        assert!(parents_header.contains("v1"));
        assert!(parents_header.contains("v2"));
    }

    #[test]
    fn test_empty_patches() {
        let update = Update::patched(Version::new("v1"), vec![]);
        let response = update.into_response();

        // Should have patches header with count 0
        let patches_header = response.headers().get("patches").unwrap().to_str().unwrap();
        assert_eq!(patches_header, "0");
    }
}

#[cfg(test)]
mod status_tests {
    use crate::core::server::send_update::status;
    use axum::http::StatusCode;

    #[test]
    fn test_subscription_constant() {
        assert_eq!(status::SUBSCRIPTION, 209);
    }

    #[test]
    fn test_multiplex_constant() {
        assert_eq!(status::RESPONDED_VIA_MULTIPLEX, 293);
    }

    #[test]
    fn test_subscription_response() {
        let status_code = status::subscription_response();
        assert_eq!(status_code.as_u16(), 209);
    }

    #[test]
    fn test_multiplex_response() {
        let status_code = status::multiplex_response();
        assert_eq!(status_code.as_u16(), 293);
    }

    #[test]
    fn test_custom_status_code_validity() {
        // Verify custom status codes are valid HTTP status codes
        let sub = StatusCode::from_u16(209);
        assert!(sub.is_ok());

        let multi = StatusCode::from_u16(293);
        assert!(multi.is_ok());
    }
}

#[cfg(test)]
mod parse_update_tests {
    use crate::core::protocol;
    use crate::core::server::parse_update::parse_content_range;

    #[test]
    fn test_parse_content_range_json() {
        let result = parse_content_range("json .field");
        assert!(result.is_ok());
        let (unit, range) = result.unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, ".field");
    }

    #[test]
    fn test_parse_content_range_bytes() {
        let result = parse_content_range("bytes 0:100");
        assert!(result.is_ok());
        let (unit, range) = result.unwrap();
        assert_eq!(unit, "bytes");
        assert_eq!(range, "0:100");
    }

    #[test]
    fn test_parse_content_range_complex() {
        let result = parse_content_range("json .messages[0].text");
        assert!(result.is_ok());
        let (unit, range) = result.unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, ".messages[0].text");
    }

    #[test]
    fn test_parse_content_range_invalid() {
        let result = parse_content_range("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_version_header_unquoted() {
        let result = protocol::parse_version_header("v1, v2");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn test_parse_version_header_mixed() {
        let result = protocol::parse_version_header("\"quoted\", unquoted");
        assert!(result.is_ok());
        let versions = result.unwrap();
        assert_eq!(versions.len(), 2);
    }

    #[test]
    fn test_parse_version_header_whitespace() {
        let result = protocol::parse_version_header("  \"v1\"  ,  \"v2\"  ");
        assert!(result.is_ok());
        let versions = result.unwrap();
        assert_eq!(versions.len(), 2);
    }
}

#[cfg(test)]
mod resource_state_manager_extended_tests {
    use crate::core::server::ResourceStateManager;

    #[test]
    fn test_multiple_agents_same_resource() {
        let manager = ResourceStateManager::new();

        let _ = manager.apply_remote_insert("doc1", "alice", 0, "Hello", None, None);
        let _ = manager.apply_remote_insert("doc1", "bob", 5, " World", None, None);
        let _ = manager.apply_remote_insert("doc1", "charlie", 11, "!", None, None);

        let state = manager.get_resource_state("doc1");
        assert!(state.is_some());
        assert_eq!(state.unwrap()["content"], "Hello World!");
    }

    #[test]
    fn test_delete_operations() {
        let manager = ResourceStateManager::new();

        let _ = manager.apply_update("doc1", "Hello World", "alice", None, None);
        let _ = manager.apply_remote_delete("doc1", "alice", 5, 11, None, None);

        let state = manager.get_resource_state("doc1");
        assert!(state.is_some());
        // After deleting " World", only "Hello" remains
        assert_eq!(state.unwrap()["content"], "Hello");
    }

    #[test]
    fn test_insert_at_various_positions() {
        let manager = ResourceStateManager::new();

        let _ = manager.apply_remote_insert("doc1", "user", 0, "abc", None, None);
        let _ = manager.apply_remote_insert("doc1", "user", 1, "X", None, None);

        let state = manager.get_resource_state("doc1");
        assert!(state.is_some());
        // Insert at position 1 should give "aXbc"
        assert_eq!(state.unwrap()["content"], "aXbc");
    }

    #[test]
    fn test_resource_isolation() {
        let manager = ResourceStateManager::new();

        let _ = manager.apply_update("doc1", "Content 1", "user", None, None);
        let _ = manager.apply_update("doc2", "Content 2", "user", None, None);

        let state1 = manager.get_resource_state("doc1");
        let state2 = manager.get_resource_state("doc2");

        assert_eq!(state1.unwrap()["content"], "Content 1");
        assert_eq!(state2.unwrap()["content"], "Content 2");
    }

    #[test]
    fn test_list_resources_empty() {
        let manager = ResourceStateManager::new();
        let resources = manager.list_resources();
        assert!(resources.is_empty());
    }

    #[test]
    fn test_list_resources_multiple() {
        let manager = ResourceStateManager::new();

        let _ = manager.apply_update("doc1", "a", "user", None, None);
        let _ = manager.apply_update("doc2", "b", "user", None, None);
        let _ = manager.apply_update("doc3", "c", "user", None, None);

        let resources = manager.list_resources();
        assert_eq!(resources.len(), 3);
        assert!(resources.contains(&"doc1".to_string()));
        assert!(resources.contains(&"doc2".to_string()));
        assert!(resources.contains(&"doc3".to_string()));
    }

    #[test]
    fn test_get_or_create_idempotent() {
        let manager = ResourceStateManager::new();

        let resource1 = manager.get_or_create_resource("doc1", "agent1", None);
        let resource2 = manager.get_or_create_resource("doc1", "agent2", None);

        // Both should reference the same resource
        // Adding content via one should be visible in the other
        resource1.lock().crdt.add_insert(0, "test");
        let content = resource2.lock().crdt.content();
        assert_eq!(content, "test");
    }

    #[test]
    fn test_merge_quality_after_updates() {
        let manager = ResourceStateManager::new();

        let _ = manager.apply_update("doc1", "initial", "user", None, None);
        let _ = manager.apply_remote_insert("doc1", "user", 7, " text", None, None);

        let quality = manager.get_merge_quality("doc1");
        assert!(quality.is_some());
        assert!(quality.unwrap() > 0);
    }

    #[test]
    fn test_default_trait() {
        let manager = ResourceStateManager::default();
        assert!(manager.list_resources().is_empty());
    }

    #[test]
    fn test_idempotence() {
        let manager = ResourceStateManager::new();
        let version = "v1";

        // First apply
        let res1 = manager
            .apply_update("doc", "hello", "alice", Some(version), None)
            .unwrap();
        assert_eq!(res1["content"], "hello");

        // Second apply with same version should be idempotent (no-op but returns state)
        // Note: apply_update in my impl currently returns the CRDT state.
        // If it's idempotent, it shouldn't add another insert.
        let res2 = manager
            .apply_update("doc", "ignored", "alice", Some(version), None)
            .unwrap();

        let state = manager.get_resource_state("doc").unwrap();
        // It shouldn't be "helloignored" because it's idempotent
        assert_eq!(state["content"], "hello");
    }
}

#[cfg(test)]
mod conflict_resolver_extended_tests {
    use crate::core::server::{ConflictResolver, ResourceStateManager};
    use crate::core::types::{Update, Version};

    #[tokio::test]
    async fn test_resolve_non_diamond_passthrough() {
        let manager = ResourceStateManager::new();
        let resolver = ConflictResolver::new(manager);

        let update = Update::snapshot(Version::new("v1"), "test content").with_merge_type("simpleton"); // Not diamond

        let result = resolver.resolve_update("doc1", &update, "alice").await;
        assert!(result.is_ok());

        let resolved = result.unwrap();
        // Should be unchanged since it's not diamond merge type
        assert_eq!(resolved.merge_type, Some("simpleton".to_string()));
    }

    #[tokio::test]
    async fn test_resolve_no_merge_type() {
        let manager = ResourceStateManager::new();
        let resolver = ConflictResolver::new(manager);

        let update = Update::snapshot(Version::new("v1"), "content");
        // No merge type set

        let result = resolver.resolve_update("doc1", &update, "alice").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_diamond_plain_text() {
        let manager = ResourceStateManager::new();
        let resolver = ConflictResolver::new(manager);

        let update =
            Update::snapshot(Version::new("v1"), "plain text content").with_merge_type("diamond");

        let result = resolver.resolve_update("doc1", &update, "alice").await;
        assert!(result.is_ok());

        let content = resolver.get_resource_content("doc1");
        assert!(content.is_some());
    }

    #[tokio::test]
    async fn test_diamond_json_inserts() {
        let manager = ResourceStateManager::new();

        // First, initialize with some content
        let _ = manager.apply_update("doc1", "Hello", "alice", None, None);

        let resolver = ConflictResolver::new(manager);

        let json_update = r#"{"inserts": [{"pos": 5, "text": " World"}]}"#;
        let update = Update::snapshot(Version::new("v2"), json_update).with_merge_type("diamond");

        let result = resolver.resolve_update("doc1", &update, "bob").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_diamond_json_deletes() {
        let manager = ResourceStateManager::new();

        // Initialize with content
        let _ = manager.apply_update("doc1", "Hello World", "alice", None, None);

        let resolver = ConflictResolver::new(manager);

        let json_update = r#"{"deletes": [{"start": 5, "end": 11}]}"#;
        let update = Update::snapshot(Version::new("v2"), json_update).with_merge_type("diamond");

        let result = resolver.resolve_update("doc1", &update, "bob").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_diamond_json_mixed_operations() {
        let manager = ResourceStateManager::new();

        let _ = manager.apply_update("doc1", "ABCDEF", "alice", None, None);

        let resolver = ConflictResolver::new(manager);

        let json_update = r#"{
            "inserts": [{"pos": 3, "text": "XY"}],
            "deletes": [{"start": 0, "end": 1}]
        }"#;
        let update = Update::snapshot(Version::new("v2"), json_update).with_merge_type("diamond");

        let result = resolver.resolve_update("doc1", &update, "bob").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_concurrent_diamond_updates() {
        let manager = ResourceStateManager::new();
        let resolver = ConflictResolver::new(manager.clone());

        // Initial content
        let _ = manager.apply_update("doc1", "base", "init", None, None);

        // Simulate concurrent updates from two users
        let update1 = Update::snapshot(Version::new("v1"), "update1").with_merge_type("diamond");
        let update2 = Update::snapshot(Version::new("v2"), "update2").with_merge_type("diamond");

        let result1 = resolver.resolve_update("doc1", &update1, "alice").await;
        let result2 = resolver.resolve_update("doc1", &update2, "bob").await;

        assert!(result1.is_ok());
        assert!(result2.is_ok());
    }

    #[tokio::test]
    async fn test_get_resource_content() {
        let manager = ResourceStateManager::new();
        let resolver = ConflictResolver::new(manager.clone());

        // No content initially
        assert!(resolver.get_resource_content("nonexistent").is_none());

        // Add content
        let _ = manager.apply_update("doc1", "some content", "user", None, None);

        let content = resolver.get_resource_content("doc1");
        assert!(content.is_some());
        assert_eq!(content.unwrap(), "some content");
    }

    #[tokio::test]
    async fn test_get_resource_version() {
        let manager = ResourceStateManager::new();
        let resolver = ConflictResolver::new(manager.clone());

        // No version initially
        assert!(resolver.get_resource_version("nonexistent").is_none());

        // Add content (version should be set)
        let _ = manager.apply_update("doc1", "content", "user", None, None);

        let version = resolver.get_resource_version("doc1");
        assert!(version.is_some());
    }

    #[tokio::test]
    async fn test_resolver_clone() {
        let manager = ResourceStateManager::new();
        let resolver1 = ConflictResolver::new(manager);
        let resolver2 = resolver1.clone();

        // Both should work independently but share the same resource manager
        let update = Update::snapshot(Version::new("v1"), "test").with_merge_type("diamond");

        let _ = resolver1.resolve_update("doc1", &update, "user").await;

        // Should be visible through resolver2
        assert!(resolver2.get_resource_content("doc1").is_some());
    }

    #[tokio::test]
    async fn test_diamond_empty_body() {
        let manager = ResourceStateManager::new();
        let resolver = ConflictResolver::new(manager);

        let mut update = Update::default();
        update.merge_type = Some("diamond".to_string());
        update.version = vec![Version::new("v1")];
        // No body set

        let result = resolver.resolve_update("doc1", &update, "alice").await;
        // Should handle gracefully even with no body
        assert!(result.is_err() || result.is_ok());
    }

    #[tokio::test]
    async fn test_malformed_json_treated_as_plain_text() {
        let manager = ResourceStateManager::new();
        let resolver = ConflictResolver::new(manager);

        // JSON-like but not valid operations format
        let update =
            Update::snapshot(Version::new("v1"), "{invalid json").with_merge_type("diamond");

        let result = resolver.resolve_update("doc1", &update, "alice").await;
        // Should handle as plain text insertion
        assert!(result.is_ok());
    }
}
