//! XDR Encoder

use crate::fs::nfs::legit_nfs::error::nfsstat3;

/// Trait for XDR-encodable types
pub trait XdrEncodable {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3>;
}

/// XDR Encoder for serializing NFS data
pub struct XdrEncoder {
    buffer: Vec<u8>,
}

impl XdrEncoder {
    /// Create a new encoder
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Get the encoded bytes
    pub fn into_bytes(self) -> Vec<u8> {
        self.buffer
    }

    /// Get current position
    pub fn pos(&self) -> usize {
        self.buffer.len()
    }

    /// Encode a u32 (big-endian)
    pub fn encode_u32(&mut self, val: u32) {
        self.buffer.extend_from_slice(&val.to_be_bytes());
    }

    /// Encode a u64 (big-endian)
    pub fn encode_u64(&mut self, val: u64) {
        self.buffer.extend_from_slice(&val.to_be_bytes());
    }

    /// Encode an i32 (big-endian)
    pub fn encode_i32(&mut self, val: i32) {
        self.buffer.extend_from_slice(&val.to_be_bytes());
    }

    /// Encode an i64 (big-endian)
    pub fn encode_i64(&mut self, val: i64) {
        self.buffer.extend_from_slice(&val.to_be_bytes());
    }

    /// Encode a boolean (as u32)
    pub fn encode_bool(&mut self, val: bool) {
        self.encode_u32(if val { 1 } else { 0 });
    }

    /// Encode variable-length opaque data (with length prefix)
    pub fn encode_opaque(&mut self, data: &[u8]) {
        self.encode_u32(data.len() as u32);
        self.buffer.extend_from_slice(data);
        // Pad to 4-byte boundary
        let padding = (4 - (data.len() % 4)) % 4;
        self.buffer.extend(std::iter::repeat(0).take(padding));
    }

    /// Encode a string
    pub fn encode_string(&mut self, s: &str) {
        self.encode_opaque(s.as_bytes());
    }

    /// Encode fixed-length opaque data (no length prefix, still padded)
    pub fn encode_fixed_opaque(&mut self, data: &[u8]) {
        self.buffer.extend_from_slice(data);
        // Pad to 4-byte boundary
        let padding = (4 - (data.len() % 4)) % 4;
        self.buffer.extend(std::iter::repeat(0).take(padding));
    }

    /// Encode an enum (as u32)
    pub fn encode_enum<T: Into<u32>>(&mut self, val: T) {
        self.encode_u32(val.into());
    }

    /// Encode an optional value (bool + value if present)
    pub fn encode_option<T: XdrEncodable>(&mut self, opt: &Option<T>) -> Result<(), nfsstat3> {
        if let Some(val) = opt {
            self.encode_bool(true);
            val.encode(self)?;
        } else {
            self.encode_bool(false);
        }
        Ok(())
    }

    /// Encode a list/array
    pub fn encode_list<T: XdrEncodable>(&mut self, items: &[T]) -> Result<(), nfsstat3> {
        for item in items {
            self.encode_bool(true); // Next item follows
            item.encode(self)?;
        }
        self.encode_bool(false); // End of list
        Ok(())
    }
}

impl Default for XdrEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl XdrEncodable for u32 {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3> {
        encoder.encode_u32(*self);
        Ok(())
    }
}

impl XdrEncodable for u64 {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3> {
        encoder.encode_u64(*self);
        Ok(())
    }
}

impl XdrEncodable for String {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3> {
        encoder.encode_string(self);
        Ok(())
    }
}

impl XdrEncodable for Vec<u8> {
    fn encode(&self, encoder: &mut XdrEncoder) -> Result<(), nfsstat3> {
        encoder.encode_opaque(self);
        Ok(())
    }
}
