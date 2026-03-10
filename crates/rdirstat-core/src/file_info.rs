// RDirStat - File and directory information model
// License: GPL-2.0

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// What kind of filesystem entry this is
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileKind {
    File,
    Directory,
    Symlink,
    BlockDevice,
    CharDevice,
    Fifo,
    Socket,
    Other,
}

/// Core metadata for a single filesystem entry.
///
/// For directories, `size` is the own (directory entry) size.
/// Use `total_size`, `total_files`, `total_dirs` for aggregate info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// Base name (not full path)
    pub name: String,

    /// Full absolute path
    pub path: PathBuf,

    /// File type
    pub kind: FileKind,

    /// Size in bytes (apparent size / file length)
    pub size: u64,

    /// Size in bytes allocated on disk (blocks * 512)
    pub allocated_size: u64,

    /// Last modification time (seconds since epoch)
    pub mtime: i64,

    /// Inode number
    pub inode: u64,

    /// Device ID (for hard link & filesystem boundary detection)
    pub device: u64,

    /// Number of hard links
    pub nlinks: u64,

    /// File mode / permissions (raw st_mode)
    pub mode: u32,

    /// UID of owner
    pub uid: u32,

    /// GID of owner
    pub gid: u32,

    /// Children (populated only for directories)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<FileInfo>,

    // --- Aggregated fields (computed after scan) ---

    /// Total size of this entry and all descendants (bytes)
    pub total_size: u64,

    /// Total allocated size of this entry and all descendants
    pub total_allocated_size: u64,

    /// Total number of files in this subtree (excluding dirs)
    pub total_files: u64,

    /// Total number of directories in this subtree (excluding self)
    pub total_dirs: u64,

    /// Total number of entries in this subtree
    pub total_entries: u64,

    /// Depth in the tree (0 = root)
    pub depth: u32,

    /// Whether this is a mount point
    pub is_mount_point: bool,

    /// Whether this entry is a hard link duplicate (already counted elsewhere)
    pub is_hardlink_duplicate: bool,

    /// File extension (lowercase, without dot), if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
}

impl FileInfo {
    /// Create a new FileInfo with sensible defaults.
    pub fn new(name: String, path: PathBuf, kind: FileKind) -> Self {
        let extension = if kind == FileKind::File {
            path.extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
        } else {
            None
        };

        Self {
            name,
            path,
            kind,
            size: 0,
            allocated_size: 0,
            mtime: 0,
            inode: 0,
            device: 0,
            nlinks: 1,
            mode: 0,
            uid: 0,
            gid: 0,
            children: Vec::new(),
            total_size: 0,
            total_allocated_size: 0,
            total_files: 0,
            total_dirs: 0,
            total_entries: 0,
            depth: 0,
            is_mount_point: false,
            is_hardlink_duplicate: false,
            extension,
        }
    }

    /// Whether this is a directory
    pub fn is_dir(&self) -> bool {
        self.kind == FileKind::Directory
    }

    /// Whether this is a regular file
    pub fn is_file(&self) -> bool {
        self.kind == FileKind::File
    }

    /// Whether this is a symlink
    pub fn is_symlink(&self) -> bool {
        self.kind == FileKind::Symlink
    }

    /// Get a human-readable size string
    pub fn human_size(&self) -> String {
        human_readable_size(self.total_size)
    }

    /// Calculate the percentage of parent's total size
    pub fn percent_of(&self, parent_total: u64) -> f64 {
        if parent_total == 0 {
            0.0
        } else {
            (self.total_size as f64 / parent_total as f64) * 100.0
        }
    }

    /// Finalize aggregated sizes by summing children recursively.
    /// Call this after all children have been added.
    pub fn finalize(&mut self) {
        if self.children.is_empty() {
            // Leaf node
            self.total_size = self.size;
            self.total_allocated_size = self.allocated_size;
            self.total_files = if self.kind == FileKind::File { 1 } else { 0 };
            self.total_dirs = 0;
            self.total_entries = 1;
            return;
        }

        // Recursively finalize children first
        for child in &mut self.children {
            child.finalize();
        }

        // Sum up children
        let mut total_size = self.size; // Include own dir entry size
        let mut total_alloc = self.allocated_size;
        let mut total_files: u64 = 0;
        let mut total_dirs: u64 = 0;
        let mut total_entries: u64 = 1; // Count self

        for child in &self.children {
            if !child.is_hardlink_duplicate {
                total_size += child.total_size;
                total_alloc += child.total_allocated_size;
            }
            total_files += child.total_files;
            total_dirs += child.total_dirs;
            total_entries += child.total_entries;

            if child.is_dir() {
                total_dirs += 1; // Count the dir itself
            }
        }

        self.total_size = total_size;
        self.total_allocated_size = total_alloc;
        self.total_files = total_files;
        self.total_dirs = total_dirs;
        self.total_entries = total_entries;

        // Sort children by total_size descending for display
        self.children.sort_by(|a, b| b.total_size.cmp(&a.total_size));
    }

    /// Collect the N largest files in the entire subtree.
    pub fn top_n_files(&self, n: usize) -> Vec<&FileInfo> {
        let mut files = Vec::new();
        self.collect_files(&mut files);
        files.sort_by(|a, b| b.size.cmp(&a.size));
        files.truncate(n);
        files
    }

    /// Collect the N largest directories in the subtree.
    pub fn top_n_dirs(&self, n: usize) -> Vec<&FileInfo> {
        let mut dirs = Vec::new();
        self.collect_dirs(&mut dirs);
        dirs.sort_by(|a, b| b.total_size.cmp(&a.total_size));
        dirs.truncate(n);
        dirs
    }

