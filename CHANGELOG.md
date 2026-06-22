# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.4](https://github.com/glennib/envoke/compare/v2.0.3...v2.0.4) - 2026-06-22

### Other

- *(deps)* lock file maintenance ([#71](https://github.com/glennib/envoke/pull/71))
- *(deps)* update dependency cargo-binstall to v1.20.1 ([#70](https://github.com/glennib/envoke/pull/70))
- *(deps)* update dependency jq to v1.8.2 ([#69](https://github.com/glennib/envoke/pull/69))
- *(deps)* update rust crate minijinja to v2.21.0 ([#67](https://github.com/glennib/envoke/pull/67))
- *(deps)* lock file maintenance ([#66](https://github.com/glennib/envoke/pull/66))
- *(deps)* update dependency cargo-binstall to v1.20.0 ([#65](https://github.com/glennib/envoke/pull/65))

## [2.0.3](https://github.com/glennib/envoke/compare/v2.0.2...v2.0.3) - 2026-06-02

### Fixed

- *(deps)* update rust crate serde_yml to 0.0.13 ([#60](https://github.com/glennib/envoke/pull/60))

### Other

- *(deps)* update rust crate minijinja to v2.20.0 ([#58](https://github.com/glennib/envoke/pull/58))
- *(deps)* update rust crate serde_json to v1.0.150 ([#59](https://github.com/glennib/envoke/pull/59))
- make renovate automerge
- pin mise dependencies

## [2.0.2](https://github.com/glennib/envoke/compare/v2.0.1...v2.0.2) - 2026-04-26

### Fixed

- use visible aliases

### Other

- don't do release builds in ci

## [2.0.1](https://github.com/glennib/envoke/compare/v2.0.0...v2.0.1) - 2026-04-22

### Other

- small fix
- set envoke-env version to 2

## [2.0.0](https://github.com/glennib/envoke/compare/v1.12.0...v2.0.0) - 2026-04-22

### Migration from v1

v2 has four breaking changes. Each section lists the v1 invocation on the left
and its v2 equivalent on the right.

#### CLI split into subcommands

The flat `envoke [FLAGS] [ENV] [-- CMD...]` grammar is gone. Each mode is now
its own subcommand.

| v1 | v2 |
| --- | --- |
| `envoke prod` | `envoke render prod` (alias `envoke r prod`) |
| `envoke prod -- ./serve` | `envoke exec prod -- ./serve` (alias `envoke x prod -- ./serve`) |
| `envoke --schema` | `envoke schema` |
| `envoke --completions bash` | `envoke completions bash` |
| `envoke --list-environments` | `envoke meta environments` |
| `envoke --list-tags` | `envoke meta tags` |
| `envoke --list-overrides` | `envoke meta overrides` |

`-o/--output`, `-f/--format`, and `--template` are now `render`-only and error
under other subcommands. `-c/--config`, `-q/--quiet`, `--no-parallel`,
`-t/--tag`, `--all-tags`, and `-O/--override` remain global and may appear
before or after the subcommand. When repeatable flags (`-t/--tag`,
`-O/--override`) are split across the subcommand boundary, they follow
last-write-wins — pick one side.

#### `--prepend-export` removed

Superseded by `--format shell-export` in v1.12.0 and now retired.

| v1 | v2 |
| --- | --- |
| `envoke prod --prepend-export` | `envoke render prod --format shell-export` |

#### `--format shell` removed; default format is now `dotenv`

| v1 | v2 |
| --- | --- |
| `envoke prod` (default `shell`) | `envoke render prod` (default `dotenv`) |
| `envoke prod --format shell` | `envoke render prod --format shell-export` |

The `dotenv` encoding was reworked for portability: literal strings round-trip
unchanged across `dotenvy` (mise, Rust), `godotenv` (Docker Compose),
`python-dotenv`, and `node dotenv`, and `$` never expands at the consumer.
A new `dotenv_escape` minijinja filter is exposed in both output templates and
variable-source templates.

#### `skip` source is now a bare string

```yaml
# v1
envs:
  prod:
    skip: true

# v2
envs:
  prod: skip
```

`skip: false` was never meaningful (it was rejected at runtime); the object
form is simply gone.

### Fixed

- *(render)* [**breaking**] drop shell format, portable dotenv encoding

### Other

- *(resolve)* bound external source fan-out with a worker pool
- sync internal doc comments and test fixtures with v2 CLI
- *(cli)* [**breaking**] drop --prepend-export
- *(cli)* [**breaking**] split into subcommands (render, exec, meta, schema, completions)
- *(config)* [**breaking**] Source::Skip becomes a unit variant

## [1.12.0](https://github.com/glennib/envoke/compare/v1.11.0...v1.12.0) - 2026-04-21

### Added

- *(cli)* add `--format` with curated output presets

## [1.11.0](https://github.com/glennib/envoke/compare/v1.10.1...v1.11.0) - 2026-04-21

### Added

- *(cli)* add exec form `envoke <env> -- <cmd>`

### Other

- polish README and sync CLAUDE.md for the exec form

## [1.10.1](https://github.com/glennib/envoke/compare/v1.10.0...v1.10.1) - 2026-04-20

### Other

- promote envoke-env mise plugin in README

## [1.10.0](https://github.com/glennib/envoke/compare/v1.9.0...v1.10.0) - 2026-04-20

### Added

- read target environment from `ENVOKE_ENV` env var

## [1.9.0](https://github.com/glennib/envoke/compare/v1.8.1...v1.9.0) - 2026-04-16

### Added

- parallelize cmd/sh source resolution

### Fixed

- disable `format_strings` in rustfmt to prevent mangling help text

## [1.8.1](https://github.com/glennib/envoke/compare/v1.8.0...v1.8.1) - 2026-04-15

### Other

- Merge pull request #41 from glennib/renovate/clap-4.x-lockfile
- Merge pull request #39 from glennib/claude/add-miette-diagnostics-KXtbn

## [1.8.0](https://github.com/glennib/envoke/compare/v1.7.2...v1.8.0) - 2026-04-15

### Added

- enable `tojson` filter in minijinja templates

### Other

- *(deps)* update rust crate clap_complete to v4.6.2
- *(deps)* update rust crate minijinja to v2.19.0
- ignore autogenerated release workflow in Renovate
- *(deps)* update actions/checkout action to v6
- upgrade dist

## [1.7.2](https://github.com/glennib/envoke/compare/v1.7.1...v1.7.2) - 2026-03-07

### Other

- update Cargo.lock dependencies

## [1.7.1](https://github.com/glennib/envoke/compare/v1.7.0...v1.7.1) - 2026-03-02

### Other

- Merge pull request #27 from glennib/renovate/clap-4.x-lockfile
- Merge pull request #28 from glennib/renovate/anyhow-1.x-lockfile
- Merge pull request #29 from glennib/renovate/minijinja-2.x-lockfile
- *(deps)* update rust crate chrono to v0.4.44
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
