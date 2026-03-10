// RDirStat - Parallel filesystem scanner using jwalk + statx
// License: GPL-2.0
//
// Speed strategy:
// 1. jwalk for parallel directory walking (rayon under the hood)
// 2. statx() for metadata (only request needed fields)
// 3. Inode-order traversal within each directory (jwalk sorts by default)
// 4. Filesystem boundary detection via device ID
// 5. Hard link deduplication via (inode, device) set

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use jwalk::WalkDirGeneric;

use rdirstat_core::dir_tree::{DirTree, ScanStats};
use rdirstat_core::file_info::{FileInfo, FileKind};

use crate::options::ScanOptions;

#[cfg(windows)]
#[path = "scanner_windows.rs"]
mod scanner_windows;

/// Progress information updated during scanning
#[derive(Debug)]
pub struct ScanProgress {
    pub files_scanned: AtomicU64,
    pub dirs_scanned: AtomicU64,
    pub bytes_counted: AtomicU64,
    pub current_path: Mutex<String>,
}

impl Default for ScanProgress {
    fn default() -> Self {
        Self {
            files_scanned: AtomicU64::new(0),
            dirs_scanned: AtomicU64::new(0),
            bytes_counted: AtomicU64::new(0),
            current_path: Mutex::new(String::new()),
        }
    }
}

/// The main scanner engine
pub struct Scanner {
    options: ScanOptions,
    progress: Arc<ScanProgress>,
}

impl Scanner {
    pub fn new(options: ScanOptions) -> Self {
        Self {
            options,
            progress: Arc::new(ScanProgress::default()),
        }
    }

    /// Get a reference to the progress tracker
    pub fn progress(&self) -> Arc<ScanProgress> {
        self.progress.clone()
    }

    /// Perform the scan and return a DirTree
    pub fn scan(&self) -> anyhow::Result<DirTree> {
        let start = Instant::now();
        let root_path = std::fs::canonicalize(&self.options.path)?;

        if !root_path.is_dir() {
            anyhow::bail!("Not a directory: {}", root_path.display());
        }

        // Get the device of the root path for filesystem boundary detection
        let root_meta = std::fs::metadata(&root_path)?;
        let root_device = get_device_id(&root_meta).unwrap_or(0);

        // Phase 1: Walk the filesystem in parallel using jwalk, collecting all entries
        let entries = self.collect_entries(&root_path, root_device)?;

        // Phase 2: Build the tree from the flat list
        let (root, stats) = self.build_tree(&root_path, entries)?;

        let duration = start.elapsed();

        Ok(DirTree::new(root, root_path, duration, stats))
    }

