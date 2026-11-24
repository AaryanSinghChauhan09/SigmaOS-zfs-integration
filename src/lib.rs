//! SigmaOS zfs-integration — ZFS dataset management helpers.
//!
//! Tracks ZFS datasets, snapshot policy, and scrub schedule for SigmaOS.
//! State is JSON-backed; actual zfs(8) operations are delegated by callers.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Errors produced by this component.
#[derive(Debug, thiserror::Error)]
pub enum ZfsError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("dataset not found: {0}")]
    NotFound(String),
    #[error("validation error: {0}")]
    Validation(String),
}

/// Dataset record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub name: String,
    /// "filesystem" | "volume" | "snapshot"
    pub kind: String,
    /// mountpoint (empty for volumes)
    #[serde(default)]
    pub mountpoint: String,
    /// compression e.g. "lz4", "zstd", "off"
    #[serde(default = "default_compression")]
    pub compression: String,
    /// whether snapshots are enabled
    #[serde(default = "default_true")]
    pub snapshots_enabled: bool,
    /// retention: keep last N snapshots
    #[serde(default = "default_retention")]
    pub snapshot_retention: u32,
}

fn default_compression() -> String {
    "lz4".into()
}
fn default_true() -> bool {
    true
}
fn default_retention() -> u32 {
    8
}

/// Scrub schedule entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrubSchedule {
    /// cron-like expression, e.g. "0 3 * * 0"
    pub cron: String,
    /// datasets included (empty = all)
    #[serde(default)]
    pub datasets: Vec<String>,
}

/// ZFS integration state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ZfsState {
    #[serde(default)]
    pub datasets: BTreeMap<String, Dataset>,
    #[serde(default)]
    pub scrub: Option<ScrubSchedule>,
    /// last scrub timestamp (unix)
    #[serde(default)]
    pub last_scrub: Option<u64>,
}

impl ZfsState {
    /// Register a dataset.
    pub fn add_dataset(&mut self, ds: Dataset) -> Result<(), ZfsError> {
        if ds.name.is_empty() || !ds.name.contains('/') {
            return Err(ZfsError::Validation(format!(
                "invalid dataset name: {}",
                ds.name
            )));
        }
        self.datasets.insert(ds.name.clone(), ds);
        Ok(())
    }

    /// Remove dataset record.
    pub fn remove_dataset(&mut self, name: &str) -> Result<(), ZfsError> {
        if self.datasets.remove(name).is_none() {
            return Err(ZfsError::NotFound(name.into()));
        }
        Ok(())
    }

    /// Datasets with snapshots enabled.
    pub fn snapshot_targets(&self) -> Vec<&Dataset> {
        self.datasets
            .values()
            .filter(|d| d.snapshots_enabled)
            .collect()
    }

    /// Datasets matching a pool prefix (e.g. "tank/").
    pub fn by_pool(&self, pool: &str) -> Vec<&Dataset> {
        self.datasets
            .values()
            .filter(|d| d.name.starts_with(pool))
            .collect()
    }

    /// Validate dataset names referenced in scrub schedule exist.
    pub fn validate(&self) -> Result<(), ZfsError> {
        if let Some(s) = &self.scrub {
            for d in &s.datasets {
                if !self.datasets.contains_key(d) {
                    return Err(ZfsError::Validation(format!(
                        "scrub references unknown dataset: {d}"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Save to JSON.
    pub fn save(&self, path: &Path) -> Result<(), ZfsError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    /// Load from JSON.
    pub fn load(path: &Path) -> Result<Self, ZfsError> {
        let text = std::fs::read_to_string(path)
            .map_err(|_| ZfsError::NotFound(path.display().to_string()))?;
        Ok(serde_json::from_str(&text)?)
    }
}

/// Default state path.
pub fn default_state_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("org", "sigmaos", "sigmaos")
        .map(|p| p.data_dir().join("zfs").join("state.json"))
}
