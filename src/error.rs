fn format_cycle(chain: &[String]) -> String {
    chain.join(" -> ")
}

fn format_override_names(names: &[String]) -> String {
    let quoted: Vec<String> = names.iter().map(|n| format!("'{n}'")).collect();
    let list = match quoted.as_slice() {
        [] => String::new(),
        [single] => single.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    if names.len() > 2 {
        format!("{list} all define sources for this variable")
    } else {
        format!("{list} both define sources for this variable")
    }
}

/// Errors that occur during variable resolution.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("{variable} [{environment}]: {kind}")]
#[diagnostic(forward(kind))]
pub struct ResolveError {
    pub variable: String,
    pub environment: String,
    pub kind: ResolveErrorKind,
}

/// The specific kind of resolution failure.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ResolveErrorKind {
    #[error("no configuration for this environment")]
    #[diagnostic(
        code(envoke::no_config),
        help("add a source for this environment in envoke.yaml, or add a `default` source")
    )]
    NoConfig,

    #[error("command `{command:?}` failed: {reason}")]
    #[diagnostic(
        code(envoke::cmd_failed),
        help("check that the command exists and is executable")
    )]
    CmdFailed {
        command: Vec<String>,
        reason: String,
    },

    #[error("command `{command:?}` exited with {exit_code:?}: {stderr}")]
    #[diagnostic(
        code(envoke::cmd_non_zero),
        help("check the command's stderr output above for details")
    )]
    CmdNonZero {
        command: Vec<String>,
        exit_code: Option<i32>,
        stderr: String,
    },

    #[error("circular dependency: {}", format_cycle(chain))]
    #[diagnostic(
        code(envoke::circular_dependency),
        help("break the cycle by removing or rewriting one of the template references")
    )]
    CircularDependency { chain: Vec<String> },

    #[error("unknown variable reference: {name}")]
    #[diagnostic(
        code(envoke::unknown_reference),
        help("check that the referenced variable is defined in envoke.yaml")
    )]
    UnknownReference { name: String },

    #[error("template error: {reason}")]
    #[diagnostic(
        code(envoke::template_render),
        help("check the template syntax — Jinja2/minijinja is used")
    )]
    TemplateRender { reason: String },

    #[error("invalid source: {reason}")]
    #[diagnostic(
        code(envoke::invalid_source),
        help("check the source definition in envoke.yaml")
    )]
    InvalidSource { reason: String },

    #[error("conflicting overrides: {}", format_override_names(names))]
    #[diagnostic(
        code(envoke::conflicting_overrides),
        help("use only one of the conflicting overrides at a time")
    )]
    ConflictingOverrides { names: Vec<String> },
}

/// Wrapper for multiple resolution errors, displayed as related diagnostics.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[error("{} variable(s) failed to resolve", self.errors.len())]
#[diagnostic(code(envoke::resolve_failed))]
pub struct ResolveErrors {
    #[related]
    pub errors: Vec<ResolveError>,
}
