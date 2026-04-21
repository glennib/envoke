use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use miette::Context;
use miette::IntoDiagnostic;

use crate::resolve::Resolved;

const SHELL_TEMPLATE: &str = include_str!("templates/shell.j2");
const SHELL_EXPORT_TEMPLATE: &str = include_str!("templates/shell-export.j2");
const DOTENV_TEMPLATE: &str = include_str!("templates/dotenv.j2");
const JSON_TEMPLATE: &str = include_str!("templates/json.j2");
const YAML_TEMPLATE: &str = include_str!("templates/yaml.j2");
const K8S_SECRET_TEMPLATE: &str = include_str!("templates/k8s-secret.j2");
const GITHUB_ACTIONS_TEMPLATE: &str = include_str!("templates/github-actions.j2");
const TERRAFORM_TFVARS_TEMPLATE: &str = include_str!("templates/terraform-tfvars.j2");

/// Curated built-in output formats selectable via `--format`.
///
/// Variant doc comments are kept to a single line because clap renders them
/// as a single-paragraph "Possible values" list in `--help` — multi-line
/// comments get collapsed into very wide lines. Full per-preset details live
/// in the `--format` arg's `long_help` in `main.rs`.
#[derive(Copy, Clone, Debug, clap::ValueEnum)]
#[clap(rename_all = "kebab-case")]
pub enum Format {
    /// POSIX shell: `KEY='value'` (the default when no format flag is given).
    Shell,
    /// POSIX shell with `export` prefix: `export KEY='value'`.
    ShellExport,
    /// `.env` file syntax: `KEY="value"` with JSON-style escapes.
    Dotenv,
    /// Compact JSON object (pipe through `jq .` for pretty output).
    Json,
    /// YAML mapping in block style: `KEY: "value"`.
    Yaml,
    /// Kubernetes `Secret` manifest with `stringData:`.
    K8sSecret,
    /// Heredoc blocks for `>> "$GITHUB_ENV"` in GitHub Actions.
    GithubActions,
    /// Terraform `*.tfvars`: `KEY = "value"`.
    TerraformTfvars,
}

impl Format {
    fn template(self) -> &'static str {
        match self {
            Self::Shell => SHELL_TEMPLATE,
            Self::ShellExport => SHELL_EXPORT_TEMPLATE,
            Self::Dotenv => DOTENV_TEMPLATE,
            Self::Json => JSON_TEMPLATE,
            Self::Yaml => YAML_TEMPLATE,
            Self::K8sSecret => K8S_SECRET_TEMPLATE,
            Self::GithubActions => GITHUB_ACTIONS_TEMPLATE,
            Self::TerraformTfvars => TERRAFORM_TFVARS_TEMPLATE,
        }
    }
}

/// Metadata about the current invocation, exposed to templates as `meta`.
#[derive(serde::Serialize)]
pub struct Meta {
    /// RFC 3339 timestamp of when the invocation started.
    pub timestamp: String,
    /// Full CLI invocation as a single string (e.g. `"envoke render local"`).
    pub invocation: String,
    /// CLI arguments as individual elements.
    pub invocation_args: Vec<String>,
    /// Target environment name.
    pub environment: String,
    /// Path to the config file used.
    pub config_file: String,
    /// Active `--tag` values.
    pub tags: Vec<String>,
    /// Active `--override` values.
    pub overrides: Vec<String>,
}

/// Rich variable entry exposed in the `variables` map.
#[derive(serde::Serialize)]
struct VariableEntry {
    value: String,
    description: Option<String>,
}

/// Everything needed to render output.
pub struct RenderContext {
    /// Resolved variables in alphabetical order.
    pub resolved: Vec<Resolved>,
    /// Invocation metadata exposed as `meta` in templates.
    pub meta: Meta,
}

/// Render a template string with the given context.
fn render(ctx: &RenderContext, template: &str) -> miette::Result<String> {
    let mut variables: BTreeMap<&str, VariableEntry> = BTreeMap::new();
    let mut v: BTreeMap<&str, &str> = BTreeMap::new();

    for r in &ctx.resolved {
        variables.insert(
            &r.name,
            VariableEntry {
                value: r.value.clone(),
                description: r.description.clone(),
            },
        );
        v.insert(&r.name, &r.value);
    }

    let mut env = minijinja::Environment::new();
    env.add_filter("shell_escape", shell_escape);
    env.add_template("output", template)
        .into_diagnostic()
        .context("failed to parse output template")?;

    let tmpl = env.get_template("output").expect("template just added");
    let rendered = tmpl
        .render(minijinja::context! {
            variables => variables,
            v => v,
            meta => &ctx.meta,
        })
        .into_diagnostic()
        .context("failed to render output template")?;

    Ok(rendered)
}

/// Render using one of the built-in format presets.
pub fn render_format(ctx: &RenderContext, format: Format) -> miette::Result<String> {
    render(ctx, format.template())
}

/// Render using a user-supplied template file.
pub fn render_custom(ctx: &RenderContext, path: &Path) -> miette::Result<String> {
    let template = fs::read_to_string(path)
        .into_diagnostic()
        .with_context(|| format!("failed to read template {}", path.display()))?;
    render(ctx, &template)
}

