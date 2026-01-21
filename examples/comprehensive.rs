#![allow(unused_variables)]
use braid_rs::{types::ContentRange, BraidClient, BraidRequest, Patch, Update, Version};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Braid-HTTP Comprehensive Example");
    println!("================================\n");

    let client = BraidClient::new();

    println!("1. VERSIONING FOR RESOURCES");
    println!("--------------------------");
    println!("   Basic version tracking:");
    let req = BraidRequest::new().with_version(Version::from("v1"));
    println!("   GET /resource with Version: v1\n");

    println!("2. GETTING HISTORICAL VERSIONS");
    println!("-----------------------------");
    println!("   Request range of versions:");
    let req = BraidRequest::new()
        .with_version(Version::from("v3"))
        .with_parents(vec![Version::from("v1a"), Version::from("v1b")]);
    println!("   GET /resource with Version: v3, Parents: v1a, v1b");
    println!("   Returns updates from v1a/v1b to v3\n");

    println!("3. UPDATES AS PATCHES OR SNAPSHOTS");
    println!("---------------------------------");

    println!("   a) Snapshot update:");
    let update = Update::snapshot(Version::from("v2"), r#"[{"text": "Hello"}]"#);
    println!("      Full JSON snapshot\n");

    println!("   b) Patch update:");
    let patch = Patch::json(".messages[1:1]", r#"[{"text": "Added"}]"#);
    let update = Update::patched(Version::from("v3"), vec![patch]);
    println!("      Incremental JSON patch\n");

    println!("   c) Range patch:");
    let content_range = ContentRange::new("json", ".messages[1:1]");
    let patch = Patch::json(".messages[1:1]", r#"[{"text": "Hello"}]"#);
    let mut update = Update::patched(Version::from("v4"), vec![patch]);
    update.content_range = Some(content_range);
    println!("      Content-Range: json .messages[1:1]\n");

    println!("4. MERGE-TYPES FOR CONFLICT RESOLUTION");
    println!("------------------------------------");
    let req = BraidRequest::new()
        .with_version(Version::from("v2"))
        .with_parents(vec![Version::from("v1a"), Version::from("v1b")])
        .with_merge_type("sync9");
    println!("   PUT /resource with Merge-Type: sync9");
    println!("   Specifies how to merge concurrent edits\n");

    println!("5. SUBSCRIPTIONS");
    println!("---------------");
    let req = BraidRequest::new().subscribe();
    println!("   GET /resource with Subscribe: true");
    println!("   Response: HTTP 209 Subscription");
    println!("   Server streams updates as they occur\n");

    println!("6. SUBSCRIPTION WITH CATCH-UP SIGNALING");
    println!("-------------------------------------");
    let req = BraidRequest::new().subscribe();
    println!("   Response includes Current-Version header");
    println!("   Client knows when it has caught up to server state\n");

    println!("7. RESUMING SUBSCRIPTIONS");
    println!("------------------------");
    let req = BraidRequest::new()
        .subscribe()
        .with_parent(Version::from("v2"));
    println!("   GET /resource with Subscribe: true, Parents: v2");
    println!("   Resumes subscription from last known version\n");

    println!("8. MULTIPLE PATCHES IN ONE UPDATE");
    println!("--------------------------------");
    let patches = vec![
        Patch::json(".messages[1:1]", r#"[{"text": "Hi"}]"#),
        Patch::json(".timestamp", r#"1234567890"#),
    ];
    let mut update = Update::patched(Version::from("v5"), patches);
    update
        .patches
        .as_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
        .for_each(|(i, p)| {
            p.content_length = Some(p.content.len());
        });
    println!("   Patches: 2");
    println!("   Each patch includes Content-Length header\n");

    println!("9. CONTENT-TYPE AND MERGE-TYPE HANDLING");
    println!("------------------------------------");
    let update = Update::snapshot(Version::from("v6"), r#"{"data": "value"}"#)
        .with_content_type("application/json")
        .with_merge_type("sync9");
    println!("   Content-Type: application/json");
    println!("   Merge-Type: sync9\n");

    println!("10. DAG VERSION TRACKING");
    println!("----------------------");
    println!("    Multiple parents represent concurrent edits:");
    let req = BraidRequest::new()
        .with_version(Version::from("v_merged"))
        .with_parents(vec![
            Version::from("v_client_edit"),
            Version::from("v_server_edit"),
        ]);
    println!("    Version: v_merged");
    println!("    Parents: v_client_edit, v_server_edit");
    println!("    Represents merge of concurrent changes\n");

    println!("11. ERROR HANDLING - DROPPED HISTORY");
    println!("----------------------------------");
    println!("    If server has dropped history: HTTP 410 GONE");
    println!("    Client receives BraidError::HistoryDropped");
    println!("    Must restart synchronization\n");

    println!("Example Features Summary:");
    println!("✓ Version DAG tracking");
    println!("✓ Historical version retrieval");
    println!("✓ Snapshot and patch updates");
    println!("✓ Range patches with Content-Range");
    println!("✓ Merge-Type specification");
    println!("✓ Subscriptions with 209 status");
    println!("✓ Current-Version catch-up signaling");
    println!("✓ Subscription resumption");
    println!("✓ Multiple patches per update");
    println!("✓ Proper error handling for 410 GONE");

    Ok(())
}
