use std::fs;
use std::path::Path;
use std::path::PathBuf;

use clap::Args;
use clap::CommandFactory;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
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
  envoke render prod                          Print resolved vars as a .env file (dotenv is the default)
  envoke r prod --format json                 Print resolved vars as a JSON object (r = render)
  envoke render prod --format shell-export    Print `export NAME='value'` lines
  envoke render prod --output .env            Write resolved vars to .env
  envoke exec prod -- psql                    Exec psql with resolved vars overlaid
  envoke x prod -- sh -c 'echo $DB_URL'       Exec an inline script (x = exec)
  envoke meta environments                    Enumerate environment names from the config
  envoke meta all                             Enumerate environments, tags, and overrides
  envoke schema                               Print JSON Schema for envoke.yaml
  envoke completions zsh                      Print shell completions",
    verbatim_doc_comment
)]
struct Cli {
    /// Path to config file.
    #[arg(short, long, default_value = "envoke.yaml", global = true)]
    config: PathBuf,

    /// Suppress informational messages on stderr.
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Disable parallel resolution of command and shell sources.
    #[arg(long, global = true)]
    no_parallel: bool,

    /// Only include tagged variables with a matching tag. Repeatable.
    /// Untagged variables are always included.
    #[arg(short = 't', long = "tag", global = true, verbatim_doc_comment)]
    tags: Vec<String>,

    /// Include all tagged variables regardless of their tags.
    #[arg(long, global = true, conflicts_with = "tags")]
    all_tags: bool,

    /// Select named overrides for source selection. Repeatable.
    /// Per variable, at most one active override may be defined.
    #[arg(short = 'O', long = "override", global = true, verbatim_doc_comment)]
    overrides: Vec<String>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Resolve variables and print or write them.
    #[command(alias = "r")]
    Render(RenderArgs),

    /// Resolve variables and exec a command with them overlaid on the current
    /// process environment.
    #[command(alias = "x")]
    Exec(ExecArgs),

    /// Enumerate names of a config dimension (environments, tags, overrides).
    Meta(MetaArgs),

    /// Print the JSON Schema for envoke.yaml and exit.
    Schema,

    /// Generate shell completions for the given shell and exit.
    Completions(CompletionsArgs),
}

#[derive(Args)]
struct RenderArgs {
    /// Target environment (e.g. local, prod).
    #[arg(env = "ENVOKE_ENV")]
    env: String,

    /// Write output to a file instead of stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Select a built-in output format preset.
    #[arg(
        short = 'f',
        long,
        value_enum,
        conflicts_with = "template",
        long_help = "\
Select a built-in output format preset.

Presets:
  dotenv            .env syntax (the default): KEY='value' when the
                    value contains no apostrophe or newline, else
                    KEY=\"value\" with conservative escapes. Safe to
                    feed into dotenvy, godotenv, python-dotenv, and
                    similar parsers; `$` never expands.
  shell-export      POSIX shell lines with `export` prefix:
                    `export KEY='value'`.
  json              Compact JSON object (pipe through `jq .` for
                    pretty output).
  yaml              YAML mapping in block style (KEY: \"value\").
  k8s-secret        Kubernetes Secret manifest with stringData.
  github-actions    Heredoc format for >> \"$GITHUB_ENV\" in a
                    GitHub Actions step.
  terraform-tfvars  Terraform *.tfvars format: KEY = \"value\".

Notes:
  - `--format` conflicts with `--template`.
  - `--format json` output is also valid YAML 1.2 if you prefer
    the compact form.
  - Some dotenv dialects (e.g. dotenvx) expand $VAR inside
    double-quoted values, so a value like `pa$word` may not
    round-trip through those parsers.
  - For fully custom output, use `--template <file.j2>` instead."
    )]
    format: Option<render::Format>,

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
}

#[derive(Args)]
struct ExecArgs {
    /// Target environment (e.g. local, prod).
    #[arg(env = "ENVOKE_ENV")]
    env: String,

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
        required = true,
        verbatim_doc_comment,
    )]
    command: Vec<String>,
}

#[derive(Args)]
struct MetaArgs {
    /// Which config dimension to enumerate.
    target: MetaTarget,
}

