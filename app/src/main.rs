#![windows_subsystem = "console"]
// Console window enabled for all platforms (released or debug)

//! Braid-Iroh Native Tauri v2 Application
//!
//! A pure native desktop application for P2P synchronization using Iroh and Braid-HTTP.
//! No external JavaScript frameworks - pure Tauri events and vanilla JavaScript.

use braid_http_rs::{Update, Version};
use bytes::Bytes;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::RwLock;
use uuid::Uuid;
use clap::Parser;

mod discovery;
mod node;
mod protocol;
mod proxy;
mod subscription;

use crate::discovery::DiscoveryConfig;
use crate::node::{BraidIrohConfig, BraidIrohNode};

use iroh_h3_client::IrohH3Client;

#[derive(Parser, Clone, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Run in P2P mode with a persistent identity (instead of simulation)
    #[arg(long)]
    p2p: bool,

    /// Name of the node (for persistent data storage in P2P mode)
    #[arg(long, default_value = "default")]
    name: String,
}

/// Emitted when peers are discovered and connected
#[derive(Clone, Debug, Serialize)]
struct PeersConnectedEvent {
    peer_a_id: String,
    peer_b_id: String,
    topic: String,
    node_name: String,        // "alice" or "bob"
    is_connected: bool,       // Whether we have a connected peer
    connected_peer: Option<String>, // The peer we're connected to (if any)
}

/// Emitted when sync data is received from the network
#[derive(Clone, Debug, Serialize)]
struct SyncReceivedEvent {
    peer: String,
    content: String,
    version: String,
}

