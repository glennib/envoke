# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Envoke is a CLI tool that resolves environment variables from a declarative YAML configuration file (`envoke.yaml`). It supports multiple value sources (literal, command execution, shell scripts, Jinja2 templates with variable interpolation), tag-based conditional inclusion, named overrides for per-variable source alternatives, topologically sorts variables to resolve dependencies, and outputs shell-safe `VAR='value'` lines.

## Build & Development Commands

This project uses `mise` as a task runner. Install mise first, then tools via `mise install`.

| Task | Command |
|------|---------|
| Build (release) | `mise run build` |
| Run tests | `mise run test` |
| Format code | `mise run fmt` |
| Format check | `mise run fmt:check` |
| Lint | `mise run clippy` |
| All CI checks | `mise run ci` |
| Install locally | `mise run install` |

Run a single test: `cargo nextest run -E 'test(test_name)'`

Formatting uses nightly features (`rustfmt.toml` has `style_edition = "2024"`). Use `mise run fmt:nightly` / `mise run fmt:check:nightly` when stable rustfmt doesn't support the options.

## Architecture

The codebase is a single Rust binary with five modules:

- **`main.rs`** -- CLI (clap) with a subcommand grammar: `render` (alias `r`) prints/writes resolved vars, `exec` (alias `x`) execs a command with them overlaid, `meta <environments|tags|overrides|all>` enumerates config dimensions, `schema` prints the JSON Schema, `completions <SHELL>` prints shell completions (via `clap_complete`). Global flags (`-c/--config`, `-q/--quiet`, `--no-parallel`, `-t/--tag`, `--all-tags`, `-O/--override`) are declared with `global = true` on the root and may appear before *or* after the subcommand; the repeatable `-t/--tag` and `-O/--override` have last-write-wins semantics when split across the subcommand boundary. `render`-only flags: `-o/--output`, `-f/--format` (presets: shell, shell-export, dotenv, json, yaml, k8s-secret, github-actions, terraform-tfvars), `--template`. `exec` collects a trailing `-- <command>...` via clap `last = true` + `num_args = 1..` + `allow_hyphen_values`. Both `render` and `exec` read `ENVOKE_ENV` as a fallback for the `<env>` positional.
- **`config.rs`** -- Data model: `Config` (top-level), `Variable` (per-env sources + optional default/description/tags/overrides), `Override` (alternative default/envs sources), `Source` enum (literal/cmd/sh/template/skip). The `skip` variant is a unit variant; its YAML surface is the bare string `skip` (not `skip: true`). Derives `JsonSchema` via `schemars`.
- **`resolve.rs`** -- Core logic. `resolve_all()` picks per-environment sources (with default fallback and override 4-level fallback chain), topologically sorts via Kahn's algorithm, resolves values in dependency order. Template rendering uses `minijinja` with all built-in filters, `urlencode`, and `shell_escape` (imported from `render.rs`). A `TemplateMeta` struct provides a `meta` object (with `environment`, `tags`, `overrides`, `timestamp`) to value source templates.
- **`render.rs`** -- Output rendering via minijinja templates. Exposes `RenderContext`/`Meta` structs (Meta includes `timestamp`, `invocation`, `invocation_args`, `environment`, `config_file`, `tags`, `overrides`), a `Format` enum (derives `clap::ValueEnum`) holding the eight built-in presets, `render_format()` for preset dispatch, `render_custom()` for `--template`, and a `pub(crate) shell_escape` filter (shared with `resolve.rs`). All filters (built-ins, `urlencode`, `shell_escape`) are available in both variable and output templates. Built-in templates (`shell.j2`, `shell-export.j2`, `dotenv.j2`, `json.j2`, `yaml.j2`, `k8s-secret.j2`, `github-actions.j2`, `terraform-tfvars.j2`) live in `src/templates/` and are compiled in via `include_str!`.
- **`exec.rs`** -- Subprocess exec for the `envoke exec <env> -- <cmd>` subcommand. `exec_command()` builds a `std::process::Command` with overlay semantics (inherit parent env, layer resolved vars on top via `Command::env` — no `env_clear`). On Unix, calls `CommandExt::exec` to replace envoke's process image; on other platforms, spawns, waits, and propagates the exit code.
- **`error.rs`** -- `ResolveError` with structured `ResolveErrorKind` variants (8 types including cycle detection with chain and conflicting overrides).

## Code Style

- Clippy pedantic lints are enabled as warnings (`[lints.clippy] pedantic = "warn"`)
- Rustfmt: `imports_granularity = "Item"`, `group_imports = "StdExternalCrate"`, Unix newlines
- Rust edition 2024
