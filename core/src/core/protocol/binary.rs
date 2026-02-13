//! Binary encoding support for Braid protocol.
//!
//! This module provides support for binary encoding blocks used by Diamond Types
//! and other binary CRDT formats.
//!
//! # Encoding Block Format
//!
//! Binary encoding blocks start with an "Encoding" header block instead of
//! standard HTTP headers:
//!
//! ```text
//! Encoding: dt
//! Length: 411813
//! <binary dt file>
//! ```
//!
//! This format allows efficient binary transmission of CRDT operations.

use bytes::Bytes;

/// Represents a binary encoding block.
#[derive(Clone, Debug, PartialEq)]
pub struct EncodingBlock {
    /// The encoding type (e.g., "dt" for diamond-types)
    pub encoding: String,
    /// The length of the binary data in bytes
    pub length: usize,
    /// The binary data
    pub data: Bytes,
}

impl EncodingBlock {
    /// Create a new encoding block.
    pub fn new(encoding: impl Into<String>, data: impl Into<Bytes>) -> Self {
        let data = data.into();
        let length = data.len();
        Self {
            encoding: encoding.into(),
            length,
            data,
        }
    }

    /// Parse an encoding block from bytes.
    ///
    /// # Format
    ///
    /// ```text
    /// Encoding: <type>\r\n
    /// Length: <length>\r\n
    /// \r\n
    /// <data>
    /// ```
    ///
    /// # Returns
    ///
    /// Returns `Some((EncodingBlock, remaining_bytes))` if parsing succeeds,
    /// or `None` if the input is incomplete or invalid.
    pub fn parse(input: &[u8]) -> Option<(Self, &[u8])> {
        // Find end of header block (double CRLF)
        let header_end = find_double_crlf(input)?;

        // Parse headers
        let header_str = std::str::from_utf8(&input[..header_end]).ok()?;
        let headers = parse_encoding_headers(header_str)?;

        let encoding = headers.get("encoding")?.clone();
        let length: usize = headers.get("length")?.parse().ok()?;

        // Check if we have enough data
        let data_start = header_end + 4; // Skip \r\n\r\n
        if input.len() < data_start + length {
            return None; // Incomplete data
        }

        let data = Bytes::copy_from_slice(&input[data_start..data_start + length]);
        let remaining = &input[data_start + length..];

        Some((
            Self {
                encoding,
                length,
                data,
            },
            remaining,
        ))
    }

    /// Serialize the encoding block to bytes.
    pub fn to_bytes(&self) -> Bytes {
        let mut result = Vec::new();
        result.extend_from_slice(format!("Encoding: {}\r\n", self.encoding).as_bytes());
        result.extend_from_slice(format!("Length: {}\r\n", self.length).as_bytes());
        result.extend_from_slice(b"\r\n");
        result.extend_from_slice(&self.data);
        Bytes::from(result)
    }

    /// Returns true if this is a Diamond Types encoding.
    pub fn is_diamond_types(&self) -> bool {
        self.encoding == "dt"
    }
}

/// Find the position of double CRLF (\r\n\r\n) in the input.
fn find_double_crlf(input: &[u8]) -> Option<usize> {
    for i in 3..input.len() {
        if input[i - 3] == b'\r'
            && input[i - 2] == b'\n'
            && input[i - 1] == b'\r'
            && input[i] == b'\n'
        {
            return Some(i - 3);
        }
    }
    None
}

/// Parse encoding block headers.
fn parse_encoding_headers(header_str: &str) -> Option<std::collections::HashMap<String, String>> {
    let mut headers = std::collections::HashMap::new();

    for line in header_str.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let (name, value) = line.split_once(':')?;
        headers.insert(name.trim().to_lowercase(), value.trim().to_string());
    }

    Some(headers)
}

/// Check if input looks like an encoding block (starts with "Encoding:").
pub fn is_encoding_block(input: &[u8]) -> bool {
    input.starts_with(b"Encoding:") || input.starts_with(b"encoding:")
}

