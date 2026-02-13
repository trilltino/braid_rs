#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

//! Braid-Iroh Native Tauri v2 Application
//!
//! A pure native desktop application for P2P synchronization using Iroh and Braid-HTTP.
//! No external JavaScript frameworks - pure Tauri events and vanilla JavaScript.

use braid_http_rs::{Update, Version};
use bytes::Bytes;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::RwLock;
use uuid::Uuid;

mod discovery;
mod node;
mod protocol;
mod proxy;
mod subscription;

use crate::discovery::DiscoveryConfig;
use crate::node::{BraidIrohConfig, BraidIrohNode};

use iroh_h3_client::IrohH3Client;

// =============================================================================
// EVENT TYPES - Native Tauri Events (No SSE, No Fetch)
// =============================================================================

/// Emitted when peers are discovered and connected
#[derive(Clone, Debug, Serialize)]
struct PeersConnectedEvent {
    peer_a_id: String,
    peer_b_id: String,
    topic: String,
}

/// Emitted when sync data is received from the network
#[derive(Clone, Debug, Serialize)]
struct SyncReceivedEvent {
    peer: String,
    content: String,
    version: String,
}

/// Emitted for network activity logging
#[derive(Clone, Debug, Serialize)]
struct NetworkEvent {
    peer: String,
    event_type: String, // "sent" | "received" | "connected" | "error"
    message: String,
    timestamp: u64,
}

// =============================================================================
// APPLICATION STATE
// =============================================================================

/// Shared application state with real P2P nodes
struct AppState {
    peer_a: Arc<BraidIrohNode>,
    peer_b: Arc<BraidIrohNode>,
    peer_a_id: String,
    peer_b_id: String,
    content_a: Arc<RwLock<String>>,
    content_b: Arc<RwLock<String>>,
    version: Arc<RwLock<String>>,
}

/// Payload for updating content
#[derive(Deserialize)]
struct UpdatePayload {
    peer: String,
    content: String,
    resource_name: Option<String>,
}

/// Response from update operation
#[derive(Serialize)]
struct UpdateResponse {
    success: bool,
    peer: String,
    version: String,
}

// =============================================================================
// TAURI COMMANDS - Native Rust Commands (No HTTP, No SSE)
// =============================================================================

/// Command: Get peer information
#[tauri::command]
async fn get_peers(state: State<'_, Arc<AppState>>) -> Result<PeersConnectedEvent, String> {
    Ok(PeersConnectedEvent {
        peer_a_id: state.peer_a_id.clone(),
        peer_b_id: state.peer_b_id.clone(),
        topic: "/demo-doc".to_string(),
    })
}

/// Command: Send an update to the network
#[tauri::command]
async fn send_update(
    payload: UpdatePayload,
    state: State<'_, Arc<AppState>>,
    app: AppHandle,
) -> Result<UpdateResponse, String> {
    let ver = Uuid::new_v4()
        .to_string()
        .chars()
        .take(9)
        .collect::<String>();

    // Update global version state
    let mut version_guard = state.version.write().await;
    *version_guard = ver.clone();
    drop(version_guard);

    let resource_url = payload.resource_name.as_deref().unwrap_or("/demo-doc");
    let update = Update::snapshot(
        Version::String(ver.clone()),
        Bytes::from(payload.content.clone()),
    );

    if payload.peer == "a" {
        // Update local state
        let mut ca = state.content_a.write().await;
        *ca = payload.content.clone();
        drop(ca);

        // Send via Peer A
        let peer_node = state.peer_a.clone();
        let url = resource_url.to_string();
        let _content_for_event = payload.content.clone();

        let ver_clone = ver.clone();
        tokio::spawn(async move {
            match peer_node.put(&url, update).await {
                Ok(_) => {
                    println!("[SENT] Peer A: PUT {} v{}", url, ver_clone);
                    let _ = app.emit(
                        "network-event",
                        NetworkEvent {
                            peer: "a".to_string(),
                            event_type: "sent".to_string(),
                            message: format!("PUT {} v{}", url, ver_clone),
                            timestamp: current_timestamp(),
                        },
                    );
                }
                Err(e) => {
                    eprintln!("[ERROR] Peer A PUT error: {}", e);
                    let _ = app.emit(
                        "network-event",
                        NetworkEvent {
                            peer: "a".to_string(),
                            event_type: "error".to_string(),
                            message: format!("PUT error: {}", e),
                            timestamp: current_timestamp(),
                        },
                    );
                }
            }
        });

        Ok(UpdateResponse {
            success: true,
            peer: "a".to_string(),
            version: ver,
        })
    } else {
        // Update local state
        let mut cb = state.content_b.write().await;
        *cb = payload.content.clone();
        drop(cb);

        // Send via Peer B
        let peer_node = state.peer_b.clone();
        let url = resource_url.to_string();

        let ver_clone = ver.clone();
        tokio::spawn(async move {
            match peer_node.put(&url, update).await {
                Ok(_) => {
                    println!("[SENT] Peer B: PUT {} v{}", url, ver_clone);
                }
                Err(e) => {
                    eprintln!("[ERROR] Peer B PUT error: {}", e);
                }
            }
        });

        Ok(UpdateResponse {
            success: true,
            peer: "b".to_string(),
            version: ver,
        })
    }
}

