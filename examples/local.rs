use raad::{Credentials, Mount, MountConfig, Result};
fn main() -> Result<()> {
    let cache = tempfile::tempdir()?;
    let mount = Mount::open(
        MountConfig::local(".", "examples/app-config")?,
        cache.path(),
        Credentials::Anonymous,
    )?;
    mount.sync(None)?;
    let result = mount.read("settings.json", None)?;
    println!(
        "{}\n{}",
        result.provenance.commit_sha,
        String::from_utf8_lossy(&result.data.bytes()?)
    );
    Ok(())
}
