use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub sync: HashMap<String, bool>,
    #[serde(default)]
    pub cookies: HashMap<String, String>,
    #[serde(default = "default_port")]
    pub port: u16,
    /// Patterns to ignore (from .braidignore)
    #[serde(default)]
    pub ignore_patterns: Vec<String>,
    /// Debounce delay in milliseconds for file changes
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,
}

fn default_debounce_ms() -> u64 {
    100
}

fn default_port() -> u16 {
    45678
}

impl Config {
    pub async fn load() -> Result<Self> {
        let config_path = get_config_path()?;

        if !config_path.exists() {
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&config_path)
            .await
            .with_context(|| format!("Failed to read config from {:?}", config_path))?;

        let config: Config =
            serde_json::from_str(&content).with_context(|| "Failed to parse config JSON")?;

        Ok(config)
    }

    pub async fn save(&self) -> Result<()> {
        let config_path = get_config_path()?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let content = serde_json::to_string_pretty(self)?;
        fs::write(&config_path, content)
            .await
            .with_context(|| format!("Failed to write config to {:?}", config_path))?;

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sync: HashMap::new(),
            cookies: HashMap::new(),
            port: default_port(),
            ignore_patterns: default_ignore_patterns(),
            debounce_ms: default_debounce_ms(),
        }
    }
}

/// Default patterns to ignore (.git, node_modules, etc.)
fn default_ignore_patterns() -> Vec<String> {
    vec![
        ".git".to_string(),
        ".git/**".to_string(),
        "node_modules/**".to_string(),
        ".DS_Store".to_string(),
        "*.swp".to_string(),
        "*.swo".to_string(),
        "*~".to_string(),
        ".braidfs/**".to_string(),
    ]
}

pub fn get_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    Ok(home.join("http").join(".braidfs").join("config"))
}

pub fn get_root_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    Ok(home.join("http"))
}

/// Get the trash directory for deleted files.
pub fn get_trash_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    Ok(home.join("http").join(".braidfs").join("trash"))
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
/// Matches JS skip_file() function from braidfs/index.js.
pub fn skip_file(path: &str) -> bool {
    // Paths with # can't map to real URLs
    if path.contains('#') {
        return true;
    }

    // .DS_Store files
    if path.ends_with(".DS_Store") {
        return true;
    }

    // Skip .braidfs/ except config and errors
    if path.starts_with(".braidfs")
        && !path.starts_with(".braidfs/config")
        && !path.starts_with(".braidfs/errors")
    {
        return true;
    }

    false
}

/// Move a file to the trash directory instead of deleting it.
/// Matches JS trash_file() function from braidfs/index.js.
pub async fn trash_file(fullpath: &std::path::Path, path: &str) -> Result<PathBuf> {
    let trash_dir = get_trash_dir()?;
    tokio::fs::create_dir_all(&trash_dir).await?;

    // Generate unique trash filename using UUID
    let random = uuid::Uuid::new_v4().to_string()[..8].to_string();

    let filename = path.replace(['/', '\\'], "_");
    let dest = trash_dir.join(format!("{}_{}", filename, random));

    tokio::fs::rename(fullpath, &dest).await?;

    tracing::warn!("Moved unsynced file to trash: {:?} -> {:?}", fullpath, dest);

    Ok(dest)
}
