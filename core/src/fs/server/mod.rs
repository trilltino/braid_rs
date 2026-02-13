//! HTTP API server - admin and file serving
//!
//! This module provides the HTTP API for BraidFS management.

pub mod handlers;
pub mod router;

pub use router::run_server;