/// Command: Initialize the P2P connection (called from welcome screen)
#[tauri::command]
async fn initialize_p2p(_app: AppHandle) -> Result<PeersConnectedEvent, String> {
    // This will be handled by the setup function, but we can emit an event
    // to confirm initialization
    Ok(PeersConnectedEvent {
        peer_a_id: "initializing...".to_string(),
        peer_b_id: "initializing...".to_string(),
        topic: "/demo-doc".to_string(),
    })
}

/// Command: Run the subscription demo
#[tauri::command]
async fn run_subscription_demo(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    resource_name: String,
) -> Result<(), String> {
    // We want Peer B to subscribe to Peer A
    let peer_b_node = state.peer_b.clone();
    let peer_a_id = state.peer_a_id.clone();

    // We need an IrohH3Client for Peer B
    let endpoint = peer_b_node.endpoint().clone();

    // Create client - using the ALPN from node config/constants
    let client = IrohH3Client::new(endpoint, b"braid-h3/0".to_vec());

    // Construct URL: https://<peer_id>/<resource_name>
    // Ensure resource_name starts with /
    let path = if resource_name.starts_with('/') {
        resource_name.clone()
    } else {
        format!("/{}", resource_name)
    };
    let url = format!("https://{}{}", peer_a_id, path);
    println!("[SUB] Peer B starting subscription to: {}", url);

    // Send request with Subscribe: true
    let req = client
        .get(&url)
        .header("Subscribe", "true")
        .build()
        .map_err(|e| e.to_string())?;

    let state_arc = (*state).clone();

    // Spawn async task to handle the request and emit events
    tauri::async_runtime::spawn(async move {
        println!("[SUB] Sending GET request with Subscribe: true");
        // Emit "Sending" event
        let _ = app.emit(
            "network-event",
            NetworkEvent {
                peer: "b".to_string(),
                event_type: "sent".to_string(),
                message: "Sending SUB request...".to_string(),
                timestamp: current_timestamp(),
            },
        );

        match req.send().await {
            Ok(resp) => {
                let status = resp.status;
                println!("[SUB] Received response status: {}", status);
                let _ = app.emit(
                    "network-event",
                    NetworkEvent {
                        peer: "b".to_string(),
                        event_type: "received".to_string(),
                        message: format!("Response Status: {}", status),
                        timestamp: current_timestamp(),
                    },
                );

                // If 209, emit success AND join gossip
                if status.as_u16() == 209 {
                    println!("[SUB] Status is 209, joining gossip...");

                    // Parse initial body if present
                    if let Ok(body_bytes) = resp.bytes().await {
                        if !body_bytes.is_empty() {
                            println!("[SUB] Received initial state (len: {})", body_bytes.len());
                            // use crate::braid_http_rs::Update; - assumed available or we use full path
                            if let Ok(update) =
                                serde_json::from_slice::<braid_http_rs::Update>(&body_bytes)
                            {
                                if let Some(content_bytes) = update.body {
                                    let content =
                                        String::from_utf8_lossy(&content_bytes).to_string();
                                    let ver = update
                                        .version
                                        .first()
                                        .map(|v| v.to_string())
                                        .unwrap_or("init".into());

                                    println!("[SUB] Initial Content: {}", content);

                                    // Emit initial event to UI
                                    app.emit(
                                        "sync-received",
                                        SyncReceivedEvent {
                                            peer: "b".into(),
                                            content,
                                            version: ver,
                                        },
                                    )
                                    .ok();
                                }
                            } else {
                                println!("[SUB] Failed to parse initial state JSON");
                            }
                        } else {
                            println!("[SUB] No initial state in 209 response");
                        }
                    }

                    let _ = app.emit(
                        "network-event",
                        NetworkEvent {
                            peer: "b".to_string(),
                            event_type: "success".to_string(),
                            message: "Subscription Active! (209)".to_string(),
                            timestamp: current_timestamp(),
                        },
                    );

                    // Join Gossip for this resource
                    let _ = app.emit(
                        "network-event",
                        NetworkEvent {
                            peer: "b".to_string(),
                            event_type: "sent".to_string(),
                            message: format!("Joining Gossip: {}", path),
                            timestamp: current_timestamp(),
                        },
                    );

                    // We need to bootstrap with Peer A.
                    // In a real app we'd look up Peer A's address or rely on discovery.
                    // Here we know Peer A's ID.
                    // But BraidIrohNode::subscribe takes Vec<EndpointId>.
                    // peer_a_id is a String here, we need to parse it back to EndpointId or store it better.
                    // For simplicity, let's try to parse it.
                    if let Ok(peer_a_endpoint) = base32_to_endpoint_id(&peer_a_id) {
                        match peer_b_node.subscribe(&path, vec![peer_a_endpoint]).await {
                            Ok(rx) => {
                                println!("[SUB] Successfully subscribed to gossip topic: {}", path);
                                let _ = app.emit(
                                    "network-event",
                                    NetworkEvent {
                                        peer: "b".to_string(),
                                        event_type: "success".to_string(),
                                        message: "Joined Gossip Topic".to_string(),
                                        timestamp: current_timestamp(),
                                    },
                                );

                                // Spawn a listener for this specific subscription
                                spawn_peer_listener(app.clone(), state_arc.clone(), rx, "b");
                            }
                            Err(e) => {
                                println!("[SUB] Gossip join failed: {}", e);
                                let _ = app.emit(
                                    "network-event",
                                    NetworkEvent {
                                        peer: "b".to_string(),
                                        event_type: "error".to_string(),
                                        message: format!("Gossip join failed: {}", e),
                                        timestamp: current_timestamp(),
                                    },
                                );
                            }
                        }
                    } else {
                        println!("[SUB] Failed to parse Peer A ID: {}", peer_a_id);
                        let _ = app.emit(
                            "network-event",
                            NetworkEvent {
                                peer: "b".to_string(),
                                event_type: "error".to_string(),
                                message: "Failed to parse Peer A ID".to_string(),
                                timestamp: current_timestamp(),
                            },
                        );
                    }
                }
            }
            Err(e) => {
                println!("[SUB] Request failed: {}", e);
                let _ = app.emit(
                    "network-event",
                    NetworkEvent {
                        peer: "b".to_string(),
                        event_type: "error".to_string(),
                        message: format!("Request failed: {}", e),
                        timestamp: current_timestamp(),
                    },
                );
            }
        }
    });

    Ok(())
}

