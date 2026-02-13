//! Shared header parsing and formatting for Braid-HTTP.

use crate::core::error::{BraidError, Result};
use crate::core::types::Version;

/// Parse version header value.
pub fn parse_version_header(value: &str) -> Result<Vec<Version>> {
    use sfv::{BareItem, List, ListEntry, Parser};
    let list: List = Parser::new(value)
        .parse()
        .map_err(|e| BraidError::HeaderParse(format!("Invalid structured header: {:?}", e)))?;
    let mut versions = Vec::new();
    for member in list {
        match member {
            ListEntry::Item(item) => match item.bare_item {
                BareItem::String(s) => versions.push(Version::String(s.into())),
                BareItem::Integer(i) => versions.push(Version::Integer(i.into())),
                BareItem::Token(t) => versions.push(Version::String(t.into())),
                _ => {
                    return Err(BraidError::HeaderParse(
                        "Unsupported item type in version list".to_string(),
                    ))
                }
            },
            ListEntry::InnerList(_) => {
                return Err(BraidError::HeaderParse(
                    "Inner lists not supported for versions".to_string(),
                ))
            }
        }
    }
    Ok(versions)
}

/// Format version header value.
pub fn format_version_header(versions: &[Version]) -> String {
    versions
        .iter()
        .map(|v| match v {
            Version::String(s) => format!("\"{}\"", s.replace("\"", "\\\"")),
            Version::Integer(i) => i.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn parse_current_version_header(value: &str) -> Result<Vec<Version>> {
    parse_version_header(value)
}

pub fn parse_content_range(value: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = value.splitn(2, ' ').collect();
    if parts.len() != 2 {
        return Err(BraidError::HeaderParse(format!(
            "Invalid Content-Range: expected 'unit range', got '{}'",
            value
        )));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

#[inline]
pub fn format_content_range(unit: &str, range: &str) -> String {
    format!("{} {}", unit, range)
}

pub fn parse_heartbeat(value: &str) -> Result<u64> {
    let trimmed = value.trim();
    if let Some(ms_str) = trimmed.strip_suffix("ms") {
        return ms_str
            .parse::<u64>()
            .map(|n| n / 1000)
            .map_err(|_| BraidError::HeaderParse(format!("Invalid heartbeat: {}", value)));
    }
    if let Some(s_str) = trimmed.strip_suffix('s') {
        return s_str
            .parse()
            .map_err(|_| BraidError::HeaderParse(format!("Invalid heartbeat: {}", value)));
    }
    trimmed
        .parse()
        .map_err(|_| BraidError::HeaderParse(format!("Invalid heartbeat: {}", value)))
}

pub fn parse_merge_type(value: &str) -> Result<String> {
    let trimmed = value.trim();
    match trimmed {
        crate::core::protocol::constants::merge_types::DIAMOND => Ok(trimmed.to_string()),
        _ => Err(BraidError::HeaderParse(format!(
            "Unsupported merge-type: {}",
            value
        ))),
    }
}

pub fn parse_tunneled_response(
    bytes: &[u8],
) -> Result<(u16, std::collections::BTreeMap<String, String>, usize)> {
    let s = String::from_utf8_lossy(bytes);
    if let Some(end_idx) = s.find("\r\n\r\n") {
        let headers_part = &s[..end_idx];
        let mut status = 200;
        let mut headers = std::collections::BTreeMap::new();
        for line in headers_part.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(val) = line.strip_prefix(":status:") {
                status = val.trim().parse().unwrap_or(200);
                continue;
            }
            if let Some((name, value)) = line.split_once(':') {
                headers.insert(name.trim().to_lowercase(), value.trim().to_string());
            }
        }
        Ok((status, headers, end_idx + 4))
    } else {
        Err(BraidError::HeaderParse("Incomplete headers".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Version Header Tests ==========

    #[test]
    fn test_parse_version_header_with_quotes() {
        let versions = parse_version_header("\"v1\", \"v2\"").unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0], Version::new("v1"));
        assert_eq!(versions[1], Version::new("v2"));
    }

    #[test]
    fn test_parse_version_header_single() {
        let versions = parse_version_header("\"v1\"").unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0], Version::new("v1"));
    }

    #[test]
    fn test_parse_version_header_integers() {
        let versions = parse_version_header("1, 2, 3").unwrap();
        assert_eq!(versions, vec![
            Version::Integer(1),
            Version::Integer(2),
            Version::Integer(3)
        ]);
    }

    #[test]
    fn test_parse_version_header_mixed() {
        let versions = parse_version_header("\"v1\", 2, \"v3\"").unwrap();
        assert_eq!(versions[0], Version::new("v1"));
        assert_eq!(versions[1], Version::Integer(2));
        assert_eq!(versions[2], Version::new("v3"));
    }

    #[test]
    fn test_parse_version_header_empty() {
        let result = parse_version_header("");
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_parse_version_header_whitespace() {
        let versions = parse_version_header("  \"v1\"  ,  2  ").unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0], Version::new("v1"));
        assert_eq!(versions[1], Version::Integer(2));
    }

    #[test]
    fn test_parse_version_header_invalid_structured_field() {
        assert!(parse_version_header("invalid[").is_err());
    }

    #[test]
    fn test_parse_version_header_inner_list_rejected() {
        assert!(parse_version_header("(a b c)").is_err());
    }

    #[test]
    fn test_parse_version_header_token() {
        // Tokens without quotes should work too
        let versions = parse_version_header("v1, v2").unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0], Version::new("v1"));
    }

    // ========== Version Header Formatting Tests ==========

    #[test]
    fn test_format_version_header_strings() {
        let versions = vec![Version::new("v1"), Version::new("v2")];
        assert_eq!(format_version_header(&versions), "\"v1\", \"v2\"");
    }

    #[test]
    fn test_format_version_header_integers() {
        let versions = vec![Version::Integer(1), Version::Integer(2)];
        assert_eq!(format_version_header(&versions), "1, 2");
    }

    #[test]
    fn test_format_version_header_mixed() {
        let versions = vec![Version::new("v1"), Version::Integer(2)];
        assert_eq!(format_version_header(&versions), "\"v1\", 2");
    }

    #[test]
    fn test_format_version_header_escapes_quotes() {
        let versions = vec![Version::new("val\"ue")];
        assert_eq!(format_version_header(&versions), "\"val\\\"ue\"");
    }

    #[test]
    fn test_format_version_header_empty() {
        assert_eq!(format_version_header(&[]), "");
    }

    #[test]
    fn test_format_version_header_single() {
        let versions = vec![Version::new("v1")];
        assert_eq!(format_version_header(&versions), "\"v1\"");
    }

    // ========== Roundtrip Tests ==========

    #[test]
    fn test_version_header_roundtrip_strings() {
        let original = vec![Version::new("v1"), Version::new("v2")];
        let formatted = format_version_header(&original);
        let parsed = parse_version_header(&formatted).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn test_version_header_roundtrip_integers() {
        let original = vec![Version::Integer(1), Version::Integer(2), Version::Integer(3)];
        let formatted = format_version_header(&original);
        let parsed = parse_version_header(&formatted).unwrap();
        assert_eq!(parsed, original);
    }

    // ========== Heartbeat Tests ==========

    #[test]
    fn test_parse_heartbeat_seconds() {
        assert_eq!(parse_heartbeat("5s").unwrap(), 5);
        assert_eq!(parse_heartbeat("10s").unwrap(), 10);
        assert_eq!(parse_heartbeat("0s").unwrap(), 0);
    }

    #[test]
    fn test_parse_heartbeat_milliseconds() {
        assert_eq!(parse_heartbeat("5000ms").unwrap(), 5);
        assert_eq!(parse_heartbeat("1000ms").unwrap(), 1);
        assert_eq!(parse_heartbeat("0ms").unwrap(), 0);
    }

    #[test]
    fn test_parse_heartbeat_plain_number() {
        assert_eq!(parse_heartbeat("5").unwrap(), 5);
        assert_eq!(parse_heartbeat("10").unwrap(), 10);
    }

    #[test]
    fn test_parse_heartbeat_whitespace() {
        assert_eq!(parse_heartbeat("  5s  ").unwrap(), 5);
        assert_eq!(parse_heartbeat("  5000ms  ").unwrap(), 5);
    }

    #[test]
    fn test_parse_heartbeat_invalid() {
        assert!(parse_heartbeat("abc").is_err());
        assert!(parse_heartbeat("5x").is_err());
        assert!(parse_heartbeat("").is_err());
        assert!(parse_heartbeat("ms").is_err());
    }

    #[test]
    fn test_parse_heartbeat_negative() {
        assert!(parse_heartbeat("-5s").is_err());
    }

    // ========== Content-Range Tests ==========

    #[test]
    fn test_parse_content_range_json() {
        let (unit, range) = parse_content_range("json .field").unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, ".field");
    }

    #[test]
    fn test_parse_content_range_bytes() {
        let (unit, range) = parse_content_range("bytes 0-100").unwrap();
        assert_eq!(unit, "bytes");
        assert_eq!(range, "0-100");
    }

    #[test]
    fn test_parse_content_range_text() {
        let (unit, range) = parse_content_range("text 0:10").unwrap();
        assert_eq!(unit, "text");
        assert_eq!(range, "0:10");
    }

    #[test]
    fn test_parse_content_range_lines() {
        let (unit, range) = parse_content_range("lines 10-20").unwrap();
        assert_eq!(unit, "lines");
        assert_eq!(range, "10-20");
    }

    #[test]
    fn test_parse_content_range_json_pointer() {
        let (unit, range) = parse_content_range("json /users/0/name").unwrap();
        assert_eq!(unit, "json");
        assert_eq!(range, "/users/0/name");
    }

    #[test]
    fn test_parse_content_range_invalid_no_space() {
        assert!(parse_content_range("invalid").is_err());
    }

    #[test]
    fn test_parse_content_range_empty() {
        assert!(parse_content_range("").is_err());
    }

    #[test]
    fn test_format_content_range() {
        assert_eq!(format_content_range("json", ".field"), "json .field");
        assert_eq!(format_content_range("bytes", "0-100"), "bytes 0-100");
    }

    // ========== Merge-Type Tests ==========

    #[test]
    fn test_parse_merge_type_diamond() {
        assert_eq!(parse_merge_type("diamond").unwrap(), "diamond");
    }

    #[test]
    fn test_parse_merge_type_whitespace() {
        assert_eq!(parse_merge_type("  diamond  ").unwrap(), "diamond");
    }

    #[test]
    fn test_parse_merge_type_invalid() {
        assert!(parse_merge_type("invalid").is_err());
        assert!(parse_merge_type("").is_err());
        assert!(parse_merge_type("sync9").is_err());
        assert!(parse_merge_type("simpleton").is_err());
    }

    // ========== Tunneled Response Tests ==========

    #[test]
    fn test_parse_tunneled_response_basic() {
        let data = b":status: 209\r\nversion: \"v1\"\r\n\r\nbody";
        let (status, headers, offset) = parse_tunneled_response(data).unwrap();
        assert_eq!(status, 209);
        assert_eq!(headers.get("version"), Some(&"\"v1\"".to_string()));
        assert_eq!(offset, 31); // 14 + 15 + 2 = 31
    }

    #[test]
    fn test_parse_tunneled_response_200() {
        let data = b":status: 200\r\ncontent-type: application/json\r\n\r\n{}";
        let (status, headers, offset) = parse_tunneled_response(data).unwrap();
        assert_eq!(status, 200);
        assert_eq!(headers.get("content-type"), Some(&"application/json".to_string()));
    }

    #[test]
    fn test_parse_tunneled_response_no_status() {
        let data = b"content-type: text/plain\r\n\r\nbody";
        let (status, _, _) = parse_tunneled_response(data).unwrap();
        assert_eq!(status, 200); // Default
    }

    #[test]
    fn test_parse_tunneled_response_incomplete() {
        let data = b":status: 200\r\n";
        assert!(parse_tunneled_response(data).is_err());
    }

    #[test]
    fn test_parse_tunneled_response_empty() {
        let data = b"";
        assert!(parse_tunneled_response(data).is_err());
    }

    #[test]
    fn test_parse_tunneled_response_multiple_headers() {
        let data = b":status: 209\r\nversion: \"v1\"\r\nparents: \"v0\"\r\nmerge-type: diamond\r\n\r\n";
        let (status, headers, _) = parse_tunneled_response(data).unwrap();
        assert_eq!(status, 209);
        assert_eq!(headers.get("version"), Some(&"\"v1\"".to_string()));
        assert_eq!(headers.get("parents"), Some(&"\"v0\"".to_string()));
        assert_eq!(headers.get("merge-type"), Some(&"diamond".to_string()));
    }
}
