use crate::{Error, FileData, Mount, Result, Source, safe_path, valid_sha};
use reqwest::{Method, Url, blocking::Client, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeSet, io::Read, time::Duration};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Change {
    Put { path: String, data: FileData },
    Delete { path: String },
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WriteMode {
    #[default]
    Proposal,
    Direct,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WriteRequest {
    pub expected_base: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    pub changes: Vec<Change>,
    #[serde(default)]
    pub mode: WriteMode,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub base_sha: String,
    pub commit_sha: String,
    pub branch: String,
    pub pull_request_url: Option<String>,
    pub mode: WriteMode,
}
pub(crate) struct Api {
    base: Url,
    client: Client,
    token: String,
}
impl Api {
    pub(crate) fn new(endpoint: &str, token: String) -> Result<Self> {
        let base =
            Url::parse(endpoint).map_err(|_| Error::Config("invalid API endpoint".into()))?;
        if !base.username().is_empty()
            || base.password().is_some()
            || base.query().is_some()
            || base.fragment().is_some()
        {
            return Err(Error::Config(
                "API endpoint must not contain credentials, query or fragment".into(),
            ));
        }
        if base.scheme() != "https"
            && !(base.scheme() == "http"
                && matches!(base.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
        {
            return Err(Error::Config(
                "API requires HTTPS (or loopback for testing)".into(),
            ));
        }
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .redirect(Policy::none())
            .build()
            .map_err(|_| Error::Remote("HTTP client setup".into()))?;
        Ok(Self {
            base,
            client,
            token,
        })
    }
    pub(crate) fn call(
        &self,
        method: Method,
        segments: &[&str],
        body: Option<Value>,
    ) -> Result<Value> {
        self.call_query(method, segments, body, &[])
    }
    pub(crate) fn call_query(
        &self,
        method: Method,
        segments: &[&str],
        body: Option<Value>,
        query: &[(&str, &str)],
    ) -> Result<Value> {
        let mut url = self.base.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| Error::Config("API endpoint".into()))?;
            path.pop_if_empty();
            for part in segments {
                path.push(part);
            }
        }
        if !query.is_empty() {
            url.query_pairs_mut().extend_pairs(query.iter().copied());
        }
        let mut req = self
            .client
            .request(method, url)
            .bearer_auth(&self.token)
            .header("User-Agent", concat!("raad/", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(body) = body {
            req = req.json(&body);
        }
        let response = req
            .send()
            .map_err(|_| Error::Remote("GitHub request failed".into()))?;
        let code = response.status().as_u16();
        if code == 429
            || (code == 403
                && (response
                    .headers()
                    .get("x-ratelimit-remaining")
                    .is_some_and(|s| s == "0")
                    || response.headers().contains_key("retry-after")))
        {
            let retry = response
                .headers()
                .get("retry-after")
                .or_else(|| response.headers().get("x-ratelimit-reset"))
                .and_then(|s| s.to_str().ok())
                .unwrap_or("server rate-limit reset")
                .to_owned();
            return Err(Error::RateLimit(
                retry
                    .chars()
                    .filter(char::is_ascii_digit)
                    .take(20)
                    .collect(),
            ));
        }
        match code {
            401 | 403 => {
                return Err(Error::Permission(
                    "GitHub credentials or repository permissions".into(),
                ));
            }
            404 => {
                return Err(Error::NotFound(
                    "GitHub resource (or private repository access denied)".into(),
                ));
            }
            409 | 422 => {
                return Err(Error::Conflict(
                    "GitHub rejected the branch update or proposal".into(),
                ));
            }
            200..=299 => {}
            _ => return Err(Error::Remote(format!("GitHub HTTP {code}"))),
        }
        let mut bytes = Vec::new();
        response.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 1024 * 1024 {
            return Err(Error::Limit("GitHub response".into()));
        }
        serde_json::from_slice(&bytes).map_err(|_| Error::Remote("invalid GitHub response".into()))
    }
}
pub(crate) fn field(value: &Value, key: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| Error::Remote(format!("GitHub response missing {key}")))
}
impl Mount {
    /// Set a trusted API endpoint for test harnesses. Not accepted from agent requests.
    #[doc(hidden)]
    pub fn with_github_api(mut self, endpoint: &str) -> Result<Self> {
        Api::new(endpoint, "endpoint-validation".into())?;
        self.api_endpoint = endpoint.into();
        Ok(self)
    }
    pub fn propose(&self, request: WriteRequest) -> Result<Proposal> {
        self.propose_with_api(request, &self.api_endpoint)
    }
    /// Trusted endpoint override for GitHub-compatible test servers. Never accepted from MCP requests.
    pub fn propose_with_api(&self, request: WriteRequest, endpoint: &str) -> Result<Proposal> {
        if (request.mode == WriteMode::Proposal && !self.config.allow_proposals)
            || (request.mode == WriteMode::Direct && !self.config.allow_direct)
        {
            return Err(Error::Permission(
                "write mode not authorized for mount".into(),
            ));
        }
        let Source::Github { repository } = &self.config.source else {
            return Err(Error::Config("proposals require a GitHub mount".into()));
        };
        if !valid_sha(&request.expected_base)
            || request.title.trim().is_empty()
            || request.title.len() > 256
            || request.body.len() > 65536
            || request.changes.is_empty()
            || request.changes.len() > self.config.limits.max_files
        {
            return Err(Error::Config("full expected SHA, title (1..256 bytes), body <=64KiB and bounded nonempty changes required".into()));
        }
        let _lock = self.lock()?;
        let status = self.status_unlocked()?;
        let branch = self.config.branch.clone().unwrap_or(status.branch);
        if branch.is_empty() {
            return Err(Error::CacheMiss("sync before writing".into()));
        }
        let token = self
            .credentials
            .resolve(repository)?
            .ok_or_else(|| Error::Permission("GitHub writes require credentials".into()))?;
        let api = Api::new(endpoint, token)?;
        let (owner, repo) = repository.split_once('/').unwrap();
        let head = api.call(
            Method::GET,
            &[
                "repos",
                owner,
                repo,
                "git",
                "ref",
                &format!("heads/{branch}"),
            ],
            None,
        )?;
        let current = field(&head["object"], "sha")?;
        if current != request.expected_base {
            return Err(Error::Conflict(current));
        }
        let prepared = self.prepare_changes(&request, self.base_files(&request.expected_base)?)?;
        self.check_cache_size()?;
        let Prepared {
            files: _,
            mut entries,
            additions,
            deletions,
        } = prepared;
        if request.mode == WriteMode::Direct {
            // GitHub's atomic expectedHeadOid check avoids the compare/update race in REST refs.
            let result = api.call(Method::POST, &["graphql"], Some(json!({
                "query": "mutation($input: CreateCommitOnBranchInput!) { createCommitOnBranch(input: $input) { commit { oid } } }",
                "variables": { "input": { "branch": { "repositoryNameWithOwner": repository, "branchName": branch }, "expectedHeadOid": request.expected_base,
                    "message": { "headline": request.title, "body": request.body }, "fileChanges": { "additions": additions, "deletions": deletions } } }
            })))?;
            if result.get("errors").is_some() {
                return Err(Error::Conflict(
                    "GitHub rejected expected head or branch policy".into(),
                ));
            }
            let commit_sha = field(&result["data"]["createCommitOnBranch"]["commit"], "oid")?;
            return Ok(Proposal {
                base_sha: request.expected_base,
                commit_sha,
                branch,
                pull_request_url: None,
                mode: request.mode,
            });
        }
        let base = api.call(
            Method::GET,
            &[
                "repos",
                owner,
                repo,
                "git",
                "commits",
                &request.expected_base,
            ],
            None,
        )?;
        let tree_sha = field(&base["tree"], "sha")?;
        for entry in &mut entries {
            if let Some(content) = entry.as_object_mut().unwrap().remove("raad_content") {
                let blob = api.call(
                    Method::POST,
                    &["repos", owner, repo, "git", "blobs"],
                    Some(json!({"content": content, "encoding": "base64"})),
                )?;
                entry["sha"] = json!(field(&blob, "sha")?);
            }
        }
        let tree = api.call(
            Method::POST,
            &["repos", owner, repo, "git", "trees"],
            Some(json!({"base_tree": tree_sha, "tree": entries})),
        )?;
        let commit = api.call(Method::POST, &["repos", owner, repo, "git", "commits"], Some(json!({"message": format!("{}\n\n{}", request.title, request.body), "tree": field(&tree, "sha")?, "parents": [request.expected_base]})))?;
        let commit_sha = field(&commit, "sha")?;
        if !valid_sha(&commit_sha) {
            return Err(Error::Remote("invalid commit SHA".into()));
        }
        // Recheck before publishing the proposal. A PR can safely remain based on the expected commit.
        let head = api.call(
            Method::GET,
            &[
                "repos",
                owner,
                repo,
                "git",
                "ref",
                &format!("heads/{branch}"),
            ],
            None,
        )?;
        if field(&head["object"], "sha")? != request.expected_base {
            return Err(Error::Conflict(
                "branch moved while preparing proposal".into(),
            ));
        }
        let proposal_branch = format!("raad/{}", &commit_sha[..20]);
        api.call(
            Method::POST,
            &["repos", owner, repo, "git", "refs"],
            Some(json!({"ref": format!("refs/heads/{proposal_branch}"), "sha": commit_sha})),
        )?;
        let pr = api.call(Method::POST, &["repos", owner, repo, "pulls"], Some(json!({"title": request.title, "body": request.body, "head": proposal_branch, "base": branch})))
            .map_err(|error| match error { Error::Remote(_) => Error::Remote(format!("PR creation failed; proposal branch {proposal_branch} remains for recovery")), other => other })?;
        Ok(Proposal {
            base_sha: request.expected_base,
            commit_sha,
            branch: proposal_branch,
            pull_request_url: Some(field(&pr, "html_url")?),
            mode: request.mode,
        })
    }
    pub(crate) fn base_files(
        &self,
        sha: &str,
    ) -> Result<std::collections::BTreeMap<String, FileData>> {
        match self.files_at_unlocked(Some(sha)) {
            Ok((_, files)) => Ok(files),
            Err(Error::CacheMiss(_)) => {
                let (_, _, token) = self.fetch_revision(Some(sha))?;
                Ok(self.candidate_files(sha, token.as_deref())?.1)
            }
            Err(error) => Err(error),
        }
    }
    pub(crate) fn prepare_changes(
        &self,
        request: &WriteRequest,
        mut files: std::collections::BTreeMap<String, FileData>,
    ) -> Result<Prepared> {
        if !valid_sha(&request.expected_base)
            || request.title.trim().is_empty()
            || request.title.len() > 256
            || request.body.len() > 65536
            || request.changes.len() > self.config.limits.max_files
        {
            return Err(Error::Config("invalid or oversized write request".into()));
        }
        let mut seen = BTreeSet::new();
        let mut entries = Vec::new();
        let mut additions = Vec::new();
        let mut deletions = Vec::new();
        for change in &request.changes {
            let path = match change {
                Change::Put { path, .. } | Change::Delete { path } => safe_path(path, false)?,
            };
            if self.excluded(&path) {
                return Err(Error::InvalidPath(
                    "writes to excluded paths are forbidden".into(),
                ));
            }
            if !seen.insert(path.clone()) {
                return Err(Error::Config("duplicate change path".into()));
            }
            let full = if self.config.path.is_empty() {
                path.clone()
            } else {
                format!("{}/{path}", self.config.path)
            };
            match change {
                Change::Put { data, .. } => {
                    let bytes = data.bytes()?;
                    if bytes.len() > self.config.limits.file_bytes {
                        return Err(Error::Limit("write file size".into()));
                    }
                    if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1\n") {
                        return Err(Error::Validation("Git LFS unsupported".into()));
                    }
                    if request.mode == WriteMode::Direct
                        && (data.executable || files.get(&path).is_some_and(|f| f.executable))
                    {
                        return Err(Error::Config(
                            "direct GraphQL writes only support regular nonexecutable files".into(),
                        ));
                    }
                    files.insert(path, data.clone());
                    entries.push(json!({"path": full, "mode": if data.executable { "100755" } else { "100644" }, "type": "blob", "raad_content": data.content}));
                    additions.push(json!({"path": full, "contents": data.content}));
                }
                Change::Delete { .. } => {
                    if files.remove(&path).is_none() {
                        return Err(Error::NotFound(path));
                    }
                    entries
                        .push(json!({"path": full, "mode": "100644", "type": "blob", "sha": null}));
                    deletions.push(json!({"path": full}));
                }
            }
        }
        let mut total = 0usize;
        for (path, file) in &files {
            total = total
                .checked_add(file.bytes()?.len())
                .ok_or_else(|| Error::Limit("write mount size".into()))?;
            if path
                .split('/')
                .scan(String::new(), |prefix, part| {
                    if !prefix.is_empty() {
                        prefix.push('/');
                    }
                    prefix.push_str(part);
                    Some(prefix.clone())
                })
                .any(|p| p != *path && files.contains_key(&p))
            {
                return Err(Error::InvalidPath("file/directory collision".into()));
            }
        }
        if total > self.config.limits.mount_bytes || files.len() > self.config.limits.max_files {
            return Err(Error::Limit("write mount size or count".into()));
        }
        self.config.validation.validate(&files)?;
        Ok(Prepared {
            files,
            entries,
            additions,
            deletions,
        })
    }
}

pub(crate) struct Prepared {
    pub files: std::collections::BTreeMap<String, FileData>,
    pub entries: Vec<Value>,
    pub additions: Vec<Value>,
    pub deletions: Vec<Value>,
}
