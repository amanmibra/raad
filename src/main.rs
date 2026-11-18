use clap::{Parser, Subcommand};
use raad::{
    Change, Credentials, Error, FileData, Mount, MountConfig, Result, Source, Validation,
    WriteMode, WriteRequest,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(
    version,
    about = "Your repo is your database. Git is your infrastructure."
)]
struct Cli {
    #[arg(long, global = true)]
    repo: Option<String>,
    #[arg(long, global = true, default_value = "")]
    root: String,
    #[arg(long, global = true)]
    branch: Option<String>,
    #[arg(long, global = true)]
    revision: Option<String>,
    #[arg(long, global = true)]
    cache_dir: Option<PathBuf>,
    /// Explicit trusted operator configuration; repository configs are never auto-executed.
    #[arg(long, global = true)]
    config: Option<PathBuf>,
    #[arg(long, global = true)]
    mount: Option<String>,
    #[arg(long, global = true)]
    json: bool,
    /// Enable PR writes on explicitly selected MCP mounts.
    #[arg(long, global = true)]
    allow_proposals: bool,
    /// Explicitly authorize direct commits from the CLI only.
    #[arg(long, global = true)]
    allow_direct: bool,
    #[command(subcommand)]
    command: Action,
}
#[derive(Subcommand)]
enum Action {
    Read {
        path: String,
    },
    List {
        #[arg(default_value = "")]
        path: String,
    },
    Search {
        query: String,
        #[arg(long, default_value_t = 100)]
        limit: usize,
    },
    History {
        path: Option<String>,
    },
    Sync,
    Status,
    Validate,
    Propose {
        path: Option<String>,
        #[arg(long)]
        file: Option<PathBuf>,
        #[arg(long)]
        delete: bool,
        /// JSON file containing an array of put/delete Change objects (base64 bytes).
        #[arg(long)]
        changes: Option<PathBuf>,
        #[arg(long)]
        base: String,
        #[arg(long)]
        title: String,
        #[arg(long, default_value = "")]
        body: String,
        #[arg(long)]
        direct: bool,
    },
    Pin {
        sha: String,
        #[arg(long)]
        remove: bool,
    },
    Prune,
    ClearCache,
    Mcp,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    version: u32,
    mounts: Vec<MountConfig>,
    #[serde(default)]
    validation: Option<Validation>,
}
fn cache_path(cli: &Cli) -> PathBuf {
    cli.cache_dir
        .clone()
        .or_else(|| std::env::var_os("RAAD_CACHE_DIR").map(PathBuf::from))
        .unwrap_or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")
                })
                .join("raad")
        })
}
fn mounts(cli: &Cli) -> Result<Vec<Mount>> {
    let mut configs = if let Some(path) = &cli.config {
        if cli.repo.is_some() || !cli.root.is_empty() || cli.branch.is_some() {
            return Err(Error::Config(
                "--config cannot be combined with --repo/--root/--branch".into(),
            ));
        }
        let config: Config = serde_yaml::from_slice(&fs::read(path)?)
            .map_err(|_| Error::Config("invalid trusted config".into()))?;
        if config.version != 1 || config.mounts.is_empty() {
            return Err(Error::Config(
                "expected version: 1 and at least one mount".into(),
            ));
        }
        let mut mounts = config.mounts;
        if let Some(validation) = config.validation {
            for mount in &mut mounts {
                mount.validation = validation.clone();
            }
        }
        mounts
    } else {
        let mut config = if let Some(repo) = &cli.repo {
            MountConfig::github(repo, &cli.root)?
        } else {
            MountConfig::local(std::env::current_dir()?, &cli.root)?
        };
        config.branch = cli.branch.clone();
        vec![config]
    };
    if let Some(name) = &cli.mount {
        configs.retain(|c| &c.name == name);
        if configs.is_empty() {
            return Err(Error::Config("unknown mount".into()));
        }
    }
    let mut names = std::collections::BTreeSet::new();
    configs
        .into_iter()
        .map(|mut config| {
            if !names.insert(config.name.clone()) {
                return Err(Error::Config("duplicate mount name".into()));
            }
            if matches!(cli.command, Action::Mcp) {
                // Explicit CLI flag enables selected mounts; trusted config may grant individual mounts.
                config.allow_proposals |= cli.allow_proposals;
                config.allow_direct = false;
            } else if matches!(cli.command, Action::Propose { .. }) {
                config.allow_proposals = true;
                config.allow_direct = cli.allow_direct;
            }
            Mount::open(config, cache_path(cli), Credentials::Auto)
        })
        .collect()
}
fn prepare(mount: &Mount) -> Result<()> {
    let status = mount.status()?;
    if status.active_sha.is_none() || matches!(mount.config().source, Source::Local { .. }) {
        match mount.sync(None) {
            Ok(_) => {}
            Err(error) if status.active_sha.is_some() => {
                eprintln!("raad: serving last-known-good snapshot: {error}")
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
fn execute(cli: &Cli, mount: &Mount) -> Result<Option<Value>> {
    if !matches!(
        cli.command,
        Action::Sync | Action::Status | Action::Prune | Action::ClearCache | Action::Pin { .. }
    ) {
        prepare(mount)?;
    }
    let revision = cli.revision.as_deref();
    let value = match &cli.command {
        Action::Read { path } => {
            let read = mount.read(path, revision)?;
            if cli.json {
                serde_json::to_value(read)?
            } else {
                io::stdout().write_all(&read.data.bytes()?)?;
                eprintln!(
                    "raad: {}{}",
                    read.provenance.commit_sha,
                    if read.provenance.stale {
                        " (stale)"
                    } else {
                        ""
                    }
                );
                return Ok(None);
            }
        }
        Action::List { path } => {
            json!({"paths":mount.list(path, revision)?,"status":mount.status()?})
        }
        Action::Search { query, limit } => {
            json!({"paths":mount.search(query, revision, *limit)?,"status":mount.status()?})
        }
        Action::History { path } => json!({"revisions":mount.history(path.as_deref())?}),
        Action::Sync => serde_json::to_value(mount.sync(revision)?)?,
        Action::Status => serde_json::to_value(mount.status()?)?,
        Action::Validate => {
            mount.validate(revision)?;
            json!({"valid":true,"revision":revision.map(str::to_owned).or(mount.status()?.active_sha)})
        }
        Action::Propose {
            path,
            file,
            delete,
            changes,
            base,
            title,
            body,
            direct,
        } => {
            let changes = if let Some(file_path) = changes {
                if path.is_some() || file.is_some() || *delete {
                    return Err(Error::Config(
                        "--changes cannot be combined with path/--file/--delete".into(),
                    ));
                }
                serde_json::from_slice(&fs::read(file_path)?)?
            } else {
                let path = path
                    .as_ref()
                    .ok_or_else(|| Error::Config("path required".into()))?
                    .clone();
                if *delete && file.is_none() {
                    vec![Change::Delete { path }]
                } else if !*delete {
                    vec![Change::Put {
                        path,
                        data: FileData::new(&fs::read(
                            file.as_ref()
                                .ok_or_else(|| Error::Config("--file required for put".into()))?,
                        )?),
                    }]
                } else {
                    return Err(Error::Config("choose --file or --delete".into()));
                }
            };
            serde_json::to_value(mount.propose(WriteRequest {
                expected_base: base.clone(),
                title: title.clone(),
                body: body.clone(),
                changes,
                mode: if *direct {
                    WriteMode::Direct
                } else {
                    WriteMode::Proposal
                },
            })?)?
        }
        Action::Pin { sha, remove } => {
            mount.pin(sha, !remove)?;
            json!({"revision":sha,"pinned":!remove})
        }
        Action::Prune => serde_json::to_value(mount.prune()?)?,
        Action::ClearCache => {
            mount.clear_cache()?;
            json!({"cleared":true})
        }
        Action::Mcp => unreachable!(),
    };
    Ok(Some(value))
}
fn main() {
    let cli = Cli::parse();
    let result: Result<()> = (|| {
        let mounts = mounts(&cli)?;
        if matches!(cli.command, Action::Mcp) {
            for mount in &mounts {
                prepare(mount)?;
            }
            let server = raad::mcp::Server::new(mounts)?;
            return tokio::runtime::Runtime::new()?.block_on(server.serve());
        }
        if mounts.len() != 1 {
            return Err(Error::Config("select one mount with --mount".into()));
        }
        if let Some(value) = execute(&cli, &mounts[0])? {
            println!(
                "{}",
                if cli.json {
                    value.to_string()
                } else {
                    serde_json::to_string_pretty(&value)?
                }
            );
        }
        Ok(())
    })();
    if let Err(error) = result {
        if cli.json {
            eprintln!("{}", json!({"error":error.to_string()}));
        } else {
            eprintln!("raad: {error}");
        }
        std::process::exit(match error {
            Error::Conflict(_) => 3,
            Error::Validation(_) => 4,
            Error::Permission(_) => 5,
            Error::RateLimit(_) => 6,
            _ => 1,
        });
    }
}
