use crate::core::Version;
use anyhow::Result; // Removed Context since unused
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct VersionStore {
    // Map URL/Path -> Version Info
    pub file_versions: HashMap<String, FileVersion>,
    #[serde(skip)]
    pub path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileVersion {
    pub current_version: Vec<String>, // Braid versions are DAGs (set of IDs)
    pub parents: Vec<String>,
    /// Content hash for this version (SHA-256).
    #[serde(default)]
    pub content_hash: Option<String>,
}

impl VersionStore {
    pub async fn load() -> Result<Self> {
        let store_path = get_store_path()?;
        Self::load_from(store_path).await
    }

    pub async fn load_from(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self {
                file_versions: HashMap::new(),
                path,
            });
        }

        let content = fs::read_to_string(&path).await?;
        let mut store: VersionStore = serde_json::from_str(&content).unwrap_or_default();
        store.path = path;
        Ok(store)
    }

    pub async fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&self.path, content).await?;
        Ok(())
    }

    pub fn update(&mut self, url: &str, version: Vec<Version>, parents: Vec<Version>) {
        self.file_versions.insert(
            url.to_string(),
            FileVersion {
                current_version: version.iter().map(|v| v.to_string()).collect(),
                parents: parents.iter().map(|v| v.to_string()).collect(),
                content_hash: None,
            },
        );
    }

    /// Update version with content hash.
    pub fn update_with_hash(
        &mut self,
        url: &str,
        version: Vec<Version>,
        parents: Vec<Version>,
        hash: Option<String>,
    ) {
        self.file_versions.insert(
            url.to_string(),
            FileVersion {
                current_version: version.iter().map(|v| v.to_string()).collect(),
                parents: parents.iter().map(|v| v.to_string()).collect(),
                content_hash: hash,
            },
        );
    }

    pub fn get(&self, url: &str) -> Option<&FileVersion> {
        self.file_versions.get(url)
    }

    /// Get version by content hash.
    /// Matches JS `hash_to_version_cache` lookup from braidfs/index.js.
    pub fn get_version_by_hash(&self, _fullpath: &str, hash: &str) -> Option<Vec<String>> {
        // Search all versions for matching hash
        for (_, fv) in &self.file_versions {
            if fv.content_hash.as_deref() == Some(hash) {
                return Some(fv.current_version.clone());
            }
        }
        None
    }

    /// Set content hash for a path.
    pub fn set_content_hash(&mut self, url: &str, hash: String) {
        if let Some(fv) = self.file_versions.get_mut(url) {
            fv.content_hash = Some(hash);
        }
    }
}

fn get_store_path() -> Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
    Ok(home.join("http").join(".braidfs").join("versions.json"))
}
