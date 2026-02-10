use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::process::Command;

use tracing::debug;

use crate::config::Config;
use crate::config::SourceKind;
use crate::error::ResolveError;
use crate::error::ResolveErrorKind;

/// A successfully resolved variable with its value and optional description.
#[derive(Debug)]
pub struct Resolved {
    pub name: String,
    pub value: String,
    pub description: Option<String>,
}

/// Extract variable references from a `MiniJinja` template string.
fn template_references(tmpl: &str) -> Result<HashSet<String>, minijinja::Error> {
    let env = minijinja::Environment::new();
    let parsed = env.template_from_str(tmpl)?;
    Ok(parsed.undeclared_variables(false))
}

/// Topologically sort variables so dependencies are resolved before dependents.
///
/// Returns the sorted variable names, or a list of errors for cycles or unknown
/// references.
fn topological_sort(
    variables: &BTreeMap<String, SourceKind>,
) -> Result<Vec<String>, Vec<ResolveError>> {
    let mut in_degree: HashMap<String, usize> = HashMap::new();
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
    let mut errors = Vec::new();

    for name in variables.keys() {
        in_degree.entry(name.clone()).or_insert(0);
    }

    for (name, source) in variables {
        if let SourceKind::Template(tmpl) = source {
            let refs = match template_references(tmpl) {
                Ok(refs) => refs,
                Err(e) => {
                    errors.push(ResolveError {
                        variable: name.clone(),
                        environment: String::new(),
                        kind: ResolveErrorKind::TemplateRender {
                            reason: e.to_string(),
                        },
                    });
                    continue;
                }
            };
            for dep in refs {
                if !variables.contains_key(&dep) {
                    errors.push(ResolveError {
                        variable: name.clone(),
                        environment: String::new(),
                        kind: ResolveErrorKind::UnknownReference { name: dep },
                    });
                    continue;
                }
                *in_degree.entry(name.clone()).or_insert(0) += 1;
                dependents.entry(dep).or_default().push(name.clone());
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Kahn's algorithm.
    let mut queue: VecDeque<String> = in_degree
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(name, _)| name.clone())
        .collect();
    let mut queue_vec: Vec<String> = queue.drain(..).collect();
    queue_vec.sort_unstable();
    queue = queue_vec.into_iter().collect();

    let mut sorted = Vec::new();

    while let Some(name) = queue.pop_front() {
        sorted.push(name.clone());
        if let Some(deps) = dependents.get(&name) {
            let mut next = Vec::new();
            for dep in deps {
                let deg = in_degree.get_mut(dep).expect("in_degree entry must exist");
                *deg -= 1;
                if *deg == 0 {
                    next.push(dep.clone());
                }
            }
            next.sort_unstable();
            queue.extend(next);
        }
    }

    if sorted.len() != variables.len() {
        let errors = find_cycles(&in_degree, &dependents);
        return Err(errors);
    }

    Ok(sorted)
}

/// Trace cycles among nodes that remain after Kahn's algorithm.
///
/// For each unvisited node with `in_degree > 0`, follows edges to find a cycle,
/// then reports the full chain (e.g. `A -> B -> C -> A`).
fn find_cycles(
    in_degree: &HashMap<String, usize>,
    dependents: &HashMap<String, Vec<String>>,
) -> Vec<ResolveError> {
    // Build a reverse map: for each node, which nodes does it depend on?
    // `dependents` maps dep -> [nodes that depend on dep], so we need the inverse.
    let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();
    for (dep, nodes) in dependents {
        for node in nodes {
            dependencies
                .entry(node.clone())
                .or_default()
                .push(dep.clone());
        }
    }

    let remaining: HashSet<String> = in_degree
        .iter()
        .filter(|(_, deg)| **deg > 0)
        .map(|(name, _)| name.clone())
        .collect();

    let mut visited = HashSet::new();
    let mut errors = Vec::new();

    // For each unvisited remaining node, trace a cycle.
    for start in &remaining {
        if visited.contains(start) {
            continue;
        }

        // Follow dependency edges until we revisit a node.
        let mut path = Vec::new();
        let mut current = start.clone();
        let mut path_set = HashSet::new();

        loop {
            if path_set.contains(&current) {
                // Found the cycle — extract from where `current` first appears.
                let cycle_start = path.iter().position(|n| *n == current).unwrap_or(0);
                let mut chain: Vec<String> = path[cycle_start..].to_vec();
                chain.push(current.clone());
                for node in &chain {
                    visited.insert(node.clone());
                }
                errors.push(ResolveError {
                    variable: chain[0].clone(),
                    environment: String::new(),
                    kind: ResolveErrorKind::CircularDependency { chain },
                });
                break;
            }

            path_set.insert(current.clone());
            path.push(current.clone());

            // Follow an edge to a dependency that is also in the remaining set.
            let next = dependencies
                .get(&current)
                .and_then(|deps| deps.iter().find(|d| remaining.contains(*d)));
            match next {
                Some(n) => current = n.clone(),
                None => break, // shouldn't happen for nodes in a cycle
            }
        }
    }

    errors
}

/// Resolve a single source to its string value.
fn resolve_source(
    source: &SourceKind,
    variable: &str,
    environment: &str,
    resolved: &HashMap<String, String>,
) -> Result<String, ResolveError> {
    match source {
        SourceKind::Literal(value) => {
            debug!(variable, "resolved from literal");
            Ok(value.clone())
        }
        SourceKind::Cmd(args) => {
            debug!(variable, ?args, "executing command");
            let output = Command::new(&args[0])
                .args(&args[1..])
                .output()
                .map_err(|e| ResolveError {
                    variable: variable.to_owned(),
                    environment: environment.to_owned(),
                    kind: ResolveErrorKind::CmdFailed {
                        command: args.clone(),
                        reason: e.to_string(),
                    },
                })?;

            if !output.status.success() {
                return Err(ResolveError {
                    variable: variable.to_owned(),
                    environment: environment.to_owned(),
                    kind: ResolveErrorKind::CmdNonZero {
                        command: args.clone(),
                        exit_code: output.status.code(),
                        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                    },
                });
            }

            let value = String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_owned();
            debug!(variable, "resolved from command");
            Ok(value)
        }
        SourceKind::Sh(script) => {
            let command = vec!["sh".to_owned(), "-c".to_owned(), script.clone()];
            debug!(variable, %script, "executing shell script");
            let output = Command::new("sh")
                .args(["-c", script])
                .output()
                .map_err(|e| ResolveError {
                    variable: variable.to_owned(),
                    environment: environment.to_owned(),
                    kind: ResolveErrorKind::CmdFailed {
                        command: command.clone(),
                        reason: e.to_string(),
                    },
                })?;

            if !output.status.success() {
                return Err(ResolveError {
                    variable: variable.to_owned(),
                    environment: environment.to_owned(),
                    kind: ResolveErrorKind::CmdNonZero {
                        command,
                        exit_code: output.status.code(),
                        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
                    },
                });
            }

            let value = String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_owned();
            debug!(variable, "resolved from shell script");
            Ok(value)
        }
        SourceKind::Template(tmpl) => {
            let env = minijinja::Environment::new();
            let value = env.render_str(tmpl, resolved).map_err(|e| ResolveError {
                variable: variable.to_owned(),
                environment: environment.to_owned(),
                kind: ResolveErrorKind::TemplateRender {
                    reason: e.to_string(),
                },
            })?;
            debug!(variable, "resolved from template");
            Ok(value)
        }
        SourceKind::Skip => unreachable!("skip sources are filtered before resolution"),
    }
}

/// Resolve all variables for the given environment.
///
/// Returns either all resolved values (in deterministic order) or all errors
/// encountered.
pub fn resolve_all(config: &Config, environment: &str) -> Result<Vec<Resolved>, Vec<ResolveError>> {
    let mut sources: BTreeMap<String, SourceKind> = BTreeMap::new();
    let mut errors = Vec::new();

    for (name, variable) in &config.variables {
        let source = variable.envs.get(environment).or(variable.default.as_ref());

        match source {
            Some(source) => match source.kind() {
                Ok(SourceKind::Skip) => {
                    debug!(variable = name.as_str(), "skipped");
                }
                Ok(kind) => {
                    sources.insert(name.clone(), kind);
                }
                Err(msg) => {
                    errors.push(ResolveError {
                        variable: name.clone(),
                        environment: environment.to_owned(),
                        kind: ResolveErrorKind::InvalidSource {
                            reason: msg.to_owned(),
                        },
                    });
                }
            },
            None => {
                errors.push(ResolveError {
                    variable: name.clone(),
                    environment: environment.to_owned(),
                    kind: ResolveErrorKind::NoConfig,
                });
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let order = topological_sort(&sources)?;

    let mut resolved_values: HashMap<String, String> = HashMap::new();
    let mut results = Vec::new();

    for name in &order {
        let source = &sources[name];
        let value =
            resolve_source(source, name, environment, &resolved_values).map_err(|e| vec![e])?;
        resolved_values.insert(name.clone(), value.clone());
        let description = config.variables[name].description.clone();
        results.push(Resolved {
            name: name.clone(),
            value,
            description,
        });
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Source;

    fn literal(value: &str) -> Source {
        Source {
            literal: Some(value.to_owned()),
            cmd: None,
            sh: None,
            template: None,
            skip: None,
        }
    }

    fn template(value: &str) -> Source {
        Source {
            literal: None,
            cmd: None,
            sh: None,
            template: Some(value.to_owned()),
            skip: None,
        }
    }

    fn cmd(args: Vec<&str>) -> Source {
        Source {
            literal: None,
            cmd: Some(args.into_iter().map(ToOwned::to_owned).collect()),
            sh: None,
            template: None,
            skip: None,
        }
    }

    fn sh(script: &str) -> Source {
        Source {
            literal: None,
            cmd: None,
            sh: Some(script.to_owned()),
            template: None,
            skip: None,
        }
    }

    fn skip() -> Source {
        Source {
            literal: None,
            cmd: None,
            sh: None,
            template: None,
            skip: Some(true),
        }
    }

    fn var(envs: BTreeMap<String, Source>) -> crate::config::Variable {
        crate::config::Variable {
            description: None,
            default: None,
            envs,
        }
    }

    fn var_with_default(
        default: Source,
        envs: BTreeMap<String, Source>,
    ) -> crate::config::Variable {
        crate::config::Variable {
            description: None,
            default: Some(default),
            envs,
        }
    }

    #[test]
    fn test_template_references() {
        let refs =
            template_references("postgresql://{{ USER }}:{{ PASS }}@localhost/{{ DB }}").unwrap();
        assert_eq!(
            refs,
            HashSet::from(["USER".to_owned(), "PASS".to_owned(), "DB".to_owned()])
        );
    }

    #[test]
    fn test_template_references_empty() {
        let refs = template_references("no references here").unwrap();
        assert!(refs.is_empty());
    }

    #[test]
    fn test_resolve_literal() {
        let config = Config {
            variables: BTreeMap::from([("FOO".to_owned(), {
                let mut v = var(BTreeMap::from([("local".to_owned(), literal("bar"))]));
                v.description = Some("A foo".to_owned());
                v
            })]),
        };
        let resolved = resolve_all(&config, "local").unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "FOO");
        assert_eq!(resolved[0].value, "bar");
    }

    #[test]
    fn test_resolve_template() {
        let config = Config {
            variables: BTreeMap::from([
                (
                    "USER".to_owned(),
                    var(BTreeMap::from([("local".to_owned(), literal("alice"))])),
                ),
                (
                    "GREETING".to_owned(),
                    var(BTreeMap::from([(
                        "local".to_owned(),
                        template("hello {{ USER }}!"),
                    )])),
                ),
            ]),
        };
        let resolved = resolve_all(&config, "local").unwrap();
        let greeting = resolved.iter().find(|r| r.name == "GREETING").unwrap();
        assert_eq!(greeting.value, "hello alice!");
    }

    #[test]
    fn test_resolve_template_urlencode() {
        let config = Config {
            variables: BTreeMap::from([
                (
                    "USER".to_owned(),
                    var(BTreeMap::from([("local".to_owned(), literal("alice"))])),
                ),
                (
                    "PASS".to_owned(),
                    var(BTreeMap::from([("local".to_owned(), literal("p@ss:word"))])),
                ),
                (
                    "CONN".to_owned(),
                    var(BTreeMap::from([(
                        "local".to_owned(),
                        template("{{ USER | urlencode }}:{{ PASS | urlencode }}"),
                    )])),
                ),
            ]),
        };
        let resolved = resolve_all(&config, "local").unwrap();
        let conn = resolved.iter().find(|r| r.name == "CONN").unwrap();
        assert_eq!(conn.value, "alice:p%40ss%3Aword");
    }

    #[test]
    fn test_missing_environment() {
        let config = Config {
            variables: BTreeMap::from([(
                "FOO".to_owned(),
                var(BTreeMap::from([("prod".to_owned(), literal("x"))])),
            )]),
        };
        let err = resolve_all(&config, "local").unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(err[0].kind, ResolveErrorKind::NoConfig));
    }

    #[test]
    fn test_circular_dependency() {
        let config = Config {
            variables: BTreeMap::from([
                (
                    "A".to_owned(),
                    var(BTreeMap::from([("local".to_owned(), template("{{ B }}"))])),
                ),
                (
                    "B".to_owned(),
                    var(BTreeMap::from([("local".to_owned(), template("{{ A }}"))])),
                ),
            ]),
        };
        let err = resolve_all(&config, "local").unwrap_err();
        assert!(err
            .iter()
            .any(|e| matches!(&e.kind, ResolveErrorKind::CircularDependency { chain } if chain.len() >= 3)));
    }

    #[test]
    fn test_unknown_reference() {
        let config = Config {
            variables: BTreeMap::from([(
                "A".to_owned(),
                var(BTreeMap::from([(
                    "local".to_owned(),
                    template("{{ NONEXISTENT }}"),
                )])),
            )]),
        };
        let err = resolve_all(&config, "local").unwrap_err();
        assert!(err.iter().any(
            |e| matches!(&e.kind, ResolveErrorKind::UnknownReference { name } if name == "NONEXISTENT")
        ));
    }

    #[test]
    fn test_resolve_cmd_echo() {
        let config = Config {
            variables: BTreeMap::from([(
                "VAL".to_owned(),
                var(BTreeMap::from([(
                    "local".to_owned(),
                    cmd(vec!["echo", "hello"]),
                )])),
            )]),
        };
        let resolved = resolve_all(&config, "local").unwrap();
        assert_eq!(resolved[0].value, "hello");
    }

    #[test]
    fn test_default_fallback() {
        let config = Config {
            variables: BTreeMap::from([(
                "FOO".to_owned(),
                var_with_default(literal("fallback"), BTreeMap::new()),
            )]),
        };
        let resolved = resolve_all(&config, "any-env").unwrap();
        assert_eq!(resolved[0].value, "fallback");
    }

    #[test]
    fn test_env_overrides_default() {
        let config = Config {
            variables: BTreeMap::from([(
                "FOO".to_owned(),
                var_with_default(
                    literal("fallback"),
                    BTreeMap::from([("local".to_owned(), literal("override"))]),
                ),
            )]),
        };
        let resolved = resolve_all(&config, "local").unwrap();
        assert_eq!(resolved[0].value, "override");
    }

    #[test]
    fn test_circular_dependency_chain_message() {
        let config = Config {
            variables: BTreeMap::from([
                (
                    "A".to_owned(),
                    var(BTreeMap::from([("local".to_owned(), template("{{ B }}"))])),
                ),
                (
                    "B".to_owned(),
                    var(BTreeMap::from([("local".to_owned(), template("{{ C }}"))])),
                ),
                (
                    "C".to_owned(),
                    var(BTreeMap::from([("local".to_owned(), template("{{ A }}"))])),
                ),
            ]),
        };
        let err = resolve_all(&config, "local").unwrap_err();
        let cycle = err
            .iter()
            .find_map(|e| match &e.kind {
                ResolveErrorKind::CircularDependency { chain } => Some(chain),
                _ => None,
            })
            .expect("should have a cycle error");
        // Chain should be e.g. ["A", "B", "C", "A"]
        assert_eq!(
            cycle.first(),
            cycle.last(),
            "chain should start and end with same node"
        );
        assert_eq!(
            cycle.len(),
            4,
            "3-node cycle should have 4 entries (A->B->C->A)"
        );
    }

    #[test]
    fn test_skip_omits_variable() {
        let config = Config {
            variables: BTreeMap::from([
                (
                    "KEEP".to_owned(),
                    var(BTreeMap::from([("local".to_owned(), literal("yes"))])),
                ),
                (
                    "DROP".to_owned(),
                    var(BTreeMap::from([("local".to_owned(), skip())])),
                ),
            ]),
        };
        let resolved = resolve_all(&config, "local").unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].name, "KEEP");
    }

    #[test]
    fn test_skip_as_default() {
        let config = Config {
            variables: BTreeMap::from([(
                "VAR".to_owned(),
                var_with_default(
                    skip(),
                    BTreeMap::from([("staging".to_owned(), literal("yes"))]),
                ),
            )]),
        };
        // In staging, the env override provides a value.
        let resolved = resolve_all(&config, "staging").unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].value, "yes");
        // In prod, the default skip applies — variable is omitted.
        let resolved = resolve_all(&config, "prod").unwrap();
        assert!(resolved.is_empty());
    }

    #[test]
    fn test_skip_reference_errors() {
        let config = Config {
            variables: BTreeMap::from([
                (
                    "SKIPPED".to_owned(),
                    var(BTreeMap::from([("local".to_owned(), skip())])),
                ),
                (
                    "USER".to_owned(),
                    var(BTreeMap::from([(
                        "local".to_owned(),
                        template("hi {{ SKIPPED }}"),
                    )])),
                ),
            ]),
        };
        let err = resolve_all(&config, "local").unwrap_err();
        assert!(err.iter().any(
            |e| matches!(&e.kind, ResolveErrorKind::UnknownReference { name } if name == "SKIPPED")
        ));
    }

    #[test]
    fn test_resolve_sh() {
        let config = Config {
            variables: BTreeMap::from([(
                "VAL".to_owned(),
                var(BTreeMap::from([("local".to_owned(), sh("echo hello"))])),
            )]),
        };
        let resolved = resolve_all(&config, "local").unwrap();
        assert_eq!(resolved[0].value, "hello");
    }
}
