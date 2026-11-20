//! Compare-and-swap proposal branches with recoverable operation identities.
use crate::github::{Api, field};
use crate::{Error, Mount, Proposal, Result, Source, WriteMode, WriteRequest, valid_sha};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpsertRequest {
    pub request: WriteRequest,
    pub branch: String,
    pub expected_head: Option<String>,
    pub operation_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalOutcome {
    pub outcome: String,
    pub proposal: Option<Proposal>,
    pub pull_request_number: Option<u64>,
    pub state: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalStatus {
    pub number: u64,
    pub url: String,
    pub state: String,
    pub head_sha: String,
    pub base_sha: String,
    pub head_branch: String,
    pub base_branch: String,
}
fn status(pr: &Value) -> Result<ProposalStatus> {
    Ok(ProposalStatus {
        number: pr["number"]
            .as_u64()
            .ok_or_else(|| Error::Remote("missing PR number".into()))?,
        url: field(pr, "html_url")?,
        state: if pr["merged"].as_bool() == Some(true) || pr["merged_at"].is_string() {
            "merged"
        } else if pr["state"] == "open" {
            "open"
        } else {
            "closed"
        }
        .into(),
        head_sha: field(&pr["head"], "sha")?,
        base_sha: field(&pr["base"], "sha")?,
        head_branch: field(&pr["head"], "ref")?,
        base_branch: field(&pr["base"], "ref")?,
    })
}
impl Mount {
    pub fn proposal_status(&self, number: u64) -> Result<ProposalStatus> {
        let Source::Github { repository } = &self.config.source else {
            return Err(Error::Config("GitHub mount required".into()));
        };
        if number == 0 {
            return Err(Error::Config("positive PR number required".into()));
        }
        let token = self
            .credentials
            .resolve(repository)?
            .ok_or_else(|| Error::Permission("GitHub credentials required".into()))?;
        let api = Api::new(&self.api_endpoint, token)?;
        let (owner, repo) = repository.split_once('/').unwrap();
        let pr = api.call(
            Method::GET,
            &["repos", owner, repo, "pulls", &number.to_string()],
            None,
        )?;
        status(&pr)
    }
    pub fn upsert_proposal(&self, input: UpsertRequest) -> Result<ProposalOutcome> {
        if !self.config.allow_proposals {
            return Err(Error::Permission(
                "proposals not authorized for mount".into(),
            ));
        }
        let Source::Github { repository } = &self.config.source else {
            return Err(Error::Config("GitHub mount required".into()));
        };
        let request = &input.request;
        if request.mode != WriteMode::Proposal
            || input.operation_id.is_empty()
            || input.branch.len() > 255
            || input.operation_id.len() > 128
            || !input
                .operation_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
            || input
                .expected_head
                .as_deref()
                .is_some_and(|s| !valid_sha(s))
        {
            return Err(Error::Config(
                "invalid proposal identity, mode or expected head".into(),
            ));
        }
        // Git itself defines branch syntax; no shell interpolation.
        crate::process::run(
            std::process::Command::new("git")
                .args(["check-ref-format", &format!("refs/heads/{}", input.branch)]),
            None,
            10,
            4096,
        )?;
        let _lock = self.lock()?;
        let branch = self
            .config
            .branch
            .clone()
            .unwrap_or(self.status_unlocked()?.branch);
        if branch.is_empty() || branch == input.branch {
            return Err(Error::Config(
                "sync or configure a distinct base branch".into(),
            ));
        }
        let original = self.base_files(&request.expected_base)?;
        let mut prepared = self.prepare_changes(request, original.clone())?;
        self.check_cache_size()?;
        let token = self
            .credentials
            .resolve(repository)?
            .ok_or_else(|| Error::Permission("GitHub credentials required".into()))?;
        let api = Api::new(&self.api_endpoint, token)?;
        let (owner, repo) = repository.split_once('/').unwrap();
        let base_ref = format!("heads/{branch}");
        let proposal_ref = format!("heads/{}", input.branch);
        let head = api.call(
            Method::GET,
            &["repos", owner, repo, "git", "ref", &base_ref],
            None,
        )?;
        let current = field(&head["object"], "sha")?;
        let existing = match api.call(
            Method::GET,
            &["repos", owner, repo, "git", "ref", &proposal_ref],
            None,
        ) {
            Ok(v) => Some(field(&v["object"], "sha")?),
            Err(Error::NotFound(_)) => None,
            Err(e) => return Err(e),
        };
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(request, &input.branch))?)
        );
        let marker = format!(
            "Raad-Branch: {}\nRaad-Operation: {}\nRaad-Request: {}",
            input.branch, input.operation_id, fingerprint
        );
        if request.body.len() + marker.len() + 2 > 65536 {
            return Err(Error::Config(
                "proposal body including operation marker exceeds 64KiB".into(),
            ));
        }
        let found = api.call_query(
            Method::GET,
            &["repos", owner, repo, "pulls"],
            None,
            &[
                ("head", &format!("{owner}:{}", input.branch)),
                ("base", &branch),
                ("state", "all"),
                ("per_page", "100"),
            ],
        )?;
        let prs = found
            .as_array()
            .ok_or_else(|| Error::Remote("invalid PR list".into()))?;
        let existing_pr = prs.iter().find(|p| {
            p["head"]["ref"] == input.branch && p["base"]["ref"] == branch && p["state"] == "open"
        });
        let mut recovered = false;
        let mut recovered_pr = None;
        if let Some(sha) = &existing {
            let commit = api.call(
                Method::GET,
                &["repos", owner, repo, "git", "commits", sha],
                None,
            )?;
            let message = field(&commit, "message")?;
            let owned = message
                .lines()
                .any(|line| line == format!("Raad-Branch: {}", input.branch));
            if !owned {
                return Err(Error::Conflict(
                    "proposal branch is not managed by raad".into(),
                ));
            }
            if message.ends_with(&marker)
                && commit["parents"].as_array().is_some_and(|p| p.len() == 1)
                && commit["parents"][0]["sha"] == request.expected_base
            {
                let (_, _, token) = self.fetch_revision(Some(sha))?;
                let actual = self.candidate_files(sha, token.as_deref())?.1;
                // Recovery must not adopt changes outside this request, including
                // excluded paths or files outside the mount.
                let changed = self.git(
                    &[
                        "diff-tree",
                        "--no-commit-id",
                        "--name-only",
                        "-r",
                        "-z",
                        &request.expected_base,
                        sha,
                    ],
                    false,
                    None,
                    self.config.limits.max_files.saturating_mul(1024),
                )?;
                let allowed: std::collections::BTreeSet<String> = request
                    .changes
                    .iter()
                    .map(|c| {
                        let path = match c {
                            crate::Change::Put { path, .. } | crate::Change::Delete { path } => {
                                path
                            }
                        };
                        if self.config.path.is_empty() {
                            path.clone()
                        } else {
                            format!("{}/{}", self.config.path, path)
                        }
                    })
                    .collect();
                let scoped = changed
                    .split(|b| *b == 0)
                    .filter(|p| !p.is_empty())
                    .all(|p| std::str::from_utf8(p).is_ok_and(|p| allowed.contains(p)));
                self.check_cache_size()?;
                recovered = scoped && actual == prepared.files;
                if recovered {
                    recovered_pr = message
                        .strip_suffix(&marker)
                        .and_then(|s| s.strip_suffix('\n'))
                        .and_then(|s| s.rsplit('\n').next())
                        .and_then(|s| s.strip_prefix("Raad-PR: "))
                        .and_then(|s| s.parse::<u64>().ok());
                }
            }
            if input.expected_head.as_deref() != Some(sha) && !recovered {
                return Err(Error::Conflict(
                    "proposal branch changed; review human edits before retrying".into(),
                ));
            }
        } else if input.expected_head.is_some() {
            return Err(Error::Conflict("proposal branch disappeared".into()));
        }
        let result =
            |kind: &str, sha: Option<&str>, pr: Option<&Value>| -> Result<ProposalOutcome> {
                let info = pr.map(status).transpose()?;
                Ok(ProposalOutcome {
                    outcome: kind.into(),
                    proposal: sha.map(|sha| Proposal {
                        base_sha: request.expected_base.clone(),
                        commit_sha: sha.into(),
                        branch: input.branch.clone(),
                        pull_request_url: info.as_ref().map(|s| s.url.clone()),
                        mode: WriteMode::Proposal,
                    }),
                    pull_request_number: info.as_ref().map(|s| s.number),
                    state: info.map(|s| s.state),
                })
            };
        if original == prepared.files {
            return result("unchanged", existing.as_deref(), existing_pr);
        }
        if recovered
            && let Some(pr) = prs.iter().find(|p| {
                p["head"]["ref"] == input.branch
                    && p["base"]["ref"] == branch
                    && (recovered_pr.is_some_and(|number| p["number"].as_u64() == Some(number))
                        || p["body"]
                            .as_str()
                            .is_some_and(|body| body.ends_with(&marker)))
            })
        {
            return result("unchanged", existing.as_deref(), Some(pr));
        }
        if recovered
            && prs.iter().any(|p| {
                p["state"] != "open"
                    && p["head"]["ref"] == input.branch
                    && p["head"]["sha"].as_str() == existing.as_deref()
                    && !p["body"]
                        .as_str()
                        .is_some_and(|body| body.contains("\nRaad-Operation: "))
            })
        {
            return Err(Error::Conflict(
                "closed PR operation identity is missing; review before retrying".into(),
            ));
        }
        if current != request.expected_base && !recovered {
            return Err(Error::StaleBase(current));
        }
        let commit_sha = if recovered {
            existing.clone().unwrap()
        } else {
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
            for entry in &mut prepared.entries {
                if let Some(content) = entry.as_object_mut().unwrap().remove("raad_content") {
                    let blob = api.call(
                        Method::POST,
                        &["repos", owner, repo, "git", "blobs"],
                        Some(json!({"content":content,"encoding":"base64"})),
                    )?;
                    entry["sha"] = json!(field(&blob, "sha")?);
                }
            }
            let tree = api.call(
                Method::POST,
                &["repos", owner, repo, "git", "trees"],
                Some(json!({"base_tree":field(&base["tree"],"sha")?,"tree":prepared.entries})),
            )?;
            let association = existing_pr
                .and_then(|p| p["number"].as_u64())
                .map(|number| format!("Raad-PR: {number}\n"))
                .unwrap_or_default();
            let commit=api.call(Method::POST,&["repos",owner,repo,"git","commits"],Some(json!({"message":format!("{}\n\n{}\n\n{}{}",request.title,request.body,association,marker),"tree":field(&tree,"sha")?,"parents":[request.expected_base]})))?;
            let sha = field(&commit, "sha")?;
            if !valid_sha(&sha) {
                return Err(Error::Remote("invalid commit SHA".into()));
            }
            let check = api.call(
                Method::GET,
                &["repos", owner, repo, "git", "ref", &base_ref],
                None,
            )?;
            if field(&check["object"], "sha")? != request.expected_base {
                return Err(Error::StaleBase("base branch moved".into()));
            }
            let metadata = api.call(Method::GET, &["repos", owner, repo], None)?;
            let updated=api.call(Method::POST,&["graphql"],Some(json!({"query":"mutation($input:UpdateRefsInput!){updateRefs(input:$input){clientMutationId}}","variables":{"input":{"repositoryId":field(&metadata,"node_id")?,"refUpdates":[{"name":format!("refs/heads/{}",input.branch),"beforeOid":existing.clone().unwrap_or_else(||"0".repeat(40)),"afterOid":sha,"force":true}]}}})))?;
            if updated.get("errors").is_some() || updated["data"]["updateRefs"].is_null() {
                return Err(Error::Conflict(
                    "atomic proposal branch update rejected".into(),
                ));
            }
            sha
        };
        if let Some(pr) = existing_pr {
            return result("updated", Some(&commit_sha), Some(pr));
        }
        let pr=api.call(Method::POST,&["repos",owner,repo,"pulls"],Some(json!({"head":input.branch,"base":branch,"title":request.title,"body":format!("{}\n\n{}",request.body,marker)})))?;
        result("created", Some(&commit_sha), Some(&pr))
    }
}
