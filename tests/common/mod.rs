#![allow(dead_code)]
use raad::{Credentials, Mount, MountConfig};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;
pub fn git(path: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().into()
}
pub struct Fixture {
    pub repo: TempDir,
    pub cache: TempDir,
    pub mount: Mount,
}
impl Fixture {
    pub fn new() -> Self {
        let repo = TempDir::new().unwrap();
        let cache = TempDir::new().unwrap();
        git(repo.path(), &["init", "-b", "main"]);
        git(
            repo.path(),
            &["config", "user.email", "tests@example.invalid"],
        );
        git(repo.path(), &["config", "user.name", "raad tests"]);
        fs::create_dir_all(repo.path().join("sops/procedures")).unwrap();
        fs::write(
            repo.path().join("sops/procedures/refunds.md"),
            "Refund within 30 days.\n",
        )
        .unwrap();
        fs::write(repo.path().join("sops/config.json"), "{\"enabled\":true}\n").unwrap();
        fs::write(repo.path().join("secret.txt"), "outside mount").unwrap();
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-m", "initial"]);
        let mount = Mount::open(
            MountConfig::local(repo.path(), "sops").unwrap(),
            cache.path(),
            Credentials::Anonymous,
        )
        .unwrap();
        Self { repo, cache, mount }
    }
    pub fn commit(&self, path: &str, bytes: &[u8]) -> String {
        fs::write(self.repo.path().join(path), bytes).unwrap();
        git(self.repo.path(), &["add", "."]);
        git(self.repo.path(), &["commit", "-m", "update"]);
        git(self.repo.path(), &["rev-parse", "HEAD"])
    }
    pub fn remote(&self) -> Mount {
        let mut config = MountConfig::github("fixture/private", "sops").unwrap();
        config.branch = Some("main".into());
        config.allow_proposals = true;
        let mount = Mount::open(
            config,
            self.cache.path(),
            Credentials::token("test-token-never-print"),
        )
        .unwrap();
        let objects = mount.cache_directory().join("objects.git");
        fs::create_dir_all(&objects).unwrap();
        git(&objects, &["init", "--bare"]);
        git(
            &objects,
            &[
                "remote",
                "add",
                "origin",
                self.repo.path().to_str().unwrap(),
            ],
        );
        git(
            self.repo.path(),
            &["config", "uploadpack.allowFilter", "true"],
        );
        mount
    }
}
