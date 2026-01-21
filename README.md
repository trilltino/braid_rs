# braid_rs

A unified Rust implementation of the [Braid Protocol](https://braid.org/) for building decentralized, synchronized web applications.

[![Crates.io](https://img.shields.io/crates/v/braid_rs.svg)](https://crates.io/crates/braid_rs)
[![Documentation](https://docs.rs/braid_rs/badge.svg)](https://docs.rs/braid_rs)
[![License](https://img.shields.io/crates/l/braid_rs.svg)](LICENSE)

## Features

| Feature | Description |
|---------|-------------|
| **Core Braid-HTTP** | Full protocol implementation with SSE subscriptions |
| **Antimatter CRDT** | JS-compatible CRDT with history compression |
| **MergeType System** | Pluggable merge algorithms (Sync9, Antimatter, Diamond) |
| **Braid-Blob** | Content-addressable storage with SHA-256 hashing |
| **BraidFS** | Filesystem sync daemon with .braidignore support |

## Quick Start

```toml
[dependencies]
braid_rs = "0.1.0"
```

### Braid-HTTP Client

```rust
use braid_rs::core::client::BraidClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = BraidClient::new();
    
    // Simple GET request
    let response = client.get("http://braid.org/resource").await?;
    println!("Content: {:?}", response.body());
    
    // Subscribe to updates
    let mut subscription = client.subscribe("http://braid.org/resource").await?;
    while let Some(update) = subscription.next().await {
        println!("Update: version={:?}", update.version);
    }
    
    Ok(())
}
```

### Antimatter CRDT

```rust
use braid_rs::antimatter::{AntimatterCrdt, PrunableCrdt};
use braid_rs::antimatter::messages::Patch;
use serde_json::json;

// Create CRDT with send callback
let mut crdt = AntimatterCrdt::new(
    Some("peer1".to_string()),
    MyCrdt::new(),
    Box::new(|msg| { /* send to other peers */ }),
);

// Apply local edit
let version = crdt.update(vec![Patch {
    range: "0:0".to_string(),
    content: json!("Hello"),
}]);

// Get current state
println!("Content: {}", crdt.crdt.get_content());
println!("Version: {:?}", crdt.current_version);

// Prune history when acknowledged
crdt.prune(false);
```

### MergeType System

```rust
use braid_rs::core::merge::{MergeType, MergeTypeRegistry, MergePatch};
use serde_json::json;

// Create registry with built-in types
let mut registry = MergeTypeRegistry::new();

// Get a merge type by name
let mut merge = registry.create("sync9", "peer1").unwrap();

// Initialize with content
merge.initialize("Hello World");

// Apply local edit
let patch = MergePatch::new("0:5", json!("Hi"));
let result = merge.local_edit(patch);

println!("New version: {:?}", result.version);
println!("Content: {}", merge.get_content());
```

### Axum Server Integration

```rust
use axum::{Router, routing::get};
use braid_rs::core::server::{BraidLayer, BraidState};

async fn handler() -> impl IntoResponse {
    "Hello from Braid!"
}

let app = Router::new()
    .route("/resource", get(handler))
    .layer(BraidLayer::new().middleware());
```

## Architecture

```
braid_rs/
├── src/
│   ├── core/           # Braid-HTTP protocol
│   │   ├── client/     # HTTP client with subscriptions
│   │   ├── server/     # Axum middleware & handlers
│   │   ├── merge/      # MergeType trait & implementations
│   │   └── protocol/   # Protocol constants & types
│   │
│   ├── antimatter/     # Antimatter CRDT (JS-compatible)
│   │   ├── antimatter.rs    # Main coordination layer
│   │   ├── sequence_crdt.rs # Text/array CRDT
│   │   ├── json_crdt.rs     # Recursive JSON CRDT
│   │   └── messages.rs      # Protocol messages
│   │
│   ├── blob/           # Content-addressable storage
│   │   ├── store.rs    # SQLite metadata + FS storage
│   │   └── http.rs     # Axum HTTP endpoints
│   │
│   └── fs/             # Filesystem sync daemon
│       ├── mod.rs      # Daemon & file watching
│       └── config.rs   # .braidignore, debouncing
```

## Binaries

### braidfs

Sync local files with a Braid server:

```bash
# Start the daemon
braidfs run

# Add a sync mapping
braidfs sync ./my-folder http://example.com/resource
```

### braid-blob

Run a standalone blob server:

```bash
braid-blob serve --port 8080 --data ./data
```

## Configuration

### .braidignore

Place a `.braidignore` file in synced directories:

```
# Ignore git files
.git
.git/**

# Ignore dependencies
node_modules/**

# Ignore temp files
*.swp
*.swo
*~
```

### BraidFS Config

Location: `~/.braidfs/config`

```json
{
  "port": 45678,
  "debounce_ms": 100,
  "sync": {
    "./project": true
  },
  "ignore_patterns": [".git/**", "node_modules/**"]
}
```

## Protocol Compliance

This implementation follows [draft-toomim-httpbis-braid-http-04](https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http):

| Header | Support |
|--------|---------|
| `Subscribe: true` | ✅ |
| `Version` | ✅ |
| `Parents` | ✅ |
| `Merge-Type` | ✅ |
| `Patches` | ✅ |
| `Content-Range` | ✅ |

## License

Dual-licensed under Apache 2.0 or MIT at your option.

## Links

- [Braid Specification](https://braid.org/spec)
- [Antimatter Algorithm](https://braid.org/antimatter)
- [Braid-HTTP Draft](https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http)
