use std::collections::BTreeMap;
use std::collections::BTreeSet;

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
    /// A minijinja (Jinja2) template string. Reference other variables with `{{
    /// VAR_NAME }}`.
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

impl Config {
    /// Returns sorted, deduplicated environment names found across all
    /// variables' `envs` maps and override `envs` maps.
    pub fn environments(&self) -> Vec<String> {
        let mut set = BTreeSet::new();
        for var in self.variables.values() {
            set.extend(var.envs.keys().cloned());
            for ovr in var.overrides.values() {
                set.extend(ovr.envs.keys().cloned());
            }
        }
        set.into_iter().collect()
    }

    /// Returns sorted, deduplicated override names found across all variables.
    pub fn override_names(&self) -> Vec<String> {
        let mut set = BTreeSet::new();
        for var in self.variables.values() {
            set.extend(var.overrides.keys().cloned());
        }
        set.into_iter().collect()
    }

    /// Returns sorted, deduplicated tag names found across all variables.
    pub fn tag_names(&self) -> Vec<String> {
        let mut set = BTreeSet::new();
        for var in self.variables.values() {
            set.extend(var.tags.iter().cloned());
        }
        set.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_literal(val: &str) -> Source {
        Source {
            literal: Some(val.to_string()),
            cmd: None,
            sh: None,
            template: None,
            skip: None,
        }
    }

    fn make_config(variables: Vec<(&str, Variable)>) -> Config {
        Config {
            variables: variables
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }

    #[test]
    fn environments_from_envs_and_overrides() {
        let config = make_config(vec![(
            "VAR",
            Variable {
                description: None,
                tags: vec![],
                default: None,
                envs: BTreeMap::from([
                    ("prod".to_string(), source_literal("a")),
                    ("staging".to_string(), source_literal("b")),
                ]),
                overrides: BTreeMap::from([(
                    "ovr".to_string(),
                    Override {
                        default: None,
                        envs: BTreeMap::from([
                            ("staging".to_string(), source_literal("c")),
                            ("dev".to_string(), source_literal("d")),
                        ]),
                    },
                )]),
            },
        )]);
        assert_eq!(config.environments(), vec!["dev", "prod", "staging"]);
    }

    #[test]
    fn environments_empty() {
        let config = make_config(vec![(
            "VAR",
            Variable {
                description: None,
                tags: vec![],
                default: Some(source_literal("x")),
                envs: BTreeMap::new(),
                overrides: BTreeMap::new(),
            },
        )]);
        assert!(config.environments().is_empty());
    }

    #[test]
    fn override_names_collected_and_deduped() {
        let config = make_config(vec![
            (
                "A",
                Variable {
                    description: None,
                    tags: vec![],
                    default: None,
                    envs: BTreeMap::new(),
                    overrides: BTreeMap::from([
                        (
                            "fast".to_string(),
                            Override {
                                default: Some(source_literal("x")),
                                envs: BTreeMap::new(),
                            },
                        ),
                        (
                            "slow".to_string(),
                            Override {
                                default: Some(source_literal("y")),
                                envs: BTreeMap::new(),
                            },
                        ),
                    ]),
                },
            ),
            (
                "B",
                Variable {
                    description: None,
                    tags: vec![],
                    default: None,
                    envs: BTreeMap::new(),
                    overrides: BTreeMap::from([(
                        "fast".to_string(),
                        Override {
                            default: Some(source_literal("z")),
                            envs: BTreeMap::new(),
                        },
                    )]),
                },
            ),
        ]);
        assert_eq!(config.override_names(), vec!["fast", "slow"]);
    }

    #[test]
    fn override_names_empty() {
        let config = make_config(vec![(
            "VAR",
            Variable {
                description: None,
                tags: vec![],
                default: Some(source_literal("x")),
                envs: BTreeMap::new(),
                overrides: BTreeMap::new(),
            },
        )]);
        assert!(config.override_names().is_empty());
    }

    #[test]
    fn tag_names_collected_and_deduped() {
        let config = make_config(vec![
            (
                "A",
                Variable {
                    description: None,
                    tags: vec!["oauth".to_string(), "vault".to_string()],
                    default: None,
                    envs: BTreeMap::new(),
                    overrides: BTreeMap::new(),
                },
            ),
            (
                "B",
                Variable {
                    description: None,
                    tags: vec!["vault".to_string(), "db".to_string()],
                    default: None,
                    envs: BTreeMap::new(),
                    overrides: BTreeMap::new(),
                },
            ),
        ]);
        assert_eq!(config.tag_names(), vec!["db", "oauth", "vault"]);
    }

    #[test]
    fn tag_names_empty() {
        let config = make_config(vec![(
            "VAR",
            Variable {
                description: None,
                tags: vec![],
                default: Some(source_literal("x")),
                envs: BTreeMap::new(),
                overrides: BTreeMap::new(),
            },
        )]);
        assert!(config.tag_names().is_empty());
    }
}
