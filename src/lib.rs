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

#[cfg(test)]
mod tests {
    use super::*;

    fn ds(name: &str) -> Dataset {
        Dataset {
            name: name.into(),
            kind: "filesystem".into(),
            mountpoint: "/mnt/x".into(),
            compression: "lz4".into(),
            snapshots_enabled: true,
            snapshot_retention: 8,
        }
    }

    #[test]
    fn add_remove_and_validation() {
        let mut st = ZfsState::default();
        st.add_dataset(ds("tank/home")).unwrap();
        assert!(st.datasets.contains_key("tank/home"));
        assert!(matches!(
            st.add_dataset(ds("noslash")),
            Err(ZfsError::Validation(_))
        ));
        assert!(matches!(
            st.add_dataset(ds("")),
            Err(ZfsError::Validation(_))
        ));
        st.remove_dataset("tank/home").unwrap();
        assert!(matches!(
            st.remove_dataset("tank/home"),
            Err(ZfsError::NotFound(_))
        ));
    }

    #[test]
    fn snapshot_targets_and_by_pool() {
        let mut st = ZfsState::default();
        let mut no_snaps = ds("tank/data");
        no_snaps.snapshots_enabled = false;
        st.add_dataset(ds("tank/home")).unwrap();
        st.add_dataset(no_snaps).unwrap();
        st.add_dataset(ds("other/vm")).unwrap();
        assert_eq!(st.snapshot_targets().len(), 2);
        assert_eq!(st.by_pool("tank/").len(), 2);
        assert_eq!(st.by_pool("other/").len(), 1);
    }

    #[test]
    fn validate_scrub_references() {
        let mut st = ZfsState::default();
        st.add_dataset(ds("tank/home")).unwrap();
        st.scrub = Some(ScrubSchedule {
            cron: "0 3 * * 0".into(),
            datasets: vec!["tank/home".into()],
        });
        assert!(st.validate().is_ok());
        st.scrub
            .as_mut()
            .unwrap()
            .datasets
            .push("tank/ghost".into());
        assert!(matches!(st.validate(), Err(ZfsError::Validation(_))));
        // Empty dataset list = all datasets, always valid.
        st.scrub.as_mut().unwrap().datasets.clear();
        assert!(st.validate().is_ok());
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/state.json");
        let mut st = ZfsState::default();
        st.add_dataset(ds("tank/home")).unwrap();
        st.last_scrub = Some(1_700_000_000);
        st.save(&path).unwrap();
        let loaded = ZfsState::load(&path).unwrap();
        assert_eq!(loaded.datasets.len(), 1);
        assert_eq!(loaded.last_scrub, Some(1_700_000_000));
        assert!(ZfsState::load(&dir.path().join("missing.json")).is_err());
    }

    // Arrange: dataset with custom retention and compression
    // Act: serialize and deserialize via save/load
    // Assert: custom fields preserved
    #[test]
    fn test_dataset_retention_and_compression_preserved_aaa() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zfs.json");
        let mut st = ZfsState::default();
        let mut custom = ds("rpool/ROOT/sigma");
        custom.compression = "zstd".into();
        custom.snapshot_retention = 24;
        st.add_dataset(custom).unwrap();
        st.save(&path).unwrap();
        let back = ZfsState::load(&path).unwrap();
        let got = back.datasets.get("rpool/ROOT/sigma").unwrap();
        assert_eq!(got.compression, "zstd");
        assert_eq!(got.snapshot_retention, 24);
    }

    // Arrange: ZfsState with datasets across pools
    // Act: query by_pool for non-existent pool
    // Assert: empty result
    #[test]
    fn test_by_pool_unknown_returns_empty_aaa() {
        let mut st = ZfsState::default();
        st.add_dataset(ds("tank/var")).unwrap();
        assert!(st.by_pool("nonexistent/").is_empty());
    }

    // Arrange: ZfsState with multiple datasets under "tank/"
    // Act: remove all datasets in "tank/"
    // Assert: remaining datasets only from other pools
    #[test]
    fn test_recursive_pool_cleanup_aaa() {
        let mut st = ZfsState::default();
        st.add_dataset(ds("tank/root")).unwrap();
        st.add_dataset(ds("tank/home")).unwrap();
        st.add_dataset(ds("backup/archive")).unwrap();
        let to_remove: Vec<String> = st.by_pool("tank/").into_iter().map(|d| d.name.clone()).collect();
        for name in to_remove {
            st.remove_dataset(&name).unwrap();
        }
        assert_eq!(st.datasets.len(), 1);
        assert!(st.datasets.contains_key("backup/archive"));
    }

    // Arrange: invalid JSON file on disk
    // Act: ZfsState::load
    // Assert: returns ZfsError::Json
    #[test]
    fn test_load_corrupt_json_returns_error_aaa() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.json");
        std::fs::write(&path, "{not valid json").unwrap();
        assert!(matches!(ZfsState::load(&path), Err(ZfsError::Json(_))));
    }
}
