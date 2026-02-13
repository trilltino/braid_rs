//! XDR (External Data Representation) codec for NFS

pub mod decoder;
pub mod encoder;

pub use decoder::{XdrDecodable, XdrDecoder};
pub use encoder::{XdrEncodable, XdrEncoder};
