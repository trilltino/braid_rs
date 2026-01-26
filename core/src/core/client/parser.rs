//! Message parser for Braid protocol streaming.
//!
//! Incremental, state-machine based parser for streaming Braid protocol messages.
//! Handles parsing from byte streams where the complete message may not arrive at once.
//!
//! # Parsing Flow
//!
//! 1. **WaitingForHeaders**: Accumulate bytes until `\r\n\r\n` is found
//! 2. **ParsingHeaders**: Extract header fields (Version, Parents, Content-Length, etc.)
//! 3. **WaitingForBody**: Wait until Content-Length bytes have arrived
//! 4. **ParsingBody**: Extract body or patches from accumulated bytes
//! 5. **Complete**: Message ready, reset state
//!
//! # Multi-Patch Support
//!
//! For multiple patches in a single response (Section 3.3), each patch declares
//! a Content-Length header in its own headers section. The parser tracks this
//! to correctly identify patch boundaries in the stream.
//!
//! # Examples
//!
//! ```ignore
//! use crate::core::client::MessageParser;
//!
//! let mut parser = MessageParser::new();
//!
//! let data1 = b"Version: \"v1\"\r\nContent-Length: 5\r\n\r\n";
//! let messages = parser.feed(data1)?;
//!
//! let data2 = b"hello";
//! let messages = parser.feed(data2)?;
//! assert!(!messages.is_empty());
//! ```
//!
//! # Specification
//!
//! See Section 3.3 of draft-toomim-httpbis-braid-http for multi-patch format.

use crate::core::error::{BraidError, Result};
use crate::core::types::Patch;
use bytes::{Buf, Bytes, BytesMut};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::BTreeMap;

/// Parse state for streaming protocol parser.
///
/// Represents the current position in the message parsing state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseState {
    /// Waiting for first bytes of headers
    WaitingForHeaders,
    /// Currently accumulating header bytes
    ParsingHeaders,
    /// Headers complete, waiting for body bytes
    /// Headers complete, waiting for body bytes (for single-patch or snapshot)
    WaitingForBody,
    /// Waiting for patch headers in multi-patch mode
    WaitingForPatchHeaders,
    /// Waiting for patch body in multi-patch mode
    WaitingForPatchBody,
    /// Skipping CRLF separator between patches
    SkippingSeparator,
    /// Message fully parsed and ready
    Complete,
    /// Error occurred during parsing
    Error,
}

/// Message parser for Braid protocol.
///
/// Incrementally parses protocol messages from a byte stream using a state machine.
/// Designed to handle streaming responses where messages arrive in fragments.
///
/// # State Machine
///
/// The parser transitions through states as it accumulates and processes bytes:
/// - Accumulates bytes until complete headers are received (`\r\n\r\n`)
/// - Extracts Content-Length from headers
/// - Accumulates body bytes until the full body is received
/// - Finalizes the message and resets for the next one
///
/// # Multi-Patch Parsing
///
/// When parsing multi-patch responses (Section 3.3), each patch is preceded by
/// its own headers that include Content-Length. The parser correctly handles
/// parsing multiple patches sequentially from a single stream.
#[derive(Debug)]
pub struct MessageParser {
    /// Input buffer accumulating bytes from stream
    buffer: BytesMut,
    /// Current state in parse state machine
    state: ParseState,
    /// Parsed headers (key-value pairs)
    headers: BTreeMap<String, String>,
    /// Accumulates body bytes
    body_buffer: BytesMut,
    /// Expected body length from Content-Length header
    expected_body_length: usize,
    /// Number of body bytes read so far
    read_body_length: usize,
    /// Accumulated patches (for multi-patch messages)
    patches: Vec<Patch>,
    /// Expected number of patches (from Patches: N header)
    expected_patches: usize,
    /// Number of patches fully read so far
    patches_read: usize,
    /// Headers for the current patch being parsed
    patch_headers: BTreeMap<String, String>,
    /// Expected length of the current patch body
    expected_patch_length: usize,
    /// Number of bytes read for the current patch body
    read_patch_length: usize,
    /// Whether we're parsing an encoding block
    is_encoding_block: bool,
}

/// Regex patterns for parsing (compiled once)
static HTTP_STATUS_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^HTTP/?\d*\.?\d* (\d{3})").unwrap());

static ENCODING_BLOCK_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)Encoding:\s*(\w+)\r?\nLength:\s*(\d+)\r?\n").unwrap());

