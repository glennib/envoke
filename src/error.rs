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
    #[error("command `{command:?}` failed: {reason}")]
    CmdFailed {
        command: Vec<String>,
        reason: String,
    },
    #[error("command `{command:?}` exited with {exit_code:?}: {stderr}")]
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
