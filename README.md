[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg)](.github/CODE_OF_CONDUCT.md)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Cubtera version release](https://github.com/cubtera/cubtera/actions/workflows/release_please.yaml/badge.svg?branch=main)](https://github.com/cubtera/cubtera/actions/workflows/release_please.yaml)
# Cubtera
## Multi-dimensional Infrastructure Manager

Cubtera is an instance-centric CLI and server for running the same Terraform,
OpenTofu, Bash, or Helm code across many "dimensions" (environments, data
centers, accounts, services, ...) without duplicating the unit's code per
target. It adds a reviewable plan/apply gate, a durable run ledger, a
cross-unit output mesh, and drift/fleet reporting on top of that - see
[Architecture](#architecture) below.

> This is a v3 rewrite. If you have an existing `config.toml`/data
> directory from an older release (keys like `deploymentLogPath`,
> `unitStatePath`, `[deploymentLog]`, `[unitState]`, or `CUBTERA_DB`), run
> `cubtera migrate` once - see [Migration](#migration).

## Installation

### Via Homebrew (MacOS and Linux)
```bash
brew tap cubtera/cubtera
brew install cubtera
```

### Manual Installation
Download the latest binary from [releases](https://github.com/cubtera/cubtera/releases) and add it to your PATH.

### Building from source
```bash
cargo build --workspace --release
# binaries land in target/release/: cubtera, cubtera-server, cubtera-mcp
```

## Core Concepts

### Dimensions
Dimensions are logical groupings that help organize your infrastructure.
Common dimension types include:

- **Environments** (dev, staging, prod)
- **Data centers / regions** (us-east-1, eu-west-1)
- **Accounts** (management, production, staging)
- **Services** (frontend, backend, database)
- Any custom type you define (storage, domains, repos, ...)

Dimensions are hierarchical, resolved through a configured `dimRelations`
chain (default `["dome", "env", "dc"]`):

```
dome:prod
  └── env:prod
      └── dc:prod-use1
```

Each dimension is a plain JSON file on disk under `inventory/<type>/`, with
optional `.default` (gap-fill) and `.schema` (JSON Schema validation) files
per type - see `example/inventory` for a working layout.

### Units
Units are atomic infrastructure operations. A unit can be:

- Terraform or OpenTofu modules (`type = "tf"` / `"tofu"`)
- A Bash script (`type = "bash"`)
- A Helm chart (`type = "helm"`)

Each unit lives under `units/<name>/` with a `manifest.toml` describing its
required/optional dimensions, access rules, runner overrides, and (for
units that need to share data with each other) `[inputs]`/`[outputs]`.

Example `manifest.toml`:
```toml
dimensions = ["dc"]
allowList = ["dc:stg1-use2"]   # allowList/denyList entries are always "type:name"
type = "tf"

[spec.files.optional]
"greeting.txt" = "greeting.txt"

[runner]
version = "1.6.6"
state_backend = "local"

# Publish `terraform output -json` for other units to read via [inputs]
[outputs]
publish = true
```

A consumer unit reads it back:
```toml
[inputs.infra]
unit = "tf_unit02"   # dims/ext auto-projected from this unit's own resolved dimensions
required = false
```

See `example/units/tf_unit02` (producer) and `example/units/bash_unit01`
(consumer) for the full working pair.

### Features

- **Multi-dimensional runs** - the same unit code, driven by `-d type:name`
  flags, across as many dimension combinations as your inventory defines.
- **Terraform / OpenTofu / Bash / Helm runners**, each expressed as a
  `RunnerStrategy` - adding a new one doesn't touch the run pipeline.
- **Reviewable plan → apply** for `tf`/`tofu`: `cubtera plan` freezes a
  `Plan` (module/inventory/config digests + a rendered artifact); `cubtera
  apply --plan <id>` re-checks those pins before touching real
  infrastructure and refuses if anything drifted.
- **Durable run ledger** (SQLite): every `Plan`/`Run`/`Instance` is a row,
  queryable with `cubtera explain run <id>`.
- **Cross-unit output mesh**: a producer's `terraform output` becomes a
  versioned `OutputSet` other units can declare as `[inputs]`, with
  built-in staleness tracking - no DAG, no auto-run of the producer.
- **Fleet visibility & drift**: `cubtera fleet ls`/`status` and `cubtera
  drift` diff a unit + a selector expression over the inventory against
  what's actually been applied, for CI gating.
- **A server + REST API + MCP server**: `cubtera-server` exposes the same
  inventory/run/state operations over HTTP (with live SSE log streaming
  for in-flight applies); `cubtera-mcp` exposes the read-only subset as
  MCP tools for IDEs/agents.

## Usage

### Basic commands

Run a unit directly (no plan artifact - the "just do it" path, and the
only path for `bash`/`helm` units):
```bash
cubtera run -u network -d env:prod -d dc:prod-use1 -- apply
```

Reviewed plan → apply, for `tf`/`tofu` units:
```bash
cubtera plan -u network -d dc:prod-use1 -- plan
# -> prints a plan id
cubtera apply --plan <plan_id> -u network -d dc:prod-use1 -- apply
```

Query the inventory:
```bash
cubtera im get-all env
cubtera im get env prod
```

Fleet visibility and drift:
```bash
cubtera fleet ls
cubtera fleet status -u network -s "env.name == 'prod'"
cubtera drift -u network -s "env.name == 'prod'"   # CI: exits non-zero on real drift
```

Look up a past run, or the deployment log:
```bash
cubtera explain run <run_id>
cubtera log get -q unit:network --limit 10
```

Run the server and the MCP server against it:
```bash
CUBTERA_CONFIG=example/config.toml cubtera-server &
cubtera-mcp --server-url http://127.0.0.1:8081
```

### Configuration

Configure Cubtera with a `config.toml` (default search path
`~/.cubtera/config.toml`, override with `--config`/`CUBTERA_CONFIG`) plus a
handful of `CUBTERA_*` environment variables (`CUBTERA_ORG`,
`CUBTERA_API_KEY`, `CUBTERA_SERVER_ADDR`, `CUBTERA_SERVER_URL`).

Example `config.toml`:
```toml
[default]
inventoryPath = "inventory"
unitsPath = "units"
modulesPath = "modules"
storePath = "~/.cubtera/store.sqlite"   # SQLite: instances, plans, runs, output sets, leases
dimRelations = ["dome", "env", "dc"]
orgs = ["mycompany"]

[mycompany.runner.tf]
version = "1.6.6"
state_backend = "s3"

[mycompany.state.s3]
bucket = "terraform-state"
region = "us-east-1"
key = "{{dim_tree}}/{{unit_name}}.tfstate"
```

See `example/config.toml` for a fully-annotated reference and
[AGENTS.md](AGENTS.md#configuration) for the full field list.

### Migration

Upgrading from a pre-v3 install (fs-jsonl deployment logs, fs-json unit
state, `[deploymentLog]`/`[unitState]`/`CUBTERA_DB` in `config.toml`)?

```bash
cubtera migrate                 # dry run - prints what would change
cubtera migrate --apply         # cleans up config.toml (backed up first) and imports history into storePath
```

See [`.github/docs/migration-guide.md`](.github/docs/migration-guide.md).

## Architecture

Cubtera is layered kernel → model → application → infrastructure →
interface, with dependencies pointing inward only:

1. **`cubtera-kernel`** - zero-I/O identifiers (`Ident`, `DimRef`,
   `InstanceId`, `Digest`) that close path-traversal/injection holes at a
   single choke point.
2. **`cubtera-model`** - pure domain types: dimensions, units, manifests,
   access policy, selectors/bindings, plans, runs, output sets.
3. **`cubtera-app`** - use cases (`ResolveUseCase`, `AssembleUseCase`,
   `ValidateUseCase`, `RunUseCase`, `BindingUseCase`) behind ports
   (`InventoryPort`, `UnitPort`, `Executor`, `IdentityProvider`, `Clock`).
4. **Adapters** - `cubtera-inventory` (filesystem), `cubtera-store`
   (SQLite), `cubtera-exec` (Terraform/OpenTofu/Bash/Helm runners),
   `cubtera-identity`, `cubtera-source`, `cubtera-config`.
5. **Interfaces** - the `cubtera` CLI, the `cubtera-server` REST API +
   SSE log streaming, and `cubtera-mcp` (an MCP server that's a pure HTTP
   client of `cubtera-server`).

For the full crate map, on-disk formats, and REST/MCP surface, see
[AGENTS.md](AGENTS.md); for the original design intent, see
[`docs/specs/2026-09-03-cubtera-v3-architecture.md`](docs/specs/2026-09-03-cubtera-v3-architecture.md).

## Development

### Project structure
```
cubtera/
├── crates/
│   ├── cubtera-kernel/      # Ident/DimRef/InstanceId/Digest - zero I/O
│   ├── cubtera-model/       # pure domain types
│   ├── cubtera-app/         # ports + use cases
│   ├── cubtera-inventory/   # filesystem inventory/unit adapters
│   ├── cubtera-store/       # SQLite Store (instances/plans/runs/output sets/leases)
│   ├── cubtera-exec/        # tf/tofu/bash/helm runners + process execution
│   ├── cubtera-identity/    # secret resolution (env:/literal: refs)
│   ├── cubtera-source/      # module pinning/resolution (git/fs)
│   ├── cubtera-config/      # config.toml loader
│   ├── cubtera/             # CLI binary
│   ├── cubtera-server/      # REST API + SSE log streaming binary
│   └── cubtera-mcp/         # MCP server binary (HTTP client of cubtera-server)
├── example/                 # config.toml, inventory/, units/ - used by e2e/golden tests and local dev
└── docs/specs/              # architecture spec(s)
```

### Building
```bash
cargo build --workspace
```

### Testing
```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Commit your changes
4. Push to the branch
5. Create a Pull Request

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.
