//! Synthetic fetch harness; invoked by scripts/benchmark.py, never against a customer repository.
use raad::{Credentials, Mount, MountConfig, Result};
use serde_json::json;
use std::{fs, path::Path, process::Command, time::Instant};
fn usage(path: &Path) -> u64 {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                usage(&entry.path())
            } else {
                entry.metadata().unwrap().len()
            }
        })
        .sum()
}
fn main() -> Result<()> {
    let origin = std::env::var("RAAD_BENCH_ORIGIN").expect("use scripts/benchmark.py");
    assert!(origin.starts_with("git://127.0.0.1:"));
    let cache = tempfile::tempdir()?;
    let mut config = MountConfig::github("synthetic/monorepo", "sops")?;
    config.branch = Some("main".into());
    let mount = Mount::open(config, cache.path(), Credentials::Anonymous)?;
    let objects = mount.cache_directory().join("objects.git");
    assert!(
        Command::new("git")
            .args(["init", "--bare", "-q"])
            .arg(&objects)
            .status()?
            .success()
    );
    assert!(
        Command::new("git")
            .arg("-C")
            .arg(&objects)
            .args(["remote", "add", "origin", &origin])
            .status()?
            .success()
    );
    let start = Instant::now();
    let status = mount.sync(None)?;
    let cold = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    for _ in 0..100 {
        mount.read("policy.md", None)?;
    }
    let read_us = start.elapsed().as_micros() / 100;
    println!(
        "{}",
        json!({"cold_sync_ms":cold,"mean_local_read_us":read_us,"cache_bytes":usage(mount.cache_directory()),"active_sha":status.active_sha,"mounted_files":mount.list("",None)?})
    );
    Ok(())
}
