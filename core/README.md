# Core Component

This directory contains the core logic and backend implementation for the Braid protocol.

## Purpose

The core library provides the fundamental building blocks for the Braid system, handling low-level operations such as filesystem interactions, data storage, and node services.

## Structure

### src/

- fs/
  - Implements the filesystem abstraction layer.
  - Handles file operations and directory management.

- blob/
  - Manages data blob storage and retrieval.
  - Handles the underlying data structures for content addressing.

- node.rs
  - Contains the core node logic and service definitions.
  - Acts as the backend service layer for the application.

- bin/
  - Contains executable binaries for standalone backend processes.

- vendor/
  - Contains vendored dependencies or specific external code.
