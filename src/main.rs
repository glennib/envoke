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
struct Cli {
    /// Target environment (e.g. local, prod).
    #[arg(required_unless_present = "schema")]
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

    /// Switches to a built-in template that prefixes each variable with `export
    /// `. Ignored when `--template` is used.
    #[arg(long)]
    prepend_export: bool,

    /// Path to config file.
    #[arg(short, long, default_value = "envoke.yaml")]
    config: PathBuf,

    /// Use a custom output template file instead of the built-in format.
    /// The template uses Jinja2 syntax (minijinja) and has access to
    /// `variables`, `v`, and `meta` context objects. See README for details.
    #[arg(long)]
    template: Option<PathBuf>,

    /// Print the JSON Schema for envoke.yaml and exit.
    #[arg(long)]
    schema: bool,

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
        println!("{json}");
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
        let config: config::Config = serde_yaml::from_str(&yaml)
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