/// Wrapper for gossip messages to include sender information
#[derive(Clone, Debug, Serialize, Deserialize)]
struct GossipMessage {
    /// The peer label of the sender ("a" for Alice, "b" for Bob)
    sender_peer: String,
    /// The actual Braid update
    update: Update,
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
/// In P2P mode, peer_b is None (we are only one node)
struct AppState {
    peer_a: Arc<BraidIrohNode>,
    peer_b: Option<Arc<BraidIrohNode>>,
    peer_a_id: String,
    peer_b_id: Option<String>,
    content_a: Arc<RwLock<String>>,
    content_b: Arc<RwLock<String>>,
    version: Arc<RwLock<String>>,
    node_name: String,  // "alice" or "bob"
    connected_peer: Arc<RwLock<Option<String>>>, // Track connected peer
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
    let connected = state.connected_peer.read().await.clone();
    Ok(PeersConnectedEvent {
        peer_a_id: state.peer_a_id.clone(),
        peer_b_id: state.peer_b_id.clone().unwrap_or_else(|| "Waiting for connection...".to_string()),
        topic: "/demo-doc".to_string(),
        node_name: state.node_name.clone(),
        is_connected: connected.is_some(),
        connected_peer: connected,
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

    // Normalize resource URL: always starts with /, no trailing /
    let resource_url = payload.resource_name.as_deref().map(|s| {
        let trimmed = s.trim_end_matches('/');
        if trimmed.starts_with('/') { trimmed.to_string() } else { format!("/{}", trimmed) }
    }).unwrap_or_else(|| "/demo-doc".to_string());
    
    println!("[SEND_UPDATE] peer={} url={} version={}", payload.peer, resource_url, ver);
    println!("[SEND_UPDATE] node_id={}", state.peer_a.node_id());
    
    // Create the Braid update
    let update = Update::snapshot(
        Version::String(ver.clone()),
        Bytes::from(payload.content.clone()),
    );
    
    // Wrap with sender information for gossip
    let gossip_msg = GossipMessage {
        sender_peer: payload.peer.clone(),
        update: update.clone(),
    };

    // In P2P mode, we only have peer_a. In simulation mode, we have both.
    // Use peer_a as the primary node for sending updates in both cases.
    let peer_node = state.peer_a.clone();
    let subscription_mgr = state.peer_a.subscriptions().clone();
    let url = resource_url.to_string();
    let is_peer_a = payload.peer == "a";
    
    // Update the appropriate local state based on which peer the UI thinks we are
    if is_peer_a {
        let mut ca = state.content_a.write().await;
        *ca = payload.content.clone();
        drop(ca);
    } else {
        let mut cb = state.content_b.write().await;
        *cb = payload.content.clone();
        drop(cb);
    }

    let ver_clone = ver.clone();
    let peer_label = payload.peer.clone();
    let url_for_broadcast = url.clone();
    tokio::spawn(async move {
        // Store locally
        match peer_node.put(&url, update).await {
            Ok(_) => {
                println!("[SENT] Peer {}: PUT {} v{}", peer_label, url, ver_clone);
                
                // Broadcast the wrapped message with sender info
                let gossip_bytes = match serde_json::to_vec(&gossip_msg) {
                    Ok(b) => Bytes::from(b),
                    Err(e) => {
                        eprintln!("[ERROR] Failed to serialize gossip message: {}", e);
                        return;
                    }
                };
                
                if let Err(e) = subscription_mgr.broadcast_raw(&url_for_broadcast, gossip_bytes).await {
                    eprintln!("[ERROR] Failed to broadcast gossip: {}", e);
                }
                
                let _ = app.emit(
                    "network-event",
                    NetworkEvent {
                        peer: peer_label,
                        event_type: "sent".to_string(),
                        message: format!("PUT {} v{}", url, ver_clone),
                        timestamp: current_timestamp(),
                    },
                );
            }
            Err(e) => {
                eprintln!("[ERROR] Peer {} PUT error: {}", peer_label, e);
                let _ = app.emit(
                    "network-event",
                    NetworkEvent {
                        peer: peer_label,
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
        peer: payload.peer,
        version: ver,
    })
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
        node_name: "unknown".to_string(),
        is_connected: false,
        connected_peer: None,
    })
}

/// Command: Run the subscription demo (Bob subscribes to Alice)
#[tauri::command]
async fn run_subscription_demo(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    resource_name: String,
    peer_id: Option<String>, // Optional: specify which peer to subscribe to
) -> Result<(), String> {
    // Get the peer to subscribe to (either provided peer_id or our connected peer)
    let target_peer_id = if let Some(id) = peer_id {
        id
    } else if let Some(connected) = state.connected_peer.read().await.clone() {
        connected
    } else {
        return Err("No target peer specified and no peer first.".to_string());
    };
    
    // We use our local node to subscribe to the remote peer
    let local_node = state.peer_a.clone();

    // We need an IrohH3Client using our local endpoint
    let endpoint = local_node.endpoint().clone();

    // Create client - using the ALPN from node config/constants
    let client = IrohH3Client::new(endpoint, b"braid-h3/0".to_vec());

    // Construct URL: https://<peer_id>/<resource_name>
    // Ensure resource_name starts with /
    let path = if resource_name.starts_with('/') {
        resource_name.clone()
    } else {
        format!("/{}", resource_name)
    };
    let url = format!("https://{}{}", target_peer_id, path);
    println!("[SUB] Starting subscription to: {}", url);

    // Send request with Subscribe: true
    let req = client
        .get(&url)
        .header("Subscribe", "true")
        .build()
        .map_err(|e| e.to_string())?;

    let state_arc = Arc::clone(&state);

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

                    // Join gossip topic for this resource, bootstrapping with the target peer
                    if let Ok(target_endpoint) = base32_to_endpoint_id(&target_peer_id) {
                        match local_node.subscribe(&path, vec![target_endpoint]).await {
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
                                spawn_peer_listener(app.clone(), state_arc.clone(), rx);
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
                        println!("[SUB] Failed to parse target peer ID: {}", target_peer_id);
                        let _ = app.emit(
                            "network-event",
                            NetworkEvent {
                                peer: "b".to_string(),
                                event_type: "error".to_string(),
                                message: "Failed to parse peer ID".to_string(),
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

/// Command: Get history of versions for a resource
/// First tries local storage, then fetches from remote peer if connected and local is empty
#[tauri::command]
async fn get_version_history(
    state: State<'_, Arc<AppState>>,
    resource_name: String,
) -> Result<Vec<String>, String> {
    // Ensure resource_name starts with /
    let path = if resource_name.starts_with('/') {
        resource_name.clone()
    } else {
        format!("/{}", resource_name)
    };

    // First check local storage
    let node = state.peer_b.as_ref().unwrap_or(&state.peer_a);
    let local_history = node.get_history(&path).await;
    
    if !local_history.is_empty() {
        // Reverse to show latest first
        return Ok(local_history.into_iter().rev().collect());
    }
    
    // If local is empty and we have a connected peer, try to fetch from them
    let connected_peer = state.connected_peer.read().await.clone();
    if let Some(peer_id) = connected_peer {
        println!("[HISTORY] Local empty, fetching from peer: {}", peer_id);
        
        // Fetch history from remote peer via HTTP
        match fetch_remote_history(&state, &peer_id, &path).await {
            Ok(history) => {
                println!("[HISTORY] Fetched {} versions from remote", history.len());
                return Ok(history);
            }
            Err(e) => {
                println!("[HISTORY] Failed to fetch from remote: {}", e);
                // Fall through to return empty
            }
        }
    }
    
    Ok(vec![])
}

/// Fetch version history from a remote peer via HTTP
async fn fetch_remote_history(
    state: &Arc<AppState>,
    peer_id: &str,
    path: &str,
) -> Result<Vec<String>, String> {
    let endpoint = state.peer_a.endpoint().clone();
    let client = IrohH3Client::new(endpoint, b"braid-h3/0".to_vec());
    
    let url = format!("https://{}{}?history=true", peer_id, path);
    println!("[HISTORY] Fetching from: {}", url);
    
    let req = client
        .get(&url)
        .build()
        .map_err(|e| format!("Failed to build request: {}", e))?;
    
    let resp = req.send().await.map_err(|e| format!("Request failed: {}", e))?;
    
    if resp.status.as_u16() != 200 {
        return Err(format!("Remote returned status: {}", resp.status));
    }
    
    let body_bytes = resp.bytes().await.map_err(|e| format!("Failed to read body: {}", e))?;
    let history: Vec<String> = serde_json::from_slice(&body_bytes)
        .map_err(|e| format!("Failed to parse history: {}", e))?;
    
    Ok(history.into_iter().rev().collect())
}

/// Command: Get specific version content
/// First tries local storage, then fetches from remote peer if connected
#[tauri::command]
async fn get_version_content(
    state: State<'_, Arc<AppState>>,
    resource_name: String,
    version_id: String,
) -> Result<String, String> {
    // Ensure resource_name starts with /
    let path = if resource_name.starts_with('/') {
        resource_name.clone()
    } else {
        format!("/{}", resource_name)
    };

    // First try local storage
    let node = state.peer_b.as_ref().unwrap_or(&state.peer_a);
    
    if let Some(update) = node.get_version(&path, &version_id).await {
        if let Some(body_bytes) = update.body {
            return Ok(String::from_utf8_lossy(&body_bytes).to_string());
        }
        return Ok("".to_string());
    }
    
    // If not found locally and we have a connected peer, try to fetch from them
    let connected_peer = state.connected_peer.read().await.clone();
    if let Some(peer_id) = connected_peer {
        println!("[VERSION] Local not found, fetching from peer: {}", peer_id);
        
        match fetch_remote_version(&state, &peer_id, &path, &version_id).await {
            Ok(content) => {
                println!("[VERSION] Fetched version {} from remote", version_id);
                return Ok(content);
            }
            Err(e) => {
                println!("[VERSION] Failed to fetch from remote: {}", e);
            }
        }
    }
    
    Err("Version not found".to_string())
}

/// Fetch specific version from a remote peer via HTTP
async fn fetch_remote_version(
    state: &Arc<AppState>,
    peer_id: &str,
    path: &str,
    version_id: &str,
) -> Result<String, String> {
    let endpoint = state.peer_a.endpoint().clone();
    let client = IrohH3Client::new(endpoint, b"braid-h3/0".to_vec());
    
    let url = format!("https://{}{}?version={}", peer_id, path, version_id);
    println!("[VERSION] Fetching from: {}", url);
    
    let req = client
        .get(&url)
        .build()
        .map_err(|e| format!("Failed to build request: {}", e))?;
    
    let resp = req.send().await.map_err(|e| format!("Request failed: {}", e))?;
    
    if resp.status.as_u16() != 200 {
        return Err(format!("Remote returned status: {}", resp.status));
    }
    
    let body_bytes = resp.bytes().await.map_err(|e| format!("Failed to read body: {}", e))?;
    
    // Try to parse as Update first
    if let Ok(update) = serde_json::from_slice::<braid_http_rs::Update>(&body_bytes) {
        if let Some(content_bytes) = update.body {
            return Ok(String::from_utf8_lossy(&content_bytes).to_string());
        }
        return Ok("".to_string());
    }
    
    // Otherwise return raw body
    Ok(String::from_utf8_lossy(&body_bytes).to_string())
}

/// Command: Connect to a peer by Node ID
#[tauri::command]
async fn connect_peer(
    state: State<'_, Arc<AppState>>,
    node_id: String,
) -> Result<String, String> {
    println!("[CONNECT] Attempting to connect to peer: {}", node_id);
    
    // Parse the node ID
    let endpoint_id = base32_to_endpoint_id(&node_id)
        .map_err(|e| format!("Invalid node ID: {}", e))?;
    
    // Join the peer to our gossip topic
    let resource_url = "/demo-doc";
    state.peer_a.join_peers(resource_url, vec![endpoint_id])
        .await
        .map_err(|e| format!("Failed to join peer: {}", e))?;
    
    // Update connection state
    let mut connected = state.connected_peer.write().await;
    *connected = Some(node_id.clone());
    drop(connected);
    
    println!("[CONNECT] Successfully connected to {}", node_id);
    Ok(format!("Connected to {}", node_id))
}

fn base32_to_endpoint_id(id: &str) -> anyhow::Result<iroh::EndpointId> {
    use std::str::FromStr;
    Ok(iroh::EndpointId::from_str(id)?)
}

// =============================================================================
// MAIN ENTRY POINT
// =============================================================================

fn main() {
    // --- LAUNCHER MODE (Run without args) ---
    // If double-clicked (no args), spawn two P2P nodes (Alice and Bob) in separate windows
    if std::env::args().len() == 1 {
        println!("[LAUNCHER] No arguments detected. Spawning Alice and Bob...");

        #[cfg(target_os = "windows")]
        {
            use std::process::Command;
            
            // Get current executable path to be safe
            let current_exe = std::env::current_exe()
                .unwrap_or_else(|_| "braid_iroh.exe".into());
            
            // Spawn Alice
            println!("[LAUNCHER] Spawning Alice...");
            Command::new("cmd")
                .args(["/c", "start", "Braid-Iroh Alice", current_exe.to_str().unwrap(), "--p2p", "--name", "alice"])
                .spawn()
                .expect("Failed to spawn Alice");

            // Wait 1s
            std::thread::sleep(std::time::Duration::from_millis(1000));

            // Spawn Bob 
            println!("[LAUNCHER] Spawning Bob...");
            Command::new("cmd")
                .args(["/c", "start", "Braid-Iroh Bob", current_exe.to_str().unwrap(), "--p2p", "--name", "bob"])
                .spawn()
                .expect("Failed to spawn Bob");

            println!("[LAUNCHER] Exiting launcher process.");
            return;
        }
        
        #[cfg(not(target_os = "windows"))]
        {
             println!("[LAUNCHER] Auto-launch is currently only supported on Windows.");
             println!("Please run manually: ./braid_iroh --p2p --name alice");
             // Fallthrough to simulation or exit? 
             // Let's fallthrough to simulation for now on non-windows to avoid confusion, 
             // or just exit. Fallthrough preserves old behavior on Mac/Linux.
        }
    }

    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "braid_iroh=info,iroh_gossip=info".to_string()),
        )
        .init();

    println!("[START] link_iroh initializing...");
    println!("[INIT] Starting with fresh in-memory database (cleaned on restart).");

    let args = Args::parse();
    if args.p2p {
        println!("[MAIN] Starting in TRUE P2P Mode as '{}'", args.name);
    } else {
        println!("[MAIN] Starting in SIMULATION Mode (Peer A + Peer B)");
    }

    // Initialize P2P nodes synchronously
    let state = tauri::async_runtime::block_on(async { initialize_network(args).await });

    let (app_state, rx_a, rx_b) = state;
    let state_for_listeners = app_state.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .setup(move |app| {
            let handle = app.handle().clone();
            
            // Set window title based on node name
            let window = app.get_webview_window("main").unwrap();
            let title = if state_for_listeners.node_name == "alice" {
                "Braid Iroh P2P - Alice"
            } else if state_for_listeners.node_name == "bob" {
                "Braid Iroh P2P - Bob"
            } else {
                "Braid Iroh P2P"
            };
            window.set_title(title).unwrap();

            // Spawn Gossip Listeners - Native Tauri Events
            spawn_peer_listener(handle.clone(), state_for_listeners.clone(), rx_a);
            
            // Only spawn peer_b listener if we have one (simulation mode)
            if let Some(rx_b) = rx_b {
                spawn_peer_listener(handle, state_for_listeners, rx_b);
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_peers,
            send_update,
            initialize_p2p,
            run_subscription_demo,
            get_version_history,
            get_version_content,
            connect_peer
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// =============================================================================
// NETWORK INITIALIZATION
// =============================================================================

async fn initialize_network(args: Args) -> (
    Arc<AppState>,
    iroh_gossip::api::GossipReceiver,
    Option<iroh_gossip::api::GossipReceiver>,
) {
    if args.p2p {
        return initialize_p2p_mode(args).await;
    }

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

    println!("[INIT] Simulation Network initialized");
    println!("   Peer A: {}", peer_a_id);
    println!("   Peer B: {}", peer_b_id);

    (
        Arc::new(AppState {
            peer_a: Arc::new(peer_a),
            peer_b: Some(Arc::new(peer_b)),
            peer_a_id: format!("{}", peer_a_id),
            peer_b_id: Some(format!("{}", peer_b_id)),
            content_a: Arc::new(RwLock::new(String::new())),
            content_b: Arc::new(RwLock::new(String::new())),
            version: Arc::new(RwLock::new(String::new())),
            node_name: "simulation".to_string(),
            connected_peer: Arc::new(RwLock::new(Some(format!("{}", peer_b_id)))),
        }),
        rx_a,
        Some(rx_b),
    )
}

// =============================================================================
// EVENT LISTENERS - Native Tauri Event Emission (No SSE)
// =============================================================================

fn spawn_peer_listener(
    app: AppHandle,
    state: Arc<AppState>,
    mut receiver: iroh_gossip::api::GossipReceiver,
) {
    tauri::async_runtime::spawn(async move {
        // Get our local node ID to determine if a message is from us or remote
        let local_node_id = state.peer_a_id.clone();
        
        println!("[LISTENER] Starting gossip listener for node {}", local_node_id);
        
        while let Some(res) = receiver.next().await {
            match res {
                Ok(iroh_gossip::api::Event::Received(msg)) => {
                    // Determine if this message is from us or from the remote peer
                    let sender_id = format!("{}", msg.delivered_from);
                    let is_from_local = sender_id == local_node_id;
                    
                    println!("[LISTENER] Received msg from {} (local={})", sender_id, is_from_local);
                    
                    // Try to deserialize as GossipMessage first (new format with sender info)
                    if let Ok(gossip_msg) = serde_json::from_slice::<GossipMessage>(&msg.content) {
                        let peer_label = gossip_msg.sender_peer;
                        let update = gossip_msg.update;
                        
                        if let Some(ref body) = update.body {
                            let content = String::from_utf8_lossy(body).to_string();

                            // Update state based on who originally sent the message
                            if peer_label == "a" {
                                let mut ca = state.content_a.write().await;
                                *ca = content.clone();
                                drop(ca);
                            } else {
                                let mut cb = state.content_b.write().await;
                                *cb = content.clone();
                                drop(cb);
                            }
                            
                            // Store update for history
                            let resource_url = "/demo-doc";
                            let _ = state.peer_a.store_update(resource_url, update.clone()).await;

                            let version_str = update
                                .version
                                .first()
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "unknown".to_string());

                            println!(
                                "[RECEIVED] {} got update from peer {}: {} (len: {})",
                                local_node_id,
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
                                    message: format!("Update from peer {} (node {})", peer_label, sender_id),
                                    timestamp: current_timestamp(),
                                },
                            );
                        }
                    } else if let Ok(update) = serde_json::from_slice::<Update>(&msg.content) {
                        // Fallback: legacy format without sender info - use node ID to determine
                        let peer_label = if is_from_local { "a".to_string() } else { "b".to_string() };
                        
                        if let Some(ref body) = update.body {
                            let content = String::from_utf8_lossy(body).to_string();

                            // Update state based on who sent the message (legacy: local=a, remote=b)
                            if is_from_local {
                                let mut ca = state.content_a.write().await;
                                *ca = content.clone();
                                drop(ca);
                            } else {
                                let mut cb = state.content_b.write().await;
                                *cb = content.clone();
                                drop(cb);
                                // Store update for history in our local node
                                let resource_url = "/demo-doc";
                                let _ = state.peer_a.store_update(resource_url, update.clone()).await;
                            }

                            let version_str = update
                                .version
                                .first()
                                .map(|v| v.to_string())
                                .unwrap_or_else(|| "unknown".to_string());

                            println!(
                                "[RECEIVED] {} got update from {}: {} (len: {}) [LEGACY]",
                                local_node_id,
                                sender_id,
                                version_str,
                                content.len()
                            );

                            let event = SyncReceivedEvent {
                                peer: peer_label,
                                content,
                                version: version_str,
                            };

                            if let Err(e) = app.emit("sync-received", event) {
                                eprintln!("Failed to emit sync event: {}", e);
                            }

                            let _ = app.emit(
                                "network-event",
                                NetworkEvent {
                                    peer: sender_id.clone(),
                                    event_type: "received".to_string(),
                                    message: format!("Update from {} [LEGACY]", sender_id),
                                    timestamp: current_timestamp(),
                                },
                            );
                        }
                    }
                }
                Ok(iroh_gossip::api::Event::NeighborUp(endpoint_id)) => {
                    // A peer joined our topic - update connection state
                    println!("[GOSSIP] NeighborUp: {}", endpoint_id);
                    let peer_id_str = format!("{}", endpoint_id);
                    let peer_id_for_event = peer_id_str.clone();
                    let mut connected = state.connected_peer.write().await;
                    *connected = Some(peer_id_str);
                    drop(connected);
                    
                    // Emit event to UI
                    let _ = app.emit(
                        "peer-connected",
                        PeersConnectedEvent {
                            peer_a_id: state.peer_a_id.clone(),
                            peer_b_id: peer_id_for_event.clone(),
                            topic: "/demo-doc".to_string(),
                            node_name: state.node_name.clone(),
                            is_connected: true,
                            connected_peer: Some(peer_id_for_event),
                        },
                    );
                }
                Ok(_) => {
                    // Other events - silently handle
                }
                Err(e) => {
                    eprintln!("[ERROR] Gossip error: {}", e);
                    let _ = app.emit(
                        "network-event",
                        NetworkEvent {
                            peer: "local".to_string(),
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

// Helper for persistent identity
async fn get_or_create_secret_key(name: &str) -> iroh::SecretKey {
    // In a real app, store this in %APPDATA%/braid_iroh/<name>.key
    // For now, we generate random to test arguments, or deterministic from string hash for stability
    // Let's use deterministic for "alice" and "bob" for easy testing
    if name == "alice" {
        return iroh::SecretKey::from_bytes(&[1u8; 32]);
    }
    if name == "bob" {
        return iroh::SecretKey::from_bytes(&[2u8; 32]);
    }
    // Use rand 0.9 API for generating secret key
    iroh::SecretKey::generate(&mut rand::rng())
}

async fn initialize_p2p_mode(args: Args) -> (
    Arc<AppState>,
    iroh_gossip::api::GossipReceiver,
    Option<iroh_gossip::api::GossipReceiver>,
) {
    // --- REAL P2P MODE ---
    #[allow(unused_mut)]
    let mut discovery = DiscoveryConfig::Real;

    // We only spawn ONE node: Peer A (Us)
    println!("[INIT] Spawning Real P2P Node: {}", args.name);

    let secret_key = get_or_create_secret_key(&args.name).await;

    let peer_a = BraidIrohNode::spawn(BraidIrohConfig {
        discovery: discovery.clone(),
        secret_key: Some(secret_key),
        proxy_config: None,
    })
    .await
    .expect("Failed to spawn P2P Node");

    let peer_a = Arc::new(peer_a);

    let peer_a_id = peer_a.node_id();
    println!("[INIT] Our Node ID: {}", peer_a_id);

    let resource_url = "/demo-doc";
    // In true P2P, we don't know Peer B yet. We subscribe to the topic generally without peers.
    let rx_a = peer_a
        .subscribe(resource_url, vec![])
        .await
        .expect("Subscribe failed");

    // In P2P mode, we only have one node (Peer A / Us)
    // Peer B is None until we connect to another peer
    let node_name = args.name.clone();
    (
        Arc::new(AppState {
            peer_a: peer_a.clone(), // We are A
            peer_b: None,           // No peer B yet
            peer_a_id: format!("{}", peer_a_id),
            peer_b_id: None,        // No peer B yet
            content_a: Arc::new(RwLock::new(String::new())),
            content_b: Arc::new(RwLock::new(String::new())),
            version: Arc::new(RwLock::new(String::new())),
            node_name,
            connected_peer: Arc::new(RwLock::new(None)),
        }),
        rx_a,
        None, // No peer B receiver in P2P mode
    )
}