    fn collect_files<'a>(&'a self, out: &mut Vec<&'a FileInfo>) {
        if self.is_file() {
            out.push(self);
        }
        for child in &self.children {
            child.collect_files(out);
        }
    }

    fn collect_dirs<'a>(&'a self, out: &mut Vec<&'a FileInfo>) {
        if self.is_dir() {
            out.push(self);
        }
        for child in &self.children {
            child.collect_dirs(out);
        }
    }

    /// Recursively attempt to find and remove a valid node by its path.
    /// If found, it removes the node from the `children` vector, adjusts its own aggregated totals,
    /// and returns the removed node so the caller (parent) can also adjust its totals.
    pub fn remove_node(&mut self, target: &std::path::Path) -> Option<FileInfo> {
        // Check if the target is a direct child
        if let Some(idx) = self.children.iter().position(|c| c.path == target) {
            let removed = self.children.remove(idx);
            
            // Adjust current node's aggregated sizes
            if !removed.is_hardlink_duplicate {
                self.total_size = self.total_size.saturating_sub(removed.total_size);
                self.total_allocated_size = self.total_allocated_size.saturating_sub(removed.total_allocated_size);
            }
            self.total_files = self.total_files.saturating_sub(removed.total_files);
            self.total_dirs = self.total_dirs.saturating_sub(removed.total_dirs);
            if removed.is_dir() {
                self.total_dirs = self.total_dirs.saturating_sub(1);
            }
            self.total_entries = self.total_entries.saturating_sub(removed.total_entries);
            
            return Some(removed);
        }

        // Otherwise recurse down the correct path
        for child in &mut self.children {
            if target.starts_with(&child.path) {
                if let Some(removed) = child.remove_node(target) {
                    // One of our children successfully removed the node, we must also subtract the sizes
                    if !removed.is_hardlink_duplicate {
                        self.total_size = self.total_size.saturating_sub(removed.total_size);
                        self.total_allocated_size = self.total_allocated_size.saturating_sub(removed.total_allocated_size);
                    }
                    self.total_files = self.total_files.saturating_sub(removed.total_files);
                    self.total_dirs = self.total_dirs.saturating_sub(removed.total_dirs);
                    if removed.is_dir() {
                        self.total_dirs = self.total_dirs.saturating_sub(1);
                    }
                    self.total_entries = self.total_entries.saturating_sub(removed.total_entries);
                    
                    return Some(removed);
                }
            }
        }

        None
    }
}

/// Format bytes into human-readable string
pub fn human_readable_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_human_readable_size() {
        assert_eq!(human_readable_size(0), "0 B");
        assert_eq!(human_readable_size(512), "512 B");
        assert_eq!(human_readable_size(1024), "1.0 KiB");
        assert_eq!(human_readable_size(1536), "1.5 KiB");
        assert_eq!(human_readable_size(1048576), "1.0 MiB");
        assert_eq!(human_readable_size(1073741824), "1.0 GiB");
    }

    #[test]
    fn test_finalize_leaf() {
        let mut f = FileInfo::new("test.txt".into(), "/test.txt".into(), FileKind::File);
        f.size = 1000;
        f.allocated_size = 4096;
        f.finalize();

        assert_eq!(f.total_size, 1000);
        assert_eq!(f.total_allocated_size, 4096);
        assert_eq!(f.total_files, 1);
        assert_eq!(f.total_dirs, 0);
    }

    #[test]
    fn test_finalize_directory() {
        let mut dir = FileInfo::new("mydir".into(), "/mydir".into(), FileKind::Directory);
        dir.size = 4096;
        dir.allocated_size = 4096;

        let mut f1 = FileInfo::new("a.txt".into(), "/mydir/a.txt".into(), FileKind::File);
        f1.size = 1000;
        f1.allocated_size = 4096;

        let mut f2 = FileInfo::new("b.txt".into(), "/mydir/b.txt".into(), FileKind::File);
        f2.size = 2000;
        f2.allocated_size = 4096;

        dir.children.push(f1);
        dir.children.push(f2);
        dir.finalize();

        assert_eq!(dir.total_size, 4096 + 1000 + 2000); // dir + files
        assert_eq!(dir.total_files, 2);
        assert_eq!(dir.total_dirs, 0);
        assert_eq!(dir.total_entries, 3); // dir + 2 files

        // Should be sorted by size descending
        assert_eq!(dir.children[0].name, "b.txt");
        assert_eq!(dir.children[1].name, "a.txt");
    }

    #[test]
    fn test_extension_extraction() {
        let f = FileInfo::new("photo.JPG".into(), "/photo.JPG".into(), FileKind::File);
        assert_eq!(f.extension, Some("jpg".to_string()));

        let f2 = FileInfo::new("Makefile".into(), "/Makefile".into(), FileKind::File);
        assert_eq!(f2.extension, None);

        let d = FileInfo::new("somedir".into(), "/somedir".into(), FileKind::Directory);
        assert_eq!(d.extension, None);
    }

    #[test]
    fn test_percent_of() {
        let f = FileInfo::new("test".into(), "/test".into(), FileKind::File);
        assert_eq!(f.percent_of(0), 0.0);

        let mut f2 = FileInfo::new("test".into(), "/test".into(), FileKind::File);
        f2.total_size = 500;
        assert!((f2.percent_of(1000) - 50.0).abs() < 0.001);
    }
}
