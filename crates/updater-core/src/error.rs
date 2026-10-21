use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid repository '{0}': expected owner/name or a github.com URL")]
    InvalidRepo(String),
    #[error("invalid asset pattern '{pattern}': {source}")]
    InvalidPattern {
        pattern: String,
        #[source]
        source: regex::Error,
    },
    #[error("GitHub API error for {repo}: {message}")]
    GitHub { repo: String, message: String },
    #[error("download refused: {0}")]
    DownloadRefused(String),
    #[error("checksum mismatch for {file}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        file: String,
        expected: String,
        actual: String,
    },
    #[error("no checksum published for {0} (require_checksum is enabled)")]
    ChecksumMissing(String),
    #[error("invalid backup file: {0}")]
    InvalidBackup(String),
    #[error("Backblaze B2: {0}")]
    Backblaze(String),
    #[error("installer: {0}")]
    Installer(String),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
