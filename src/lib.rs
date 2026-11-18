//! Git-backed application data with local, validated, immutable snapshots.
//! Network access occurs only in explicit sync and write operations.
mod auth;
mod github;
pub mod mcp;
mod process;
mod store;
mod validation;

pub use auth::{Credentials, TokenProvider};
pub use github::{Change, Proposal, WriteMode, WriteRequest};
use std::path::{Component, Path};
pub use store::{
    FileData, HistoryEntry, Limits, Mount, MountConfig, Provenance, ReadResult, Source, Status,
};
pub use validation::{CommandValidator, SchemaRule, Validation};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("no validated snapshot; run raad sync ({0})")]
    CacheMiss(String),
    #[error("cache corruption: {0}")]
    Corrupt(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("stale expected SHA; current branch head is {0}")]
    Conflict(String),
    #[error("remote unavailable: {0}")]
    Remote(String),
    #[error("GitHub rate limit; retry after {0}")]
    RateLimit(String),
    #[error("limit exceeded: {0}")]
    Limit(String),
    #[error("process failed: {0}")]
    Process(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("configuration: {0}")]
    Config(String),
}
pub type Result<T> = std::result::Result<T, Error>;

/// Strict portable paths: no traversal, absolute paths, Git metadata or platform aliases.
pub fn safe_path(value: &str, allow_empty: bool) -> Result<String> {
    let value = value.trim_end_matches('/');
    if value.is_empty() && allow_empty {
        return Ok(String::new());
    }
    if value.is_empty()
        || value.contains(['\\', ':', '\0', '\n', '\r'])
        || value.starts_with('/')
        || value.split('/').any(|c| {
            c.is_empty()
                || c == "."
                || c == ".."
                || c.eq_ignore_ascii_case(".git")
                || c.ends_with(['.', ' '])
        })
        || Path::new(value)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::InvalidPath(value.into()));
    }
    Ok(value.into())
}

pub(crate) fn valid_sha(s: &str) -> bool {
    matches!(s.len(), 40 | 64)
        && s.bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_normalization_is_idempotent_and_rejects_traversal_components() {
        for a in ["sops", "policies", "prompts", "a b", "é"] {
            for b in ["rules.md", "data.json", "文件.yaml"] {
                let path = format!("{a}/{b}");
                assert_eq!(
                    safe_path(&safe_path(&path, false).unwrap(), false).unwrap(),
                    path
                );
                for component in ["..", ".", ".git", ".GIT"] {
                    assert!(safe_path(&format!("{a}/{component}/{b}"), false).is_err());
                }
            }
        }
    }
    #[test]
    fn revisions_are_full_lowercase_object_ids() {
        assert!(valid_sha(&"a".repeat(40)));
        assert!(valid_sha(&"b".repeat(64)));
        for value in ["HEAD", "main", "1234", "--help", &"G".repeat(40)] {
            assert!(!valid_sha(value));
        }
    }
}