/// Command: Get history of versions for a resource (Peer B)
#[tauri::command]
async fn get_version_history(
    state: State<'_, Arc<AppState>>,
    resource_name: String,
) -> Result<Vec<String>, String> {
    // Ensure resource_name starts with /
    let path = if resource_name.starts_with('/') {
        resource_name
    } else {
        format!("/{}", resource_name)
    };

    let history = state.peer_b.get_history(&path).await;
    // Reverse to show latest first
    Ok(history.into_iter().rev().collect())
}

/// Command: Get specific version content (Peer B)
#[tauri::command]
async fn get_version_content(
    state: State<'_, Arc<AppState>>,
    resource_name: String,
    version_id: String,
) -> Result<String, String> {
    // Ensure resource_name starts with /
    let path = if resource_name.starts_with('/') {
        resource_name
    } else {
        format!("/{}", resource_name)
    };

    if let Some(update) = state.peer_b.get_version(&path, &version_id).await {
        if let Some(body_bytes) = update.body {
            return Ok(String::from_utf8_lossy(&body_bytes).to_string());
        }
        return Ok("".to_string());
    }
    Err("Version not found".to_string())
}

fn base32_to_endpoint_id(id: &str) -> anyhow::Result<iroh::EndpointId> {
    use std::str::FromStr;
    Ok(iroh::EndpointId::from_str(id)?)
}

