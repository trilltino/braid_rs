//! XDR Decoder

use crate::fs::nfs::legit_nfs::error::nfsstat3;

/// XDR Decoder for deserializing NFS data
pub struct XdrDecoder<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> XdrDecoder<'a> {
    /// Create a new decoder
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    
    /// Get remaining bytes
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    
    /// Check if at end
    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }
    
    /// Decode a u32 (big-endian)
    pub fn decode_u32(&mut self) -> Result<u32, nfsstat3> {
        if self.remaining() < 4 {
            return Err(nfsstat3::ERR_INVAL);
        }
        let val = u32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(val)
    }
    
    /// Decode a u64 (big-endian)
    pub fn decode_u64(&mut self) -> Result<u64, nfsstat3> {
        if self.remaining() < 8 {
            return Err(nfsstat3::ERR_INVAL);
        }
        let val = u64::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
            self.data[self.pos + 4],
            self.data[self.pos + 5],
            self.data[self.pos + 6],
            self.data[self.pos + 7],
        ]);
        self.pos += 8;
        Ok(val)
    }
    
    /// Decode an i32 (big-endian)
    pub fn decode_i32(&mut self) -> Result<i32, nfsstat3> {
        self.decode_u32().map(|v| v as i32)
    }
    
    /// Decode an i64 (big-endian)
    pub fn decode_i64(&mut self) -> Result<i64, nfsstat3> {
        self.decode_u64().map(|v| v as i64)
    }
    
    /// Decode a boolean
    pub fn decode_bool(&mut self) -> Result<bool, nfsstat3> {
        self.decode_u32().map(|v| v != 0)
    }
    
    /// Decode variable-length opaque data
    pub fn decode_opaque(&mut self) -> Result<Vec<u8>, nfsstat3> {
        let len = self.decode_u32()? as usize;
        self.decode_fixed_opaque(len)
    }
    
    /// Decode fixed-length opaque data
    pub fn decode_fixed_opaque(&mut self, len: usize) -> Result<Vec<u8>, nfsstat3> {
        let padded_len = ((len + 3) / 4) * 4; // Pad to 4-byte boundary
        if self.remaining() < padded_len {
            return Err(nfsstat3::ERR_INVAL);
        }
        let data = self.data[self.pos..self.pos + len].to_vec();
        self.pos += padded_len;
        Ok(data)
    }
    
    /// Decode a string
    pub fn decode_string(&mut self) -> Result<String, nfsstat3> {
        let bytes = self.decode_opaque()?;
        String::from_utf8(bytes).map_err(|_| nfsstat3::ERR_INVAL)
    }
    
    /// Decode an optional value
    pub fn decode_option<T, F>(&mut self, decode_fn: F) -> Result<Option<T>, nfsstat3>
    where
        F: FnOnce(&mut Self) -> Result<T, nfsstat3>,
    {
        let present = self.decode_bool()?;
        if present {
            Ok(Some(decode_fn(self)?))
        } else {
            Ok(None)
        }
    }
    
    /// Decode an enum
    pub fn decode_enum<T, F>(&mut self, convert: F) -> Result<T, nfsstat3>
    where
        F: FnOnce(u32) -> Option<T>,
    {
        let val = self.decode_u32()?;
        convert(val).ok_or(nfsstat3::ERR_INVAL)
    }
    
    /// Get raw bytes at current position
    pub fn peek_bytes(&self, len: usize) -> Option<&[u8]> {
        if self.remaining() >= len {
            Some(&self.data[self.pos..self.pos + len])
        } else {
            None
        }
    }
    
    /// Advance position
    pub fn advance(&mut self, len: usize) {
        self.pos = (self.pos + len).min(self.data.len());
    }
    
    /// Get current position
    pub fn pos(&self) -> usize {
        self.pos
    }
}

/// Trait for XDR-decodable types
pub trait XdrDecodable: Sized {
    fn decode(decoder: &mut XdrDecoder) -> Result<Self, nfsstat3>;
}
