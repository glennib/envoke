use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::Context;

use crate::resolve::Resolved;

const DEFAULT_TEMPLATE: &str = include_str!("templates/default.sh.j2");
const DEFAULT_EXPORT_TEMPLATE: &str = include_str!("templates/default-export.sh.j2");

/// Metadata about the current invocation, exposed to templates as `meta`.
#[derive(serde::Serialize)]
pub struct Meta {
    /// RFC 3339 timestamp of when the invocation started.
    pub timestamp: String,
    /// Full CLI invocation as a single string (e.g. `"envoke local"`).
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
fn render(ctx: &RenderContext, template: &str) -> anyhow::Result<String> {
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
        .context("failed to parse output template")?;

    let tmpl = env.get_template("output").expect("template just added");
    let rendered = tmpl
        .render(minijinja::context! {
            variables => variables,
            v => v,
            meta => &ctx.meta,
        })
        .context("failed to render output template")?;

    Ok(rendered)
}

/// Render using the built-in default template (no `export` prefix).
pub fn render_default(ctx: &RenderContext) -> anyhow::Result<String> {
    render(ctx, DEFAULT_TEMPLATE)
}

/// Render using the built-in export template (`export` prefix).
pub fn render_default_export(ctx: &RenderContext) -> anyhow::Result<String> {
    render(ctx, DEFAULT_EXPORT_TEMPLATE)
}

/// Render using a user-supplied template file.
pub fn render_custom(ctx: &RenderContext, path: &Path) -> anyhow::Result<String> {
    let template = fs::read_to_string(path)
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
            invocation: "envoke local".to_owned(),
            invocation_args: vec!["envoke".to_owned(), "local".to_owned()],
            environment: "local".to_owned(),
            config_file: "envoke.yaml".to_owned(),
            tags: vec![],
            overrides: vec![],
        }
    }

    #[test]
    fn test_render_default_basic() {
        let ctx = RenderContext {
            resolved: vec![Resolved {
                name: "FOO".to_owned(),
                value: "bar".to_owned(),
                description: None,
            }],
            meta: test_meta(),
        };
        let output = render_default(&ctx).unwrap();
        assert!(output.contains("FOO='bar'"));
        assert!(output.contains("@generated"));
        assert!(!output.contains("export"));
    }

    #[test]
    fn test_render_default_export() {
        let ctx = RenderContext {
            resolved: vec![Resolved {
                name: "FOO".to_owned(),
                value: "bar".to_owned(),
                description: None,
            }],
            meta: test_meta(),
        };
        let output = render_default_export(&ctx).unwrap();
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
        let output = render_default(&ctx).unwrap();
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
        let output = render_default(&ctx).unwrap();
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
        assert_eq!(output, "[envoke][local]");
    }

    #[test]
    fn test_default_template_matches_old_output() {
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
        let output = render_default(&ctx).unwrap();
        let expected = "\
# @generated by `envoke local` at 2025-01-01T00:00:00+00:00
# Do not edit manually. Modify envoke.yaml instead.

# A description
A_VAR='hello'
B_VAR='world'
";
        assert_eq!(output, expected);
    }
}
