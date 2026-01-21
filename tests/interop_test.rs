use braid_rs::{BraidClient, BraidRequest, Version};
use std::process::Child;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;

struct JsServer {
    child: Child,
}

impl JsServer {
    fn start() -> Self {
        let child = Command::new("node")
            .arg("tests/interop/server.js")
            .spawn()
            .expect("Failed to start JS interop server");
        JsServer { child }
    }
}

impl Drop for JsServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

#[tokio::test]
async fn test_rust_client_vs_js_server() {
    // Start JS server
    let _server = JsServer::start();

    // Wait for server to be ready
    sleep(Duration::from_secs(2)).await;

    let client = BraidClient::new();
    let url = "http://localhost:3009/test";

    // 1. Test standard GET
    println!("Testing standard GET...");
    let response = client.get(url).await.expect("Failed simple GET");
    assert_eq!(response.status, 200);
    assert!(response.headers.contains_key("version"));
    println!("Standard GET passed.");

    // 2. Test Subscription
    println!("Testing Subscription...");
    let request = BraidRequest::new().subscribe();
    let mut stream = client
        .subscribe(url, request)
        .await
        .expect("Failed to subscribe");

    // Receive first update (v0)
    let update1 = stream
        .next()
        .await
        .expect("Stream ended early")
        .expect("Failed to get update");
    assert!(update1.version.contains(&Version::new("v0")));
    println!("Received v0: {:?}", update1.version);

    // Receive second update (v1)
    let update2 = stream
        .next()
        .await
        .expect("Stream ended early")
        .expect("Failed to get update");
    assert!(update2.version.contains(&Version::new("v1")));
    println!("Received v1: {:?}", update2.version);

    println!("Interop test passed!");
}
