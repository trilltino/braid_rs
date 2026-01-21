//! Version identifier for the Braid-HTTP protocol.
//!
//! Version IDs are unique identifiers for specific points in a resource's history.
//! They form the foundation of Braid's state synchronization by enabling:
//!
//! - **History tracking**: Mark distinct changes at distinct points in time
//! - **DAG construction**: Build a Directed Acyclic Graph of version history
//! - **Concurrent editing**: Support multiple parents for merged branches
//! - **Conflict detection**: Identify when clients have diverged
//!
//! # Format
//!
//! Version IDs follow [RFC 8941 Structured Headers](https://datatracker.ietf.org/doc/html/rfc8941)
//! format. They can be:
//!
//! - **Strings**: UUIDs, timestamps, hashes, or any unique identifier
//! - **Integers**: Sequential version numbers
//!
//! In HTTP headers, versions are formatted as quoted strings:
//! ```text
//! Version: "abc123"
//! Parents: "v1", "v2"
//! ```
//!
//! # Examples
//!
//! ```
//! use crate::core::Version;
//!
//! // String-based versions (most common)
//! let v1 = Version::new("dkn7ov2vwg");
//! let v2 = Version::new("commit-abc123");
//!
//! // Integer-based versions
//! let v3 = Version::Integer(42);
//!
//! // From string literals
//! let v4: Version = "my-version".into();
//!
//! // Display formatting
//! assert_eq!(v1.to_string(), "dkn7ov2vwg");
//! ```
//!
//! # Version DAG
//!
//! Versions form a Directed Acyclic Graph (DAG) where each version points to its
//! parent(s). This enables tracking concurrent edits and merges:
//!
//! ```text
//!     v1
//!    /  \
//!   v2   v3    (concurrent edits)
//!    \  /
//!     v4       (merge)
//! ```
//!
//! # Specification
//!
//! See Section 2 of [draft-toomim-httpbis-braid-http-04] for complete details.
//!
//! [draft-toomim-httpbis-braid-http-04]: https://datatracker.ietf.org/doc/html/draft-toomim-httpbis-braid-http

use std::hash::Hash;

/// A version identifier in the Braid protocol.
///
/// Version IDs uniquely identify specific points in a resource's history.
/// They are used in the `Version`, `Parents`, and `Current-Version` headers
/// to track state and enable synchronization.
///
/// # Variants
///
/// - [`Version::String`]: String-based ID (UUIDs, hashes, timestamps)
/// - [`Version::Integer`]: Integer-based ID (sequential numbers)
///
/// # Creating Versions
///
/// ```
/// use crate::core::Version;
///
/// // Using the constructor
/// let v1 = Version::new("abc123");
///
/// // Using From trait
/// let v2: Version = "def456".into();
/// let v3: Version = String::from("ghi789").into();
///
/// // Integer version
/// let v4 = Version::Integer(1);
/// ```
///
/// # Serialization
///
/// Versions serialize to JSON as their underlying value:
///
/// ```
/// use crate::core::Version;
///
/// let v = Version::new("abc");
/// assert_eq!(v.to_json(), serde_json::json!("abc"));
///
/// let v = Version::Integer(42);
/// assert_eq!(v.to_json(), serde_json::json!(42));
/// ```
///
/// # Specification
///
/// See Section 2 of draft-toomim-httpbis-braid-http for version semantics.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Version {
    /// String-based version ID.
    ///
    /// Most common format. Can be UUIDs, commit hashes, timestamps, or any
    /// unique string identifier.
    ///
    /// # Example
    /// ```
    /// use crate::core::Version;
    /// let v = Version::String("abc123".to_string());
    /// ```
    String(String),

    /// Integer-based version ID.
    ///
    /// Useful for sequential versioning schemes.
    ///
    /// # Example
    /// ```
    /// use crate::core::Version;
    /// let v = Version::Integer(42);
    /// ```
    Integer(i64),
}

