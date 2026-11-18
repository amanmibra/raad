use crate::{Error, Result, process::run};
use std::{fmt, process::Command, sync::Arc};

/// Called on each remote operation; hosts can refresh GitHub App installation tokens.
pub trait TokenProvider: Send + Sync {
    fn token(&self, repository: &str) -> Result<Option<String>>;
}
impl<F> TokenProvider for F
where
    F: Fn(&str) -> Result<Option<String>> + Send + Sync,
{
    fn token(&self, repository: &str) -> Result<Option<String>> {
        self(repository)
    }
}
#[derive(Clone, Default)]
pub enum Credentials {
    /// Environment, Git credential helper, then `gh auth token`; anonymous fallback.
    #[default]
    Auto,
    Anonymous,
    Provider(Arc<dyn TokenProvider>),
}
impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Credentials([redacted])")
    }
}
impl Credentials {
    pub fn token(value: impl Into<String>) -> Self {
        let value = value.into();
        Self::Provider(Arc::new(move |_: &str| Ok(Some(value.clone()))))
    }
    pub(crate) fn resolve(&self, repo: &str) -> Result<Option<String>> {
        let result = match self {
            Self::Anonymous => None,
            Self::Provider(provider) => provider
                .token(repo)
                .map_err(|_| Error::Permission("token provider failed".into()))?,
            Self::Auto => {
                let env = ["RAAD_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"]
                    .into_iter()
                    .find_map(|key| std::env::var(key).ok().filter(|s| !s.is_empty()));
                env.or_else(|| {
                    let input = format!("protocol=https\nhost=github.com\npath={repo}.git\n\n");
                    let out = run(
                        Command::new("git")
                            .args(["credential", "fill"])
                            .env("GIT_TERMINAL_PROMPT", "0"),
                        Some(input.into_bytes()),
                        5,
                        16384,
                    )
                    .ok()?;
                    String::from_utf8(out)
                        .ok()?
                        .lines()
                        .find_map(|line| line.strip_prefix("password=").map(str::to_owned))
                })
                .or_else(|| {
                    String::from_utf8(
                        run(
                            Command::new("gh").args(["auth", "token", "--hostname", "github.com"]),
                            None,
                            5,
                            16384,
                        )
                        .ok()?,
                    )
                    .ok()
                    .map(|s| s.trim().to_owned())
                })
            }
        };
        if result
            .as_ref()
            .is_some_and(|s| s.is_empty() || s.contains(['\n', '\r', '\0']))
        {
            return Err(Error::Permission("invalid credential".into()));
        }
        Ok(result)
    }
}
