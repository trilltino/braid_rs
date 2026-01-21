use crate::core::Version;
use anyhow::Result;
use bytes::Bytes;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::broadcast;
use tokio::sync::Mutex;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlobMetadata {
    pub key: String,
    pub version: Vec<Version>,
    pub content_type: Option<String>,
    pub parents: Vec<Version>,
    /// SHA-256 hash of blob content for deduplication.
    #[serde(default)]
    pub content_hash: Option<String>,
    /// Size of the blob in bytes.
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Clone, Debug)]
pub enum StoreEvent {
    Put {
        meta: BlobMetadata,
        data: Bytes,
    },
    Delete {
        key: String,
        version: Vec<Version>,
        content_type: Option<String>,
    },
}

#[derive(Clone, Debug)]
pub struct BlobStore {
    db_path: PathBuf,
    meta_db_path: PathBuf,
    meta_conn: Arc<Mutex<Connection>>,
    tx: broadcast::Sender<StoreEvent>,
}

impl BlobStore {
    pub async fn new(db_path: PathBuf, meta_db_path: PathBuf) -> Result<Self> {
        fs::create_dir_all(&db_path).await?;
        if let Some(parent) = meta_db_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let conn = Connection::open(&meta_db_path)?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value JSON
            )",
            [],
        )?;

        let (tx, _) = broadcast::channel(100);

        Ok(Self {
            db_path,
            meta_db_path,
            meta_conn: Arc::new(Mutex::new(conn)),
            tx,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<StoreEvent> {
        self.tx.subscribe()
    }

    pub async fn get(&self, key: &str) -> Result<Option<(Bytes, BlobMetadata)>> {
        let meta = {
            let conn = self.meta_conn.lock().await;
            let mut stmt = conn.prepare("SELECT value FROM meta WHERE key = ?")?;
            let mut rows = stmt.query(params![key])?;

            if let Some(row) = rows.next()? {
                let value_str: String = row.get(0)?;
                serde_json::from_str::<BlobMetadata>(&value_str)?
            } else {
                return Ok(None);
            }
        };

        let file_path = self.get_file_path(key);
        if fs::try_exists(&file_path).await? {
            let data = fs::read(&file_path).await?;
            Ok(Some((Bytes::from(data), meta)))
        } else {
            Ok(None)
        }
    }

    pub async fn get_meta(&self, key: &str) -> Result<Option<BlobMetadata>> {
        let conn = self.meta_conn.lock().await;
        let mut stmt = conn.prepare("SELECT value FROM meta WHERE key = ?")?;
        let mut rows = stmt.query(params![key])?;

        if let Some(row) = rows.next()? {
            let value_str: String = row.get(0)?;
            Ok(Some(serde_json::from_str::<BlobMetadata>(&value_str)?))
        } else {
            Ok(None)
        }
    }

    pub async fn put(
        &self,
        key: &str,
        data: Bytes,
        version: Vec<Version>,
        parents: Vec<Version>,
        content_type: Option<String>,
    ) -> Result<Vec<Version>> {
        let current_meta = self.get_meta(key).await?;
        let new_ver_str = version.first().map(|v| v.to_string()).unwrap_or_default();
        if let Some(meta) = &current_meta {
            let current_ver_str = meta
                .version
                .first()
                .map(|v| v.to_string())
                .unwrap_or_default();
            if compare_versions(&new_ver_str, &current_ver_str) <= 0 {
                return Ok(meta.version.clone());
            }
        }

        // Compute content hash for deduplication
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let hash_bytes = hasher.finalize();
        let content_hash = format!("{:x}", hash_bytes);

        let new_meta = BlobMetadata {
            key: key.to_string(),
            version: version.clone(),
            content_type,
            parents,
            content_hash: Some(content_hash.clone()),
            size: Some(data.len() as u64),
        };

        // Write file
        let file_path = self.get_file_path(key);
        fs::write(&file_path, &data).await?;

        // Update metadata
        {
            let conn = self.meta_conn.lock().await;
            let val_str = serde_json::to_string(&new_meta)?;
            conn.execute(
                "INSERT OR REPLACE INTO meta (key, value) VALUES (?, ?)",
                params![key, val_str],
            )?;
        }

        // Notify subscribers
        let _ = self.tx.send(StoreEvent::Put {
            meta: new_meta.clone(),
            data: data.clone(),
        });

        Ok(version)
    }

    pub async fn delete(&self, key: &str) -> Result<()> {
        let current_meta = self.get_meta(key).await?;

        {
            let conn = self.meta_conn.lock().await;
            conn.execute("DELETE FROM meta WHERE key = ?", params![key])?;
        }

        let file_path = self.get_file_path(key);
        if fs::try_exists(&file_path).await? {
            fs::remove_file(&file_path).await?;
        }

        if let Some(meta) = current_meta {
            let _ = self.tx.send(StoreEvent::Delete {
                key: key.to_string(),
                version: meta.version,
                content_type: meta.content_type,
            });
        } else {
            let _ = self.tx.send(StoreEvent::Delete {
                key: key.to_string(),
                version: vec![],
                content_type: None,
            });
        }

        Ok(())
    }

    fn get_file_path(&self, key: &str) -> PathBuf {
        self.db_path.join(encode_filename(key))
    }
}

fn compare_versions(a: &str, b: &str) -> i32 {
    let seq_a = get_event_seq(a);
    let seq_b = get_event_seq(b);

    let c = compare_seqs(seq_a, seq_b);
    if c != 0 {
        return c;
    }

    if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }
}