impl Version {
    /// Create a new string-based version.
    ///
    /// This is the most common way to create versions. The input is converted
    /// to a `String` using the `Into<String>` trait.
    ///
    /// # Arguments
    ///
    /// * `s` - Any type that can be converted into a `String`
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Version;
    ///
    /// let v1 = Version::new("abc123");
    /// let v2 = Version::new(String::from("def456"));
    /// ```
    #[inline]
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Version::String(s.into())
    }

    /// Create a new integer-based version.
    ///
    /// # Arguments
    ///
    /// * `n` - The version number
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Version;
    ///
    /// let v = Version::integer(42);
    /// assert_eq!(v, Version::Integer(42));
    /// ```
    #[inline]
    #[must_use]
    pub fn integer(n: i64) -> Self {
        Version::Integer(n)
    }

    /// Check if this is a string version.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Version;
    ///
    /// assert!(Version::new("abc").is_string());
    /// assert!(!Version::Integer(42).is_string());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_string(&self) -> bool {
        matches!(self, Version::String(_))
    }

    /// Check if this is an integer version.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Version;
    ///
    /// assert!(Version::Integer(42).is_integer());
    /// assert!(!Version::new("abc").is_integer());
    /// ```
    #[inline]
    #[must_use]
    pub fn is_integer(&self) -> bool {
        matches!(self, Version::Integer(_))
    }

    /// Get the string value if this is a string version.
    ///
    /// # Returns
    ///
    /// `Some(&str)` if this is a string version, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Version;
    ///
    /// let v = Version::new("abc");
    /// assert_eq!(v.as_str(), Some("abc"));
    ///
    /// let v = Version::Integer(42);
    /// assert_eq!(v.as_str(), None);
    /// ```
    #[inline]
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Version::String(s) => Some(s),
            Version::Integer(_) => None,
        }
    }

    /// Get the integer value if this is an integer version.
    ///
    /// # Returns
    ///
    /// `Some(i64)` if this is an integer version, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Version;
    ///
    /// let v = Version::Integer(42);
    /// assert_eq!(v.as_integer(), Some(42));
    ///
    /// let v = Version::new("abc");
    /// assert_eq!(v.as_integer(), None);
    /// ```
    #[inline]
    #[must_use]
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Version::Integer(i) => Some(*i),
            Version::String(_) => None,
        }
    }

    /// Convert to a JSON value.
    ///
    /// String versions become JSON strings, integer versions become JSON numbers.
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Version;
    ///
    /// let v = Version::new("abc");
    /// assert_eq!(v.to_json(), serde_json::json!("abc"));
    ///
    /// let v = Version::Integer(42);
    /// assert_eq!(v.to_json(), serde_json::json!(42));
    /// ```
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Version::String(s) => serde_json::json!(s),
            Version::Integer(i) => serde_json::json!(i),
        }
    }

    /// Parse from a JSON value.
    ///
    /// - JSON strings become string versions
    /// - JSON numbers become integer versions (if representable as i64)
    /// - Other JSON types are converted to their string representation
    ///
    /// # Examples
    ///
    /// ```
    /// use crate::core::Version;
    /// use serde_json::json;
    ///
    /// let v = Version::from_json(json!("abc"));
    /// assert_eq!(v, Version::String("abc".to_string()));
    ///
    /// let v = Version::from_json(json!(42));
    /// assert_eq!(v, Version::Integer(42));
    /// ```
    #[must_use]
    pub fn from_json(value: serde_json::Value) -> Self {
        match value {
            serde_json::Value::String(s) => Version::String(s),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Version::Integer(i)
                } else {
                    Version::String(n.to_string())
                }
            }
            v => Version::String(v.to_string()),
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::String(s) => write!(f, "{}", s),
            Version::Integer(i) => write!(f, "{}", i),
        }
    }
}

impl From<String> for Version {
    #[inline]
    fn from(s: String) -> Self {
        Version::String(s)
    }
}

impl From<&str> for Version {
    #[inline]
    fn from(s: &str) -> Self {
        Version::String(s.to_string())
    }
}

impl From<i64> for Version {
    #[inline]
    fn from(n: i64) -> Self {
        Version::Integer(n)
    }
}

impl From<i32> for Version {
    #[inline]
    fn from(n: i32) -> Self {
        Version::Integer(n as i64)
    }
}

impl Default for Version {
    fn default() -> Self {
        Version::String(String::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_new() {
        let v = Version::new("abc123");
        assert_eq!(v, Version::String("abc123".to_string()));
    }

    #[test]
    fn test_version_integer() {
        let v = Version::integer(42);
        assert_eq!(v, Version::Integer(42));
    }

    #[test]
    fn test_version_display_string() {
        let v = Version::String("v1".to_string());
        assert_eq!(v.to_string(), "v1");
    }

    #[test]
    fn test_version_display_integer() {
        let v = Version::Integer(42);
        assert_eq!(v.to_string(), "42");
    }

    #[test]
    fn test_version_from_str() {
        let v: Version = "v1".into();
        assert_eq!(v, Version::String("v1".to_string()));
    }

    #[test]
    fn test_version_from_string() {
        let v: Version = String::from("v1").into();
        assert_eq!(v, Version::String("v1".to_string()));
    }

    #[test]
    fn test_version_from_i64() {
        let v: Version = 42i64.into();
        assert_eq!(v, Version::Integer(42));
    }

    #[test]
    fn test_version_from_i32() {
        let v: Version = 42i32.into();
        assert_eq!(v, Version::Integer(42));
    }

    #[test]
    fn test_is_string() {
        assert!(Version::new("abc").is_string());
        assert!(!Version::Integer(42).is_string());
    }

    #[test]
    fn test_is_integer() {
        assert!(Version::Integer(42).is_integer());
        assert!(!Version::new("abc").is_integer());
    }

    #[test]
    fn test_as_str() {
        let v = Version::new("abc");
        assert_eq!(v.as_str(), Some("abc"));

        let v = Version::Integer(42);
        assert_eq!(v.as_str(), None);
    }

    #[test]
    fn test_as_integer() {
        let v = Version::Integer(42);
        assert_eq!(v.as_integer(), Some(42));

        let v = Version::new("abc");
        assert_eq!(v.as_integer(), None);
    }

    #[test]
    fn test_to_json_string() {
        let v = Version::new("abc");
        assert_eq!(v.to_json(), serde_json::json!("abc"));
    }

    #[test]
    fn test_to_json_integer() {
        let v = Version::Integer(42);
        assert_eq!(v.to_json(), serde_json::json!(42));
    }

    #[test]
    fn test_from_json_string() {
        let v = Version::from_json(serde_json::json!("abc"));
        assert_eq!(v, Version::String("abc".to_string()));
    }

    #[test]
    fn test_from_json_integer() {
        let v = Version::from_json(serde_json::json!(42));
        assert_eq!(v, Version::Integer(42));
    }

    #[test]
    fn test_version_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Version::new("v1"));
        set.insert(Version::new("v2"));
        set.insert(Version::Integer(1));
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn test_version_default() {
        let v = Version::default();
        assert_eq!(v, Version::String(String::new()));
    }
}
