fn format_cycle(chain: &[String]) -> String {
    chain.join(" -> ")
}

fn format_override_names(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("'{n}'"))
        .collect::<Vec<_>>()
        .join(" and ")
        + " both define sources for this variable"
}

/// Errors that occur during variable resolution.
#[derive(Debug, thiserror::Error)]
#[error("{variable} [{environment}]: {kind}")]
pub struct ResolveError {
    pub variable: String,
    pub environment: String,
    pub kind: ResolveErrorKind,
}

/// The specific kind of resolution failure.
#[derive(Debug, thiserror::Error)]
pub enum ResolveErrorKind {
    #[error("no configuration for this environment")]
    NoConfig,
    #[error("command failed: {reason}")]
    CmdFailed {
        command: Vec<String>,
        reason: String,
    },
    #[error("command exited with {exit_code:?}: {stderr}")]
    CmdNonZero {
        command: Vec<String>,
        exit_code: Option<i32>,
        stderr: String,
    },
    #[error("circular dependency: {}", format_cycle(chain))]
    CircularDependency { chain: Vec<String> },
    #[error("unknown variable reference: {name}")]
    UnknownReference { name: String },
    #[error("template error: {reason}")]
    TemplateRender { reason: String },
    #[error("invalid source: {reason}")]
    InvalidSource { reason: String },
    #[error("conflicting overrides: {}", format_override_names(names))]
    ConflictingOverrides { names: Vec<String> },
}
