mod common;
use common::*;
use raad::{Credentials, Error, Mount, MountConfig, SchemaRule, safe_path};
use std::{fs, sync::Arc, thread};

#[test]
fn local_committed_scoped_pinned_reads_and_search() {
    let f = Fixture::new();
    assert!(matches!(
        f.mount.read("config.json", None),
        Err(Error::CacheMiss(_))
    ));
    let first = f.mount.sync(None).unwrap().active_sha.unwrap();
    assert_eq!(
        f.mount.list("", None).unwrap(),
        vec!["config.json", "procedures/refunds.md"]
    );
    assert_eq!(
        f.mount.search("30 days", None, 10).unwrap(),
        vec!["procedures/refunds.md"]
    );
    fs::write(f.repo.path().join("sops/config.json"), b"uncommitted").unwrap();
    assert!(
        String::from_utf8(
            f.mount
                .read("config.json", None)
                .unwrap()
                .data
                .bytes()
                .unwrap()
        )
        .unwrap()
        .contains("enabled")
    );
    let next = f.commit("sops/config.json", b"{\"enabled\":false}");
    f.mount.sync(None).unwrap();
    assert_eq!(
        f.mount
            .read("config.json", Some(&first))
            .unwrap()
            .provenance
            .commit_sha,
        first
    );
    assert_eq!(
        f.mount
            .read("config.json", None)
            .unwrap()
            .provenance
            .commit_sha,
        next
    );
    assert_eq!(f.mount.history(None).unwrap().len(), 2);
    assert!(f.mount.read("secret.txt", None).is_err());
    assert!(f.mount.list("pro", None).unwrap().is_empty());
}
#[test]
fn hostile_paths_are_rejected() {
    for path in [
        "../secret",
        "/etc/passwd",
        "a/../../b",
        "a/./b",
        "a//b",
        "a\\b",
        ".git/config",
        "a/.GIT/config",
        "C:/test",
        "a\0b",
        "a\nb",
        "a./x",
    ] {
        assert!(safe_path(path, false).is_err(), "{path:?}");
    }
    let f = Fixture::new();
    f.mount.sync(None).unwrap();
    assert!(f.mount.read("config.json", Some("HEAD")).is_err());
    assert!(f.mount.read("config.json", Some("../../x")).is_err());
}
#[test]
fn invalid_candidate_and_failed_first_start() {
    let f = Fixture::new();
    let old = f.mount.sync(None).unwrap().active_sha;
    f.commit("sops/config.json", b"bad json");
    assert!(matches!(f.mount.sync(None), Err(Error::Validation(_))));
    let state = f.mount.status().unwrap();
    assert_eq!(state.active_sha, old);
    assert!(state.stale);
    assert!(state.last_error.is_some());
    let cache = tempfile::tempdir().unwrap();
    let fresh = Mount::open(
        f.mount.config().clone(),
        cache.path(),
        Credentials::Anonymous,
    )
    .unwrap();
    assert!(fresh.sync(None).is_err());
    assert!(fresh.read("config.json", None).is_err());
}
#[test]
fn warm_offline_cold_failure_and_force_push() {
    let f = Fixture::new();
    let mount = f.remote();
    let initial = mount.sync(None).unwrap().active_sha.unwrap();
    let next = f.commit("sops/config.json", b"{}");
    mount.sync(None).unwrap();
    git(f.repo.path(), &["reset", "--hard", &initial]);
    assert_eq!(mount.sync(None).unwrap().active_sha.unwrap(), initial);
    assert!(mount.read("config.json", Some(&next)).is_ok());
    git(
        &mount.cache_directory().join("objects.git"),
        &["remote", "set-url", "origin", "/nonexistent-raad-test-repo"],
    );
    assert!(mount.sync(None).is_err());
    assert!(mount.read("config.json", None).unwrap().provenance.stale);
    mount.clear_cache().unwrap();
    // A local missing source is a deterministic cold-start network-equivalent failure.
    let mut config = f.mount.config().clone();
    config.source = raad::Source::Local {
        path: "/nonexistent-raad-test-repo".into(),
    };
    let cold = Mount::open(config, f.cache.path(), Credentials::Anonymous).unwrap();
    assert!(cold.sync(None).is_err());
    assert!(cold.read("config.json", None).is_err());
}
#[test]
fn corruption_and_limits() {
    let f = Fixture::new();
    let sha = f.mount.sync(None).unwrap().active_sha.unwrap();
    fs::write(
        f.mount
            .cache_directory()
            .join(format!("snapshots/{sha}.json")),
        "{}",
    )
    .unwrap();
    assert!(matches!(
        f.mount.read("config.json", None),
        Err(Error::Corrupt(_))
    ));
    f.mount.sync(None).unwrap(); // Explicit resync repairs a damaged snapshot.
    let mut config = f.mount.config().clone();
    config.limits.file_bytes = 1;
    let tiny = Mount::open(config, f.cache.path(), Credentials::Anonymous).unwrap();
    assert!(matches!(tiny.sync(None), Err(Error::Limit(_))));
}
#[cfg(unix)]
#[test]
fn symlinks_and_mount_ancestor_symlinks_are_rejected() {
    let f = Fixture::new();
    std::os::unix::fs::symlink("../secret.txt", f.repo.path().join("sops/leak")).unwrap();
    git(f.repo.path(), &["add", "."]);
    git(f.repo.path(), &["commit", "-m", "symlink"]);
    assert!(matches!(f.mount.sync(None), Err(Error::Validation(_))));
    let config = MountConfig::local(f.repo.path(), "sops/leak").unwrap();
    let mount = Mount::open(config, f.cache.path(), Credentials::Anonymous).unwrap();
    assert!(mount.sync(None).is_err());
}
#[test]
fn concurrent_sync_retention_and_pin() {
    let f = Fixture::new();
    let first = f.mount.sync(None).unwrap().active_sha.unwrap();
    f.mount.pin(&first, true).unwrap();
    let mount = Arc::new(f.mount.clone());
    let joins: Vec<_> = (0..4)
        .map(|_| {
            let m = mount.clone();
            thread::spawn(move || m.sync(None).unwrap())
        })
        .collect();
    for join in joins {
        join.join().unwrap();
    }
    assert_eq!(mount.history(None).unwrap().len(), 1);
    let mut config = mount.config().clone();
    config.limits.retained_revisions = 1;
    let small = Mount::open(config, f.cache.path(), Credentials::Anonymous).unwrap();
    f.commit("sops/config.json", b"{}");
    small.sync(None).unwrap();
    small.prune().unwrap();
    assert_eq!(small.history(None).unwrap().len(), 2);
    small.pin(&first, false).unwrap();
    small.prune().unwrap();
    assert!(small.read("config.json", Some(&first)).is_err());
}
#[test]
fn schemas_and_custom_validator_isolation() {
    let f = Fixture::new();
    let mut config = f.mount.config().clone();
    config.validation.schemas.push(SchemaRule {
        path: "config.json".into(),
        schema: serde_json::json!({"type":"object","required":["missing"]}),
    });
    let mount = Mount::open(config, f.cache.path(), Credentials::Anonymous).unwrap();
    assert!(mount.sync(None).is_err());
    let mut config = f.mount.config().clone();
    config.validation.commands.push(raad::CommandValidator { name: "scope".into(), command: vec!["sh".into(), "-c".into(), "test -f config.json && test -f procedures/refunds.md && test ! -e secret.txt && test ! -e .git && test -z \"$GITHUB_TOKEN\" && test -z \"$RAAD_GITHUB_TOKEN\"".into()], timeout_seconds: 2 });
    let mount = Mount::open(config, f.cache.path(), Credentials::Anonymous).unwrap();
    mount.sync(None).unwrap();
    // Only mounted file paths are written to the validation directory.
    assert!(!mount.cache_directory().join("secret.txt").exists());
}

