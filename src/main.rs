use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use tracing_subscriber::EnvFilter;

mod config;
mod error;
mod render;
mod resolve;

#[derive(Parser)]
#[command(about = "Resolve environment variables from envoke.yaml", version)]
#[allow(clippy::struct_excessive_bools)]
struct Cli {
    /// Target environment (e.g. local, prod). Not required with --schema or
    /// --list-* flags.
    #[arg(required_unless_present_any = ["schema", "list_environments", "list_overrides", "list_tags"])]
    environment: Option<String>,

    /// Write output to a file instead of stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Only include tagged variables with a matching tag. Repeatable.
    /// Untagged variables are always included.
    #[arg(short = 't', long = "tag")]
    tags: Vec<String>,

    /// Select named overrides for source selection. Repeatable.
    /// Per variable, at most one active override may be defined.
    #[arg(short = 'O', long = "override")]
    overrides: Vec<String>,

    /// Prefix each line with `export`. Ignored when `--template` is used.
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

Available filters:

  shell_escape  Escapes single quotes for shell safety

Note: the urlencode filter is only available in resolution templates
(the `template` source type), not in output templates."
    )]
    template: Option<PathBuf>,

    /// Print the JSON Schema for envoke.yaml and exit.
    #[arg(long)]
    schema: bool,

    /// List all environment names found in the config and exit.
    #[arg(long)]
    list_environments: bool,

    /// List all override names found in the config and exit.
    #[arg(long)]
    list_overrides: bool,

    /// List all tag names found in the config and exit.
    #[arg(long)]
    list_tags: bool,

    /// Suppress informational messages on stderr.
    #[arg(short, long)]
    quiet: bool,
}

fn run() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    if cli.schema {
        let schema = schemars::schema_for!(config::Config);
        let json = serde_json::to_string_pretty(&schema).context("failed to serialize schema")?;
        if let Some(path) = &cli.output {
            fs::write(path, &json)
                .with_context(|| format!("failed to write {}", path.display()))?;
        } else {
            println!("{json}");
        }
        return Ok(());
    }

    if cli.list_environments || cli.list_overrides || cli.list_tags {
        let yaml = fs::read_to_string(&cli.config)
            .with_context(|| format!("failed to read {}", cli.config.display()))?;
        let config: config::Config = serde_yml::from_str(&yaml)
            .with_context(|| format!("failed to parse {}", cli.config.display()))?;

        if cli.list_environments {
            for name in config.environments() {
                println!("{name}");
            }
        }
        if cli.list_overrides {
            for name in config.override_names() {
                println!("{name}");
            }
        }
        if cli.list_tags {
            for name in config.tag_names() {
                println!("{name}");
            }
        }
        return Ok(());
    }

    // Default: generate
    {
        let tags = cli.tags;
        let overrides = cli.overrides;
        let environment = cli.environment.expect("required by clap");
        let output = cli.output;
        let prepend_export = cli.prepend_export;
        let template_path = cli.template;
        let quiet = cli.quiet;

        let yaml = fs::read_to_string(&cli.config)
            .with_context(|| format!("failed to read {}", cli.config.display()))?;
        let config: config::Config = serde_yml::from_str(&yaml)
            .with_context(|| format!("failed to parse {}", cli.config.display()))?;

        if !quiet {
            eprintln!("Generating environment variables for {environment}...");
        }

        let resolved =
            resolve::resolve_all(&config, &environment, &tags, &overrides).map_err(|errors| {
                for err in &errors {
                    eprintln!("error: {err}");
                }
                anyhow::anyhow!("{} variable(s) failed to resolve", errors.len())
            })?;

        let invocation_args: Vec<String> = std::env::args().collect();
        let ctx = render::RenderContext {
            resolved,
            meta: render::Meta {
                timestamp: chrono::Local::now().to_rfc3339(),
                invocation: invocation_args.join(" "),
                invocation_args,
                environment,
                config_file: cli.config.display().to_string(),
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
                .with_context(|| format!("failed to write {}", path.display()))?;
            if !quiet {
                eprintln!("Wrote to {}", path.display());
            }
        } else {
            print!("{content}");
        }

        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    run()
}