// =============================================================================
// MAIN ENTRY POINT
// =============================================================================

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "braid_iroh=info,iroh_gossip=info".to_string()),
        )
        .init();

    println!("[START] link_iroh initializing...");
    println!("[INIT] Starting with fresh in-memory database (cleaned on restart).");

    // Initialize P2P nodes synchronously
    let state = tauri::async_runtime::block_on(async { initialize_network().await });

    let (app_state, rx_a, rx_b) = state;
    let state_for_listeners = app_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .setup(move |app| {
            let handle = app.handle().clone();

            // Spawn Gossip Listeners - Native Tauri Events
            spawn_peer_listener(handle.clone(), state_for_listeners.clone(), rx_a, "a");
            spawn_peer_listener(handle, state_for_listeners, rx_b, "b");

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_peers,
            send_update,
            initialize_p2p,
            run_subscription_demo,
            get_version_history,
            get_version_content
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// =============================================================================
// NETWORK INITIALIZATION
// =============================================================================

async fn initialize_network() -> (
    Arc<AppState>,
    iroh_gossip::api::GossipReceiver,
    iroh_gossip::api::GossipReceiver,
) {
    let discovery = DiscoveryConfig::mock();

    println!("[INIT] Spawning Peer A...");
    let peer_a = BraidIrohNode::spawn(BraidIrohConfig {
        discovery: discovery.clone(),
        secret_key: None,
        proxy_config: None,
    })
    .await
    .expect("Failed to spawn Peer A");
    let peer_a_id = peer_a.node_id();

    discovery.add_node(peer_a.node_addr().await.unwrap());

    println!("[INIT] Spawning Peer B (with Proxy)...");
    let peer_b = BraidIrohNode::spawn(BraidIrohConfig {
        discovery: discovery.clone(),
        secret_key: None,
        #[cfg(feature = "proxy")]
        proxy_config: Some(crate::node::ProxyConfig {
            listen_addr: "127.0.0.1:8080".parse().unwrap(),
            default_peer: peer_a.node_id(), 
        }),
        #[cfg(not(feature = "proxy"))]
        proxy_config: None,
    })
    .await
    .expect("Failed to spawn Peer B");
    let peer_b_id = peer_b.node_id();

    discovery.add_node(peer_b.node_addr().await.unwrap());

    let resource_url = "/demo-doc";

    // Subscribe to gossip
    let rx_a = peer_a
        .subscribe(resource_url, vec![peer_b_id])
        .await
        .expect("Peer A subscribe failed");

    let rx_b = peer_b
        .subscribe(resource_url, vec![peer_a_id])
        .await
        .expect("Peer B subscribe failed");

    println!("[INIT] P2P Network initialized");
    println!("   Peer A: {}", peer_a_id);
    println!("   Peer B: {}", peer_b_id);

    (
        Arc::new(AppState {
            peer_a: Arc::new(peer_a),
            peer_b: Arc::new(peer_b),
            peer_a_id: format!("{}", peer_a_id),
            peer_b_id: format!("{}", peer_b_id),
            content_a: Arc::new(RwLock::new(String::new())),
            content_b: Arc::new(RwLock::new(String::new())),
            version: Arc::new(RwLock::new(String::new())),
        }),
        rx_a,
        rx_b,
    )
}

// =============================================================================
// EVENT LISTENERS - Native Tauri Event Emission (No SSE)
// =============================================================================

fn spawn_peer_listener(
    app: AppHandle,
    state: Arc<AppState>,
    mut receiver: iroh_gossip::api::GossipReceiver,
    peer_label: &'static str,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(res) = receiver.next().await {
            match res {
                Ok(iroh_gossip::api::Event::Received(msg)) => {
                    if let Ok(update) = serde_json::from_slice::<Update>(&msg.content) {
                        if let Some(ref body) = update.body {
                            let content = String::from_utf8_lossy(&body).to_string();

                            // Update state
                            if peer_label == "a" {
                                let mut ca = state.content_a.write().await;
                                *ca = content.clone();
                                drop(ca);
                                // Peer A is source, so it already has it via put()
                            } else {
                                let mut cb = state.content_b.write().await;
                                *cb = content.clone();
                                drop(cb);
                                // Peer B needs to store this for history
                                state.peer_b.store_update("/my-chat", update.clone()).await;
                                // Note: Hardcoded /my-chat for now as we don't fully track topic in msg
                                // Ideally we'd extract topic from gossip event or message
                            }

                            // Extract version
                            let version_str = update
                                .version
                                .first()
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "unknown".to_string());

                            println!(
                                "[RECEIVED] Peer {} got update: {} (len: {})",
                                peer_label,
                                version_str,
                                content.len()
                            );

                            // Emit native Tauri event to frontend
                            let event = SyncReceivedEvent {
                                peer: peer_label.to_string(),
                                content,
                                version: version_str,
                            };

                            if let Err(e) = app.emit("sync-received", event) {
                                eprintln!("Failed to emit sync event: {}", e);
                            }

                            // Also emit network event for logging
                            let _ = app.emit(
                                "network-event",
                                NetworkEvent {
                                    peer: peer_label.to_string(),
                                    event_type: "received".to_string(),
                                    message: "Update received via gossip".to_string(),
                                    timestamp: current_timestamp(),
                                },
                            );
                        }
                    }
                }
                Ok(_) => {
                    // Other events (Joined, etc.) - silently handle
                }
                Err(e) => {
                    eprintln!("[ERROR] Peer {} gossip error: {}", peer_label, e);
                    let _ = app.emit(
                        "network-event",
                        NetworkEvent {
                            peer: peer_label.to_string(),
                            event_type: "error".to_string(),
                            message: format!("Gossip error: {}", e),
                            timestamp: current_timestamp(),
                        },
                    );
                }
            }
        }
    });
}

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
