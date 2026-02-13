//! NFSv3 protocol implementation

pub mod consts;
pub mod handlers;
pub mod procedures;
pub mod types;

pub use types::*;
// Re-export XDR traits so encode/decode methods are available
pub use crate::fs::nfs::legit_nfs::xdr::{XdrDecodable, XdrEncodable, XdrEncoder, XdrDecoder};