#[test]
fn partial_fetch_keeps_unrelated_blobs_unmaterialized() {
    let f = Fixture::new();
    let outside = git(f.repo.path(), &["rev-parse", "HEAD:secret.txt"]);
    let remote = f.remote();
    remote.sync(None).unwrap();
    let objects = remote.cache_directory().join("objects.git");
    let mut pack_listing = String::new();
    for entry in fs::read_dir(objects.join("objects/pack")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|s| s == "idx") {
            pack_listing.push_str(&git(
                &objects,
                &["verify-pack", "-v", path.to_str().unwrap()],
            ));
        }
    }
    assert!(!pack_listing.contains(&outside));
    assert!(!objects.join("sops").exists());
    assert!(!objects.join("secret.txt").exists());
    assert_eq!(
        remote.list("", None).unwrap(),
        vec!["config.json", "procedures/refunds.md"]
    );
}
#[cfg(unix)]
#[test]
fn validators_time_out_and_do_not_hang_on_background_pipe_holders() {
    let f = Fixture::new();
    let mut config = f.mount.config().clone();
    config.validation.commands.push(raad::CommandValidator {
        name: "timeout".into(),
        command: vec!["sh".into(), "-c".into(), "sleep 30 & wait".into()],
        timeout_seconds: 1,
    });
    let mount = Mount::open(config, f.cache.path(), Credentials::Anonymous).unwrap();
    let started = std::time::Instant::now();
    assert!(mount.sync(None).is_err());
    assert!(started.elapsed().as_secs() < 5);
}

#[cfg(unix)]
#[test]
fn interrupted_sync_cannot_replace_active_snapshot() {
    use std::{
        io::Write,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    let f = Fixture::new();
    let controls = tempfile::tempdir().unwrap();
    let hold = controls.path().join("hold");
    let marker = controls.path().join("marker");
    let mut config = f.mount.config().clone();
    config.validation.commands.push(raad::CommandValidator {
        name: "interrupt".into(),
        command: vec![
            "sh".into(),
            "-c".into(),
            "if test -f \"$1\"; then printf '%s' \"$$\" > \"$2\"; exec sleep 30; fi".into(),
            "validator".into(),
            hold.to_str().unwrap().into(),
            marker.to_str().unwrap().into(),
        ],
        timeout_seconds: 30,
    });
    let mount = Mount::open(config.clone(), f.cache.path(), Credentials::Anonymous).unwrap();
    let old = mount.sync(None).unwrap().active_sha.unwrap();
    f.commit("sops/config.json", b"{}");
    fs::File::create(&hold).unwrap().write_all(b"hold").unwrap();
    let config_path = controls.path().join("config.json");
    fs::write(
        &config_path,
        serde_json::to_vec(&serde_json::json!({"version":1,"mounts":[config]})).unwrap(),
    )
    .unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_raad"))
        .args(["sync", "--config"])
        .arg(&config_path)
        .arg("--cache-dir")
        .arg(f.cache.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    while !marker.exists() {
        assert!(started.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(10));
    }
    child.kill().unwrap();
    child.wait().unwrap();
    let pid: i32 = fs::read_to_string(&marker).unwrap().parse().unwrap();
    // SAFETY: this PID was emitted by the validator in the process group created by raad.
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    assert_eq!(
        mount
            .read("config.json", None)
            .unwrap()
            .provenance
            .commit_sha,
        old
    );
    fs::remove_file(hold).unwrap();
    assert_ne!(mount.sync(None).unwrap().active_sha.unwrap(), old);
}
