//! Internal binding surface shared by native Python and Node packages.
//! Language wrappers provide their public types; this module reuses core operations unchanged.
use crate::{Credentials, Error, Limits, Mount, MountConfig, Result, Validation, WriteRequest};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Options {
    repository: Option<String>,
    local_path: Option<PathBuf>,
    root: String,
    branch: Option<String>,
    cache_dir: Option<PathBuf>,
    refresh_seconds: u64,
    validation: Validation,
    limits: Limits,
    allow_proposals: bool,
    allow_direct: bool,
    anonymous: bool,
    /// Trusted host setting, never accepted from repository files or MCP requests.
    github_api_url: Option<String>,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            repository: None,
            local_path: None,
            root: String::new(),
            branch: None,
            cache_dir: None,
            refresh_seconds: 60,
            validation: Validation::default(),
            limits: Limits::default(),
            allow_proposals: false,
            allow_direct: false,
            anonymous: false,
            github_api_url: None,
        }
    }
}
#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "snake_case", deny_unknown_fields)]
enum Call {
    Sync {
        #[serde(default)]
        revision: Option<String>,
    },
    Read {
        path: String,
        #[serde(default)]
        revision: Option<String>,
    },
    List {
        #[serde(default)]
        prefix: String,
        #[serde(default)]
        revision: Option<String>,
    },
    Search {
        query: String,
        #[serde(default)]
        revision: Option<String>,
        limit: usize,
    },
    History {
        #[serde(default)]
        path: Option<String>,
    },
    Status,
    Validate {
        #[serde(default)]
        revision: Option<String>,
    },
    Propose {
        request: WriteRequest,
    },
    Pin {
        sha: String,
        pinned: bool,
    },
    Prune,
    ClearCache,
}
#[derive(Clone, Debug)]
pub struct Repository {
    mount: Mount,
}
impl Repository {
    pub fn open(options: &str) -> Result<Self> {
        let options: Options = serde_json::from_str(options)
            .map_err(|_| Error::Config("invalid SDK repository options".into()))?;
        if options.repository.is_some() && options.local_path.is_some() {
            return Err(Error::Config("choose repository or local_path".into()));
        }
        let mut config = if let Some(repo) = options.repository {
            MountConfig::github(&repo, &options.root)?
        } else {
            MountConfig::local(
                options.local_path.unwrap_or(std::env::current_dir()?),
                &options.root,
            )?
        };
        config.branch = options.branch;
        config.refresh_seconds = options.refresh_seconds;
        config.validation = options.validation;
        config.limits = options.limits;
        config.allow_proposals = options.allow_proposals;
        config.allow_direct = options.allow_direct;
        let cache = options
            .cache_dir
            .or_else(|| std::env::var_os("RAAD_CACHE_DIR").map(PathBuf::from))
            .unwrap_or_else(|| {
                std::env::var_os("XDG_CACHE_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")
                    })
                    .join("raad")
            });
        let mut mount = Mount::open(
            config,
            cache,
            if options.anonymous {
                Credentials::Anonymous
            } else {
                Credentials::Auto
            },
        )?;
        if let Some(endpoint) = options.github_api_url {
            mount = mount.with_github_api(&endpoint)?;
        }
        Ok(Self { mount })
    }
    pub fn call(&self, request: &str, token: Option<String>) -> Result<String> {
        let request: Call = serde_json::from_str(request)
            .map_err(|_| Error::Config("invalid SDK operation".into()))?;
        let mount = if let Some(token) = token {
            self.mount.with_credentials(Credentials::token(token))
        } else {
            self.mount.clone()
        };
        let result: Value = match request {
            Call::Sync { revision } => serde_json::to_value(mount.sync(revision.as_deref())?)?,
            Call::Read { path, revision } => {
                serde_json::to_value(mount.read(&path, revision.as_deref())?)?
            }
            Call::List { prefix, revision } => json!(mount.list(&prefix, revision.as_deref())?),
            Call::Search {
                query,
                revision,
                limit,
            } => json!(mount.search(&query, revision.as_deref(), limit)?),
            Call::History { path } => serde_json::to_value(mount.history(path.as_deref())?)?,
            Call::Status => serde_json::to_value(mount.status()?)?,
            Call::Validate { revision } => {
                mount.validate(revision.as_deref())?;
                json!(true)
            }
            Call::Propose { request } => serde_json::to_value(mount.propose(request)?)?,
            Call::Pin { sha, pinned } => {
                mount.pin(&sha, pinned)?;
                Value::Null
            }
            Call::Prune => serde_json::to_value(mount.prune()?)?,
            Call::ClearCache => {
                mount.clear_cache()?;
                Value::Null
            }
        };
        serde_json::to_string(&result).map_err(Error::from)
    }
}
/// Stable machine-readable error codes; bindings never infer types from display strings.
pub fn error_json(error: Error) -> String {
    let code = match &error {
        Error::InvalidPath(_) => "invalid_path",
        Error::NotFound(_) => "not_found",
        Error::CacheMiss(_) => "cache_miss",
        Error::Corrupt(_) => "cache_corruption",
        Error::Validation(_) => "validation",
        Error::Permission(_) => "permission_denied",
        Error::Conflict(_) => "conflict",
        Error::Remote(_) => "remote_unavailable",
        Error::RateLimit(_) => "rate_limit",
        Error::Limit(_) => "limit",
        Error::Process(_) => "process",
        Error::Io(_) => "io",
        Error::Json(_) => "json",
        Error::Config(_) => "configuration",
    };
    json!({"code":code,"message":error.to_string()}).to_string()
}
