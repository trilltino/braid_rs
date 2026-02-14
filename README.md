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

## Using the Demo

Once the two windows (Alice and Bob) are open, follow these steps to test the functionality.

### 1. Update Demo (Bidirectional Sync)
This mode tests real-time P2P updates.

1.  Click **Update Demo** in both Alice and Bob windows.
2.  **Connect Peers**:
    - Copy Bob's **Node ID** (displayed at the top of his window).
    - Paste it into Alice's **Connect** input field at the top right.
    - Click **Connect**. The status should change to "Connected".
3.  **Test Sync**:
    - Type in Alice's text box (Left Panel).
    - Watch the text appear instantly in Bob's read-only text box (Left Panel).
    - Type in Bob's text box (Right Panel).
    - Watch the text appear in Alice's read-only text box (Right Panel).

### 2. Subscription Demo (Pub/Sub)
This mode tests resource subscriptions and version history.

1.  Click **Subscribe Demo** in both windows.
2.  **Alice (Publisher)**:
    - In the Left Panel ("Me - Publisher"), enter a **Resource Name** (e.g., `/chat`).
    - Type content into the text area. This content is now being published.
3.  **Bob (Subscriber)**:
    - Ensure you are connected to Alice (if not, use the Connect box at top right).
    - In the Right Panel ("Me - Subscriber"), enter the *same* **Resource Name** (`/chat`).
    - Click **Subscribe**.
4.  **Verify**:
    - Bob's text area should update with Alice's content.
    - As Alice types, Bob receives updates in real-time.
    - Click **History** on Bob's side to view and load previous versions of the content.