    /// Phase 1: Use jwalk to walk the filesystem in parallel,
    /// returning a flat list of (path, FileInfo) entries.
    fn collect_entries(
        &self,
        root_path: &Path,
        root_device: u64,
    ) -> anyhow::Result<Vec<FileInfo>> {
        #[cfg(windows)]
        {
            // If on Windows and Admin, attempt MFT!
            if scanner_windows::is_admin() {
                log::info!("Admin privileges detected. Attempting raw MFT scan...");
                match scanner_windows::collect_entries_mft(&self.options, &self.progress, root_path) {
                    Ok(entries) => return Ok(entries),
                    Err(e) => log::warn!("MFT scan failed: {}. Falling back to standard recursive scan...", e),
                }
            } else {
                log::info!("No Admin privileges. Using slow recursive scan.");
            }
        }

        let progress = self.progress.clone();
        let cross_fs = self.options.cross_filesystems;
        let follow_symlinks = self.options.follow_symlinks;
        let skip_hidden = self.options.skip_hidden;
        let max_depth = self.options.max_depth;
        let exclude_patterns: Vec<glob::Pattern> = self
            .options
            .exclude_patterns
            .iter()
            .filter_map(|p| glob::Pattern::new(p).ok())
            .collect();

        // Configure jwalk
        let mut walk_dir = WalkDirGeneric::<((), Option<FileInfo>)>::new(&root_path)
            .follow_links(follow_symlinks)
            .skip_hidden(skip_hidden)
            .sort(true) // Sort by name (helps with inode locality on many filesystems)
            .parallelism(if self.options.num_threads == 0 {
                jwalk::Parallelism::RayonDefaultPool {
                    busy_timeout: std::time::Duration::from_secs(1),
                }
            } else {
                let pool = rayon::ThreadPoolBuilder::new()
                    .num_threads(self.options.num_threads)
                    .build()
                    .expect("Failed to build rayon thread pool");
                jwalk::Parallelism::RayonExistingPool {
                    pool: Arc::new(pool),
                    busy_timeout: Some(std::time::Duration::from_secs(1)),
                }
            })
            .process_read_dir(move |_depth, _path, _read_dir_state, children| {
                // Filter out excluded entries before jwalk descends into them
                children.iter_mut().for_each(|dir_entry_result| {
                    if let Ok(dir_entry) = dir_entry_result {
                        let name = dir_entry.file_name.to_string_lossy().to_string();

                        // Check exclude patterns
                        if exclude_patterns.iter().any(|p| p.matches(&name)) {
                            dir_entry.read_children_path = None; // Don't descend
                            return;
                        }

                        // Check filesystem boundary
                        if !cross_fs {
                            if let Ok(meta) = dir_entry.path().symlink_metadata() {
                                if meta.is_dir() {
                                    if let Some(dev) = get_device_id(&meta) {
                                        if dev != root_device {
                                            dir_entry.read_children_path = None; // Don't cross
                                        }
                                    }
                                }
                            }
                        }
                    }
                });
            });

        if let Some(depth) = max_depth {
            walk_dir = walk_dir.max_depth(depth as usize);
        }

        // Iterate and collect entries
        let mut entries = Vec::new();

        for entry_result in walk_dir {
            match entry_result {
                Ok(entry) => {
                    let path = entry.path();
                    match std::fs::symlink_metadata(&path) {
                        Ok(meta) => {
                            let file_info = metadata_to_file_info(&path, &meta, entry.depth as u32);

                            // Update progress
                            if file_info.is_file() {
                                progress.files_scanned.fetch_add(1, Ordering::Relaxed);
                                progress
                                    .bytes_counted
                                    .fetch_add(file_info.size, Ordering::Relaxed);
                            } else if file_info.is_dir() {
                                progress.dirs_scanned.fetch_add(1, Ordering::Relaxed);
                                if let Ok(mut cp) = progress.current_path.lock() {
                                    *cp = path.to_string_lossy().to_string();
                                }
                            }

                            entries.push(file_info);
                        }
                        Err(e) => {
                            log::warn!("Failed to stat {}: {}", path.display(), e);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Walk error: {}", e);
                }
            }
        }

        Ok(entries)
    }

    /// Phase 2: Build a tree from a flat list of FileInfo entries.
    ///
    /// This reconstructs the parent-child relationships from the flat paths,
    /// performs hard link deduplication, and computes aggregate sizes.
    fn build_tree(
        &self,
        root_path: &Path,
        entries: Vec<FileInfo>,
    ) -> anyhow::Result<(FileInfo, ScanStats)> {
        let mut stats = ScanStats::default();
        let mut seen_inodes: HashSet<(u64, u64)> = HashSet::new();

        // Index: path -> list of children
        let mut children_map: HashMap<PathBuf, Vec<FileInfo>> = HashMap::new();
        let mut root_info: Option<FileInfo> = None;

        for mut entry in entries {
            // Hard link deduplication: for files with nlinks > 1
            if entry.is_file() && entry.nlinks > 1 {
                let key = (entry.inode, entry.device);
                if !seen_inodes.insert(key) {
                    entry.is_hardlink_duplicate = true;
                    stats.hard_link_duplicates += 1;
                }
            }

            // Track stats
            match entry.kind {
                FileKind::File => stats.total_files += 1,
                FileKind::Directory => stats.total_dirs += 1,
                FileKind::Symlink => stats.total_symlinks += 1,
                _ => {}
            }

            if entry.path == root_path {
                root_info = Some(entry);
            } else if let Some(parent) = entry.path.parent() {
                children_map
                    .entry(parent.to_path_buf())
                    .or_default()
                    .push(entry);
            }
        }

        let mut root = root_info.unwrap_or_else(|| {
            let mut fi = FileInfo::new(
                root_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "/".to_string()),
                root_path.to_path_buf(),
                FileKind::Directory,
            );
            fi.depth = 0;
            fi
        });

        // Recursively attach children
        assemble_tree(&mut root, &mut children_map);

        // Compute aggregate sizes
        root.finalize();

        Ok((root, stats))
    }
}

/// Recursively attach children from the flat map into the tree.
fn assemble_tree(node: &mut FileInfo, children_map: &mut HashMap<PathBuf, Vec<FileInfo>>) {
    if let Some(mut children) = children_map.remove(&node.path) {
        // Recursively assemble each child's subtree
        for child in &mut children {
            if child.is_dir() {
                assemble_tree(child, children_map);
            }
        }
        node.children = children;
    }
}

/// Convert std::fs::Metadata (via symlink_metadata) to our FileInfo
fn metadata_to_file_info(path: &Path, meta: &std::fs::Metadata, depth: u32) -> FileInfo {
    let kind = get_file_kind(meta);

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| {
            // Root directory case
            path.to_string_lossy().to_string()
        });

