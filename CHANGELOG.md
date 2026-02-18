# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.7.1](https://github.com/glennib/envoke/compare/v1.7.0...v1.7.1) - 2026-02-18

### Other

- *(deps)* update actions/cache action to v5
- Add renovate.json

## [1.7.0](https://github.com/glennib/envoke/compare/v1.6.0...v1.7.0) - 2026-02-17

### Added

- expand template meta contexts with tags, overrides, and timestamp

## [1.6.0](https://github.com/glennib/envoke/compare/v1.5.0...v1.6.0) - 2026-02-17

### Added

- add `meta` object to value source template context

## [1.5.0](https://github.com/glennib/envoke/compare/v1.4.1...v1.5.0) - 2026-02-13

### Added

- add --completions flag for shell completion generation

## [1.4.1](https://github.com/glennib/envoke/compare/v1.4.0...v1.4.1) - 2026-02-12

### Other

- add mise with github backend as the primary install method

## [1.4.0](https://github.com/glennib/envoke/compare/v1.3.0...v1.4.0) - 2026-02-12

### Added

- unify template filters across variable and output templates

## [1.3.0](https://github.com/glennib/envoke/compare/v1.2.0...v1.3.0) - 2026-02-12

### Added

- add --list-everything flag and make list flags mutually exclusive

## [1.2.0](https://github.com/glennib/envoke/compare/v1.1.1...v1.2.0) - 2026-02-12

### Added

- add --all-tags CLI flag to include all tagged variables

## [1.1.1](https://github.com/glennib/envoke/compare/v1.1.0...v1.1.1) - 2026-02-11

### Other

- improve documentation across codebase

## [1.1.0](https://github.com/glennib/envoke/compare/v1.0.0...v1.1.0) - 2026-02-11

### Added

- add --list-environments, --list-overrides, --list-tags flags

### Other

- replace Source struct with enum and switch to serde_yml

## [1.0.0](https://github.com/glennib/envoke/compare/v0.1.6...v1.0.0) - 2026-02-11

First stable release. Envoke resolves variables from a declarative YAML
configuration file. The default output is shell-safe environment variable
assignments, but custom output templates are also supported.

### Features

- **Five source types**: `literal` (fixed values), `cmd` (run a command),
  `sh` (run a shell script), `template` (Jinja2 with variable interpolation),
  and `skip` (omit a variable from output).
- **Per-environment sources**: define different sources per environment name
  with a `default` fallback.
- **Tag-based conditional inclusion**: variables can carry `tags`; untagged
  variables are always included, tagged variables require an explicit `--tag`
  match (OR semantics).
- **Named overrides** (`--override`): per-variable alternative source sets
  with a 4-level fallback chain (override env, override default, base env,
  base default). Multiple overrides allowed on disjoint variables; conflicting
  overrides on the same variable are rejected.
- **Dependency resolution**: templates reference other variables via
  `{{ VAR }}`; dependencies are topologically sorted (Kahn's algorithm) and
  cycles are detected with full chain reporting before any execution.
- **Output formats**: built-in default (`VAR='value'`), built-in export
  (`export VAR='value'` via `--prepend-export`), or a custom Jinja2 template
  file (`--template`). Output includes an `@generated` header with invocation
  and timestamp. Variables are sorted alphabetically.
- **Custom template context**: templates receive `variables` (with value and
  description), `v` (flat name-to-value map), and `meta` (timestamp,
  invocation, environment, config file path).
- **Filters**: `urlencode` in variable templates; `shell_escape` in output
  templates.
- **JSON Schema generation** (`--schema`): produces a JSON Schema for
  `envoke.yaml` for editor autocompletion and validation.
- **Shell safety**: values are single-quoted with embedded quotes escaped.
- **Comprehensive error reporting**: all errors collected and reported
  together (not fail-fast), with 8 structured error kinds including cycle
  chains, conflicting overrides, unknown references, and command failures.
- **CLI flags**: `-c` (config path), `-o` (output file), `-t/--tag`,
  `-O/--override`, `--prepend-export`, `--template`, `--schema`,
  `-q/--quiet`, `--version`.

### Changes since 0.1.6

#### Added

- add --quiet flag to suppress status messages
- add --version flag

#### Fixed

- update schema file
- make --schema respect -o flag
- include environment in cycle and reference errors
- reject empty cmd source
- use actual config path in parse error message
- correct template doc comment and regenerate schema

#### Other

- fmt
- update installation instructions in README

## [0.1.6](https://github.com/glennib/envoke/compare/v0.1.5...v0.1.6) - 2026-02-11

### Other

- improve readme and cargo description
- remove deprecation from --prepend-export
- improve readme
- improve env examples

## [0.1.5](https://github.com/glennib/envoke/compare/v0.1.4...v0.1.5) - 2026-02-11

### Added

- add template-based output rendering with --template flag

## [0.1.4](https://github.com/glennib/envoke/compare/v0.1.3...v0.1.4) - 2026-02-11

### Added

- include @generated header in stdout output
- add --override flag for per-variable source overrides

### Other

- update CLAUDE.md and README.md
- add example file

## [0.1.3](https://github.com/glennib/envoke/compare/v0.1.2...v0.1.3) - 2026-02-10

### Fixed

- tagged variables require explicit opt-in via --tag

## [0.1.2](https://github.com/glennib/envoke/compare/v0.1.1...v0.1.2) - 2026-02-10

### Added

- add tag-based conditional inclusion of variables

### Fixed

- *(ci)* sort schema prior to writing/checking

## [0.1.1](https://github.com/glennib/envoke/compare/v0.1.0...v0.1.1) - 2026-02-10

### Added

- add schema to repository

### Other

- add yaml-language-server schema location at github to readme

## [0.1.0](https://github.com/glennib/envoke/releases/tag/v0.1.0) - 2026-02-10

### Added

- add envoke tool

### Other

- add CLAUDE.md
- add README.md
- repo scaffolding
