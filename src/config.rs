use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
pub struct Config {
    /// Cosine similarity threshold for the speaker gate (default 0.75).
    pub threshold: f32,
    /// Number of consecutive below-threshold frames before muting (default 3).
    pub vote_window: usize,
    /// Whether to run the nnnoiseless denoiser before speaker embedding (default true).
    #[serde(default = "default_true")]
    pub noise_suppression: bool,
}

fn default_true() -> bool {
    true
}

impl Default for Config {
    fn default() -> Self {
        Self {
            threshold: 0.75,
            vote_window: 3,
            noise_suppression: true,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if path.exists() {
            let json = std::fs::read_to_string(path)?;
            Ok(serde_json::from_str(&json)?)
        } else {
            Ok(Self::default())
        }
    }

    #[allow(dead_code)]
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)?)?;
        Ok(())
    }
}

/// Returns the Voce data directory: `~/.voce/`
pub fn voce_dir() -> PathBuf {
    dirs::home_dir()
        .expect("home directory not found")
        .join(".voce")
}

pub fn models_dir() -> PathBuf {
    voce_dir().join("models")
}

pub fn enrolled_embedding_path() -> PathBuf {
    voce_dir().join("enrolled_embedding.json")
}

pub fn config_path() -> PathBuf {
    voce_dir().join("config.json")
}
