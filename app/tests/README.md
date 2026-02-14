# Braid-Iroh Integration Tests

This directory (`app/tests`) contains integration tests for verifying the Braid protocol behavior and P2P interactions in the Braid-Iroh application.

## Test Structure

- **`common/`**: Shared test utilities and helper functions. This includes:
  - Tracing initialization (`init_test_tracing`).
  - Helper functions for creating test nodes (`create_test_node`).
  - Simplified setup for two-peer scenarios (`setup_two_peers`).

- **`test_two_peers.rs`**: The primary integration test suite. It verifies core P2P functionality by simulating two peers (Alice and Bob) in various scenarios:
  - **Connection**: Verifies peers can discover and connect to each other.
  - **Gossip Propagation**: Peer A `PUT`s an update, and Peer B receives it via gossip subscription.
  - **Bi-directional Updates**: Both peers update the same resource concurrently.
  - **Resource Isolation**: Ensuring updates to one resource don't leak to subscriptions on another.
  - **Reconnection**: Verifying state recovery and connectivity after a peer restarts.

## Running Tests

To run the integration tests for the `app` crate:

```bash
cargo test --test test_two_peers
```

Or to run all tests in the workspace:

```bash
cargo test --workspace
```
