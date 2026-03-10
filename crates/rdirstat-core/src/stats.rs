// RDirStat - Statistics aggregation
// License: GPL-2.0

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::file_info::FileInfo;
use crate::file_type::{FileCategory, FileCategorizer};

/// Aggregated file-type statistics for the "File Type View"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTypeStats {
    /// Stats grouped by file extension
    pub by_extension: Vec<ExtensionStats>,
    /// Stats grouped by broad category
    pub by_category: Vec<CategoryStats>,
    /// Grand total size
    pub total_size: u64,
    /// Grand total file count
    pub total_files: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionStats {
    pub extension: String,
    pub category: FileCategory,
    pub total_size: u64,
    pub file_count: u64,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStats {
    pub category: FileCategory,
    pub total_size: u64,
    pub file_count: u64,
    pub percent: f64,
}

impl FileTypeStats {
    /// Compute file type statistics from a scanned tree.
    pub fn from_tree(root: &FileInfo) -> Self {
        let categorizer = FileCategorizer::new();
        let mut ext_map: HashMap<String, (u64, u64, FileCategory)> = HashMap::new();
        let mut total_size: u64 = 0;
        let mut total_files: u64 = 0;

        Self::collect_stats(root, &categorizer, &mut ext_map, &mut total_size, &mut total_files);

        // Build by-extension stats
        let mut by_extension: Vec<ExtensionStats> = ext_map
            .into_iter()
            .map(|(ext, (size, count, cat))| ExtensionStats {
                extension: ext,
                category: cat,
                total_size: size,
                file_count: count,
                percent: if total_size > 0 {
                    (size as f64 / total_size as f64) * 100.0
                } else {
                    0.0
                },
            })
            .collect();
        by_extension.sort_by(|a, b| b.total_size.cmp(&a.total_size));

        // Build by-category stats
        let mut cat_map: HashMap<FileCategory, (u64, u64)> = HashMap::new();
        for ext in &by_extension {
            let entry = cat_map.entry(ext.category).or_insert((0, 0));
            entry.0 += ext.total_size;
            entry.1 += ext.file_count;
        }
        let mut by_category: Vec<CategoryStats> = cat_map
            .into_iter()
            .map(|(cat, (size, count))| CategoryStats {
                category: cat,
                total_size: size,
                file_count: count,
                percent: if total_size > 0 {
                    (size as f64 / total_size as f64) * 100.0
                } else {
                    0.0
                },
            })
            .collect();
        by_category.sort_by(|a, b| b.total_size.cmp(&a.total_size));

        Self {
            by_extension,
            by_category,
            total_size,
            total_files,
        }
    }

    fn collect_stats(
        node: &FileInfo,
        categorizer: &FileCategorizer,
        ext_map: &mut HashMap<String, (u64, u64, FileCategory)>,
        total_size: &mut u64,
        total_files: &mut u64,
    ) {
        if node.is_file() && !node.is_hardlink_duplicate {
            let ext = node.extension.clone().unwrap_or_default();
            let cat = categorizer.categorize(&ext);
            let entry = ext_map
                .entry(if ext.is_empty() { "(no extension)".to_string() } else { ext })
                .or_insert((0, 0, cat));
            entry.0 += node.size;
            entry.1 += 1;
            *total_size += node.size;
            *total_files += 1;
        }

        for child in &node.children {
            Self::collect_stats(child, categorizer, ext_map, total_size, total_files);
        }
    }
}
