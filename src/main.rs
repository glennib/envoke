use std::fs;
use std::path::PathBuf;

use clap::CommandFactory;
use clap::Parser;
use miette::Context;
use miette::IntoDiagnostic;
use tracing_subscriber::EnvFilter;

mod config;
mod error;
mod exec;
mod render;
mod resolve;

#[derive(Parser)]
/// Resolve environment variables from envoke.yaml and either print them, write
/// them to a file, or exec a command with them overlaid on the current process
/// environment.
#[command(
    version,
    after_help = "\
Examples:
  envoke prod                           Print resolved vars as NAME='value' lines
  envoke prod --prepend-export          Print `export NAME='value'` lines
  envoke prod --output .env             Write resolved vars to .env
  envoke prod -- psql                   Exec psql with resolved vars overlaid
  envoke prod -- sh -c 'echo $DB_URL'   Exec an inline script"
)]
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    /// Target environment (e.g. local, prod). Not required with --schema,
    /// --completions, or --list-* flags.
    #[arg(
        env = "ENVOKE_ENV",
        verbatim_doc_comment,
        required_unless_present_any = ["schema", "completions", "list_environments", "list_overrides", "list_tags", "list_everything"]
    )]
    environment: Option<String>,

    /// Write output to a file instead of stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Only include tagged variables with a matching tag. Repeatable.
    /// Untagged variables are always included.
    #[arg(short = 't', long = "tag", verbatim_doc_comment)]
    tags: Vec<String>,

    /// Include all tagged variables regardless of their tags.
    #[arg(long, conflicts_with = "tags")]
    all_tags: bool,

    /// Select named overrides for source selection. Repeatable.
    /// Per variable, at most one active override may be defined.
    #[arg(short = 'O', long = "override", verbatim_doc_comment)]
    overrides: Vec<String>,

    /// Prefix each line with `export`. Ignored when --template is used.
    #[arg(long)]
    prepend_export: bool,

    /// Path to config file.
    #[arg(short, long, default_value = "envoke.yaml")]
    config: PathBuf,

    /// Use a custom output template file instead of the built-in format.
    #[arg(
        long,
        long_help = "\
Use a custom output template file instead of the built-in format.
The template uses Jinja2 syntax (minijinja).

Template context:

  variables  Map of name -> {value, description}. Iterate with:
               {% for name, var in variables | items %}
             Access fields: {{ variables.DB_URL.value }}

  v          Flat map of name -> value string. Shorthand:
               {{ v.DB_URL }}

  meta       Invocation metadata:
               meta.timestamp        RFC 3339 timestamp
               meta.invocation       Full CLI invocation string
               meta.invocation_args  CLI args as a list
               meta.environment      Target environment name
               meta.config_file      Path to the config file
               meta.tags             Active --tag values as a list
               meta.overrides        Active --override values as a list

Available filters:

  Built-in (minijinja builtins):
    upper, lower, replace, trim, default, join, sort, length,
    first, last, reverse, title, capitalize, list, int, float,
    abs, round, batch, slice, indent, truncate, unique, map,
    select, reject, selectattr, rejectattr, tojson, and more.
    See https://docs.rs/minijinja/latest/minijinja/filters

  Additional filters:
    shell_escape  Escapes single quotes for shell safety
    urlencode     Percent-encodes special characters

All filters are available in both variable templates (the `template`
source type) and custom output templates.