#[derive(Copy, Clone, ValueEnum)]
enum MetaTarget {
    /// Environment names found across all `envs` maps.
    Environments,
    /// Tag names found across all variables.
    Tags,
    /// Override names found across all variables.
    Overrides,
    /// All three (each line prefixed with `environment:`, `tag:`, `override:`).
    All,
}

#[derive(Args)]
struct CompletionsArgs {
    /// Shell to generate completions for.
    shell: clap_complete::Shell,
}

fn run() -> miette::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Schema => {
            let schema = schemars::schema_for!(config::Config);
            let json = serde_json::to_string_pretty(&schema)
                .into_diagnostic()
                .context("failed to serialize schema")?;
            println!("{json}");
            Ok(())
        }
        Cmd::Completions(args) => {
            clap_complete::generate(
                args.shell,
                &mut Cli::command(),
                "envoke",
                &mut std::io::stdout(),
            );
            Ok(())
        }
        Cmd::Meta(args) => cmd_meta(&cli.config, args.target),
        Cmd::Render(args) => cmd_render(
            args,
            &cli.config,
            cli.quiet,
            cli.no_parallel,
            cli.tags,
            cli.all_tags,
            cli.overrides,
        ),
        Cmd::Exec(args) => cmd_exec(
            args,
            &cli.config,
            cli.no_parallel,
            cli.tags,
            cli.all_tags,
            cli.overrides,
        ),
    }
}

fn load_config(config_path: &Path) -> miette::Result<config::Config> {
    let yaml = fs::read_to_string(config_path)
        .into_diagnostic()
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    serde_yml::from_str(&yaml)
        .into_diagnostic()
        .with_context(|| format!("failed to parse {}", config_path.display()))
}

