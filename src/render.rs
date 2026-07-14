use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use miette::Context;
use miette::IntoDiagnostic;

use crate::resolve::Resolved;

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
    /// `.env` syntax (the default): `'value'` when safe, else `"value"` with
    /// conservative escapes.
    Dotenv,
    /// POSIX shell with `export` prefix: `export KEY='value'`.
    ShellExport,
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
            Self::Dotenv => DOTENV_TEMPLATE,
            Self::ShellExport => SHELL_EXPORT_TEMPLATE,
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
    env.add_filter("dotenv_escape", dotenv_escape);
    env.add_filter("wrap", wrap);
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

/// Encode a value as a portable `.env` token, delimiters included.
///
/// Returns the fully-quoted form — do not wrap the output in additional
/// quotes when using this filter in templates.
///
/// Encoding rule:
/// - If the value contains no `'` and no newline, emit `'value'` (single-
///   quoted literal). Every byte passes through unchanged; `$` is never
///   expanded inside single quotes by any dotenv parser.
/// - Otherwise, emit `"value"` with the conservative escape set shared by
///   `dotenvy`, `godotenv`, `python-dotenv`, and similar parsers: `\\`, `\"`,
///   `\$`, and `\n` for newline. All other bytes (including literal tab and CR)
///   pass through as-is. `\t` and `\r` escape *sequences* are deliberately not
///   emitted because `dotenvy` rejects unknown escapes as parse errors.
pub(crate) fn dotenv_escape(value: &str) -> String {
    let needs_double_quote = value.contains('\'') || value.contains('\n');
    let mut out = String::with_capacity(value.len() + 2);
    if !needs_double_quote {
        out.push('\'');
        out.push_str(value);
        out.push('\'');
        return out;
    }
    out.push('"');
    for c in value.chars() {
        match c {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str(r#"\""#),
            '$' => out.push_str(r"\$"),
            '\n' => out.push_str(r"\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Wraps a string into lines of maximum `width` columns.
pub(crate) fn wrap(s: &str, width: usize) -> Vec<String> {
    textwrap::wrap(s, width)
        .into_iter()
        .map(|c| c.to_string())
        .collect()
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
        let output = render_format(&ctx, Format::ShellExport).unwrap();
        assert!(output.contains("# Database host\n"));
        assert!(output.contains("export DB='localhost'"));
    }

    #[test]
    fn test_render_shell_export_escape() {
        let ctx = RenderContext {
            resolved: vec![Resolved {
                name: "VAL".to_owned(),
                value: "it's a test".to_owned(),
                description: None,
            }],
            meta: test_meta(),
        };
        let output = render_format(&ctx, Format::ShellExport).unwrap();
        assert!(output.contains("export VAL='it'\\''s a test'"));
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
        insta::assert_snapshot!(output, @"
        # @generated by `envoke render local` at 2025-01-01T00:00:00+00:00
        # Do not edit manually. Modify envoke.yaml instead.

        # A description
        export A_VAR='hello'
        export B_VAR='world'
        ");
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
                Resolved {
                    name: "C".to_owned(),
                    value: "long description".to_owned(),
                    description: Some("Lorem ipsum dolor sit amet, consectetur adipiscing elit. Proin eget elementum libero, ut iaculis odio. Nulla vitae ante volutpat, tincidunt neque ut, sagittis arcu. Aenean sed arcu pretium purus sagittis.".to_owned()),
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
    fn test_format_dotenv_matches_snapshot() {
        // Full-output assertion on the matrix. The fixture's `B` value carries
        // an embedded `"` AND a newline — exercises the double-quoted fallback
        // with escapes. `A` is plain and stays in the single-quoted form.
        let output = render_format(&format_fixture(), Format::Dotenv).unwrap();
        insta::assert_snapshot!(output, @r#"
        # @generated by `envoke render local` at 2025-01-01T00:00:00+00:00
        # Do not edit manually. Modify envoke.yaml instead.

        # plain ascii
        A='hello'
        B="it\"s\nmultiline"
        # Lorem ipsum dolor sit amet, consectetur adipiscing elit. Proin eget elementum
        # libero, ut iaculis odio. Nulla vitae ante volutpat, tincidunt neque ut, sagittis
        # arcu. Aenean sed arcu pretium purus sagittis.
        C='long description'
        "#);
    }

    /// The full matrix of (input, encoded) pairs that the dotenv encoding
    /// must preserve. Shared between the filter-matrix test and the
    /// dotenvy round-trip test so both stay in lockstep.
    const DOTENV_MATRIX: &[(&str, &str)] = &[
        ("hello", "'hello'"),
        ("$HOME", "'$HOME'"),
        (r#"say "hi""#, r#"'say "hi"'"#),
        (r"C:\path", r"'C:\path'"),
        ("café", "'café'"),
        ("日本語", "'日本語'"),
        ("hello ", "'hello '"),
        ("", "''"),
        ("a\tb", "'a\tb'"),
        ("O'Brien", "\"O'Brien\""),
        ("a\nb", "\"a\\nb\""),
        ("$HOME O'Brien", "\"\\$HOME O'Brien\""),
        ("line1\n$X", "\"line1\\n\\$X\""),
        ("a\tb'c", "\"a\tb'c\""),
        (r"back\slash", r"'back\slash'"),
        ("back\\'slash", "\"back\\\\'slash\""),
    ];

    #[test]
    fn test_dotenv_escape_filter_matrix() {
        // Load-bearing properties pinned here:
        // - single-quoted form passes every byte through unchanged, including `$`, `"`,
        //   `\`, literal tab;
        // - double-quoted fallback is chosen *only* for `'` or newline;
        // - in the fallback, `$` is escaped (so parsers that expand it don't corrupt
        //   the value), `\r` and `\t` escape sequences are *not* emitted (dotenvy
        //   rejects unknown escapes — literal tab/CR pass through instead).
        for (input, expected) in DOTENV_MATRIX {
            assert_eq!(
                &dotenv_escape(input),
                expected,
                "dotenv_escape({input:?}) mismatch",
            );
        }
    }

    /// End-to-end pin: every encoding in the matrix must parse back to the
    /// original value when read by `dotenvy`. This is the real test that the
    /// chosen escape set (including literal tab/CR pass-through) doesn't
    /// trip dotenvy's parser and that `$` never round-trips as an expanded
    /// value.
    #[test]
    fn test_dotenv_round_trip_via_dotenvy() {
        for (input, encoded) in DOTENV_MATRIX {
            let line = format!("KEY={encoded}\n");
            let mut iter = dotenvy::Iter::new(line.as_bytes());
            let parsed = iter
                .next()
                .unwrap_or_else(|| panic!("no line parsed for {input:?}: {line:?}"))
                .unwrap_or_else(|e| panic!("dotenvy rejected {line:?}: {e}"));
            assert_eq!(parsed.0, "KEY");
            assert_eq!(
                parsed.1, *input,
                "round-trip mismatch for {input:?} via encoding {encoded:?}",
            );
        }
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

    #[test]
    fn test_wrap() {
        let context = RenderContext {
            resolved: vec![Resolved {
                name: "A".to_string(),
                value: "a".to_string(),
                description: Some("One two three four".to_string()),
            }],
            meta: test_meta(),
        };
        let template =
            "{% for line in variables.A.description | wrap(10) %}# {{ line }}\n{% endfor %}";
        let output = render(&context, template).unwrap();
        assert_eq!(output, "# One two\n# three four\n");
    }
}