Note: Variable template sources (the `template` source type in
envoke.yaml) also have access to a `meta` object:
  meta.environment      Target environment name
  meta.tags             Active --tag values as a list
  meta.overrides        Active --override values as a list
  meta.timestamp        RFC 3339 timestamp"
    )]
    template: Option<PathBuf>,

    /// Print the JSON Schema for envoke.yaml and exit.
    #[arg(long)]
    schema: bool,

    /// List all environment names found in the config and exit.
    #[arg(long, group = "list")]
    list_environments: bool,

    /// List all override names found in the config and exit.
    #[arg(long, group = "list")]
    list_overrides: bool,

    /// List all tag names found in the config and exit.
    #[arg(long, group = "list")]
    list_tags: bool,

    /// List all environments, overrides, and tags found in the config and exit.
    /// Each line is prefixed with the type (environment, override, tag).
    #[arg(long, group = "list", verbatim_doc_comment)]
    list_everything: bool,

    /// Generate shell completions for the given shell and exit.
    #[arg(long)]
    completions: Option<clap_complete::Shell>,

    /// Disable parallel resolution of command and shell sources.
    #[arg(long)]
    no_parallel: bool,

    /// Suppress informational messages on stderr.
    #[arg(short, long)]
    quiet: bool,

    /// Command to execute with resolved variables overlaid on the current
    /// environment. Everything after `--` is passed verbatim to the child.
    ///
    /// The child inherits envoke's process environment (PATH, HOME, ...); the
    /// resolved variables from envoke.yaml override any inherited values with
    /// the same name. On Unix the child replaces envoke's process image (via
    /// `execvp`), so PID, TTY, and signals carry over directly. On other
    /// platforms envoke spawns the child and forwards its exit code.
    #[arg(
        last = true,
        allow_hyphen_values = true,
        value_name = "COMMAND",
        num_args = 1..,
        verbatim_doc_comment,
        conflicts_with_all = [
            "output",
            "template",
            "prepend_export",
            "schema",
            "completions",
            "list_environments",
            "list_overrides",
            "list_tags",
            "list_everything",
        ],
    )]
    command: Vec<String>,
}

fn run() -> miette::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    if cli.schema {
        let schema = schemars::schema_for!(config::Config);
        let json = serde_json::to_string_pretty(&schema)
            .into_diagnostic()
            .context("failed to serialize schema")?;
        if let Some(path) = &cli.output {
            fs::write(path, &json)
                .into_diagnostic()
                .with_context(|| format!("failed to write {}", path.display()))?;
        } else {
            println!("{json}");
        }
        return Ok(());
    }

    if let Some(shell) = cli.completions {
        clap_complete::generate(shell, &mut Cli::command(), "envoke", &mut std::io::stdout());
        return Ok(());
    }

    if cli.list_environments || cli.list_overrides || cli.list_tags || cli.list_everything {
        let yaml = fs::read_to_string(&cli.config)
            .into_diagnostic()
            .with_context(|| format!("failed to read {}", cli.config.display()))?;
        let config: config::Config = serde_yml::from_str(&yaml)
            .into_diagnostic()
            .with_context(|| format!("failed to parse {}", cli.config.display()))?;

        if cli.list_environments {
            for name in config.environments() {
                println!("{name}");
            }
        } else if cli.list_overrides {
            for name in config.override_names() {
                println!("{name}");
            }
        } else if cli.list_tags {
            for name in config.tag_names() {
                println!("{name}");
            }
        } else if cli.list_everything {
            for name in config.environments() {
                println!("environment:{name}");
            }
            for name in config.override_names() {
                println!("override:{name}");
            }
            for name in config.tag_names() {
                println!("tag:{name}");
            }
        }
        return Ok(());
    }

    let tags = cli.tags;
    let all_tags = cli.all_tags;
    let overrides = cli.overrides;
    let environment = cli.environment.expect("required by clap");
    let output = cli.output;
    let prepend_export = cli.prepend_export;
    let template_path = cli.template;
    let quiet = cli.quiet;
    let command = cli.command;

    let yaml = fs::read_to_string(&cli.config)
        .into_diagnostic()
        .with_context(|| format!("failed to read {}", cli.config.display()))?;
    let config: config::Config = serde_yml::from_str(&yaml)
        .into_diagnostic()
        .with_context(|| format!("failed to parse {}", cli.config.display()))?;

    let tags = if all_tags { config.tag_names() } else { tags };

    // Exec path owns stdout/stderr after handoff; stay silent.
    if !quiet && command.is_empty() {
        eprintln!("Generating environment variables for {environment}...");
    }

    let timestamp = chrono::Local::now().to_rfc3339();

    let parallel = !cli.no_parallel;
    let resolved = resolve::resolve_all(
        &config,
        &environment,
        &tags,
        &overrides,
        &timestamp,
        parallel,
    )
    .map_err(|errors| error::ResolveErrors { errors })?;

    if !command.is_empty() {
        return exec::exec_command(&command, &resolved);
    }

    let invocation_args: Vec<String> = std::env::args().collect();
    let ctx = render::RenderContext {
        resolved,
        meta: render::Meta {
            timestamp,
            invocation: invocation_args.join(" "),
            invocation_args,
            environment,
            config_file: cli.config.display().to_string(),
            tags,
            overrides,
        },
    };

    let content = if let Some(path) = &template_path {
        render::render_custom(&ctx, path)?
    } else if prepend_export {
        render::render_default_export(&ctx)?
    } else {
        render::render_default(&ctx)?
    };

    if let Some(path) = &output {
        fs::write(path, &content)
            .into_diagnostic()
            .with_context(|| format!("failed to write {}", path.display()))?;
        if !quiet {
            eprintln!("Wrote to {}", path.display());
        }
    } else {
        print!("{content}");
    }

    Ok(())
}