/// Escape a value for safe inclusion in a single-quoted shell string.
///
/// Embedded single quotes are replaced with `'\''` (end quote, escaped quote,
/// start quote).
pub(crate) fn shell_escape(value: &str) -> String {
    value.replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_meta() -> Meta {
        Meta {
            timestamp: "2025-01-01T00:00:00+00:00".to_owned(),
            invocation: "envoke render local".to_owned(),
            invocation_args: vec!["envoke".to_owned(), "render".to_owned(), "local".to_owned()],
            environment: "local".to_owned(),
            config_file: "envoke.yaml".to_owned(),
            tags: vec![],
            overrides: vec![],
        }
    }

    #[test]
    fn test_render_shell_basic() {
        let ctx = RenderContext {
            resolved: vec![Resolved {
                name: "FOO".to_owned(),
                value: "bar".to_owned(),
                description: None,
            }],
            meta: test_meta(),
        };
        let output = render_format(&ctx, Format::Shell).unwrap();
        assert!(output.contains("FOO='bar'"));
        assert!(output.contains("@generated"));
        assert!(!output.contains("export"));
    }

    #[test]
    fn test_render_shell_export_basic() {
        let ctx = RenderContext {
            resolved: vec![Resolved {
                name: "FOO".to_owned(),
                value: "bar".to_owned(),
                description: None,
            }],
            meta: test_meta(),
        };
        let output = render_format(&ctx, Format::ShellExport).unwrap();
        assert!(output.contains("export FOO='bar'"));
    }

    #[test]
    fn test_render_with_description() {
        let ctx = RenderContext {
            resolved: vec![Resolved {
                name: "DB".to_owned(),
                value: "localhost".to_owned(),
                description: Some("Database host".to_owned()),
            }],
            meta: test_meta(),
        };
        let output = render_format(&ctx, Format::Shell).unwrap();
        assert!(output.contains("# Database host\n"));
        assert!(output.contains("DB='localhost'"));
    }

    #[test]
    fn test_render_shell_escape() {
        let ctx = RenderContext {
            resolved: vec![Resolved {
                name: "VAL".to_owned(),
                value: "it's a test".to_owned(),
                description: None,
            }],
            meta: test_meta(),
        };
        let output = render_format(&ctx, Format::Shell).unwrap();
        assert!(output.contains("VAL='it'\\''s a test'"));
    }

    #[test]
    fn test_render_custom_template() {
        let ctx = RenderContext {
            resolved: vec![
                Resolved {
                    name: "A".to_owned(),
                    value: "1".to_owned(),
                    description: None,
                },
                Resolved {
                    name: "B".to_owned(),
                    value: "2".to_owned(),
                    description: None,
                },
            ],
            meta: test_meta(),
        };
        let template =
            "{% for name, var in variables | items %}{{ name }}={{ var.value }}\n{% endfor %}";
        let output = render(&ctx, template).unwrap();
        assert_eq!(output, "A=1\nB=2\n");
    }

    #[test]
    fn test_render_v_shorthand() {
        let ctx = RenderContext {
            resolved: vec![Resolved {
                name: "DB_URL".to_owned(),
                value: "postgres://localhost".to_owned(),
                description: None,
            }],
            meta: test_meta(),
        };
        let template = "url={{ v.DB_URL }}";
        let output = render(&ctx, template).unwrap();
        assert_eq!(output, "url=postgres://localhost");
    }

    #[test]
    fn test_render_meta_fields() {
        let ctx = RenderContext {
            resolved: vec![],
            meta: test_meta(),
        };
        let template = "env={{ meta.environment }} file={{ meta.config_file }}";
        let output = render(&ctx, template).unwrap();
        assert_eq!(output, "env=local file=envoke.yaml");
    }

    #[test]
    fn test_render_meta_invocation_args() {
        let ctx = RenderContext {
            resolved: vec![],
            meta: test_meta(),
        };
        let template = "{% for arg in meta.invocation_args %}[{{ arg }}]{% endfor %}";
        let output = render(&ctx, template).unwrap();
        assert_eq!(output, "[envoke][render][local]");
    }

    #[test]
    fn test_shell_template_matches_snapshot() {
        let ctx = RenderContext {
            resolved: vec![
                Resolved {
                    name: "A_VAR".to_owned(),
                    value: "hello".to_owned(),
                    description: Some("A description".to_owned()),
                },
                Resolved {
                    name: "B_VAR".to_owned(),
                    value: "world".to_owned(),
                    description: None,
                },
            ],
            meta: test_meta(),
        };
        let output = render_format(&ctx, Format::Shell).unwrap();
        let expected = "\
# @generated by `envoke render local` at 2025-01-01T00:00:00+00:00
# Do not edit manually. Modify envoke.yaml instead.

# A description
A_VAR='hello'
B_VAR='world'
";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_shell_export_template_matches_snapshot() {
        let ctx = RenderContext {
            resolved: vec![
                Resolved {
                    name: "A_VAR".to_owned(),
                    value: "hello".to_owned(),
                    description: Some("A description".to_owned()),
                },
                Resolved {
                    name: "B_VAR".to_owned(),
                    value: "world".to_owned(),
                    description: None,
                },
            ],
            meta: test_meta(),
        };
        let output = render_format(&ctx, Format::ShellExport).unwrap();
        let expected = "\
# @generated by `envoke render local` at 2025-01-01T00:00:00+00:00
# Do not edit manually. Modify envoke.yaml instead.

# A description
export A_VAR='hello'
export B_VAR='world'
";
        assert_eq!(output, expected);
    }

    /// Fixture used by the format-preset tests. `B` embeds a double-quote and
    /// a newline to exercise escaping across json, yaml, dotenv, tfvars, and
    /// k8s-secret in a single pass.
    fn format_fixture() -> RenderContext {
        RenderContext {
            resolved: vec![
                Resolved {
                    name: "A".to_owned(),
                    value: "hello".to_owned(),
                    description: Some("plain ascii".to_owned()),
                },
                Resolved {
                    name: "B".to_owned(),
                    value: "it\"s\nmultiline".to_owned(),
                    description: None,
                },
            ],
            meta: test_meta(),
        }
    }

    #[test]
    fn test_format_json_round_trips() {
        let output = render_format(&format_fixture(), Format::Json).unwrap();
        let parsed: BTreeMap<String, String> =
            serde_json::from_str(output.trim_end()).expect("json output should parse");
        assert_eq!(parsed.get("A").map(String::as_str), Some("hello"));
        assert_eq!(
            parsed.get("B").map(String::as_str),
            Some("it\"s\nmultiline")
        );
        // JSON must not carry a `# @generated` header.
        assert!(!output.contains('#'));
    }

    #[test]
    fn test_format_yaml_round_trips() {
        let output = render_format(&format_fixture(), Format::Yaml).unwrap();
        let parsed: BTreeMap<String, String> =
            serde_yml::from_str(&output).expect("yaml output should parse");
        assert_eq!(parsed.get("A").map(String::as_str), Some("hello"));
        assert_eq!(
            parsed.get("B").map(String::as_str),
            Some("it\"s\nmultiline")
        );
        assert!(output.contains("# plain ascii"));
        assert!(output.contains("@generated"));
    }

    #[test]
    fn test_format_dotenv_escapes() {
        let output = render_format(&format_fixture(), Format::Dotenv).unwrap();
        assert!(output.contains("A=\"hello\""));
        // tojson escapes the double-quote as \" and the newline as \n.
        assert!(output.contains(r#"B="it\"s\nmultiline""#));
        assert!(output.contains("# plain ascii"));
    }

    #[test]
    fn test_format_tfvars_escapes() {
        let output = render_format(&format_fixture(), Format::TerraformTfvars).unwrap();
        assert!(output.contains("A = \"hello\""));
        assert!(output.contains(r#"B = "it\"s\nmultiline""#));
        assert!(output.contains("# plain ascii"));
    }

    #[test]
    fn test_format_k8s_secret_parses() {
        let output = render_format(&format_fixture(), Format::K8sSecret).unwrap();
        let parsed: serde_yml::Value =
            serde_yml::from_str(&output).expect("k8s-secret output should parse");
        assert_eq!(parsed["kind"].as_str(), Some("Secret"));
        assert_eq!(parsed["apiVersion"].as_str(), Some("v1"));
        assert_eq!(parsed["metadata"]["name"].as_str(), Some("envoke-local"));
        assert_eq!(parsed["stringData"]["A"].as_str(), Some("hello"));
        assert_eq!(parsed["stringData"]["B"].as_str(), Some("it\"s\nmultiline"));
    }

    #[test]
    fn test_format_k8s_secret_name_normalizes_env() {
        let mut ctx = format_fixture();
        ctx.meta.environment = "Prod_EU".to_owned();
        let output = render_format(&ctx, Format::K8sSecret).unwrap();
        let parsed: serde_yml::Value =
            serde_yml::from_str(&output).expect("k8s-secret output should parse");
        assert_eq!(parsed["metadata"]["name"].as_str(), Some("envoke-prod-eu"));
    }

    #[test]
    fn test_format_github_actions_heredoc_shape() {
        let output = render_format(&format_fixture(), Format::GithubActions).unwrap();
        // No header — $GITHUB_ENV rejects comments.
        assert!(!output.starts_with('#'));
        // Delimiter is timestamp-derived; the fixture timestamp is
        // "2025-01-01T00:00:00+00:00", so after stripping -, :, +, . we get
        // "20250101T0000000000" appended.
        assert!(
            output.contains("ENVOKE_EOF_20250101T0000000000"),
            "expected timestamp-derived delimiter, got: {output}"
        );
        // Each variable appears as a heredoc block.
        assert!(output.contains("A<<ENVOKE_EOF_"));
        assert!(output.contains("B<<ENVOKE_EOF_"));
        // The multiline value is preserved verbatim inside the heredoc.
        assert!(output.contains("it\"s\nmultiline"));
    }
}
