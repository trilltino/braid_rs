use crate::core::{BraidError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

/// Default application name used for directories, peer IDs, etc.
pub const DEFAULT_APP_NAME: &str = "braidfs";

/// Configuration for the filesystem sync daemon.
/// 
/// This struct can be used by any application - set `app_name` to customize
/// directory names and peer ID prefixes.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub peer_id: String,
    #[serde(default)]
    pub sync: HashMap<String, bool>,
    #[serde(default)]
    pub cookies: HashMap<String, String>,
    #[serde(default)]
    pub identities: HashMap<String, String>,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Patterns to ignore (from .braidignore)
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    /// Debounce delay in milliseconds for file changes
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
    /// Application name - used for directory naming and peer ID prefix.
    /// Defaults to "braidfs" if not set.
    #[serde(default = "default_app_name")]
    pub app_name: String,
    /// Root directory name within the home directory.
    /// If not set, defaults to the app_name.
    #[serde(default)]
    pub root_dir_name: Option<String>,
}

fn default_debounce_ms() -> u64 {
    100
}

fn default_port() -> u16 {
    0 // Let OS assign a random port by default
}

fn default_app_name() -> String {
    DEFAULT_APP_NAME.to_string()
}

impl Config {
    /// Load config from the default location.
    /// Uses default app name for initial load.
    pub async fn load() -> Result<Self> {
        let default_config = Config::default();
        let config_path = default_config.get_config_path()?;

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&config_path)
            .await
            .map_err(|e| BraidError::Io(e))?;

        let mut config: Config = serde_json::from_str(&content).map_err(|e| BraidError::Json(e))?;

        if config.peer_id.is_empty() {
            let prefix = &config.app_name;
            config.peer_id = format!("{}-{}", prefix, &uuid::Uuid::new_v4().to_string()[..8]);
            config.save().await?;
        }

        Ok(config)
    }

    pub async fn save(&self) -> Result<()> {
        let config_path = self.get_config_path()?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| BraidError::Io(e))?;
        }

        let content = serde_json::to_string_pretty(self).map_err(|e| BraidError::Json(e))?;
        let _ = fs::write(&config_path, content)
            .await
            .map_err(|e| BraidError::Io(e))?;

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        let app_name = default_app_name();
        Self {
            peer_id: format!("{}-{}", app_name, &uuid::Uuid::new_v4().to_string()[..8]),
            sync: HashMap::new(),
            cookies: HashMap::new(),
            identities: HashMap::new(),
            port: default_port(),
            ignore_patterns: default_ignore_patterns(),
            debounce_ms: default_debounce_ms(),
            app_name,
            root_dir_name: None,
        }
    }
}

/// Default patterns to ignore (.git, node_modules, etc.)
/// Note: App-specific directories (like .braidfs) are added dynamically
fn default_ignore_patterns() -> Vec<String> {
    vec![
        ".git".to_string(),
        ".git/**".to_string(),
        "node_modules/**".to_string(),
        ".DS_Store".to_string(),
        "*.swp".to_string(),
        "*.swo".to_string(),
        "*~".to_string(),
    ]
}

impl Config {
    /// Get the root directory for synced files.
    /// Uses `root_dir_name` if set, otherwise uses `app_name`.
    pub fn get_root_dir(&self) -> Result<PathBuf> {
        let home =
            dirs::home_dir().ok_or_else(|| BraidError::Fs("Could not find home directory".into()))?;
        let dir_name = self.root_dir_name.as_ref().unwrap_or(&self.app_name);
        Ok(home.join(dir_name))
    }

    /// Get the metadata directory path (e.g., .braidfs)
    pub fn get_meta_dir(&self) -> Result<PathBuf> {
        Ok(self.get_root_dir()?.join(format!(".{}", self.app_name)))
    }

    /// Get the config file path
    pub fn get_config_path(&self) -> Result<PathBuf> {
        Ok(self.get_meta_dir()?.join("config"))
    }

    /// Get the trash directory for deleted files
    pub fn get_trash_dir(&self) -> Result<PathBuf> {
        Ok(self.get_meta_dir()?.join("trash"))
    }

    /// Get the versions file path
    pub fn get_versions_path(&self) -> Result<PathBuf> {
        Ok(self.get_meta_dir()?.join("versions.json"))
    }