/// Parse either an encoding block or regular headers.
///
/// This is the main entry point for parsing. It detects whether the input
/// is a binary encoding block or regular HTTP-style headers.
pub fn parse_message(input: &[u8]) -> MessageParseResult<'_> {
    if is_encoding_block(input) {
        match EncodingBlock::parse(input) {
            Some((block, remaining)) => MessageParseResult::EncodingBlock(block, remaining),
            None => MessageParseResult::Incomplete,
        }
    } else {
        // Regular headers - not handled here
        MessageParseResult::RegularHeaders
    }
}

/// Result of parsing a message.
pub enum MessageParseResult<'a> {
    /// A binary encoding block was parsed.
    EncodingBlock(EncodingBlock, &'a [u8]),
    /// Regular HTTP-style headers (parse elsewhere).
    RegularHeaders,
    /// Input is incomplete, need more data.
    Incomplete,
}

/// Encode a u64 as a variable-length integer (varint).
/// Uses LEB128 encoding (Little Endian Base 128).
pub fn encode_varint(value: u64) -> Vec<u8> {
    let mut result = Vec::new();
    let mut value = value;

    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        result.push(byte);
        if value == 0 {
            break;
        }
    }

    result
}

/// Decode a varint from bytes.
/// Returns (value, bytes_consumed) or an error if the input is invalid.
pub fn decode_varint(bytes: &[u8]) -> crate::core::error::Result<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift = 0;
    let mut consumed = 0;

    for &byte in bytes {
        consumed += 1;
        let value = (byte & 0x7f) as u64;

        // Check for overflow
        if shift >= 64 {
            return Err(crate::core::error::BraidError::Protocol(
                "Varint overflow".to_string(),
            ));
        }

        result |= value << shift;

        if byte & 0x80 == 0 {
            return Ok((result, consumed));
        }

        shift += 7;
    }

    Err(crate::core::error::BraidError::Protocol(
        "Incomplete varint".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Encoding Block Tests ==========

    #[test]
    fn test_encoding_block_roundtrip() {
        let data = Bytes::from(vec![0x01, 0x02, 0x03, 0x04, 0x05]);
        let block = EncodingBlock::new("dt", data.clone());

        let bytes = block.to_bytes();
        let (parsed, remaining) = EncodingBlock::parse(&bytes).unwrap();

        assert_eq!(parsed.encoding, "dt");
        assert_eq!(parsed.length, 5);
        assert_eq!(parsed.data, data);
        assert!(remaining.is_empty());
    }

    #[test]
    fn test_is_encoding_block() {
        assert!(is_encoding_block(b"Encoding: dt\r\n"));
        assert!(is_encoding_block(b"encoding: dt\r\n"));
        assert!(!is_encoding_block(b"HTTP/1.1 200 OK\r\n"));
        assert!(!is_encoding_block(b"Version: v1\r\n"));
    }

    #[test]
    fn test_parse_encoding_block() {
        let input = b"Encoding: dt\r\nLength: 4\r\n\r\nABCDmore data";
        let (block, remaining) = EncodingBlock::parse(input).unwrap();

        assert_eq!(block.encoding, "dt");
        assert_eq!(block.length, 4);
        assert_eq!(&block.data[..], b"ABCD");
        assert_eq!(remaining, b"more data");
    }

    #[test]
    fn test_incomplete_data() {
        let input = b"Encoding: dt\r\nLength: 100\r\n\r\nsmall";
        assert!(EncodingBlock::parse(input).is_none());
    }

    #[test]
    fn test_is_diamond_types() {
        let dt_block = EncodingBlock::new("dt", vec![1, 2, 3]);
        assert!(dt_block.is_diamond_types());

        let other_block = EncodingBlock::new("other", vec![1, 2, 3]);
        assert!(!other_block.is_diamond_types());
    }

    #[test]
    fn test_encoding_block_empty_data() {
        let block = EncodingBlock::new("dt", Bytes::new());
        assert_eq!(block.length, 0);

        let bytes = block.to_bytes();
        let (parsed, _) = EncodingBlock::parse(&bytes).unwrap();
        assert!(parsed.data.is_empty());
    }

    #[test]
    fn test_encoding_block_large_data() {
        let data = Bytes::from(vec![0u8; 10000]);
        let block = EncodingBlock::new("dt", data.clone());

        let bytes = block.to_bytes();
        let (parsed, _) = EncodingBlock::parse(&bytes).unwrap();
        assert_eq!(parsed.length, 10000);
        assert_eq!(parsed.data.len(), 10000);
    }

    #[test]
    fn test_parse_message_encoding_block() {
        let input = b"Encoding: dt\r\nLength: 3\r\n\r\nABC";
        match parse_message(input) {
            MessageParseResult::EncodingBlock(block, remaining) => {
                assert_eq!(block.encoding, "dt");
                assert!(remaining.is_empty());
            }
            _ => panic!("Expected EncodingBlock"),
        }
    }

    #[test]
    fn test_parse_message_regular_headers() {
        let input = b"HTTP/1.1 200 OK\r\nVersion: v1\r\n\r\n";
        match parse_message(input) {
            MessageParseResult::RegularHeaders => {}
            _ => panic!("Expected RegularHeaders"),
        }
    }

    #[test]
    fn test_parse_message_incomplete() {
        let input = b"Encoding: dt\r\nLength: 100\r\n\r\nsmall";
        match parse_message(input) {
            MessageParseResult::Incomplete => {}
            _ => panic!("Expected Incomplete"),
        }
    }

    // ========== Varint Tests ==========

    #[test]
    fn test_encode_varint_zero() {
        assert_eq!(encode_varint(0), vec![0]);
    }

    #[test]
    fn test_encode_varint_single_byte() {
        assert_eq!(encode_varint(1), vec![1]);
        assert_eq!(encode_varint(127), vec![127]);
    }

    #[test]
    fn test_encode_varint_two_bytes() {
        assert_eq!(encode_varint(128), vec![0x80, 0x01]);
        assert_eq!(encode_varint(16383), vec![0xFF, 0x7F]);
    }

    #[test]
    fn test_encode_varint_three_bytes() {
        assert_eq!(encode_varint(16384), vec![0x80, 0x80, 0x01]);
        assert_eq!(encode_varint(2097151), vec![0xFF, 0xFF, 0x7F]);
    }

    #[test]
    fn test_encode_varint_max_u64() {
        let encoded = encode_varint(u64::MAX);
        assert_eq!(encoded.len(), 10);

        // Verify roundtrip
        let (decoded, _) = decode_varint(&encoded).unwrap();
        assert_eq!(decoded, u64::MAX);
    }

    #[test]
    fn test_decode_varint_zero() {
        let (value, consumed) = decode_varint(&[0]).unwrap();
        assert_eq!(value, 0);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_decode_varint_single_byte() {
        let (value, consumed) = decode_varint(&[127]).unwrap();
        assert_eq!(value, 127);
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_decode_varint_two_bytes() {
        let (value, consumed) = decode_varint(&[0x80, 0x01]).unwrap();
        assert_eq!(value, 128);
        assert_eq!(consumed, 2);
    }

    #[test]
    fn test_decode_varint_incomplete() {
        let result = decode_varint(&[0x80, 0x80]); // Missing continuation
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_varint_empty() {
        let result = decode_varint(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_varint_roundtrip() {
        let test_values = [
            0,
            1,
            127,
            128,
            16383,
            16384,
            100000,
            1000000,
            10000000,
            u64::MAX,
        ];

        for &value in &test_values {
            let encoded = encode_varint(value);
            let (decoded, consumed) = decode_varint(&encoded).unwrap();
            assert_eq!(decoded, value, "Failed for value {}", value);
            assert_eq!(consumed, encoded.len());
        }
    }

    #[test]
    fn test_varint_multiple_in_sequence() {
        let values = vec![1u64, 128, 1000, 10000];
        let mut encoded = Vec::new();

        for &value in &values {
            encoded.extend_from_slice(&encode_varint(value));
        }

        let mut offset = 0;
        for &expected in &values {
            let (decoded, consumed) = decode_varint(&encoded[offset..]).unwrap();
            assert_eq!(decoded, expected);
            offset += consumed;
        }
    }
}
