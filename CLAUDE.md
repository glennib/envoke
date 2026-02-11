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

The codebase is a single Rust binary with three modules:

- **`main.rs`** -- CLI (clap), reads YAML config, orchestrates resolution, delegates output formatting to `render.rs`. Supports `--output` file writing, `--prepend-export`, `--template` (custom output template), `--tag` (conditional inclusion), `--override` (per-variable source overrides), and `--schema` (JSON Schema output).
- **`config.rs`** -- Data model: `Config` (top-level), `Variable` (per-env sources + optional default/description/tags/overrides), `Override` (alternative default/envs sources), `Source` (one-of: literal/cmd/sh/template/skip), `SourceKind` (validated variant). Derives `JsonSchema` via `schemars`.
- **`resolve.rs`** -- Core logic. `resolve_all()` picks per-environment sources (with default fallback and override 4-level fallback chain), topologically sorts via Kahn's algorithm, resolves values in dependency order. Template rendering uses `minijinja` (supports `urlencode` filter). Contains the test suite (~39 tests).
- **`render.rs`** -- Output rendering via minijinja templates. Exposes `RenderContext`/`Meta` structs, built-in default and export templates (compiled via `include_str!`), custom template file support, and a `shell_escape` filter. Built-in templates live in `src/templates/`.
- **`error.rs`** -- `ResolveError` with structured `ResolveErrorKind` variants (8 types including cycle detection with chain and conflicting overrides).

## Code Style

- Clippy pedantic lints are enabled as warnings (`[lints.clippy] pedantic = "warn"`)
- Rustfmt: `imports_granularity = "Item"`, `group_imports = "StdExternalCrate"`, Unix newlines
- Rust edition 2024
