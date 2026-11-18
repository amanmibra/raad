//! Host-side pattern: reuse a GitHub App's refreshing token provider.
//! Run with RAAD_EXAMPLE_REPO=owner/repo and a token available in the host environment.
use raad::{Credentials, Mount, MountConfig, Result};
use std::sync::Arc;
fn main() -> Result<()> {
    let repo = std::env::var("RAAD_EXAMPLE_REPO").unwrap_or_else(|_| "sopops-dev/sop".into());
    let root = std::env::var("RAAD_EXAMPLE_ROOT").unwrap_or_else(|_| "spec".into());
    let provider = Credentials::Provider(Arc::new(|_: &str| {
        // Replace with the host's installation-token cache/refresh function.
        Ok(std::env::var("RAAD_GITHUB_TOKEN").ok())
    }));
    let cache = tempfile::tempdir()?;
    let mount = Mount::open(MountConfig::github(&repo, &root)?, cache.path(), provider)?;
    let status = mount.sync(None)?;
    let sha = status.active_sha.expect("successful first sync");
    for path in mount.list("", Some(&sha))? {
        println!("{sha} {path}");
    }
    // A host schedules future syncs. Publishing pins the SHA + compiled artifact.
    // Operational sessions, transcripts and analytics remain the host's responsibility.
    Ok(())
}
