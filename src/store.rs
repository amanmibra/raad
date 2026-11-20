use crate::{Credentials, Error, Result, Validation, process::run, safe_path, valid_sha};
use base64::{Engine, engine::general_purpose::STANDARD};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Source {
    Local { path: PathBuf },
    Github { repository: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub file_bytes: usize,
    pub mount_bytes: usize,
    pub max_files: usize,
    pub response_bytes: usize,
    pub cache_bytes: u64,
    pub retained_revisions: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            file_bytes: 2 * 1024 * 1024,
            mount_bytes: 32 * 1024 * 1024,
            max_files: 10000,
            response_bytes: 4 * 1024 * 1024,
            cache_bytes: 512 * 1024 * 1024,
            retained_revisions: 8,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountConfig {
    /// Trusted mount-relative path prefixes omitted before materialization and validation.
    #[serde(default)]
    pub excluded_paths: Vec<String>,
    pub name: String,
    pub source: Source,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub path: String,
    #[serde(default = "refresh_default")]
    pub refresh_seconds: u64,
    #[serde(default)]
    pub validation: Validation,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub allow_proposals: bool,
    #[serde(default)]
    pub allow_direct: bool,
}
fn refresh_default() -> u64 {
    60
}
impl MountConfig {
    pub fn local(path: impl AsRef<Path>, root: &str) -> Result<Self> {
        let path = fs::canonicalize(path)?;
        let top = run(
            Command::new("git")
                .arg("-C")
                .arg(&path)
                .args(["rev-parse", "--show-toplevel"]),
            None,
            10,
            16384,
        )?;
        let top = PathBuf::from(String::from_utf8_lossy(&top).trim());
        let cwd_root = path
            .strip_prefix(&top)
            .map_err(|_| Error::InvalidPath("local directory".into()))?;
        let joined = cwd_root.join(root);
        let root = joined
            .to_str()
            .ok_or_else(|| Error::InvalidPath("non UTF-8 directory".into()))?;
        Ok(Self {
            excluded_paths: vec![],
            name: "default".into(),
            source: Source::Local { path: top },
            branch: None,
            path: safe_path(root, true)?,
            refresh_seconds: 60,
            validation: Validation::default(),
            limits: Limits::default(),
            allow_proposals: false,
            allow_direct: false,
        })
    }
    pub fn github(repository: &str, root: &str) -> Result<Self> {
        validate_repository(repository)?;
        Ok(Self {
            excluded_paths: vec![],
            name: "default".into(),
            source: Source::Github {
                repository: repository.into(),
            },
            branch: None,
            path: safe_path(root, true)?,
            refresh_seconds: 60,
            validation: Validation::default(),
            limits: Limits::default(),
            allow_proposals: false,
            allow_direct: false,
        })
    }
}
pub(crate) fn validate_repository(repo: &str) -> Result<()> {
    let parts: Vec<_> = repo.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|s| {
            s.is_empty()
                || s.starts_with('.')
                || !s
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
        })
    {
        return Err(Error::Config("GitHub repository must be owner/name".into()));
    }
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct FileData {
    pub content: String,
    pub executable: bool,
}
impl FileData {
    pub fn new(bytes: &[u8]) -> Self {
        Self {
            content: STANDARD.encode(bytes),
            executable: false,
        }
    }
    pub fn bytes(&self) -> Result<Vec<u8>> {
        STANDARD
            .decode(&self.content)
            .map_err(|_| Error::Corrupt("blob encoding".into()))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub repository: String,
    pub branch: String,
    pub mount_path: String,
    pub commit_sha: String,
    pub file_path: String,
    pub stale: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadResult {
    pub data: FileData,
    pub provenance: Provenance,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Status {
    pub active_sha: Option<String>,
    pub branch: String,
    pub last_attempt: Option<u64>,
    pub last_sync: Option<u64>,
    pub last_validation: Option<u64>,
    pub last_error: Option<String>,
    pub stale: bool,
    pub retained: Vec<String>,
    pub pinned: BTreeSet<String>,
    pub(crate) digests: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub commit_sha: String,
    pub active: bool,
    pub pinned: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub commit_sha: String,
    pub files: BTreeMap<String, FileData>,
}
/// Explicit, unactivated source inspection. Files may fail validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub commit_sha: String,
    pub branch: String,
    pub root_exists: bool,
    pub files: BTreeMap<String, FileData>,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Mount {
    pub(crate) config: MountConfig,
    pub(crate) directory: PathBuf,
    pub(crate) credentials: Credentials,
    pub(crate) api_endpoint: String,
}
pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
impl Mount {
    pub fn open(
        mut config: MountConfig,
        cache: impl AsRef<Path>,
        credentials: Credentials,
    ) -> Result<Self> {
        config.path = safe_path(&config.path, true)?;
        config.excluded_paths = config
            .excluded_paths
            .iter()
            .map(|p| safe_path(p, false))
            .collect::<Result<_>>()?;
        if let Source::Github { repository } = &config.source {
            validate_repository(repository)?;
        }
        if let Some(branch) = &config.branch {
            if branch.starts_with('-') {
                return Err(Error::Config("invalid branch".into()));
            }
            run(
                Command::new("git").args(["check-ref-format", &format!("refs/heads/{branch}")]),
                None,
                5,
                1024,
            )
            .map_err(|_| Error::Config("invalid branch".into()))?;
        }
        if config.limits.retained_revisions == 0 || config.limits.max_files == 0 {
            return Err(Error::Config("limits must be positive".into()));
        }
        let identity = serde_json::to_vec(&(
            &config.source,
            &config.branch,
            &config.path,
            &config.validation,
            &config.excluded_paths,
            config.limits.file_bytes,
            config.limits.mount_bytes,
            config.limits.max_files,
        ))?;
        let directory = cache.as_ref().join(digest(&identity));
        fs::create_dir_all(directory.join("snapshots"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(cache.as_ref(), fs::Permissions::from_mode(0o700))?;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self {
            config,
            directory,
            credentials,
            api_endpoint: "https://api.github.com/".into(),
        })
    }
    pub fn config(&self) -> &MountConfig {
        &self.config
    }
    /// Clone this mount with credentials for one operation, without changing its cache or state.
    pub fn with_credentials(&self, credentials: Credentials) -> Self {
        Self {
            credentials,
            ..self.clone()
        }
    }
    pub fn cache_directory(&self) -> &Path {
        &self.directory
    }
    pub(crate) fn lock(&self) -> Result<File> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join("lock"))?;
        file.lock_exclusive()?;
        Ok(file)
    }
    fn read_lock(&self) -> Result<File> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join("lock"))?;
        FileExt::lock_shared(&file)?;
        Ok(file)
    }
    fn state(&self) -> Result<Status> {
        let path = self.directory.join("state.json");
        match fs::read(path) {
            Ok(bytes) => {
                let state: Status = serde_json::from_slice(&bytes)
                    .map_err(|_| Error::Corrupt("state.json; clear-cache to recover".into()))?;
                if state
                    .retained
                    .iter()
                    .chain(state.pinned.iter())
                    .chain(state.active_sha.iter())
                    .any(|s| !valid_sha(s))
                {
                    return Err(Error::Corrupt("state revision".into()));
                }
                Ok(state)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Status::default()),
            Err(e) => Err(e.into()),
        }
    }
    fn save_state(&self, state: &Status) -> Result<()> {
        atomic_write(
            &self.directory.join("state.json"),
            &serde_json::to_vec(state)?,
        )
    }
    pub fn status(&self) -> Result<Status> {
        let _lock = self.read_lock()?;
        self.status_unlocked()
    }
    pub(crate) fn status_unlocked(&self) -> Result<Status> {
        let mut state = self.state()?;
        state.stale = state.last_error.is_some()
            || state
                .last_sync
                .is_none_or(|t| now().saturating_sub(t) >= self.config.refresh_seconds);
        if let Some(sha) = &state.active_sha {
            self.load_snapshot(sha, &state)?;
        }
        Ok(state)
    }
    pub(crate) fn git_path(&self) -> PathBuf {
        match &self.config.source {
            Source::Local { path } => path.clone(),
            Source::Github { .. } => self.directory.join("objects.git"),
        }
    }
    pub(crate) fn git(
        &self,
        args: &[&str],
        network: bool,
        token: Option<&str>,
        limit: usize,
    ) -> Result<Vec<u8>> {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(self.git_path());
        for var in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CONFIG_COUNT",
        ] {
            cmd.env_remove(var);
        }
        cmd.env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_NO_REPLACE_OBJECTS", "1")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        if !network {
            cmd.env("GIT_NO_LAZY_FETCH", "1");
        }
        if let Some(token) = token {
            let value = STANDARD.encode(format!("x-access-token:{token}"));
            cmd.env("GIT_CONFIG_COUNT", "2")
                .env("GIT_CONFIG_KEY_0", "http.https://github.com/.extraheader")
                .env(
                    "GIT_CONFIG_VALUE_0",
                    format!("Authorization: Basic {value}"),
                )
                .env("GIT_CONFIG_KEY_1", "credential.helper")
                .env("GIT_CONFIG_VALUE_1", "");
        }
        cmd.args(args);
        run(&mut cmd, None, 120, limit)
    }
    fn text_git(&self, args: &[&str], network: bool, token: Option<&str>) -> Result<String> {
        Ok(String::from_utf8(self.git(args, network, token, 65536)?)
            .map_err(|_| Error::Process("non UTF-8 Git output".into()))?
            .trim()
            .into())
    }
    /// Explicit synchronization. Failed candidates preserve the previous snapshot and record status.
    /// `revision` fetches/validates a historical SHA without activating it.
    pub fn sync(&self, revision: Option<&str>) -> Result<Status> {
        self.sync_options(revision, false)
    }
    /// Sync and retain the selected revision under the same exclusive cache lock.
    pub fn sync_pinned(&self, revision: Option<&str>) -> Result<Status> {
        self.sync_options(revision, true)
    }
    fn sync_options(&self, revision: Option<&str>, pin: bool) -> Result<Status> {
        if revision.is_some_and(|s| !valid_sha(s)) {
            return Err(Error::InvalidPath(
                "revision must be a full lowercase commit SHA".into(),
            ));
        }
        let _lock = self.lock()?;
        let mut state = self.state()?;
        state.last_attempt = Some(now());
        let outcome = self.sync_candidate(revision, &mut state);
        match outcome {
            Ok(()) => {
                if pin {
                    let selected = revision
                        .or(state.active_sha.as_deref())
                        .ok_or_else(|| Error::CacheMiss("sync result".into()))?;
                    state.pinned.insert(selected.to_owned());
                }
                if revision.is_none() {
                    state.last_error = None;
                }
                self.prune_locked(&mut state)?;
                self.save_state(&state)?;
                self.status_unlocked()
            }
            Err(error) => {
                state.last_error = Some(error.to_string());
                self.save_state(&state)?;
                Err(error)
            }
        }
    }
    fn sync_candidate(&self, revision: Option<&str>, state: &mut Status) -> Result<()> {
        let (sha, branch, token) = self.fetch_revision(revision)?;
        self.activate_candidate(revision, state, sha, branch, token)
    }
    pub(crate) fn fetch_revision(
        &self,
        revision: Option<&str>,
    ) -> Result<(String, String, Option<String>)> {
        if revision.is_some_and(|s| !valid_sha(s)) {
            return Err(Error::InvalidPath(
                "revision must be a full lowercase commit SHA".into(),
            ));
        }
        self.check_cache_size()?;
        let mut token = None;
        let branch;
        let sha = match &self.config.source {
            Source::Local { .. } => {
                branch = self.config.branch.clone().unwrap_or_else(|| "HEAD".into());
                let target = revision.unwrap_or(&branch);
                self.text_git(
                    &["rev-parse", "--verify", &format!("{target}^{{commit}}")],
                    false,
                    None,
                )?
            }
            Source::Github { repository } => {
                token = self.credentials.resolve(repository)?;
                if !self.git_path().exists() {
                    run(
                        Command::new("git")
                            .args(["init", "--bare"])
                            .arg(self.git_path()),
                        None,
                        10,
                        16384,
                    )?;
                    self.git(
                        &[
                            "remote",
                            "add",
                            "origin",
                            &format!("https://github.com/{repository}.git"),
                        ],
                        false,
                        None,
                        16384,
                    )?;
                    self.git(
                        &["config", "remote.origin.promisor", "true"],
                        false,
                        None,
                        16384,
                    )?;
                    self.git(
                        &["config", "remote.origin.partialclonefilter", "blob:none"],
                        false,
                        None,
                        16384,
                    )?;
                }
                branch = if let Some(branch) = &self.config.branch {
                    branch.clone()
                } else {
                    let head = self
                        .text_git(
                            &["ls-remote", "--symref", "origin", "HEAD"],
                            true,
                            token.as_deref(),
                        )
                        .map_err(|_| Error::Remote("cannot discover default branch".into()))?;
                    head.lines()
                        .find_map(|l| {
                            l.strip_prefix("ref: refs/heads/")
                                .and_then(|s| s.strip_suffix("\tHEAD"))
                        })
                        .ok_or_else(|| Error::Remote("default branch missing".into()))?
                        .into()
                };
                let refspec = revision
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("+refs/heads/{branch}:refs/remotes/origin/raad"));
                self.git(
                    &[
                        "fetch",
                        "--no-tags",
                        "--filter=blob:none",
                        "origin",
                        &refspec,
                    ],
                    true,
                    token.as_deref(),
                    65536,
                )
                .map_err(|_| {
                    Error::Remote("fetch failed (check credentials, network and branch)".into())
                })?;
                self.text_git(
                    &["rev-parse", "--verify", "FETCH_HEAD^{commit}"],
                    false,
                    None,
                )?
            }
        };
        if !valid_sha(&sha) {
            return Err(Error::Corrupt("Git returned invalid SHA".into()));
        }
        Ok((sha, branch, token))
    }
    fn activate_candidate(
        &self,
        revision: Option<&str>,
        state: &mut Status,
        sha: String,
        branch: String,
        token: Option<String>,
    ) -> Result<()> {
        let already_validated =
            state.digests.contains_key(&sha) && self.load_snapshot(&sha, state).is_ok();
        let snapshot = if already_validated {
            None
        } else {
            let snapshot = self.collect_snapshot(&sha, token.as_deref())?;
            self.config.validation.validate(&snapshot.files)?;
            Some(snapshot)
        };
        // Serialize syncs under the OS lock; additionally reject a moving remote head.
        if let Source::Github { .. } = &self.config.source
            && revision.is_none()
        {
            let head = self
                .text_git(
                    &["ls-remote", "origin", &format!("refs/heads/{branch}")],
                    true,
                    token.as_deref(),
                )
                .map_err(|_| Error::Remote("head recheck failed".into()))?;
            if head.split_whitespace().next() != Some(sha.as_str()) {
                return Err(Error::Conflict(
                    "branch moved during validation; sync again".into(),
                ));
            }
        }
        self.check_cache_size()?;
        if let Some(snapshot) = snapshot {
            let bytes = serde_json::to_vec(&snapshot)?;
            let previous_bytes = fs::metadata(self.snapshot_path(&sha))
                .map(|m| m.len())
                .unwrap_or(0);
            if disk_usage(&self.directory)?.saturating_sub(previous_bytes) + bytes.len() as u64
                > self.config.limits.cache_bytes
            {
                return Err(Error::Limit(
                    "cache disk budget; prune or clear-cache".into(),
                ));
            }
            atomic_write(&self.snapshot_path(&sha), &bytes)?;
            state.digests.insert(sha.clone(), digest(&bytes));
            state.last_validation = Some(now());
        }
        state.retained.retain(|s| s != &sha);
        state.retained.push(sha.clone());
        if revision.is_none() {
            state.active_sha = Some(sha);
            state.branch = branch;
            state.last_sync = Some(now());
        }
        Ok(())
    }
    pub(crate) fn excluded(&self, path: &str) -> bool {
        self.config
            .excluded_paths
            .iter()
            .any(|prefix| path == prefix || path.starts_with(&format!("{prefix}/")))
    }
    fn mount_exists(&self, sha: &str) -> Result<bool> {
        let mut prefix = String::new();
        for part in self.config.path.split('/').filter(|p| !p.is_empty()) {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            let kind =
                match self.text_git(&["cat-file", "-t", &format!("{sha}:{prefix}")], false, None) {
                    Ok(kind) => kind,
                    Err(Error::Process(_)) => return Ok(false),
                    Err(e) => return Err(e),
                };
            if kind != "tree" {
                return Err(Error::InvalidPath(
                    "mount ancestors must be Git trees".into(),
                ));
            }
        }
        Ok(true)
    }
    pub(crate) fn candidate_files(
        &self,
        sha: &str,
        token: Option<&str>,
    ) -> Result<(bool, BTreeMap<String, FileData>)> {
        if !self.mount_exists(sha)? {
            return Ok((false, BTreeMap::new()));
        }
        Ok((true, self.collect_snapshot(sha, token)?.files))
    }
    /// Fetch scoped source bytes without changing the active validated revision.
    pub fn inspect_candidate(&self, revision: Option<&str>) -> Result<Candidate> {
        let _lock = self.lock()?;
        let (sha, branch, token) = self.fetch_revision(revision)?;
        let (root_exists, files) = self.candidate_files(&sha, token.as_deref())?;
        let issues = if !root_exists {
            vec!["mount directory does not exist".into()]
        } else {
            self.config
                .validation
                .validate(&files)
                .err()
                .map(|e| vec![e.to_string()])
                .unwrap_or_default()
        };
        self.check_cache_size()?;
        let result = Candidate {
            commit_sha: sha,
            branch,
            root_exists,
            files,
            issues,
        };
        self.bound_response(&result)?;
        Ok(result)
    }
    /// Read all validated files once under the snapshot read lock, without network access.
    pub fn snapshot(&self, revision: Option<&str>) -> Result<Snapshot> {
        let _lock = self.read_lock()?;
        let (commit_sha, files) = self.files_at_unlocked(revision)?;
        let result = Snapshot { commit_sha, files };
        self.bound_response(&result)?;
        Ok(result)
    }
    fn collect_snapshot(&self, sha: &str, token: Option<&str>) -> Result<Snapshot> {
        if !self.mount_exists(sha)? {
            return Err(Error::NotFound(self.config.path.clone()));
        }
        let tree_spec = if self.config.path.is_empty() {
            format!("{sha}^{{tree}}")
        } else {
            format!("{sha}:{}", self.config.path)
        };
        // Enumerate the mounted tree itself, so a symlink or submodule in an ancestor is never followed.
        let tree_type = self
            .text_git(&["cat-file", "-t", &tree_spec], false, None)
            .map_err(|_| Error::NotFound(self.config.path.clone()))?;
        if tree_type != "tree" {
            return Err(Error::InvalidPath("mount must be a Git tree".into()));
        }
        let listing = self.git(
            &["ls-tree", "-r", "-z", &tree_spec],
            false,
            None,
            self.config.limits.max_files.saturating_mul(1024),
        )?;
        let mut files = BTreeMap::new();
        let mut total = 0usize;
        for record in listing.split(|c| *c == 0).filter(|s| !s.is_empty()) {
            let text = std::str::from_utf8(record)
                .map_err(|_| Error::InvalidPath("non UTF-8 filename".into()))?;
            let (meta, path) = text
                .split_once('\t')
                .ok_or_else(|| Error::Corrupt("tree entry".into()))?;
            let path = safe_path(path, false)?;
            if self.excluded(&path) {
                continue;
            }
            let fields: Vec<_> = meta.split_whitespace().collect();
            if fields.len() != 3
                || !["100644", "100755"].contains(&fields[0])
                || fields[1] != "blob"
            {
                return Err(Error::Unsupported(format!(
                    "{path}: symlinks and submodules are unsupported"
                )));
            }
            if files.len() >= self.config.limits.max_files {
                return Err(Error::Limit("file count".into()));
            }
            // Explicit lazy fetch is permitted only during sync, and only for mounted blobs.
            let size = self
                .text_git(&["cat-file", "-s", fields[2]], true, token)?
                .parse::<usize>()
                .map_err(|_| Error::Corrupt("blob size".into()))?;
            total = total
                .checked_add(size)
                .ok_or_else(|| Error::Limit("mount size".into()))?;
            if size > self.config.limits.file_bytes || total > self.config.limits.mount_bytes {
                return Err(Error::Limit(format!("mounted content: {path}")));
            }
            let bytes = self.git(
                &["cat-file", "blob", fields[2]],
                true,
                token,
                self.config.limits.file_bytes,
            )?;
            if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1\n") {
                return Err(Error::Validation(format!("{path}: Git LFS is unsupported")));
            }
            let mut file = FileData::new(&bytes);
            file.executable = fields[0] == "100755";
            files.insert(path, file);
        }
        Ok(Snapshot {
            commit_sha: sha.into(),
            files,
        })
    }
    fn snapshot_path(&self, sha: &str) -> PathBuf {
        self.directory.join("snapshots").join(format!("{sha}.json"))
    }
    fn load_snapshot(&self, sha: &str, state: &Status) -> Result<Snapshot> {
        if !valid_sha(sha) {
            return Err(Error::InvalidPath("full commit SHA required".into()));
        }
        let expected = state
            .digests
            .get(sha)
            .ok_or_else(|| Error::CacheMiss(sha.into()))?;
        let bytes = fs::read(self.snapshot_path(sha))
            .map_err(|_| Error::Corrupt(format!("missing snapshot {sha}")))?;
        if digest(&bytes) != *expected {
            return Err(Error::Corrupt(format!("snapshot {sha} checksum mismatch")));
        }
        let snapshot: Snapshot = serde_json::from_slice(&bytes)
            .map_err(|_| Error::Corrupt("snapshot encoding".into()))?;
        if snapshot.commit_sha != sha || snapshot.files.keys().any(|p| safe_path(p, false).is_err())
        {
            return Err(Error::Corrupt("snapshot paths or SHA".into()));
        }
        Ok(snapshot)
    }
    pub(crate) fn files_at(
        &self,
        revision: Option<&str>,
    ) -> Result<(String, BTreeMap<String, FileData>)> {
        let _lock = self.read_lock()?;
        self.files_at_unlocked(revision)
    }
    pub(crate) fn files_at_unlocked(
        &self,
        revision: Option<&str>,
    ) -> Result<(String, BTreeMap<String, FileData>)> {
        let state = self.state()?;
        let sha = revision
            .or(state.active_sha.as_deref())
            .ok_or_else(|| Error::CacheMiss("uninitialized mount".into()))?;
        let snapshot = self.load_snapshot(sha, &state)?;
        Ok((sha.into(), snapshot.files))
    }
    pub fn read(&self, path: &str, revision: Option<&str>) -> Result<ReadResult> {
        let path = safe_path(path, false)?;
        let (sha, mut files) = self.files_at(revision)?;
        let data = files
            .remove(&path)
            .ok_or_else(|| Error::NotFound(path.clone()))?;
        if data.content.len() > self.config.limits.response_bytes {
            return Err(Error::Limit("read response".into()));
        }
        let status = self.status()?;
        Ok(ReadResult {
            data,
            provenance: Provenance {
                repository: self.identity(),
                branch: status.branch,
                mount_path: self.config.path.clone(),
                commit_sha: sha,
                file_path: path,
                stale: status.stale,
            },
        })
    }
    pub fn list(&self, prefix: &str, revision: Option<&str>) -> Result<Vec<String>> {
        let prefix = safe_path(prefix, true)?;
        let (_, files) = self.files_at(revision)?;
        let paths: Vec<_> = files
            .into_keys()
            .filter(|p| prefix.is_empty() || p == &prefix || p.starts_with(&format!("{prefix}/")))
            .collect();
        self.bound_response(&paths)?;
        Ok(paths)
    }
    pub fn search(&self, query: &str, revision: Option<&str>, limit: usize) -> Result<Vec<String>> {
        if query.is_empty() || query.len() > 4096 || limit == 0 || limit > 1000 {
            return Err(Error::Config(
                "query must be 1..4096 bytes; limit 1..1000".into(),
            ));
        }
        let (_, files) = self.files_at(revision)?;
        let mut matches = Vec::new();
        for (path, file) in files {
            if path.contains(query)
                || std::str::from_utf8(&file.bytes()?).is_ok_and(|s| s.contains(query))
            {
                matches.push(path);
            }
            if matches.len() == limit {
                break;
            }
        }
        self.bound_response(&matches)?;
        Ok(matches)
    }
    /// History is the retained, validated snapshot history, newest sync first.
    pub fn history(&self, path: Option<&str>) -> Result<Vec<HistoryEntry>> {
        let _lock = self.read_lock()?;
        let path = path.map(|p| safe_path(p, false)).transpose()?;
        let state = self.state()?;
        let mut entries = Vec::new();
        for sha in state.retained.iter().rev() {
            let snap = self.load_snapshot(sha, &state)?;
            if path.as_ref().is_none_or(|p| snap.files.contains_key(p)) {
                entries.push(HistoryEntry {
                    commit_sha: sha.clone(),
                    active: state.active_sha.as_ref() == Some(sha),
                    pinned: state.pinned.contains(sha),
                });
            }
        }
        self.bound_response(&entries)?;
        Ok(entries)
    }
    pub fn validate(&self, revision: Option<&str>) -> Result<()> {
        self.config.validation.validate(&self.files_at(revision)?.1)
    }
    pub(crate) fn bound_response(&self, value: &impl Serialize) -> Result<()> {
        if serde_json::to_vec(value)?.len() > self.config.limits.response_bytes {
            return Err(Error::Limit("response size".into()));
        }
        Ok(())
    }
    pub(crate) fn identity(&self) -> String {
        match &self.config.source {
            Source::Local { path } => path.display().to_string(),
            Source::Github { repository } => repository.clone(),
        }
    }
    pub(crate) fn check_cache_size(&self) -> Result<()> {
        if disk_usage(&self.directory)? > self.config.limits.cache_bytes {
            return Err(Error::Limit(
                "cache disk budget; prune or clear-cache".into(),
            ));
        }
        Ok(())
    }
    pub fn pin(&self, sha: &str, pinned: bool) -> Result<()> {
        let _lock = self.lock()?;
        let mut state = self.state()?;
        self.load_snapshot(sha, &state)?;
        if pinned {
            state.pinned.insert(sha.into());
        } else {
            state.pinned.remove(sha);
        }
        self.save_state(&state)
    }
    pub fn prune(&self) -> Result<Status> {
        let _lock = self.lock()?;
        let mut state = self.state()?;
        self.prune_locked(&mut state)?;
        self.save_state(&state)?;
        self.status_unlocked()
    }
    fn prune_locked(&self, state: &mut Status) -> Result<()> {
        let keep: BTreeSet<_> = state
            .retained
            .iter()
            .rev()
            .take(self.config.limits.retained_revisions)
            .cloned()
            .chain(state.pinned.iter().cloned())
            .chain(state.active_sha.iter().cloned())
            .collect();
        let removed: Vec<_> = state
            .retained
            .iter()
            .filter(|s| !keep.contains(*s))
            .cloned()
            .collect();
        // Commit metadata first: crashes may leave harmless orphan files, never a missing retained snapshot.
        state.retained.retain(|s| keep.contains(s));
        state.digests.retain(|s, _| keep.contains(s));
        self.save_state(state)?;
        for sha in removed {
            let _ = fs::remove_file(self.snapshot_path(&sha));
        }
        Ok(())
    }
    pub fn clear_cache(&self) -> Result<()> {
        let _lock = self.lock()?;
        self.save_state(&Status::default())?;
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            if entry.file_name() == "lock" || entry.file_name() == "state.json" {
                continue;
            }
            if entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
            } else {
                fs::remove_file(entry.path())?;
            }
        }
        fs::create_dir_all(self.directory.join("snapshots"))?;
        Ok(())
    }
}
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    file.write_all(bytes)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
fn disk_usage(path: &Path) -> Result<u64> {
    let mut size = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        size += if meta.is_dir() {
            disk_usage(&entry.path())?
        } else {
            meta.len()
        };
    }
    Ok(size)
}
