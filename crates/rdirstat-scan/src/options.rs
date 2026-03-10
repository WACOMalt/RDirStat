// RDirStat - Scan options
// License: GPL-2.0

use std::path::PathBuf;

/// Configuration options for a filesystem scan
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Root path to scan
    pub path: PathBuf,

    /// Whether to follow symbolic links
    pub follow_symlinks: bool,

    /// Whether to cross filesystem boundaries (mount points)
    pub cross_filesystems: bool,

    /// Glob patterns to exclude (e.g., ".git", "node_modules")
    pub exclude_patterns: Vec<String>,

    /// Maximum depth to scan (None = unlimited)
    pub max_depth: Option<u32>,

    /// Number of threads for parallel scanning (0 = auto)
    pub num_threads: usize,

    /// Whether to use apparent size (file length) vs allocated size (disk blocks)
    pub use_apparent_size: bool,

    /// Whether to skip hidden files/dirs (names starting with '.')
    pub skip_hidden: bool,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            path: PathBuf::from("."),
            follow_symlinks: false,
            cross_filesystems: false,
            exclude_patterns: Vec::new(),
            max_depth: None,
            num_threads: 0, // auto-detect
            skip_hidden: false,
            use_apparent_size: false,
        }
    }
}
