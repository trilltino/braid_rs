# Braid-Iroh Code Integration

This document corresponds to the direct code connections between the Braid-HTTP protocol implementation and the Iroh peer-to-peer networking stack within the `braid_iroh` application.

## 1. Dependency Integration (`app/Cargo.toml`)

The application consumes both Iroh and Braid libraries as primary dependencies.

```toml
[dependencies]
# Iroh P2P Stack
iroh = { version = "0.96.1", features = ["test-utils"] }
iroh-gossip = { path = "../references/iroh-gossip", features = ["net"] }
iroh-h3-axum = { path = "../references/iroh-h3/iroh-h3-axum" }
iroh-h3-client = { path = "../references/iroh-h3/iroh-h3-client" }

# Braid Protocol (Core Logic)
braid_http_rs = { path = "../core", features = ["server", "client"] }
```

## 2. Node Initialization (`app/src/node.rs`)

The `BraidIrohNode` struct is the anchor point where Iroh's endpoint and Braid's protocol handler are fused.

### ALPN Negotiation
The node identifies itself using a specific Application-Layer Protocol Negotiation (ALPN) string during the QUIC handshake.

```rust
// app/src/node.rs
pub const BRAID_H3_ALPN: &[u8] = b"braid-h3/0";

// inside spawn()
let mut builder = Endpoint::builder().alpns(vec![
    BRAID_H3_ALPN.to_vec(),
    iroh_gossip::net::GOSSIP_ALPN.to_vec(),
]);
```

### Protocol Handler Mounting
The Braid logic is wrapped in an Axum router and then mounted onto the Iroh `Router`. This allows Iroh to accept incoming connections and route them to the Braid HTTP handler based on the ALPN.

```rust
// app/src/node.rs

// 1. Create Braid State
let app_state = BraidAppState { ... };

// 2. Build Braid Protocol Handler (Axum Router wrapped in IrohAxum)
let braid_handler = protocol::build_protocol_handler(app_state);

// 3. Mount on Iroh Router
let router = Router::builder(endpoint.clone())
    .accept(BRAID_H3_ALPN.to_vec(), braid_handler) // Handle Braid Traffic
    .accept(iroh_gossip::net::GOSSIP_ALPN.to_vec(), gossip.clone()) // Handle Gossip
    .spawn();
```

## 3. Protocol Implementation (`app/src/protocol.rs`)

The `protocol.rs` module defines how HTTP methods (GET, PUT) are mapped to Braid semantics using the `iroh-h3-axum` adapter, which translates Iroh's QUIC streams into Axum-compatible HTTP/3 requests.

### Router Setup
```rust
// app/src/protocol.rs
pub fn build_protocol_handler(state: BraidAppState) -> IrohAxum {
    let router = Router::new()
        .route("/{resource}", get(handle_get)) // Braid GET / Subscribe
        .route("/{resource}", put(handle_put)) // Braid PUT (Updates)
        .with_state(state);

    IrohAxum::new(router)
}
```

### Handling Updates (PUT)
When a peer sends a Braid update, the `handle_put` function processes it. It extracts Braid headers and broadcasts the update via Iroh Gossip.

```rust
// app/src/protocol.rs
async fn handle_put(..., headers: HeaderMap, body: String) -> impl IntoResponse {
    // 1. Parse Version and Data from Headers/Body
    let version = headers.get("version")...;
    
    // 2. Store Locally
    state.resources.write().await.push(update);

    // 3. Broadcast to Gossip Swarm
    state.subscriptions.broadcast(&url, &update).await;
}
```

## 4. Client & Subscriptions (`app/src/main.rs`)

The client side uses `IrohH3Client` to initiate connections to other peers, effectively acting as an HTTP/3 client over Iroh's p2p network.

### Sending Requests
```rust
// app/src/main.rs
let client = IrohH3Client::new(endpoint, b"braid-h3/0".to_vec());
let url = format!("https://{}/{}", target_peer_id, path);

// Subscription Request (GET with Subscribe: true)
let req = client
    .get(&url)
    .header("Subscribe", "true")
    .build()?;

let resp = req.send().await?;
```

## 5. Architecture Verification (True P2P)

The implementation has been verified to be strictly Peer-to-Peer without any central server, polling, or mock transport.

### No Central Server
- **Evidence**: `app/src/main.rs` constructs URLs using Peer IDs (`https://<peer_id>/...`) and resolves them using Iroh's Distributed Hash Table (DHT) and direct hole-punching via `IrohH3Client`.
- There are no hardcoded IP addresses or central relay servers in the application logic.

### No Polling (`setInterval`)
- **Evidence**: `app/src/subscription.rs` uses `iroh_gossip::net::Gossip`.
- The `subscribe` method returns a `GossipReceiver` stream.
- Updates are pushed to the application via `tokio` async tasks that await new messages on this stream.
- **Code Trace**: `GossipReceiver::await` -> `Iroh Network Event` -> `QUIC Stream` -> `UDP Packet`.

### Verified Code Path
1.  **Transport**: `IrohH3Client` initiates a QUIC connection directly to the target `EndpointId`.
2.  **Routing**: The connection is established via the `braid-h3/0` ALPN.
3.  **Delivery**: The `IrohAxum` handler receives the request and routes it to `handle_put` or `handle_get`.

## 6. Future Improvements

The current implementation utilizes basic Braid-HTTP methods for simplicity. The following enhancements can be implemented using existing methods in `core/types/update.rs` to improve User Experience (UX):

### Conflict Detection (Branching History)
- **Current**: Updates are treated as a linear timeline; last write wins.
- **Improvement**: Use `Update::with_parents(parents)` to explicitly state which version an update is based on.
- **Benefit**: The UI can detect concurrent edits (forks) and display a "Merge Conflict" warning instead of silently overwriting changes.

### Rich Text & Metadata Support
- **Current**: Content is assumed to be plain text.
- **Improvement**: Use `Update::with_content_type("text/markdown")` or `Update::with_header("Author", "Alice")`.
- **Benefit**: Enables the app to render Markdown, JSON, or other content types dynamically and attribute edits to specific users.

### Real-Time Collaboration (Delta Updates)
- **Current**: `Update::snapshot()` sends the entire file content on every keystroke/save.
- **Improvement**: Use `Update::patched()` to send only the changes (patches).
- **Benefit**: Reduces bandwidth usage and enables true real-time collaborative editing (Google Docs style) without blocking other users.

### Protocol Signaling
- **Current**: Updates are only for persistent state changes.
- **Improvement**: Use `Update::with_status(102)` (Processing) or custom codes.
- **Benefit**: Can be used to implement ephemeral signals like "Alice is typing..." indicators without polluting the version history.
