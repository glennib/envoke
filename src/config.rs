use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::Deserialize;

/// Top-level envoke configuration, typically loaded from `envoke.yaml`.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Config {
    /// Map of variable names to their definitions.
    pub variables: BTreeMap<String, Variable>,
}

/// A single environment variable with per-environment sources.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Variable {
    /// Human-readable description, rendered as a comment in output.
    pub description: Option<String>,
    /// Tags for conditional inclusion. When `--tag` flags are passed on the
    /// CLI, only variables with at least one matching tag (or no tags) are
    /// included.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Fallback source used when the requested environment has no entry in
    /// `envs`.
    pub default: Option<Source>,
    /// Map of environment names to value sources.
    #[serde(default)]
    pub envs: BTreeMap<String, Source>,
    /// Named overrides that can be activated via `--override` on the CLI.
    /// Each override provides alternative `default`/`envs` sources that
    /// take precedence over the base sources when active.
    #[serde(default)]
    pub overrides: BTreeMap<String, Override>,
}

/// An override provides alternative sources for a variable, activated via
/// the `--override` CLI flag.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct Override {
    /// Fallback source for this override when the environment has no entry.
    pub default: Option<Source>,
    /// Map of environment names to value sources for this override.
    #[serde(default)]
    pub envs: BTreeMap<String, Source>,
}

/// How to obtain the value for a variable in a given environment.
///
/// Exactly one of `literal`, `cmd`, `sh`, `template`, or `skip` must be
/// specified.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    /// A fixed string value.
    pub literal: Option<String>,
    /// A command to execute; stdout is captured and trimmed.
    pub cmd: Option<Vec<String>>,
    /// A shell script to execute via `sh -c`; stdout is captured and trimmed.
    pub sh: Option<String>,
    /// A minijinja (Jinja2) template string. Reference other variables with `{{ VAR_NAME }}`.
    pub template: Option<String>,
    /// When `true`, the variable is silently omitted from output.
    pub skip: Option<bool>,
}

/// The resolved kind of a source after validation.
#[derive(Debug)]
pub enum SourceKind {
    Literal(String),
    Cmd(Vec<String>),
    Sh(String),
    Template(String),
    Skip,
}

impl Source {
    /// Validate that exactly one field is set and return the resolved kind.
    pub fn kind(&self) -> Result<SourceKind, &'static str> {
        match (
            &self.literal,
            &self.cmd,
            &self.sh,
            &self.template,
            &self.skip,
        ) {
            (None, None, None, None, Some(true)) => Ok(SourceKind::Skip),
            (Some(v), None, None, None, None) => Ok(SourceKind::Literal(v.clone())),
            (None, Some(v), None, None, None) if v.is_empty() => {
                Err("`cmd` must have at least one element")
            }
            (None, Some(v), None, None, None) => Ok(SourceKind::Cmd(v.clone())),
            (None, None, Some(v), None, None) => Ok(SourceKind::Sh(v.clone())),
            (None, None, None, Some(v), None) => Ok(SourceKind::Template(v.clone())),
            (None, None, None, None, None | Some(false)) => {
                Err("one of `literal`, `cmd`, `sh`, `template`, or `skip` must be specified")
            }
            _ => Err("only one of `literal`, `cmd`, `sh`, `template`, or `skip` may be specified"),
        }
    }
}
