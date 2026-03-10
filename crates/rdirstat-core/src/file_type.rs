// RDirStat - File type categorization
// License: GPL-2.0

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Broad file type categories (inspired by QDirStat's MimeCategory)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FileCategory {
    /// Images: jpg, png, gif, bmp, svg, webp, ico, tiff, etc.
    Image,
    /// Video: mp4, mkv, avi, mov, wmv, flv, webm, etc.
    Video,
    /// Audio: mp3, flac, wav, ogg, aac, wma, m4a, etc.
    Audio,
    /// Documents: pdf, doc, docx, odt, rtf, txt, md, etc.
    Document,
    /// Compressed/Archives: zip, tar, gz, bz2, xz, 7z, rar, etc.
    Archive,
    /// Source code: rs, py, js, ts, c, cpp, java, go, rb, etc.
    SourceCode,
    /// Executables: exe, dll, so, bin, elf, etc.
    Executable,
    /// Databases: db, sqlite, mdb, etc.
    Database,
    /// Fonts: ttf, otf, woff, woff2, etc.
    Font,
    /// Configuration: json, yaml, toml, ini, conf, xml, etc.
    Config,
    /// Temporary / cache files
    Temporary,
    /// Unknown / other
    Other,
}

impl FileCategory {
    /// Get a display label for this category
    pub fn label(&self) -> &'static str {
        match self {
            FileCategory::Image => "Images",
            FileCategory::Video => "Video",
            FileCategory::Audio => "Audio",
            FileCategory::Document => "Documents",
            FileCategory::Archive => "Archives",
            FileCategory::SourceCode => "Source Code",
            FileCategory::Executable => "Executables",
            FileCategory::Database => "Databases",
            FileCategory::Font => "Fonts",
            FileCategory::Config => "Configuration",
            FileCategory::Temporary => "Temporary",
            FileCategory::Other => "Other",
        }
    }
}

/// Maps file extensions to categories
pub struct FileCategorizer {
    ext_map: HashMap<&'static str, FileCategory>,
}

impl Default for FileCategorizer {
    fn default() -> Self {
        Self::new()
    }
}

impl FileCategorizer {
    pub fn new() -> Self {
        let mut m = HashMap::new();

        // Images
        for ext in &["jpg", "jpeg", "png", "gif", "bmp", "svg", "webp", "ico",
                      "tiff", "tif", "psd", "raw", "cr2", "nef", "heic", "heif",
                      "avif", "jxl"] {
            m.insert(*ext, FileCategory::Image);
        }

        // Video
        for ext in &["mp4", "mkv", "avi", "mov", "wmv", "flv", "webm", "m4v",
                      "mpg", "mpeg", "3gp", "ogv", "ts", "vob"] {
            m.insert(*ext, FileCategory::Video);
        }

        // Audio
        for ext in &["mp3", "flac", "wav", "ogg", "aac", "wma", "m4a", "opus",
                      "aiff", "ape", "alac", "mid", "midi"] {
            m.insert(*ext, FileCategory::Audio);
        }

        // Documents
        for ext in &["pdf", "doc", "docx", "odt", "rtf", "txt", "md", "tex",
                      "ppt", "pptx", "odp", "xls", "xlsx", "ods", "csv",
                      "epub", "mobi", "djvu", "pages", "numbers", "keynote"] {
            m.insert(*ext, FileCategory::Document);
        }

        // Archives
        for ext in &["zip", "tar", "gz", "bz2", "xz", "7z", "rar", "zst",
                      "lz4", "lzma", "cab", "iso", "dmg", "deb", "rpm",
                      "tgz", "tbz2", "txz"] {
            m.insert(*ext, FileCategory::Archive);
        }

        // Source code
        for ext in &["rs", "py", "js", "ts", "jsx", "tsx", "c", "cpp", "cc",
                      "h", "hpp", "java", "go", "rb", "php", "swift", "kt",
                      "scala", "cs", "lua", "pl", "pm", "r", "m", "mm",
                      "asm", "s", "v", "sv", "vhd", "vhdl", "zig", "nim",
                      "elm", "erl", "ex", "exs", "hs", "ml", "mli", "fs",
                      "fsx", "clj", "cljs", "lisp", "scm", "rkt",
                      "html", "htm", "css", "scss", "sass", "less",
                      "vue", "svelte", "astro",
                      "sh", "bash", "zsh", "fish", "ps1", "bat", "cmd",
                      "sql", "graphql", "proto", "thrift"] {
            m.insert(*ext, FileCategory::SourceCode);
        }

        // Executables
        for ext in &["exe", "dll", "so", "dylib", "bin", "elf", "app",
                      "msi", "apk", "appimage", "flatpak", "snap"] {
            m.insert(*ext, FileCategory::Executable);
        }

        // Databases
        for ext in &["db", "sqlite", "sqlite3", "mdb", "accdb", "ldb"] {
            m.insert(*ext, FileCategory::Database);
        }

        // Fonts
        for ext in &["ttf", "otf", "woff", "woff2", "eot", "pfb", "pfm"] {
            m.insert(*ext, FileCategory::Font);
        }

        // Configuration
        for ext in &["json", "yaml", "yml", "toml", "ini", "conf", "cfg",
                      "xml", "properties", "env", "rc", "lock"] {
            m.insert(*ext, FileCategory::Config);
        }

        // Temporary
        for ext in &["tmp", "temp", "swp", "swo", "bak", "old", "orig",
                      "cache", "log"] {
            m.insert(*ext, FileCategory::Temporary);
        }

        Self { ext_map: m }
    }

    /// Categorize a file by its extension (lowercase, no dot)
    pub fn categorize(&self, extension: &str) -> FileCategory {
        self.ext_map
            .get(extension)
            .copied()
            .unwrap_or(FileCategory::Other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_categorize() {
        let cat = FileCategorizer::new();
        assert_eq!(cat.categorize("jpg"), FileCategory::Image);
        assert_eq!(cat.categorize("mp4"), FileCategory::Video);
        assert_eq!(cat.categorize("rs"), FileCategory::SourceCode);
        assert_eq!(cat.categorize("zip"), FileCategory::Archive);
        assert_eq!(cat.categorize("unknown_ext"), FileCategory::Other);
    }
}