impl MessageParser {
    /// Create a new message parser
    pub fn new() -> Self {
        MessageParser {
            buffer: BytesMut::with_capacity(8192),
            state: ParseState::WaitingForHeaders,
            headers: BTreeMap::new(),
            body_buffer: BytesMut::new(),
            expected_body_length: 0,
            read_body_length: 0,
            patches: Vec::new(),
            expected_patches: 0,
            patches_read: 0,
            patch_headers: BTreeMap::new(),
            expected_patch_length: 0,
            read_patch_length: 0,
            is_encoding_block: false,
        }
    }

    /// Feed bytes to the parser
    pub fn feed(&mut self, data: &[u8]) -> Result<Vec<Message>> {
        self.buffer.extend_from_slice(data);
        let mut messages = Vec::new();

        loop {
            match self.state {
                ParseState::WaitingForHeaders => {
                    // Skip leading whitespace (\r\n)
                    while !self.buffer.is_empty()
                        && (self.buffer[0] == b'\r' || self.buffer[0] == b'\n')
                    {
                        self.buffer.advance(1);
                    }

                    if self.buffer.is_empty() {
                        break;
                    }

                    // Check for encoding block (starts with 'E' or 'e')
                    if self.check_encoding_block()? {
                        self.state = ParseState::WaitingForBody;
                        continue;
                    }

                    if let Some(pos) = self.find_header_end() {
                        self.parse_headers(pos)?;
                        self.state = ParseState::WaitingForBody;
                    } else {
                        break;
                    }
                }
                ParseState::WaitingForBody => {
                    if self.expected_patches > 0 {
                        self.state = ParseState::WaitingForPatchHeaders;
                    } else if self.try_parse_body()? {
                        if let Some(msg) = self.finalize_message()? {
                            messages.push(msg);
                        }
                        self.reset();
                        self.state = ParseState::WaitingForHeaders;
                    } else {
                        break;
                    }
                }
                ParseState::WaitingForPatchHeaders => {
                    if let Some(pos) = self.find_header_end() {
                        self.parse_patch_headers(pos)?;
                        self.state = ParseState::WaitingForPatchBody;
                    } else {
                        break;
                    }
                }
                ParseState::WaitingForPatchBody => {
                    if self.try_parse_patch_body()? {
                        self.patches_read += 1;
                        if self.patches_read < self.expected_patches {
                            self.state = ParseState::SkippingSeparator;
                        } else {
                            if let Some(msg) = self.finalize_message()? {
                                messages.push(msg);
                            }
                            self.reset();
                            self.state = ParseState::WaitingForHeaders;
                        }
                    } else {
                        break;
                    }
                }
                ParseState::SkippingSeparator => {
                    // Skip the CRLF between patches
                    if self.buffer.len() >= 2 {
                        if &self.buffer[..2] == b"\r\n" {
                            self.buffer.advance(2);
                        } else if self.buffer[0] == b'\n' {
                            self.buffer.advance(1);
                        }
                        self.state = ParseState::WaitingForPatchHeaders;
                    } else if self.buffer.len() == 1 && self.buffer[0] == b'\n' {
                        self.buffer.advance(1);
                        self.state = ParseState::WaitingForPatchHeaders;
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }

        Ok(messages)
    }

    /// Check for and parse an encoding block.
    ///
    /// Encoding blocks look like:
    /// ```text
    /// Encoding: dt
    /// Length: 411813
    /// <binary dt file>
    /// ```
    ///
    /// Returns true if an encoding block was detected and parsed.
    fn check_encoding_block(&mut self) -> Result<bool> {
        // Encoding blocks start with 'E' or 'e'
        if self.buffer.is_empty() || (self.buffer[0] != b'E' && self.buffer[0] != b'e') {
            return Ok(false);
        }

        // Look for double newline
        if let Some(end) = self.find_double_newline() {
            let header_bytes = &self.buffer[..end];
            let header_str = std::str::from_utf8(header_bytes).map_err(|e| {
                BraidError::Protocol(format!("Invalid encoding block UTF-8: {}", e))
            })?;

            // Try to match encoding block format
            if let Some(caps) = ENCODING_BLOCK_REGEX.captures(header_str) {
                let encoding = caps.get(1).unwrap().as_str().to_string();
                let length: usize = caps.get(2).unwrap().as_str().parse().map_err(|_| {
                    BraidError::Protocol("Invalid length in encoding block".to_string())
                })?;

                // Consume the header part
                let _ = self.buffer.split_to(end);

                // Store encoding info
                self.headers.insert("encoding".to_string(), encoding);
                self.headers
                    .insert("length".to_string(), length.to_string());
                self.expected_body_length = length;
                self.is_encoding_block = true;

                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Find double newline (\r\n\r\n or \n\n)
    fn find_double_newline(&self) -> Option<usize> {
        // Check for \r\n\r\n
        if let Some(pos) = self.buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            return Some(pos + 4);
        }
        // Check for \n\n
        if let Some(pos) = self.buffer.windows(2).position(|w| w == b"\n\n") {
            return Some(pos + 2);
        }
        None
    }

    /// Find the end of HTTP headers (\r\n\r\n)
    fn find_header_end(&self) -> Option<usize> {
        self.buffer
            .windows(4)
            .rposition(|w| w == b"\r\n\r\n")
            .map(|p| p + 4)
    }

    /// Parse headers from buffer
    fn parse_headers(&mut self, end: usize) -> Result<()> {
        let header_bytes = self.buffer.split_to(end);
        let mut header_str = String::from_utf8(header_bytes[..header_bytes.len() - 4].to_vec())?;

        // Convert HTTP status line to :status header
        // "HTTP/1.1 200 OK" -> ":status: 200"
        if let Some(caps) = HTTP_STATUS_REGEX.captures(&header_str) {
            if let Some(status_match) = caps.get(1) {
                let status = status_match.as_str();
                // Replace the first line with :status header
                if let Some(first_newline) = header_str.find('\n') {
                    let replacement = format!(":status: {}\r", status);
                    header_str = replacement + &header_str[first_newline..];
                }
            }
        }

        for line in header_str.lines() {
            if let Some(colon_pos) = line.find(':') {
                let key = line[..colon_pos].trim().to_lowercase();
                let value = line[colon_pos + 1..].trim().to_string();
                self.headers.insert(key, value);
            }
        }

        // Check for Patches header (multi-patch support)
        if let Some(patches_str) = self.headers.get("patches") {
            self.expected_patches = patches_str.parse().unwrap_or(0);
        }

        // Check for content-length or length (encoding blocks use "length")
        if let Some(len_str) = self
            .headers
            .get("content-length")
            .or_else(|| self.headers.get("length"))
        {
            self.expected_body_length = len_str.parse().map_err(|_| {
                BraidError::HeaderParse(format!("Invalid content-length: {}", len_str))
            })?;
        }

        Ok(())
    }

    /// Parse patch headers from buffer
    fn parse_patch_headers(&mut self, end: usize) -> Result<()> {
        let header_bytes = self.buffer.split_to(end);
        let header_str = String::from_utf8(header_bytes[..header_bytes.len() - 4].to_vec())?;

        self.patch_headers.clear();
        for line in header_str.lines() {
            if let Some(colon_pos) = line.find(':') {
                let key = line[..colon_pos].trim().to_lowercase();
                let value = line[colon_pos + 1..].trim().to_string();
                self.patch_headers.insert(key, value);
            }
        }

        if let Some(len_str) = self.patch_headers.get("content-length") {
            self.expected_patch_length = len_str.parse().map_err(|_| {
                BraidError::HeaderParse(format!("Invalid patch content-length: {}", len_str))
            })?;
        } else {
            return Err(BraidError::Protocol(
                "Every patch in a multi-patch update MUST include Content-Length".to_string(),
            ));
        }

        self.read_patch_length = 0;
        Ok(())
    }

    /// Try to parse patch body from buffer
    fn try_parse_patch_body(&mut self) -> Result<bool> {
        let remaining = self.expected_patch_length - self.read_patch_length;
        if self.buffer.len() >= remaining {
            let body_chunk = self.buffer.split_to(remaining);

            // Build the patch
            let unit = self
                .patch_headers
                .get("content-range")
                .and_then(|cr| cr.split_whitespace().next())
                .unwrap_or("bytes")
                .to_string();

            let range = self
                .patch_headers
                .get("content-range")
                .and_then(|cr| cr.split_whitespace().nth(1))
                .unwrap_or("")
                .to_string();

            let patch = Patch::with_length(unit, range, body_chunk, self.expected_patch_length);
            self.patches.push(patch);

            self.read_patch_length += remaining;
            Ok(true)
        } else {
            // We need more data, but we can't partially build the patch easily here
            // because we need the full patch content to move to next state.
            // Actually, we could accumulate it in a temporary buffer.
            // For multi-patch, we'll just wait until we have the full patch.
            Ok(false)
        }
    }

    /// Try to parse body from buffer
    fn try_parse_body(&mut self) -> Result<bool> {
        if self.expected_body_length == 0 {
            return Ok(true);
        }

        let remaining = self.expected_body_length - self.read_body_length;
        if self.buffer.len() >= remaining {
            let body_chunk = self.buffer.split_to(remaining);
            self.body_buffer.extend_from_slice(&body_chunk);
            self.read_body_length += body_chunk.len();
            Ok(true)
        } else {
            self.body_buffer
                .extend_from_slice(&self.buffer.split_to(self.buffer.len()));
            self.read_body_length += self.body_buffer.len();
            Ok(false)
        }
    }

    /// Finalize and return a complete message
    fn finalize_message(&mut self) -> Result<Option<Message>> {
        let body = self.body_buffer.split().freeze();

        let headers = std::mem::take(&mut self.headers);
        let url = headers.get("content-location").cloned();
        let encoding = headers.get("encoding").cloned();

        Ok(Some(Message {
            headers,
            body,
            patches: std::mem::take(&mut self.patches),
            status_code: None,
            encoding,
            url,
        }))
    }

    /// Reset parser for next message
    fn reset(&mut self) {
        self.headers.clear();
        self.body_buffer.clear();
        self.expected_body_length = 0;
        self.read_body_length = 0;
        self.patches.clear();
        self.expected_patches = 0;
        self.patches_read = 0;
        self.patch_headers.clear();
        self.expected_patch_length = 0;
        self.read_patch_length = 0;
        self.is_encoding_block = false;
    }

    /// Get current parse state
    pub fn state(&self) -> ParseState {
        self.state
    }

    /// Get parsed headers
    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    /// Get parsed body
    pub fn body(&self) -> &[u8] {
        &self.body_buffer
    }
}

impl Default for MessageParser {
    fn default() -> Self {
        Self::new()
    }
}

/// A parsed protocol message
#[derive(Debug, Clone)]
pub struct Message {
    /// Headers (lowercase keys)
    pub headers: BTreeMap<String, String>,
    /// Body content
    pub body: Bytes,
    /// Patches (for multi-patch messages)
    pub patches: Vec<Patch>,
    /// HTTP status code (parsed from status line or :status header)
    pub status_code: Option<u16>,
    /// Encoding of the body content (e.g., "dt")
    pub encoding: Option<String>,
    /// Target URL for this message (from Content-Location header)
    pub url: Option<String>,
}

impl Message {
    /// Get the HTTP status code from the message.
    ///
    /// Parses from `:status` header or HTTP status line if present.
    pub fn status(&self) -> Option<u16> {
        self.status_code
            .or_else(|| self.headers.get(":status").and_then(|v| v.parse().ok()))
    }

    /// Get the version from headers.
    ///
    /// Returns parsed version array if the Version header is present.
    pub fn version(&self) -> Option<&str> {
        self.headers.get("version").map(|s| s.as_str())
    }

    /// Get the current-version from headers.
    pub fn current_version(&self) -> Option<&str> {
        self.headers.get("current-version").map(|s| s.as_str())
    }

    /// Get the parents from headers.
    ///
    /// Returns parsed parents array if the Parents header is present.
    pub fn parents(&self) -> Option<&str> {
        self.headers.get("parents").map(|s| s.as_str())
    }

    /// Decode body based on encoding.
    ///
    /// Handles binary encodings like "dt" (diamond-types).
    pub fn decode_body(&self) -> Result<Bytes> {
        match self.encoding.as_deref() {
            Some("dt") => {
                // For now, return raw bytes as we don't have a DT decoder here yet
                // In full implementation, this would use diamond-types crate
                Ok(self.body.clone())
            }
            Some(enc) => Err(BraidError::Protocol(format!("Unknown encoding: {}", enc))),
            None => Ok(self.body.clone()),
        }
    }

    /// Get extra headers (non-Braid headers).
    ///
    /// Returns a map of headers that are not part of the Braid protocol,
    /// matching the JS reference `extra_headers()` function.
    pub fn extra_headers(&self) -> BTreeMap<String, String> {
        // Known Braid headers to exclude
        const KNOWN_HEADERS: &[&str] = &[
            "version",
            "parents",
            "current-version",
            "patches",
            "content-length",
            "content-range",
            ":status",
        ];

        self.headers
            .iter()
            .filter(|(k, _)| !KNOWN_HEADERS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    /// Get the body as UTF-8 text.
    ///
    /// Returns None if the body is not valid UTF-8.
    pub fn body_text(&self) -> Option<String> {
        std::str::from_utf8(&self.body).ok().map(|s| s.to_string())
    }
}

/// Parse an HTTP status line and extract the status code.
///
/// Parses formats like:
/// - "HTTP/1.1 200 OK"
/// - "HTTP 200 OK"
/// - "HTTP/2 200"
///
/// Returns the status code if found.
pub fn parse_status_line(line: &str) -> Option<u16> {
    // Match "HTTP/x.x NNN" or "HTTP NNN"
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 && parts[0].to_uppercase().starts_with("HTTP") {
        parts[1].parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_creation() {
        let parser = MessageParser::new();
        assert_eq!(parser.state(), ParseState::WaitingForHeaders);
    }

    #[test]
    fn test_simple_message_parsing() {
        let mut parser = MessageParser::new();
        let data = b"Content-Length: 5\r\n\r\nHello";
        let messages = parser.feed(data).unwrap();
        assert!(!messages.is_empty());
        assert_eq!(messages[0].body, Bytes::from_static(b"Hello"));
    }

    #[test]
    fn test_parse_status_line() {
        assert_eq!(parse_status_line("HTTP/1.1 200 OK"), Some(200));
        assert_eq!(parse_status_line("HTTP 209 Subscription"), Some(209));
        assert_eq!(parse_status_line("HTTP/2 404"), Some(404));
        assert_eq!(parse_status_line("Not a status"), None);
    }

    #[test]
    fn test_message_extra_headers() {
        let mut headers = BTreeMap::new();
        headers.insert("version".to_string(), "\"v1\"".to_string());
        headers.insert("parents".to_string(), "\"v0\"".to_string());
        headers.insert("content-length".to_string(), "5".to_string());
        headers.insert("x-custom-header".to_string(), "value".to_string());
        headers.insert("cache-control".to_string(), "no-cache".to_string());

        let msg = Message {
            headers,
            body: Bytes::new(),
            patches: vec![],
            status_code: None,
            encoding: None,
            url: None,
        };

        let extra = msg.extra_headers();
        assert_eq!(extra.len(), 2);
        assert!(extra.contains_key("x-custom-header"));
        assert!(extra.contains_key("cache-control"));
        assert!(!extra.contains_key("version"));
        assert!(!extra.contains_key("content-length"));
    }

    #[test]
    fn test_message_body_text() {
        let msg = Message {
            headers: BTreeMap::new(),
            body: Bytes::from_static(b"Hello World"),
            patches: vec![],
            status_code: Some(200),
            encoding: None,
            url: None,
        };

        assert_eq!(msg.body_text(), Some("Hello World".to_string()));
        assert_eq!(msg.status(), Some(200));
    }

    #[test]
    fn test_multi_patch_parsing() {
        let mut parser = MessageParser::new();
        let data = b"Patches: 2\r\n\r\n\
                     Content-Length: 5\r\n\
                     Content-Range: json .a\r\n\r\n\
                     hello\r\n\
                     Content-Length: 5\r\n\
                     Content-Range: json .b\r\n\r\n\
                     world\r\n";

        let messages = parser.feed(data).unwrap();
        assert!(!messages.is_empty());
        let msg = &messages[0];
        assert_eq!(msg.patches.len(), 2);
        assert_eq!(msg.patches[0].range, ".a");
        assert_eq!(msg.patches[0].content, Bytes::from_static(b"hello"));
        assert_eq!(msg.patches[1].range, ".b");
        assert_eq!(msg.patches[1].content, Bytes::from_static(b"world"));
    }

    #[test]
    fn test_incremental_multi_patch_parsing() {
        let mut parser = MessageParser::new();

        // Feed in fragments
        parser.feed(b"Patches: 2\r\n\r\n").unwrap();
        parser.feed(b"Content-Length: 5\r\n").unwrap();
        parser.feed(b"Content-Range: json .a\r\n\r\n").unwrap();
        parser.feed(b"hello\r\n").unwrap();

        // At this point, we should have 1 patch read, but message not finalized
        assert_eq!(parser.patches.len(), 1);

        parser.feed(b"Content-Length: 5\r\n").unwrap();
        parser.feed(b"Content-Range: json .b\r\n\r\n").unwrap();
        let messages = parser.feed(b"world\r\n").unwrap();

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].patches.len(), 2);
    }
}
