# Braid-Iroh Application Source Code

This directory (`app/src`) contains the core Rust implementation of the Braid-Iroh Tauri application.

## Core Modules

- **`main.rs`**: The application entry point. It initializes the Tauri runtime, sets up the application state (`AppState`), and handles native Tauri commands for frontend interaction. It orchestrates the `BraidIrohNode` instances (simulating Peer A and Peer B locally).

- **`node.rs`**: Defines the `BraidIrohNode` struct, which encapsulates an Iroh `Endpoint` and manages the Braid protocol integration. It handles:
  - Binding the Iroh endpoint with ALPN negotiation.
  - Setting up the Gossip network.
  - Managing subscriptions (`SubscriptionManager`).
  - Storing and retrieving resource updates (`Update`).
  - `put` and `get` operations for Braid resources.

- **`protocol.rs`**: Implements the Braid-HTTP protocol handler for Iroh connections. It uses `IrohAxum` to bridge standard Axum routes to the Iroh QUIC transport.
  - Handle `GET` requests (latest state or history).
  - Handle `PUT` requests (new updates via Braid protocol).
  - Broadcast updates via Gossip.

- **`discovery.rs`**: Manages peer discovery.
  - `MockDiscoveryMap`: An in-memory registry for local simulation (peers finding each other without external network).
  - `DiscoveryConfig`: Abstract over Mock vs. Real discovery strategies.

- **`subscription.rs`**: Manages resource subscriptions over Iroh Gossip. It handles joining gossip topics and broadcasting updates to subscribed peers.

- **`proxy.rs`**: (If enabled) Provides a TCP proxy bridge, allowing standard HTTP tools (like curl or browsers) to interact with resources hosted on the Iroh network.

## Architecture Overview

The application follows a "Local-First P2P" architecture:
1.  **Nodes**: Each instance runs a full Iroh node capable of serving and consuming content.
2.  **Transport**: Braid-HTTP runs over HTTP/3 on top of Iroh's QUIC connections.
3.  **Synchronization**: Updates are propagated via Iroh Gossip to subscribed peers.
4.  **State**: Changes are stored locally in-memory (for this demo/prototype) and reconciled via Braid version vectors.