fn get_event_seq(e: &str) -> &str {
    if e.is_empty() {
        return "";
    }
    if let Some(idx) = e.rfind('-') {
        &e[idx + 1..]
    } else {
        e
    }
}

fn compare_seqs(a: &str, b: &str) -> i32 {
    if a.len() != b.len() {
        return (a.len() as i32) - (b.len() as i32);
    }
    if a < b {
        -1
    } else if a > b {
        1
    } else {
        0
    }
}

pub fn encode_filename(s: &str) -> String {
    // 1. Calculate postfix based on case sensitivity (c === c.toUpperCase())
    // In JS: var bits = s.match(/\p{L}/ug).map(c => +(c === c.toUpperCase())).join('')
    // In Rust, we iterate chars, check if alphabetic, then check uppercase.
    // Note: JS logic uses \p{L} (Unicode letters).
    let bits: String = s
        .chars()
        .filter(|c| c.is_alphabetic())
        .map(|c| if c.is_uppercase() { "1" } else { "0" })
        .collect();

    // var postfix = BigInt('0b0' + bits).toString(16)
    let postfix = if bits.is_empty() {
        "0".to_string()
    } else {
        // Rust doesn't have BigInt built-in, but we can handle this.
        // For very long filenames, bits string can be huge.
        // JS uses BigInt.
        // For MVP, if bits is small enough, u128. If larger... we might need num-bigint.
        // Let's assume for now it fits or we use a simple chunked hex conversion.
        // Actually, we can just process the bits into hex directly.
        bits_to_hex(&bits)
    };

    // 2. Swap ! and /
    // s = s.replace(/[\/!]/g, x => x === '/' ? '!' : '/')
    let mut s = s
        .chars()
        .map(|c| match c {
            '/' => '!',
            '!' => '/',
            _ => c,
        })
        .collect::<String>();

    // 3. Encode unsafe characters
    // s = s.replace(/[<>:"/|\\?*%\x00-\x1f\x7f]/g, encode_char)
    // Note: '/' was swapped to '!', so we check for other unsafe chars.
    // The previous swap guarantees no '/' remains, but we might have new '!' which are safe.
    // JS Regex: /[<>:"/|\\?*%\x00-\x1f\x7f]/
    // Wait, JS swap happens BEFORE encoding unsafe chars.
    // The unsafe chars array includes '/'. But we just swapped '/' to '!'.
    // So '/' will not match.
    // '!' is NOT in the unsafe list.
    let mut encoded = String::new();
    for c in s.chars() {
        if matches!(
            c,
            '<' | '>' | ':' | '"' | '/' | '|' | '\\' | '?' | '*' | '%' | '\x00'..='\x1f' | '\x7f'
        ) {
            encoded.push_str(&format!("%{:02X}", c as u8));
        } else {
            encoded.push(c);
        }
    }
    s = encoded;

    // 4. Deal with windows reserved words
    // if (s.match(/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i))
    //     s = s.slice(0, 2) + encode_char(s[2]) + s.slice(3)
    // using regex for simplicity or manual check
    let is_reserved = {
        let lower = s.to_lowercase();
        let name_part = lower.split('.').next().unwrap_or("");
        matches!(name_part, "con" | "prn" | "aux" | "nul")
            || (name_part.len() == 4
                && name_part.starts_with("com")
                && name_part
                    .chars()
                    .nth(3)
                    .map_or(false, |c| c.is_ascii_digit() && c != '0'))
            || (name_part.len() == 4
                && name_part.starts_with("lpt")
                && name_part
                    .chars()
                    .nth(3)
                    .map_or(false, |c| c.is_ascii_digit() && c != '0'))
    };

    if is_reserved {
        // encode 3rd char (index 2)
        // Check if string has at least 3 chars
        if s.len() >= 3 {
            let char_at_2 = s.chars().nth(2).unwrap();
            let encoded_char = format!("%{:02X}", char_at_2 as u8);
            // Reconstruct: slice(0,2) + encoded + slice(3)
            // Careful with verify unicode boundaries if needed, but here simple ascii check basically
            let mut chars: Vec<char> = s.chars().collect();
            let prefix: String = chars.iter().take(2).collect();
            let suffix: String = chars.iter().skip(3).collect();
            s = format!("{}{}{}", prefix, encoded_char, suffix);
        }
    }

    // 5. Append postfix
    format!("{}.{}", s, postfix)
}