fn cmd_meta(config_path: &Path, target: MetaTarget) -> miette::Result<()> {
    let config = load_config(config_path)?;

    match target {
        MetaTarget::Environments => {
            for name in config.environments() {
                println!("{name}");
            }
        }
        MetaTarget::Tags => {
            for name in config.tag_names() {
                println!("{name}");
            }
        }
        MetaTarget::Overrides => {
            for name in config.override_names() {
                println!("{name}");
            }
        }
        MetaTarget::All => {
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
    }
    Ok(())
}

struct Resolution {
    resolved: Vec<resolve::Resolved>,
    tags: Vec<String>,
    overrides: Vec<String>,
    timestamp: String,
}

fn resolve_for(
    config: &config::Config,
    environment: &str,
    tags: Vec<String>,
    all_tags: bool,
    overrides: Vec<String>,
    no_parallel: bool,
) -> miette::Result<Resolution> {
    let tags = if all_tags { config.tag_names() } else { tags };
    let timestamp = chrono::Local::now().to_rfc3339();
    let parallel = !no_parallel;
    let resolved =
        resolve::resolve_all(config, environment, &tags, &overrides, &timestamp, parallel)
            .map_err(|errors| error::ResolveErrors { errors })?;

    Ok(Resolution {
        resolved,
        tags,
        overrides,
        timestamp,
    })
}

fn cmd_render(
    args: RenderArgs,
    config_path: &Path,
    quiet: bool,
    no_parallel: bool,
    tags: Vec<String>,
    all_tags: bool,
    overrides: Vec<String>,
) -> miette::Result<()> {
    let environment = args.env;
    if !quiet {
        eprintln!("Generating environment variables for {environment}...");
    }

    let config = load_config(config_path)?;
    let res = resolve_for(
        &config,
        &environment,
        tags,
        all_tags,
        overrides,
        no_parallel,
    )?;

    let invocation_args: Vec<String> = std::env::args().collect();
    let ctx = render::RenderContext {
        resolved: res.resolved,
        meta: render::Meta {
            timestamp: res.timestamp,
            invocation: invocation_args.join(" "),
            invocation_args,
            environment,
            config_file: config_path.display().to_string(),
            tags: res.tags,
            overrides: res.overrides,
        },
    };

    let content = if let Some(path) = &args.template {
        render::render_custom(&ctx, path)?
    } else if let Some(format) = args.format {
        render::render_format(&ctx, format)?
    } else {
        render::render_format(&ctx, render::Format::Dotenv)?
    };

    if let Some(path) = &args.output {
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

fn cmd_exec(
    args: ExecArgs,
    config_path: &Path,
    no_parallel: bool,
    tags: Vec<String>,
    all_tags: bool,
    overrides: Vec<String>,
) -> miette::Result<()> {
    let ExecArgs { env, command } = args;
    let config = load_config(config_path)?;
    let res = resolve_for(&config, &env, tags, all_tags, overrides, no_parallel)?;
    exec::exec_command(&command, &res.resolved)
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
    use super::Cmd;
    use super::MetaTarget;
    use super::render::Format;

    #[test]
    fn format_json_parses() {
        let cli = Cli::try_parse_from(["envoke", "r", "prod", "--format", "json"]).unwrap();
        let Cmd::Render(args) = cli.cmd else {
            panic!("expected Render subcommand");
        };
        assert!(matches!(args.format, Some(Format::Json)));
    }

    #[test]
    fn format_k8s_secret_parses() {
        // Canary for heck's kebab-case conversion over the `K` + digit
        // boundary. If this fails, add `#[value(name = "k8s-secret")]` to
        // the variant in render.rs.
        let cli = Cli::try_parse_from(["envoke", "r", "prod", "--format", "k8s-secret"]).unwrap();
        let Cmd::Render(args) = cli.cmd else {
            panic!("expected Render subcommand");
        };
        assert!(matches!(args.format, Some(Format::K8sSecret)));
    }

    #[test]
    fn format_github_actions_parses() {
        let cli =
            Cli::try_parse_from(["envoke", "r", "prod", "--format", "github-actions"]).unwrap();
        let Cmd::Render(args) = cli.cmd else {
            panic!("expected Render subcommand");
        };
        assert!(matches!(args.format, Some(Format::GithubActions)));
    }

    #[test]
    fn format_terraform_tfvars_parses() {
        let cli =
            Cli::try_parse_from(["envoke", "r", "prod", "--format", "terraform-tfvars"]).unwrap();
        let Cmd::Render(args) = cli.cmd else {
            panic!("expected Render subcommand");
        };
        assert!(matches!(args.format, Some(Format::TerraformTfvars)));
    }

    #[test]
    fn format_conflicts_with_template() {
        assert!(
            Cli::try_parse_from([
                "envoke",
                "r",
                "prod",
                "--format",
                "json",
                "--template",
                "t.j2",
            ])
            .is_err()
        );
    }

    #[test]
    fn format_and_output_coexist() {
        let cli = Cli::try_parse_from([
            "envoke",
            "r",
            "prod",
            "--format",
            "json",
            "--output",
            "/tmp/x.json",
        ])
        .unwrap();
        let Cmd::Render(args) = cli.cmd else {
            panic!("expected Render subcommand");
        };
        assert!(matches!(args.format, Some(Format::Json)));
        assert_eq!(
            args.output.as_deref().and_then(|p| p.to_str()),
            Some("/tmp/x.json")
        );
    }

    #[test]
    fn exec_rejects_render_only_format_flag() {
        // --format is a render-only flag; under `exec` it should not parse.
        assert!(
            Cli::try_parse_from(["envoke", "exec", "prod", "--format", "json", "--", "psql"])
                .is_err()
        );
    }

    #[test]
    fn exec_rejects_render_only_output_flag() {
        assert!(
            Cli::try_parse_from(["envoke", "exec", "prod", "--output", "x", "--", "psql"]).is_err()
        );
    }

    #[test]
    fn exec_rejects_render_only_template_flag() {
        assert!(
            Cli::try_parse_from(["envoke", "exec", "prod", "--template", "t.j2", "--", "psql",])
                .is_err()
        );
    }

    #[test]
    fn bare_envoke_env_errors() {
        // v1's `envoke prod` shorthand is gone; now it's an unknown subcommand.
        assert!(Cli::try_parse_from(["envoke", "prod"]).is_err());
    }

    #[test]
    fn command_after_double_dash_is_collected() {
        let cli = Cli::try_parse_from(["envoke", "exec", "prod", "--", "psql"]).unwrap();
        let Cmd::Exec(args) = cli.cmd else {
            panic!("expected Exec subcommand");
        };
        assert_eq!(args.env, "prod");
        assert_eq!(args.command, vec!["psql".to_owned()]);
    }

    #[test]
    fn command_args_with_hyphens_pass_through() {
        let cli = Cli::try_parse_from([
            "envoke",
            "exec",
            "prod",
            "--",
            "psql",
            "--host=db",
            "-U",
            "me",
        ])
        .unwrap();
        let Cmd::Exec(args) = cli.cmd else {
            panic!("expected Exec subcommand");
        };
        assert_eq!(
            args.command,
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
        // `envoke exec prod psql` should fail — the command must come after `--`.
        assert!(Cli::try_parse_from(["envoke", "exec", "prod", "psql"]).is_err());
    }

    #[test]
    fn meta_has_no_trailing_command() {
        // `meta` does not collect a trailing command — `--` + extra args errors.
        assert!(Cli::try_parse_from(["envoke", "meta", "environments", "--", "psql"]).is_err());
    }

    #[test]
    fn command_is_compatible_with_tags_and_overrides() {
        let cli = Cli::try_parse_from([
            "envoke",
            "exec",
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
        let Cmd::Exec(args) = cli.cmd else {
            panic!("expected Exec subcommand");
        };
        assert_eq!(args.command, vec!["psql".to_owned()]);
    }

    #[test]
    fn render_alias_r_works() {
        let cli = Cli::try_parse_from(["envoke", "r", "prod"]).unwrap();
        let Cmd::Render(args) = cli.cmd else {
            panic!("expected Render subcommand");
        };
        assert_eq!(args.env, "prod");
    }

    #[test]
    fn exec_alias_x_works() {
        let cli = Cli::try_parse_from(["envoke", "x", "prod", "--", "psql"]).unwrap();
        let Cmd::Exec(args) = cli.cmd else {
            panic!("expected Exec subcommand");
        };
        assert_eq!(args.env, "prod");
        assert_eq!(args.command, vec!["psql".to_owned()]);
    }

    #[test]
    fn meta_target_parses() {
        let cli = Cli::try_parse_from(["envoke", "meta", "tags"]).unwrap();
        let Cmd::Meta(args) = cli.cmd else {
            panic!("expected Meta subcommand");
        };
        assert!(matches!(args.target, MetaTarget::Tags));
    }

    #[test]
    fn meta_all_variant_parses() {
        let cli = Cli::try_parse_from(["envoke", "meta", "all"]).unwrap();
        let Cmd::Meta(args) = cli.cmd else {
            panic!("expected Meta subcommand");
        };
        assert!(matches!(args.target, MetaTarget::All));
    }

    #[test]
    fn global_tag_before_subcommand() {
        let cli = Cli::try_parse_from(["envoke", "--tag", "vault", "r", "prod"]).unwrap();
        assert_eq!(cli.tags, vec!["vault".to_owned()]);
    }

    #[test]
    fn global_tag_after_subcommand() {
        let cli = Cli::try_parse_from(["envoke", "r", "prod", "--tag", "vault"]).unwrap();
        assert_eq!(cli.tags, vec!["vault".to_owned()]);
    }

    #[test]
    fn global_tag_replace_semantics_across_boundary() {
        // With `global = true` on a `Vec<String>`, clap collects the
        // subcommand-level occurrences as a fresh ArgMatches value, replacing
        // (not appending to) the root-level occurrences. Documented so the
        // behavior doesn't drift silently.
        let cli = Cli::try_parse_from(["envoke", "--tag", "a", "r", "prod", "--tag", "b"]).unwrap();
        assert_eq!(cli.tags, vec!["b".to_owned()]);
    }

    #[test]
    fn envoke_env_fills_positional() {
        // With ENVOKE_ENV set, the env positional becomes optional.
        // SAFETY: test sets and unsets a process-wide env var; nextest runs
        // each test in its own process, so this is isolated.
        unsafe {
            std::env::set_var("ENVOKE_ENV", "prod");
        }
        let cli = Cli::try_parse_from(["envoke", "r"]).unwrap();
        unsafe {
            std::env::remove_var("ENVOKE_ENV");
        }
        let Cmd::Render(args) = cli.cmd else {
            panic!("expected Render subcommand");
        };
        assert_eq!(args.env, "prod");
    }
}