    let mut fi = FileInfo::new(name, path.to_path_buf(), kind);
    fi.size = meta.len();
    
    // We modify mtime here since it's available cross-platform, though returns SystemTime
    fi.mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
    
    populate_extra_metadata(&mut fi, meta);
    fi.depth = depth;

    fi
}

#[cfg(unix)]
fn get_device_id(meta: &std::fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(meta.dev())
}

#[cfg(not(unix))]
fn get_device_id(_meta: &std::fs::Metadata) -> Option<u64> {
    None
}

#[cfg(unix)]
fn populate_extra_metadata(fi: &mut FileInfo, meta: &std::fs::Metadata) {
    use std::os::unix::fs::MetadataExt;
    fi.allocated_size = meta.blocks() * 512;
    fi.inode = meta.ino();
    fi.device = meta.dev();
    fi.nlinks = meta.nlink();
    fi.mode = meta.mode();
    fi.uid = meta.uid();
    fi.gid = meta.gid();
}

#[cfg(not(unix))]
fn populate_extra_metadata(fi: &mut FileInfo, meta: &std::fs::Metadata) {
    // Windows fallback: allocated size is just the file size for now, no inode/device deduplication easily
    fi.allocated_size = meta.len();
    fi.inode = 0;
    fi.device = 0;
    fi.nlinks = 1;
    fi.mode = 0;
    fi.uid = 0;
    fi.gid = 0;
}

#[cfg(unix)]
fn get_file_kind(meta: &std::fs::Metadata) -> FileKind {
    use std::os::unix::fs::MetadataExt;
    if meta.is_dir() {
        FileKind::Directory
    } else if meta.is_symlink() {
        FileKind::Symlink
    } else if meta.is_file() {
        FileKind::File
    } else {
        let mode = meta.mode();
        if (mode & libc::S_IFMT as u32) == libc::S_IFBLK as u32 {
            FileKind::BlockDevice
        } else if (mode & libc::S_IFMT as u32) == libc::S_IFCHR as u32 {
            FileKind::CharDevice
        } else if (mode & libc::S_IFMT as u32) == libc::S_IFIFO as u32 {
            FileKind::Fifo
        } else if (mode & libc::S_IFMT as u32) == libc::S_IFSOCK as u32 {
            FileKind::Socket
        } else {
            FileKind::Other
        }
    }
}

#[cfg(not(unix))]
fn get_file_kind(meta: &std::fs::Metadata) -> FileKind {
    if meta.is_dir() {
        FileKind::Directory
    } else if meta.is_symlink() {
        FileKind::Symlink
    } else if meta.is_file() {
        FileKind::File
    } else {
        FileKind::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_scan_current_dir() {
        let opts = ScanOptions {
            path: PathBuf::from("."),
            max_depth: Some(2),
            ..Default::default()
        };
        let scanner = Scanner::new(opts);
        let tree = scanner.scan().expect("Scan should succeed");

        assert!(tree.total_size() > 0);
        assert!(tree.stats.total_files > 0);
        println!(
            "Scanned: {} files, {} dirs, {} bytes in {:?}",
            tree.stats.total_files,
            tree.stats.total_dirs,
            tree.total_size(),
            tree.scan_duration
        );
    }

    #[test]
    fn test_scan_with_excludes() {
        let opts = ScanOptions {
            path: PathBuf::from("."),
            max_depth: Some(3),
            exclude_patterns: vec![".git".to_string(), "target".to_string()],
            ..Default::default()
        };
        let scanner = Scanner::new(opts);
        let tree = scanner.scan().expect("Scan should succeed");

        // Verify .git and target are excluded
        fn check_no_excluded(node: &FileInfo) {
            assert_ne!(node.name, ".git", "Should not contain .git");
            assert_ne!(node.name, "target", "Should not contain target");
            for child in &node.children {
                check_no_excluded(child);
            }
        }
        check_no_excluded(&tree.root);
    }
}
