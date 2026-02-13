//! Merge algorithms and CRDT implementations for conflict resolution.
//!
//! This module provides merge strategies for resolving concurrent edits to the same resource
//! when multiple clients make simultaneous mutations.
//!
//! # Merge-Types in Braid-HTTP
//!
//! | Merge Type | Description |
//! |------------|-------------|
//! | `"simpleton"` | Simple text-based merge |
//! | `"diamond"` | Diamond-types CRDT for text documents |
//! | Custom | Application-defined merge algorithms |
//!
//! # Key Types
//!
//! | Type | Description |
//! |------|-------------|
//! | [`MergeType`] | Trait for pluggable merge algorithms |
//! | [`SimpletonMergeType`] | Simple text-based merge |
//! | [`DiamondCRDT`] | High-performance text CRDT |
//! | [`DiamondMergeType`] | Diamond merge type adapter |
//! | [`MergeTypeRegistry`] | Factory for creating merge type instances |

pub mod merge_type;
pub mod simpleton;

#[cfg(not(target_arch = "wasm32"))]
pub mod diamond;

// Re-exports
pub use merge_type::{MergePatch, MergeResult, MergeType, MergeTypeRegistry};
pub use simpleton::SimpletonMergeType;

#[cfg(not(target_arch = "wasm32"))]
pub use diamond::DiamondCRDT;
