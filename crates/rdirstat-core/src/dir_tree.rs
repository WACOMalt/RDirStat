// RDirStat - DirTree: top-level scan result container
// License: GPL-2.0

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use crate::file_info::FileInfo;

/// Holds the result of a complete filesystem scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirTree {
    /// Root entry of the scanned directory
    pub root: FileInfo,

    /// Path that was scanned
    pub scan_path: PathBuf,

    /// How long the scan took
    pub scan_duration: Duration,

    /// Scan statistics
    pub stats: ScanStats,
}

/// Statistics collected during scanning
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanStats {
    /// Total files scanned
    pub total_files: u64,

    /// Total directories scanned
    pub total_dirs: u64,

    /// Total symlinks encountered
    pub total_symlinks: u64,

    /// Total hard link duplicates found (not double-counted)
    pub hard_link_duplicates: u64,

    /// Total errors encountered (permission denied, etc.)
    pub total_errors: u64,

    /// Paths that could not be read (permission denied)
    pub permission_denied: Vec<String>,

    /// Paths that triggered other errors
    pub other_errors: Vec<String>,

    /// Number of mount points skipped
    pub mount_points_skipped: u64,
}

impl DirTree {
    /// Create a new DirTree from a completed scan
    pub fn new(root: FileInfo, scan_path: PathBuf, scan_duration: Duration, stats: ScanStats) -> Self {
        Self {
            root,
            scan_path,
            scan_duration,
            stats,
        }
    }

    /// Total size of the scanned tree
    pub fn total_size(&self) -> u64 {
        self.root.total_size
    }

    /// Total number of files
    pub fn total_files(&self) -> u64 {
        self.root.total_files
    }

    /// Total number of directories
    pub fn total_dirs(&self) -> u64 {
        self.root.total_dirs
    }

    /// Get the N largest files
    pub fn top_files(&self, n: usize) -> Vec<&FileInfo> {
        self.root.top_n_files(n)
    }

    /// Get the N largest directories
    pub fn top_dirs(&self, n: usize) -> Vec<&FileInfo> {
        self.root.top_n_dirs(n)
    }

    /// Dynamically remove a node and its children from the tree, updating all parent sizes.
    /// Returns the removed `FileInfo` if successful.
    pub fn remove_node(&mut self, target: &std::path::Path) -> Option<FileInfo> {
        self.root.remove_node(target)
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
