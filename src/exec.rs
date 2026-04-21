//! Exec a subprocess with resolved variables overlaid on the current
//! process environment.

use std::process::Command;

use miette::Context;
use miette::IntoDiagnostic;

use crate::resolve::Resolved;

/// Build a `Command` whose environment is the current process env plus the
/// resolved variables overlaid on top (resolved values win on name
/// collisions). Stdio is inherited.
///
/// Split from `exec_command` so the env-assembly is testable without spawning.
fn build_command(program: &str, args: &[String], resolved: &[Resolved]) -> Command {
    let mut command = Command::new(program);
    command.args(args);
    for r in resolved {
        command.env(&r.name, &r.value);
    }
    command
}

/// Exec `cmd[0] cmd[1..]` with `resolved` overlaid on the current environment.
///
/// On Unix this replaces envoke's process image via `execvp` — the child
/// inherits PID, TTY, signal disposition, and file descriptors; this function
/// only returns if the exec syscall itself fails. On non-Unix, envoke spawns
/// the child, waits for it, and terminates with the child's exit code (this
/// path is best-effort; Unix is the primary target).
pub fn exec_command(cmd: &[String], resolved: &[Resolved]) -> miette::Result<()> {
    debug_assert!(!cmd.is_empty(), "exec_command called with empty cmd");
    let program = &cmd[0];
    let args = &cmd[1..];
    let mut command = build_command(program, args, resolved);

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = command.exec();
        Err(err)
            .into_diagnostic()
            .with_context(|| format!("failed to exec `{program}`"))
    }

    #[cfg(not(unix))]
    {
        let status = command
            .spawn()
            .into_diagnostic()
            .with_context(|| format!("failed to spawn `{program}`"))?
            .wait()
            .into_diagnostic()
            .with_context(|| format!("failed to wait for `{program}`"))?;
        std::process::exit(status.code().unwrap_or(1));
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::*;

    fn resolved(name: &str, value: &str) -> Resolved {
        Resolved {
            name: name.to_owned(),
            value: value.to_owned(),
            description: None,
        }
    }

    #[test]
    fn build_command_sets_resolved_env_entries() {
        let resolved = vec![resolved("FOO", "bar"), resolved("BAZ", "qux")];
        let cmd = build_command("true", &[], &resolved);
        let envs: Vec<_> = cmd
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(OsStr::to_owned)))
            .collect();
        assert_eq!(envs.len(), 2);
        assert!(
            envs.iter()
                .any(|(k, v)| k == "FOO" && v.as_deref() == Some(OsStr::new("bar")))
        );
        assert!(
            envs.iter()
                .any(|(k, v)| k == "BAZ" && v.as_deref() == Some(OsStr::new("qux")))
        );
    }

    #[test]
    fn build_command_overlay_does_not_clear_inherited_env() {
        // `Command::env` adds/overrides entries without wiping inherited ones.
        // We verify indirectly: get_envs() reports only explicit modifications,
        // so a command with no resolved vars has zero env modifications (i.e.,
        // PATH/HOME/... pass through untouched to the child).
        let cmd = build_command("true", &[], &[]);
        assert_eq!(cmd.get_envs().count(), 0);
    }

    #[test]
    fn build_command_passes_args_through() {
        let cmd = build_command("echo", &["hello".to_owned(), "world".to_owned()], &[]);
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, vec![OsStr::new("hello"), OsStr::new("world")]);
    }

    #[cfg(unix)]
    #[test]
    fn spawn_inherits_parent_path_and_applies_overlay() {
        // End-to-end sanity: spawn a subprocess (not exec, which would replace
        // the test harness) and verify that PATH comes through from the parent
        // and that a resolved variable is visible to the child.
        let resolved = vec![resolved("ENVOKE_TEST_VAR", "overlaid")];
        let mut cmd = build_command(
            "sh",
            &[
                "-c".to_owned(),
                "printf '%s|%s' \"${PATH:-MISSING}\" \"${ENVOKE_TEST_VAR:-MISSING}\"".to_owned(),
            ],
            &resolved,
        );
        let output = cmd.output().expect("spawn sh");
        assert!(output.status.success());
        let stdout = String::from_utf8(output.stdout).unwrap();
        let (path, foo) = stdout.split_once('|').expect("delimited output");
        assert_ne!(path, "MISSING", "PATH should be inherited from parent");
        assert!(!path.is_empty(), "PATH should be non-empty");
        assert_eq!(foo, "overlaid");
    }

    #[cfg(unix)]
    #[test]
    fn spawn_overrides_parent_env_for_same_name() {
        // Overlay semantics: when the parent env has VAR=X and resolved has
        // VAR=Y, the child sees VAR=Y. We target a variable cargo already
        // sets for test binaries (`CARGO_MANIFEST_DIR`) so we only read
        // env (safe) and never mutate the process-wide `environ` (which
        // would race with other threads under parallel test execution).
        let inherited =
            std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR for tests");
        assert_ne!(inherited, "from-resolved", "inherited sentinel collision");
        let resolved = vec![resolved("CARGO_MANIFEST_DIR", "from-resolved")];
        let mut cmd = build_command(
            "sh",
            &[
                "-c".to_owned(),
                "printf '%s' \"${CARGO_MANIFEST_DIR:-MISSING}\"".to_owned(),
            ],
            &resolved,
        );
        let output = cmd.output().expect("spawn sh");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert_eq!(stdout, "from-resolved");
    }
}
