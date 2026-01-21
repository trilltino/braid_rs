//! Protocol-level utilities for Braid-HTTP.
//!
//! This module contains shared protocol logic used by both client and server
//! implementations. It provides a centralized location for protocol constants,
//! header parsing/formatting, and other protocol-level operations.
//!
//! # Module Organization
//!
//! ```text
//! protocol/
//! ├── constants   - Status codes, header names, merge types
//! ├── headers     - Header parsing and formatting utilities
//! └── parser      - HeaderParser type for structured access
//! ```
//!
//! # Key Components
//!
//! | Component | Description |
//! |-----------|-------------|
//! | [`STATUS_SUBSCRIPTION`] | HTTP 209 status code |
//! | [`STATUS_MERGE_CONFLICT`] | HTTP 293 status code |
//! | [`parse_version_header`] | Parse Version header |
//! | [`format_version_header`] | Format Version header |
//! | [`parse_content_range`] | Parse Content-Range header |
//! | [`HeaderParser`] | Structured header parsing API |
//!
//! # Design Philosophy
//!
//! This module eliminates code duplication between client and server.
//! All protocol-level operations should go through this module to ensure
//! consistency and maintainability.
//!
//! # Examples
//!
//! ## Using Protocol Constants
//!
//! ```
//! use crate::core::protocol;
//! use crate::core::protocol::constants::headers;
//!
//! // Check for subscription response
//! let status = 209u16;
//! if status == protocol::STATUS_SUBSCRIPTION {
//!     println!("Subscription response");
//! }
//!
//! // Use header name constants
//! let header_name = headers::VERSION;
//! assert_eq!(header_name, "Version");
//! ```
//!
//! ## Parsing Headers
//!
//! ```
//! use crate::core::protocol;
//!
//! // Parse version header
//! let versions = protocol::parse_version_header(r#""v1", "v2""#).unwrap();
//! assert_eq!(versions.len(), 2);
//!
//! // Parse content range
//! let (unit, range) = protocol::parse_content_range("json .field").unwrap();
//! assert_eq!(unit, "json");
//! ```
//!
//! ## Formatting Headers
//!
//! ```
//! use crate::core::{Version, protocol};
//!
//! let versions = vec![Version::new("v1"), Version::new("v2")];
//! let header_value = protocol::format_version_header(&versions);
//! assert_eq!(header_value, r#""v1", "v2""#);
//! ```
//!
//! ## Using HeaderParser
//!
//! ```
//! use crate::core::protocol::HeaderParser;
//!
//! let versions = HeaderParser::parse_version(r#""v1""#).unwrap();
//! let header = HeaderParser::format_version(&versions);
//! ```
//!
//! # Specification
//!
//! This module implements header formats from [draft-toomim-httpbis-braid-http-04]
//! and [RFC 8941 Structured Headers].
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http
//! [RFC 8941 Structured Headers]: https://datatracker.ietf.org/doc/html/rfc8941

pub mod constants;
pub mod formatter;
pub mod headers;
pub mod multiplex;
pub mod parser;

pub use constants::*;
pub use formatter::*;
pub use headers::*;
pub use parser::*;
