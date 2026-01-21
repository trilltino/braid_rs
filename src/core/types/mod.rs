//! Core data types for the Braid-HTTP protocol.
//!
//! This module defines the fundamental types used throughout the Braid-HTTP protocol,
//! providing a type-safe Rust API for building state synchronization applications.
//!
//! # Overview
//!
//! Braid-HTTP extends HTTP from a state *transfer* protocol into a state *synchronization*
//! protocol. This module provides the core types that enable:
//!
//! - **Version tracking** via directed acyclic graphs (DAGs)
//! - **Incremental updates** through patches instead of full snapshots
//! - **Real-time subscriptions** for streaming updates
//! - **Conflict resolution** through merge-type specifications
//!
//! # Type Hierarchy
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                        BraidRequest                         │
//! │  (Client → Server: version, parents, subscribe, patches)   │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                          Update                             │
//! │  (State mutation: version, parents, body OR patches)        │
//! │  ├── Snapshot: complete state representation                │
//! │  └── Patched: incremental changes via Patch list            │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                       BraidResponse                         │
//! │  (Server → Client: status, headers, body, subscription)     │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Core Types
//!
//! | Type | Description | Spec Section |
//! |------|-------------|--------------|
//! | [`Version`] | Unique identifier for a point in resource history | Section 2 |
//! | [`Update`] | State mutation (snapshot or patches) | Section 3 |
//! | [`Patch`] | Incremental change to a specific range | Section 3.1-3.3 |
//! | [`ContentRange`] | Range specification for patches | Section 3.1 |
//! | [`BraidRequest`] | Client request with Braid headers | Sections 2-4 |
//! | [`BraidResponse`] | Server response with Braid headers | Sections 2-4 |
//!
//! # Versioning (Section 2)
//!
//! Each resource has a version history represented as a Directed Acyclic Graph (DAG).
//! Versions enable:
//!
//! - **History tracking**: Know exactly what state a client has
//! - **Concurrent editing**: Multiple parents indicate merged branches
//! - **Conflict detection**: Identify when clients diverge
//!
//! ```
//! use crate::core::Version;
//!
//! // String-based versions (most common)
//! let v1 = Version::new("abc123");
//! let v2 = Version::new("def456");
//!
//! // Integer-based versions
//! let v3 = Version::Integer(42);
//! ```
//!
//! # Updates (Section 3)
//!
//! Updates transform resources from one state to another. They can be:
//!
//! - **Snapshots**: Complete representation of the new state
//! - **Patches**: Incremental changes to specific ranges
//!
//! ```
//! use crate::core::{Update, Version, Patch};
//!
//! // Snapshot update (complete state)
//! let snapshot = Update::snapshot(
//!     Version::new("v2"),
//!     r#"{"name": "Alice", "age": 30}"#
//! ).with_parent(Version::new("v1"));
//!
//! // Patch update (incremental change)
//! let patched = Update::patched(
//!     Version::new("v3"),
//!     vec![Patch::json(".age", "31")]
//! ).with_parent(Version::new("v2"));
//! ```
//!
//! # Patches (Section 3.1-3.3)
//!
//! Patches specify changes to specific ranges within a resource:
//!
//! ```
//! use crate::core::Patch;
//!
//! // JSON patch (JSONPath-like addressing)
//! let json_patch = Patch::json(".users[0].name", r#""Bob""#);
//!
//! // Byte range patch
//! let byte_patch = Patch::bytes("100:200", &b"new content"[..]);
//!
//! // Text patch
//! let text_patch = Patch::text(".title", "New Title");
//! ```
//!
//! # Requests and Responses
//!
//! Build requests with the fluent builder pattern:
//!
//! ```
//! use crate::core::{BraidRequest, Version};
//!
//! let request = BraidRequest::new()
//!     .with_version(Version::new("v1"))
//!     .with_parent(Version::new("v0"))
//!     .subscribe()
//!     .with_heartbeat(30)
//!     .with_merge_type("diamond");
//! ```
//!
//! # Specification References
//!
//! This module implements types from [draft-toomim-httpbis-braid-http-04]:
//!
//! - **Section 2**: Versioning - Version IDs, Parents, DAG structure
//! - **Section 2.2**: Merge-Types - Conflict resolution strategies
//! - **Section 3**: Updates as Patches - Patch format and Content-Range
//! - **Section 3.3**: Multi-Patch Updates - Multiple patches per response
//! - **Section 4**: Subscriptions - Subscribe header and HTTP 209
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http

mod version;
mod content_range;
mod patch;
mod update;
mod request;
mod response;

pub use version::Version;
pub use content_range::ContentRange;
pub use patch::Patch;
pub use update::Update;
pub use request::BraidRequest;
pub use response::BraidResponse;

