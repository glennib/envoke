# envoke

Resolve environment variables from a declarative YAML config file.

envoke reads an `envoke.yaml` file, resolves variables in dependency order, and
outputs shell-safe `VAR='value'` lines. Variables can be literal strings, command
output, shell scripts, or [minijinja](https://github.com/mitsuhiko/minijinja)
templates that reference other variables.

## Installation

### From source

```sh
cargo install --git https://github.com/glennib/envoke envoke-cli
```

### From GitHub releases

Pre-built binaries are available on the
[releases page](https://github.com/glennib/envoke/releases) for:

- Linux (x86_64, aarch64)
- macOS (x86_64, Apple Silicon)
- Windows (x86_64)

## Quick start

Create an `envoke.yaml`:

```yaml
variables:
  DB_HOST:
    default:
      literal: localhost
    envs:
      prod:
        literal: db.example.com

  DB_USER:
    default:
      literal: app

  DB_PASS:
    envs:
      local:
        literal: devpassword
      prod:
        sh: vault kv get -field=password secret/db

  DB_URL:
    default:
      template: "postgresql://{{ DB_USER }}:{{ DB_PASS | urlencode }}@{{ DB_HOST }}/mydb"
```

Generate variables for an environment:

```sh
$ envoke local
DB_HOST='localhost'
DB_PASS='devpassword'
DB_URL='postgresql://app:devpassword@localhost/mydb'
DB_USER='app'
```

Source them into your shell:

```sh
eval "$(envoke local)"
```

Or write them to a file:

```sh
envoke local --output .env --prepend-export
```

## Configuration

The config file (default: `envoke.yaml`) has a single top-level key `variables`
that maps variable names to their definitions.

### Variable definition

Each variable can have:

| Field | Description |
|-------|-------------|
| `description` | Optional. Rendered as a `# comment` above the variable in output. |
| `default` | Optional. Fallback source used when the target environment has no entry in `envs`. |
| `envs` | Map of environment names to sources. |

A variable must have either an `envs` entry matching the target environment or a
`default`. If neither exists, resolution fails with an error.

### Source types

Each source specifies exactly one of the following fields:

#### `literal`

A fixed string value.

```yaml
DB_HOST:
  default:
    literal: localhost
```

#### `cmd`

Run a command and capture its stdout (trimmed). The value is a list where the
first element is the executable and the rest are arguments.

```yaml
GIT_SHA:
  default:
    cmd: [git, rev-parse, --short, HEAD]
```

#### `sh`

Run a shell script via `sh -c` and capture its stdout (trimmed).

```yaml
TIMESTAMP:
  default:
    sh: date -u +%Y-%m-%dT%H:%M:%SZ
```

#### `template`

A [minijinja](https://github.com/mitsuhiko/minijinja) template string, compatible
with [Jinja2](https://jinja.palletsprojects.com/). Reference other variables
with `{{ VAR_NAME }}`. Dependencies are automatically detected and resolved first
via topological sorting.

```yaml
DB_URL:
  default:
    template: "postgresql://{{ DB_USER }}:{{ DB_PASS }}@{{ DB_HOST }}/{{ DB_NAME }}"
```

The `urlencode` filter is available for escaping special characters:

```yaml
CONN_STRING:
  default:
    template: "postgresql://{{ USER | urlencode }}:{{ PASS | urlencode }}@localhost/db"
```

#### `skip`

Omit this variable from the output. Useful for conditionally excluding a
variable in certain environments while including it in others.

```yaml
DEBUG_TOKEN:
  default:
    skip: true
  envs:
    local:
      literal: debug-token-value
```

### Environments and defaults

envoke selects the source for each variable by checking the `envs` map for the
target environment. If no match is found, it falls back to `default`. This lets
you define shared defaults and override them per environment:

```yaml
LOG_LEVEL:
  default:
    literal: info
  envs:
    local:
      literal: debug
    prod:
      literal: warn
```

## CLI usage

```
envoke [OPTIONS] [ENVIRONMENT]
```

| Option | Description |
|--------|-------------|
| `ENVIRONMENT` | Target environment name (e.g. `local`, `prod`). Required unless `--schema` is used. |
| `-c, --config <PATH>` | Path to config file. Default: `envoke.yaml`. |
| `-o, --output <PATH>` | Write output to a file instead of stdout. Adds an `@generated` header with timestamp. |
| `--prepend-export` | Prefix each line with `export `. |
| `--schema` | Print the JSON Schema for `envoke.yaml` and exit. |

### JSON Schema

Generate a JSON Schema for editor autocompletion and validation:

```sh
envoke --schema > envoke-schema.json
```

Use it in your `envoke.yaml` with a schema comment for editors that support it:

```yaml
# yaml-language-server: $schema=envoke-schema.json
variables:
  # ...
```

Alternatively, point directly at the hosted schema without writing a local file:

```yaml
# yaml-language-server: $schema=https://raw.githubusercontent.com/glennib/envoke/refs/heads/main/envoke.schema.json
variables:
  # ...
```

## How it works

1. Parse the YAML config file.
2. For each variable, select the source matching the target environment (or the
   default).
3. Extract template dependencies and topologically sort all variables using
   Kahn's algorithm.
4. Resolve values in dependency order -- literals are used as-is, commands and
   shell scripts are executed, templates are rendered with already-resolved
   values.
5. Output sorted `VAR='value'` lines with shell-safe escaping.

Circular dependencies and references to undefined variables are detected before
any resolution begins and reported as errors.

## Development

This project uses [mise](https://mise.jdx.dev/) as a task runner. After
installing mise:

```sh
mise install       # Install tool dependencies
mise run build     # Build release binary
mise run test      # Run tests (via cargo-nextest)
mise run clippy    # Run lints
mise run fmt       # Format code
mise run ci        # Run all checks (fmt, clippy, test, build)
```

Run a single test:

```sh
cargo nextest run -E 'test(test_name)'
```

## License

MIT OR Apache-2.0
