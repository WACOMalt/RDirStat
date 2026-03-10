// RDirStat - Error types
// License: GPL-2.0

use thiserror::Error;

#[derive(Error, Debug)]
pub enum RDirStatError {
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Not a directory: {0}")]
    NotADirectory(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Scan error: {0}")]
    ScanError(String),

    #[error("{0}")]
    Other(String),
}