fn main() -> miette::Result<()> {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(2)
                .build(),
        )
    }))
    .expect("miette hook should only be set once");
    run()
}

#[cfg(test)]
mod cli_tests {
    use clap::Parser;

    use super::Cli;

    #[test]
    fn no_command_parses_as_normal_invocation() {
        let cli = Cli::try_parse_from(["envoke", "prod"]).unwrap();
        assert_eq!(cli.environment.as_deref(), Some("prod"));
        assert!(cli.command.is_empty());
    }

    #[test]
    fn command_after_double_dash_is_collected() {
        let cli = Cli::try_parse_from(["envoke", "prod", "--", "psql"]).unwrap();
        assert_eq!(cli.environment.as_deref(), Some("prod"));
        assert_eq!(cli.command, vec!["psql".to_owned()]);
    }

    #[test]
    fn command_args_with_hyphens_pass_through() {
        let cli =
            Cli::try_parse_from(["envoke", "prod", "--", "psql", "--host=db", "-U", "me"]).unwrap();
        assert_eq!(
            cli.command,
            vec![
                "psql".to_owned(),
                "--host=db".to_owned(),
                "-U".to_owned(),
                "me".to_owned(),
            ]
        );
    }

    #[test]
    fn positional_without_double_dash_errors() {
        // `envoke prod psql` should fail — the command must come after `--`.
        assert!(Cli::try_parse_from(["envoke", "prod", "psql"]).is_err());
    }

    #[test]
    fn command_conflicts_with_output() {
        assert!(Cli::try_parse_from(["envoke", "prod", "--output", "x", "--", "psql"]).is_err());
    }

    #[test]
    fn command_conflicts_with_template() {
        assert!(
            Cli::try_parse_from(["envoke", "prod", "--template", "t.j2", "--", "psql"]).is_err()
        );
    }

    #[test]
    fn command_conflicts_with_prepend_export() {
        assert!(Cli::try_parse_from(["envoke", "prod", "--prepend-export", "--", "psql"]).is_err());
    }

    #[test]
    fn command_conflicts_with_list_flags() {
        assert!(Cli::try_parse_from(["envoke", "--list-environments", "--", "psql"]).is_err());
    }

    #[test]
    fn command_is_compatible_with_tags_and_overrides() {
        let cli = Cli::try_parse_from([
            "envoke",
            "prod",
            "--tag",
            "vault",
            "--override",
            "read-replica",
            "--",
            "psql",
        ])
        .unwrap();
        assert_eq!(cli.tags, vec!["vault".to_owned()]);
        assert_eq!(cli.overrides, vec!["read-replica".to_owned()]);
        assert_eq!(cli.command, vec!["psql".to_owned()]);
    }
}
