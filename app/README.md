# Braid-Iroh

Braid-HTTP over Iroh P2P networking — decentralized state synchronization with real-time debug visibility.

## Quick Start (One Script)

### Windows (Command Prompt)
```cmd
cd braid_iroh
run.bat
```

### Windows (PowerShell)
```powershell
cd braid_iroh
.\run.ps1
```

That's it! The script will:
- Compile and run the application
- Show real-time P2P debug output with color coding
- Display a legend explaining what each color means

## Usage Modes

```cmd
:: Run the application (default)
run.bat

:: Run tests with debug output
run.bat test

:: Run the two-peers example (no browser needed)
run.bat example

:: Run with cargo-leptos (if installed)
run.bat leptos
```

## Debug Output Explained

When you run the script, you'll see color-coded output:

- **C** (Cyan) - Connection events: QUIC handshake, peer connects, endpoint binding
- **G** (Magenta) - Gossip events: topic subscriptions, broadcasts, membership changes
- **P** (Green) - PUT operations: saving/updating data, versioning, broadcasting to peers
- **R** (Yellow) - GET operations: retrieving data, HTTP/3 requests
- **E** (Red) - Errors and failures: connection failures, protocol errors
- **I** (Gray) - General information

### Example Session

```
[C] Endpoint bound id=z6Mkf...x9Qr              <- Your peer identity
[G] Subscribing to topic=7f3a...e2b1            <- Joining gossip topic
[P] PUT /demo-doc version=v2                    <- You typed something
[P] Broadcasting via gossip                     <- Sending to peers
[G] Received gossip message from=z6Mkh...      <- Peer received it
```

## Project Structure

```
braid_iroh/
├── run.bat              <- Main script (CMD)
├── run.ps1              <- Main script (PowerShell)
├── src/
│   ├── lib.rs           # Public API
│   ├── main.rs          # Entry point
│   ├── node.rs          # BraidIrohNode (main type)
│   ├── discovery.rs     # Peer discovery
│   ├── subscription.rs  # Gossip subscriptions
│   └── protocol.rs      # HTTP/3 protocol handler
├── tests/               # Integration tests
│   ├── test_node.rs
│   ├── test_two_peers.rs
│   └── ...
├── examples/
│   └── two_peers_debug.rs  # Standalone demo
└── TESTING_PLAN.md      # Testing documentation
```

## Manual Build (Without Script)

```bash
# Run with debug logging
set RUST_LOG=braid_iroh=debug,iroh_gossip=debug
cargo run

# Or run tests
cargo test

# Run the example
cargo run --example two_peers_debug
```

## Running Tests

```cmd
:: Run all tests
run.bat test

:: Or with cargo directly
cargo test
```

## Documentation

- `TESTING_PLAN.md` — Comprehensive testing strategy
- `braid_iroh_plan.md` — Original architecture plan

## Architecture Overview

```
+-------------------------------------------------------------+
|                       braid_iroh                            |
|  +------------+    +----------+    +---------------------+  |
|  | braid_http |<-->  iroh-h3 |<--> | iroh (QUIC P2P)     |  |
|  |  (types)   |    | (HTTP/3) |    | - endpoints         |  |
|  +------------+    +----------+    | - gossip topics     |  |
|                                    | - peer discovery    |  |
|  +-----------------+               +---------------------+  |
|  |  iroh-gossip    |                                         |
|  |  (broadcast)    |                                         |
|  +-----------------+                                         |
+-------------------------------------------------------------+
```

## License

MIT OR Apache-2.0