    /// Check if a path should be skipped during sync.
    /// Now includes the app-specific meta directory.
    pub fn should_skip_path(&self, path: &str) -> bool {
        // Paths with # can't map to real URLs
        if path.contains('#') {
            return true;
        }

        // .DS_Store files
        if path.ends_with(".DS_Store") {
            return true;
        }

        // Skip app meta directory except config and errors
        let meta_dir = format!(".{}/", self.app_name);
        if path.starts_with(&meta_dir)
            && !path.starts_with(&format!(".{}/config", self.app_name))
            && !path.starts_with(&format!(".{}/errors", self.app_name))
        {
            return true;
        }

        // Check user-defined patterns
        for pattern in &self.ignore_patterns {
            if matches_pattern(path, pattern) {
                return true;
            }
        }

        false
    }
}

/// Simple glob-like pattern matching
fn matches_pattern(path: &str, pattern: &str) -> bool {
    // Handle ** wildcards (match any directory depth)
    if pattern.contains("/**") {
        let prefix = pattern.trim_end_matches("/**");
        return path == prefix || path.starts_with(&format!("{}/", prefix));
    }
    
    // Handle * wildcards
    if pattern.contains('*') {
        // Simple case: *.ext
        if pattern.starts_with("*.") {
            let ext = &pattern[1..]; // including the dot
            return path.ends_with(ext);
        }
        // For more complex patterns, we'd need a proper glob library
    }
    
    // Exact match
    path == pattern
}

/// Backward compatibility: Get config path using default app name
#[deprecated(since = "0.2.0", note = "Use Config::get_config_path() instead")]
pub fn get_config_path() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| BraidError::Fs("Could not find home directory".into()))?;
    Ok(home.join("braidfs").join(".braidfs").join("config"))
}

/// Backward compatibility: Get root dir using default app name  
#[deprecated(since = "0.2.0", note = "Use Config::get_root_dir() instead")]
pub fn get_root_dir() -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| BraidError::Fs("Could not find home directory".into()))?;
    Ok(home.join("braidfs"))
}

/// Check if a file is binary based on its extension.
/// Matches JS is_binary() function from braidfs/index.js.
pub fn is_binary(filename: &str) -> bool {
    let binary_extensions = [
        ".jpg", ".jpeg", ".png", ".gif", ".mp4", ".mp3", ".zip", ".tar", ".rar", ".pdf", ".doc",
        ".docx", ".xls", ".xlsx", ".ppt", ".pptx", ".exe", ".dll", ".so", ".dylib", ".bin", ".iso",
        ".img", ".bmp", ".tiff", ".svg", ".webp", ".avi", ".mov", ".wmv", ".flv", ".mkv", ".wav",
        ".flac", ".aac", ".ogg", ".wma", ".7z", ".gz", ".bz2", ".xz",
    ];

    let filename_lower = filename.to_lowercase();
    binary_extensions
        .iter()
        .any(|ext| filename_lower.ends_with(ext))
}

/// Check if a path should be skipped during sync.
/// 
/// Note: This is a standalone function that doesn't have access to Config.
/// For app-specific skip patterns, use `Config::should_skip_path()` instead.
/// 
/// Matches JS skip_file() function from braidfs/index.js:289.
pub fn skip_file(path: &str) -> bool {
    // Paths with # can't map to real URLs
    if path.contains('#') {
        return true;
    }

    // .DS_Store files
    if path.ends_with(".DS_Store") {
        return true;
    }

    false
}

/// Move a file to the trash directory instead of deleting it.
/// Matches JS trash_file() function from braidfs/index.js.
pub async fn trash_file(trash_dir: &std::path::Path, fullpath: &std::path::Path, path: &str) -> Result<PathBuf> {
    tokio::fs::create_dir_all(&trash_dir).await?;

    // Generate unique trash filename using UUID
    let random = uuid::Uuid::new_v4().to_string()[..8].to_string();

    let filename = path.replace(['/', '\\'], "_");
    let dest = trash_dir.join(format!("{}_{}", filename, random));

    tokio::fs::rename(fullpath, &dest).await?;

    tracing::warn!("Moved unsynced file to trash: {:?} -> {:?}", fullpath, dest);

    Ok(dest)
}
