use crate::{Error, FileData, Result, process::run, safe_path};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, fs, process::Command};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Validation {
    pub syntax: bool,
    pub frontmatter: bool,
    pub schemas: Vec<SchemaRule>,
    pub commands: Vec<CommandValidator>,
}
impl Default for Validation {
    fn default() -> Self {
        Self {
            syntax: true,
            frontmatter: false,
            schemas: vec![],
            commands: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaRule {
    pub path: String,
    pub schema: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandValidator {
    pub name: String,
    pub command: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}
fn default_timeout() -> u64 {
    30
}
impl Validation {
    pub fn validate(&self, files: &BTreeMap<String, FileData>) -> Result<()> {
        for (path, file) in files {
            let data = STANDARD
                .decode(&file.content)
                .map_err(|_| Error::Corrupt("invalid blob encoding".into()))?;
            if self.syntax {
                match path.rsplit('.').next().unwrap_or("") {
                    "json" => {
                        serde_json::from_slice::<Value>(&data).map_err(|e| {
                            Error::Validation(format!(
                                "{path}: JSON at line {} column {}",
                                e.line(),
                                e.column()
                            ))
                        })?;
                    }
                    "yaml" | "yml" => {
                        serde_yaml::from_slice::<Value>(&data)
                            .map_err(|_| Error::Validation(format!("{path}: invalid YAML")))?;
                    }
                    "md" if self.frontmatter => {
                        let text = std::str::from_utf8(&data)
                            .map_err(|_| Error::Validation(format!("{path}: invalid UTF-8")))?;
                        if let Some(rest) = text.strip_prefix("---\n") {
                            let (front, _) = rest.split_once("\n---").ok_or_else(|| {
                                Error::Validation(format!("{path}: unclosed frontmatter"))
                            })?;
                            serde_yaml::from_str::<Value>(front).map_err(|_| {
                                Error::Validation(format!("{path}: invalid frontmatter"))
                            })?;
                        }
                    }
                    _ => {}
                }
            }
        }
        for rule in &self.schemas {
            let path = safe_path(&rule.path, false)?;
            let file = files
                .get(&path)
                .ok_or_else(|| Error::Validation(format!("schema target missing: {path}")))?;
            let bytes = file.bytes()?;
            let instance: Value = if path.ends_with(".json") {
                serde_json::from_slice(&bytes)
                    .map_err(|_| Error::Validation(format!("{path}: invalid JSON")))?
            } else {
                serde_yaml::from_slice(&bytes)
                    .map_err(|_| Error::Validation(format!("{path}: invalid YAML")))?
            };
            let schema = jsonschema::validator_for(&rule.schema).map_err(|_| {
                Error::Config(format!(
                    "invalid or unresolved schema for {path}; external references are disabled"
                ))
            })?;
            schema
                .validate(&instance)
                .map_err(|_| Error::Validation(format!("{path}: JSON Schema rejected instance")))?;
        }
        if !self.commands.is_empty() {
            let stage = tempfile::tempdir()?;
            for (path, file) in files {
                let dest = stage.path().join(safe_path(path, false)?);
                fs::create_dir_all(dest.parent().unwrap())?;
                fs::write(&dest, file.bytes()?)?;
                let mut permissions = fs::metadata(&dest)?.permissions();
                permissions.set_readonly(true);
                fs::set_permissions(dest, permissions)?;
            }
            for validator in &self.commands {
                let executable = validator
                    .command
                    .first()
                    .ok_or_else(|| Error::Config("empty validator command".into()))?;
                // Credentials and host environment are not forwarded. No shell interpolation.
                let mut command = Command::new(executable);
                command
                    .args(&validator.command[1..])
                    .current_dir(stage.path())
                    .env_clear()
                    .env("PATH", std::env::var_os("PATH").unwrap_or_default())
                    .env("HOME", stage.path())
                    .env("TMPDIR", stage.path());
                run(
                    &mut command,
                    None,
                    validator.timeout_seconds.clamp(1, 300),
                    65536,
                )
                .map_err(|_| {
                    Error::Validation(format!(
                        "validator {} failed, timed out, or exceeded output limit",
                        validator.name
                    ))
                })?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn schema_references_do_not_fetch_network_or_host_files() {
        let files = BTreeMap::from([("config.json".into(), FileData::new(b"{}"))]);
        for reference in ["https://example.invalid/schema.json", "file:///etc/passwd"] {
            let validation = Validation {
                schemas: vec![SchemaRule {
                    path: "config.json".into(),
                    schema: serde_json::json!({"$ref":reference}),
                }],
                ..Validation::default()
            };
            assert!(validation.validate(&files).is_err());
        }
    }
    #[test]
    fn syntax_optional_raw_bytes_and_frontmatter_validation() {
        let files = BTreeMap::from([("bad.json".into(), FileData::new(b"bad"))]);
        assert!(Validation::default().validate(&files).is_err());
        assert!(
            Validation {
                syntax: false,
                ..Validation::default()
            }
            .validate(&files)
            .is_ok()
        );
        let files = BTreeMap::from([(
            "policy.md".into(),
            FileData::new(b"---\nitems: [\n---\nbody"),
        )]);
        assert!(Validation::default().validate(&files).is_ok());
        assert!(
            Validation {
                frontmatter: true,
                ..Validation::default()
            }
            .validate(&files)
            .is_err()
        );
    }
}
