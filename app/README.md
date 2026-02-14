# App Component

This directory contains the main desktop application code, built with Tauri v2 and Rust.

## Purpose

The app serves as the user interface and the integration point for the Braid-Iroh P2P system. It bridges the frontend (HTML/JS) with the backend Rust logic.

## Structure

### src/

- main.rs
  - The entry point of the application.
  - Manages the Tauri application lifecycle.
  - Defines Tauri commands callable from the frontend.
  - Handles the initialization of the P2P node.

- node.rs
  - Encapsulates the logic for spawning and managing the Iroh node.
  - Configures the Iroh endpoint and gossip protocol.

- subscription.rs
  - Implements the Braid subscription logic.
  - Handles the flow of updates between peers.

- discovery.rs
  - Manages peer discovery mechanisms (e.g., local network discovery).

- proxy.rs
  - Handles proxying requests if applicable.

- p2p_init_clean.rs
  - Contains initialization logic for P2P networking.

## Frontend

The frontend assets are located in the `public` directory.

- public/index.html
  - The main user interface file.
  - Contains the logic for the "Update Demo" and "Subscription Demo" views.
