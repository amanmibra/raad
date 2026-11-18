mod common;
use common::*;
use raad::{CommandValidator, Credentials, Mount, MountConfig};
use std::{fs, path::Path};
fn copy_tree(source: &Path, target: &Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let dest = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), dest).unwrap();
        }
    }
}
#[test]
fn shipped_data_examples_validate() {
    let f = Fixture::new();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/sopops/sops"),
        &f.repo.path().join("sops"),
    );
    git(f.repo.path(), &["add", "."]);
    git(f.repo.path(), &["commit", "-m", "example fixture"]);
    f.mount.sync(None).unwrap();
}
#[test]
fn sop_validate_integration_when_installed() {
    let executable = std::env::var("SOP_BIN").unwrap_or_else(|_| "sop".into());
    if std::process::Command::new(&executable)
        .arg("--version")
        .output()
        .is_err()
    {
        assert!(
            std::env::var_os("RAAD_REQUIRE_SOP").is_none(),
            "sop is required by this test run"
        );
        eprintln!("sop not installed; set SOP_BIN or RAAD_REQUIRE_SOP=1 to require it");
        return;
    }
    let f = Fixture::new();
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/sopops/sops"),
        &f.repo.path().join("sops"),
    );
    git(f.repo.path(), &["add", "."]);
    git(f.repo.path(), &["commit", "-m", "sop fixture"]);
    let mut config = MountConfig::local(f.repo.path(), "sops").unwrap();
    config.validation.commands.push(CommandValidator {
        name: "sop".into(),
        command: vec![executable, "validate".into(), "--dir".into(), ".".into()],
        timeout_seconds: 30,
    });
    let mount = Mount::open(config, f.cache.path(), Credentials::Anonymous).unwrap();
    mount.sync(None).unwrap();
}
