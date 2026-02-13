# Braid-Iroh P2P

This project implements a decentralized synchronization system by layering the Braid-HTTP protocol over the Iroh peer-to-peer network stack. It enables direct, serverless state synchronization between peers using standard HTTP semantics (GET, PUT, PATCH) tunneled through QUIC connections.

## Braid and Iroh Integration

The core architecture leverages Iroh for peer discovery, connectivity, and NAT traversal, providing a secure substrate for p2p communication. Iroh handles the underlying transport (QUIC) and provides a content-addressable storage implementation.

On top of this network layer, Braid-HTTP is implemented to manage state synchronization. When a peer updates a resource (PUT), the change is versioned and a delta is generated. This update is then propagated to subscribers. The system supports:
- **Subscriptions**: Peers establish persistent subscriptions to resources (GET with Subscribe: true).
- **Differential Synchronization**: Updates are transmitted as patches rather than full state, minimizing bandwidth usage.
- **Version History**: Each resource maintains a graph of versions, allowing peers to traverse history and resolve concurrent edits.
- **Gossip**: Updates are broadcast via Iroh's gossip protocol to ensure efficient distribution among interested peers.

The integration maps HTTP methods to Iroh's request/response mechanism, treating the p2p network as a distributed HTTP server where every node is both a client and a host.

## Applications

### Update Demo
A bidirectional synchronization demonstration where two peers (Alice and Bob) can modify a shared text document. Changes made by one peer are instantly reflected in the other's view. This mode showcases the real-time propagation of edits (PUT) and the handling of incoming updates (PATCH/SUBSCRIBE).

### Subscription Demo
A publisher-subscriber model demonstration. One peer (Alice) acts as the publisher of a resource stream, while the other (Bob) subscribes to it. It highlights the ability to stream updates over time, view historical versions of the resource, and resume synchronization from a disconnect.

## Technology Stack

- **Language**: Rust (Application Logic & Systems Programming)
- **Frontend**: HTML5, JavaScript (No Frameworks), CSS
- **GUI Framework**: Tauri v2 (Native WebView integration)
- **Networking**: Iroh (P2P, QUIC, Gossip, Direct Connections)
- **Protocol**: Braid-HTTP (State Synchronization, Versioning)
- **Build System**: Cargo (Rust), Github Actions (CI/CD)

## Running the Demo

The application includes a built-in launcher for easy testing.

1.  **Double-click `braid_iroh.exe`**: The application will automatically spawn two separate instances ("Alice" and "Bob") in their own terminal windows.
2.  **Manual Mode**: You can also run individual nodes manually via command line:
    ```bash
    ./braid_iroh --p2p --name alice
    ./braid_iroh --p2p --name bob
    ```
