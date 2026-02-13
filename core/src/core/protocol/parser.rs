//! Protocol parsing utilities.

use crate::core::error::Result;
use crate::core::types::Version;

/// Header parser for protocol messages.
pub struct HeaderParser;

impl HeaderParser {
    pub fn parse_version(value: &str) -> Result<Vec<Version>> {
        crate::core::protocol::parse_version_header(value)
    }
    pub fn parse_content_range(value: &str) -> Result<(String, String)> {
        crate::core::protocol::parse_content_range(value)
    }
    pub fn format_version(versions: &[Version]) -> String {
        crate::core::protocol::format_version_header(versions)
    }
    pub fn format_content_range(unit: &str, range: &str) -> String {
        crate::core::protocol::format_content_range(unit, range)
    }
}

/// Parser for protocol messages (status lines, body chunks).
pub struct MessageParser {
    buffer: String,
}

impl MessageParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    pub fn feed(&mut self, data: &[u8]) -> Result<Vec<ParseEvent>> {
        self.buffer.push_str(&String::from_utf8_lossy(data));
        self.parse()
    }

    fn parse(&mut self) -> Result<Vec<ParseEvent>> {
        let mut events = Vec::new();
        
        // Check for status line
        if let Some(idx) = self.buffer.find("\r\n") {
            let line = &self.buffer[..idx];
            if line.starts_with("HTTP/") {
                events.push(ParseEvent::StatusLine(line.to_string()));
                self.buffer = self.buffer[idx + 2..].to_string();
            }
        }
        
        Ok(events)
    }

    pub fn is_partial(&self) -> bool {
        !self.buffer.is_empty()
    }
}

/// Events emitted by the message parser.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseEvent {
    StatusLine(String),
    Header(String, String),
    BodyChunk(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_parser_parse_version() {
        let result = HeaderParser::parse_version("\"v1\", \"v2\"").unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], Version::new("v1"));
    }

    #[test]
    fn test_header_parser_parse_content_range() {
        let (unit, range) = HeaderParser::parse_content_range("json .field").unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, ".field");
    }

    #[test]
    fn test_header_parser_format_version() {
        let versions = vec![Version::new("v1")];
        assert_eq!(HeaderParser::format_version(&versions), "\"v1\"");
    }

    #[test]
    fn test_header_parser_format_content_range() {
        assert_eq!(
            HeaderParser::format_content_range("bytes", "0-100"),
            "bytes 0-100"
        );
    }

    #[test]
    fn test_message_parser_new() {
        let parser = MessageParser::new();
        assert!(!parser.is_partial());
    }

    #[test]
    fn test_message_parser_status_line() {
        let mut parser = MessageParser::new();
        let events = parser.feed(b"HTTP/1.1 209 Subscription\r\n").unwrap();
        
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], ParseEvent::StatusLine(s) if s == "HTTP/1.1 209 Subscription"));
    }

    #[test]
    fn test_message_parser_partial_status() {
        let mut parser = MessageParser::new();
        let events = parser.feed(b"HTTP/1.1 2").unwrap();
        assert!(events.is_empty());
        assert!(parser.is_partial());
        
        let events = parser.feed(b"09 OK\r\n").unwrap();
        assert_eq!(events.len(), 1);
        assert!(!parser.is_partial());
    }

    #[test]
    fn test_message_parser_multiple_feeds() {
        let mut parser = MessageParser::new();
        
        parser.feed(b"HTTP/1.1 ").unwrap();
        parser.feed(b"200 OK\r\n").unwrap();
        
        // Should have complete status line
    }

    #[test]
    fn test_parse_event_debug() {
        let event = ParseEvent::StatusLine("HTTP/1.1 200 OK".to_string());
        let debug = format!("{:?}", event);
        assert!(debug.contains("StatusLine"));
    }
}