fn bits_to_hex(bits: &str) -> String {
    if bits.is_empty() {
        return "0".to_string();
    }

    // Pad to multiple of 4 with leading 0s
    let rem = bits.len() % 4;
    let padded = if rem == 0 {
        bits.to_string()
    } else {
        format!("{}{}", "0".repeat(4 - rem), bits)
    };

    let mut hex = String::new();
    for chunk in padded.as_bytes().chunks(4) {
        let chunk_str = std::str::from_utf8(chunk).unwrap();
        let val = u8::from_str_radix(chunk_str, 2).unwrap();
        hex.push_str(&format!("{:x}", val));
    }

    // JS BigInt(0b...) toString(16) does not output leading zeros
    // But our chunking might.
    // Example: "0001" -> "1". "0000" -> "0".
    // "00010000" -> "10".

    // Find first non-zero char to trim leading zeros, unless it is just "0"
    let trimmed = hex.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Decode a filename back to the original key.
/// Matches JS `decode_filename()` from braid-blob/index.js.
pub fn decode_filename(s: &str) -> String {
    // 1. Remove the postfix (everything after the last '.')
    let s = if let Some(idx) = s.rfind('.') {
        &s[..idx]
    } else {
        s
    };

    // 2. Decode percent-encoded characters
    let mut decoded = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    decoded.push(byte as char);
                    continue;
                }
            }
            decoded.push('%');
            decoded.push_str(&hex);
        } else {
            decoded.push(c);
        }
    }

    // 3. Swap ! and / back
    decoded
        .chars()
        .map(|c| match c {
            '!' => '/',
            '/' => '!',
            _ => c,
        })
        .collect()
}

/// Increment a sequence string.
/// Matches JS `increment_seq()` from braid-blob/index.js.
pub fn increment_seq(s: &str) -> String {
    if s.is_empty() {
        return "1".to_string();
    }

    let mut chars: Vec<char> = s.chars().collect();
    let mut carry = true;

    for i in (0..chars.len()).rev() {
        if !carry {
            break;
        }

        let c = chars[i];
        if c == '9' {
            chars[i] = '0';
            carry = true;
        } else if c.is_ascii_digit() {
            chars[i] = (c as u8 + 1) as char;
            carry = false;
        } else {
            // Non-digit, stop
            break;
        }
    }

    if carry {
        // Need to prepend a 1
        format!("1{}", chars.iter().collect::<String>())
    } else {
        chars.iter().collect()
    }
}

/// Return the maximum of two sequences.
/// Matches JS `max_seq()` from braid-blob/index.js.
pub fn max_seq<'a>(a: &'a str, b: &'a str) -> &'a str {
    if compare_seqs(a, b) >= 0 {
        a
    } else {
        b
    }
}

/// Atomically write data to a file.
/// Writes to a temp file first, then renames.
/// Matches JS `atomic_write()` from braid-blob/index.js.
pub async fn atomic_write(
    dest: &std::path::Path,
    data: &[u8],
    temp_folder: &std::path::Path,
) -> Result<std::fs::Metadata> {
    use tokio::fs;

    // Create temp folder if needed
    fs::create_dir_all(temp_folder).await?;

    // Generate temp file name
    let temp_name = format!("tmp_{}", uuid::Uuid::new_v4());
    let temp_path = temp_folder.join(temp_name);

    // Write to temp file
    fs::write(&temp_path, data).await?;

    // Ensure parent directory exists
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Rename to final destination (atomic on most filesystems)
    fs::rename(&temp_path, dest).await?;

    // Return metadata
    let metadata = std::fs::metadata(dest)?;
    Ok(metadata)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let original = "path/to/file.txt";
        let encoded = encode_filename(original);
        let decoded = decode_filename(&encoded);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_increment_seq() {
        assert_eq!(increment_seq(""), "1");
        assert_eq!(increment_seq("0"), "1");
        assert_eq!(increment_seq("9"), "10");
        assert_eq!(increment_seq("99"), "100");
        assert_eq!(increment_seq("123"), "124");
    }

    #[test]
    fn test_max_seq() {
        assert_eq!(max_seq("1", "2"), "2");
        assert_eq!(max_seq("10", "2"), "10");
        assert_eq!(max_seq("99", "100"), "100");
    }
}
