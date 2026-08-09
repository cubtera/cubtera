# Cubtera V2 - AI Agent Context Guide

## Project Overview

**Cubtera** is a multi-dimensional Infrastructure Manager: a CLI and REST API for
running Terraform, OpenTofu, or Bash against the same code across different
"dimensions" (data centers, environments, "domes", services, ...) without
duplicating unit code per target.

This is `v2-rewrite`: a hexagonal-architecture rewrite of the `main` (v1.x)
codebase, which now lives at [`v1/`](v1/) for reference only (do not modify
it, do not build against it - it will be deleted once v2 reaches parity).
v2 is a **breaking major**: new `config.toml` schema, new CLI flags, new
REST API. There are no compatibility shims. The one thing that is *not*
allowed to break is the on-disk inventory format under `example/inventory` -
see [Inventory on-disk format](#inventory-on-disk-format) below.

If you're migrating a v1 deployment, see the [migration guide](.github/docs/migration-guide.md).

### Wave 1 vs wave 2

- **Wave 1 (done):** FS-backed inventory, `tf`/`tofu`/`bash` runners, CLI
  `run`/`im`/`config`, REST API for orgs/dimensions/units.
- **Wave 2 (done):** MongoDB adapter for
  `InventoryRepository` (`--features mongodb`, contract-tested against a
  real server), `DeploymentLogRepository` (fs-jsonl default +
  `MongoDeploymentLogRepository`) and `cubtera log get` / `GET
  /v1/{org}/dlog`, `im sync-defaults`/`im sync-all`/`im sync` (FS -> Mongo),
  a Helm runner (`RunnerStrategy` renders `values.yaml.tpl` via handlebars
  before invoking `helm`), and `cubtera-mcp` (an `rmcp`-based MCP server,
  stdio transport, exposing read-only inventory/unit/dlog query tools -
  no `run`/write tools). Postgres is not planned; it was dropped from the
  design entirely.
- **Wave 2.1 (done): cross-unit state mesh.** Producers opt in with
  `[outputs] publish = true`; after a successful `apply`/`destroy`, `tf`/
  `tofu` runners capture `<binary> output -json`, flatten it
  (`flatten_tf_outputs`), and publish it to a `UnitStateRepository`
  (fs-json default at `unitStatePath`, `MongoUnitStateRepository` opt-in via
  `[unitState]`). Consumers declare `[inputs.<alias>]` (producer unit name,
  optional explicit `dims`/`ext`, `required` default `true`); `UnitService`
  projects the consumer's own resolved dimension chain onto the producer's
  required dimensions (`cubtera_domain::project_state_key` - deliberately
  not a DAG, no auto-run of the producer, no fan-out fallback) and
  materializes the result as `cubtera_in_<alias>.json` (plus an aggregate
  `cubtera_inputs.json`) in the consumer's temp folder; `BashRunner` also
  exposes each alias as a `CUBTERA_IN_<ALIAS>` env var. Read-only access
  outside a run: `cubtera state get/ls/rm`, `GET
  /v1/{org}/units/{name}/state`, and the MCP `get_unit_state` tool.

---

## Architecture (Hexagonal / Ports & Adapters)

### Project structure

```
cubtera/
├── Cargo.toml                  # workspace root, single version (2.0.0) across all members
├── crates/
│   │
│   │ ─────────── DOMAIN LAYER (zero I/O) ───────────
│   ├── cubtera-domain/         # Pure business logic, only `serde_json` + `std` as deps
│   │   └── src/
│   │       ├── dimension.rs    # DimType, RawDimension, Dimension, DimHierarchy, gap_fill_merge, sha256_of_*
│   │       ├── unit.rs         # Unit entity: temp folder, materialize(), dim_tree(), resolved_inputs -> cubtera_in_*.json
│   │       ├── unit_state.rs   # UnitStateKey/Record, project_state_key(), flatten_tf_outputs() (cross-unit state mesh)
│   │       ├── manifest.rs     # Manifest (TOML schema), RunnerType, Spec, InputSpec/OutputsSpec ([inputs]/[outputs])
│   │       ├── access.rs       # AccessPolicy::evaluate -> AccessDecision (Allowed/Denied)
│   │       ├── materialization.rs # MaterializationPlan (copy/symlink/write ops), --dry-run printing
│   │       ├── runner.rs       # RunParams, RunResult, StatePath, render_state_backend_config (handlebars)
│   │       ├── schema.rs       # validate_against_schema (JSON Schema via `jsonschema`)
│   │       └── error.rs        # DomainError
│   │
│   │ ─────────── APPLICATION LAYER ───────────
│   ├── cubtera-core/            # Ports (traits) + services (use cases) + composition root
│   │   └── src/
│   │       ├── ports/
│   │       │   ├── inventory.rs     # InventoryRepository (raw records only, no business logic)
│   │       │   ├── repository.rs    # UnitRepository (manifests)
│   │       │   ├── runner.rs        # RunnerStrategy, RunnerFactory, RunContext, PrepareMode, CopyConfig
│   │       │   ├── process.rs       # ProcessRunner, ProcessSpec, ProcessOutput
│   │       │   ├── workspace.rs     # Workspace (executes a MaterializationPlan) + read_file (for collect_outputs)
│   │       │   ├── deployment_log.rs# DeploymentLogRepository + entry_matches/matches_all_dimensions query helpers
│   │       │   └── unit_state.rs    # UnitStateRepository (get/put/delete/list published outputs)
│   │       ├── services/
│   │       │   ├── dimension.rs     # DimensionService: assembles Dimension from RawDimension + defaults + parent chain
│   │       │   ├── unit.rs          # UnitService: builds Unit, applies AccessPolicy, resolves [inputs] via UnitStateRepository
│   │       │   └── run.rs           # RunService: owns the runner pipeline (see below), publishes [outputs] after apply/destroy
│   │       ├── app.rs                # App / AppBuilder - composition root, wires everything
│   │       └── error.rs              # AppError + AppResult, mapped to exit codes/HTTP status at the edges
│   │
│   │ ─────────── INFRASTRUCTURE LAYER ───────────
│   ├── cubtera-persistence/     # feature-gated adapters: `fs` (default), `mongodb` (opt-in, `--features mongodb`)
│   │   └── src/
│   │       ├── fs/
│   │       │   ├── dimension.rs      # FsInventoryRepository - implements the naming convention below
│   │       │   ├── unit.rs           # FsUnitRepository - reads manifest.toml
│   │       │   ├── workspace.rs      # FsWorkspace - applies a MaterializationPlan with tokio::fs
│   │       │   ├── deployment_log.rs # FsDeploymentLogRepository - one append-only `{org}.jsonl` file per org
│   │       │   └── unit_state.rs     # FsUnitStateRepository - one outputs.json per {org}/{unit}/{dims}/{ext}
│   │       ├── mongodb.rs          # MongoInventoryRepository + MongoDeploymentLogRepository + MongoUnitStateRepository, contract-tested
│   │       └── factory.rs          # Repositories::from_config - picks fs vs mongo (inventory/dlog/unit_state independently) from Config
│   │
│   ├── cubtera-runners/         # RunnerStrategy implementations
│   │   └── src/
│   │       ├── terraform/       # TerraformRunner: tfvars, cubtera_vars.tf, backend HCL, init-lock, tfswitch, collect_outputs
│   │       ├── opentofu/        # OpenTofuRunner: same shape, no forced tfswitch dependency
│   │       ├── bash/            # BashRunner: no version/backend concerns, just execute; exposes [inputs] as CUBTERA_IN_<ALIAS>
│   │       ├── helm/            # HelmRunner: renders values.yaml.tpl (handlebars) from cubtera_*.json (incl. cubtera_in_*.json), then `helm ...`
│   │       ├── process.rs       # TokioProcessRunner (ProcessRunner via tokio::process, inherited stdio)
│   │       └── factory.rs       # DefaultRunnerFactory
│   │
│   ├── cubtera-config/          # Config, ConfigProvider/ConfigSource (injectable path+env sources)
│   │
│   │ ─────────── INTERFACE LAYER ───────────
│   ├── cubtera/                 # CLI (binary: `cubtera`) - commands/{run,im,config,log,state}.rs
│   ├── cubtera-api/             # REST API (binary: `cubtera-api`), Axum, auth middleware, problem+json
│   └── cubtera-mcp/             # MCP server (binary: `cubtera-mcp`), rmcp SDK, stdio transport, read-only tools
│
├── v1/                          # legacy pre-rewrite code, reference only - do not edit, do not build
└── example/                     # fixtures used by golden tests, e2e tests, and local dev (config.toml, inventory/, units/)
```

A web UI is **not implemented** - don't create files for it unless you're
explicitly starting that work.

### Dependency rule

Dependencies point inward only, and it's enforced by `Cargo.toml`, not just convention:

- `cubtera-domain` depends on nothing but `serde_json`/`sha2`/`jsonschema`/`std` - **no `tokio`, no `async-trait`, no filesystem, no process spawning.**
- `cubtera-core` depends on `cubtera-domain` only. Ports are `async_trait` traits; services orchestrate domain types through those traits.
- `cubtera-persistence` / `cubtera-runners` / `cubtera-config` depend on `cubtera-core` + `cubtera-domain` and implement the ports.
- `cubtera` (CLI) / `cubtera-api` depend on everything and wire it together in `main.rs`/`server.rs` via `App::new(...)`.
- `cubtera-mcp` depends on `cubtera-core`/`cubtera-config`/`cubtera-persistence` only (no `cubtera-runners`): its tools are read-only queries through `DimensionService`/`UnitService`/`DeploymentLogRepository`, so it never needs `RunService`'s runner/workspace/process ports.

```rust
// CORRECT: adapter implements a core port
impl InventoryRepository for FsInventoryRepository { /* ... */ }

// WRONG: domain reaching into infrastructure or async runtimes
use tokio::fs; // never in cubtera-domain
```

### Ports and adapters actually in the tree

| Port (trait, in `cubtera-core::ports`) | Adapter(s) |
| --- | --- |
| `InventoryRepository` | `FsInventoryRepository` (default), `MongoInventoryRepository` (`--features mongodb`, selected by setting `CUBTERA_DB`) |
| `UnitRepository` | `FsUnitRepository` |
| `Workspace` | `FsWorkspace` |
| `ProcessRunner` | `TokioProcessRunner` |
| `RunnerStrategy` (+ `RunnerFactory`) | `TerraformRunner`, `OpenTofuRunner`, `BashRunner`, `HelmRunner` via `DefaultRunnerFactory` |
| `DeploymentLogRepository` | `FsDeploymentLogRepository` (default, `deploymentLogPath`), `MongoDeploymentLogRepository` (selected by setting `[deploymentLog]` in `config.toml`) |
| `UnitStateRepository` | `FsUnitStateRepository` (default, `unitStatePath`), `MongoUnitStateRepository` (selected by setting `[unitState]` in `config.toml`) |

`InventoryRepository` is deliberately "dumb": it returns `RawDimension`
(sections keyed by name, plus includes) with **zero** business logic -  no
defaults gap-fill, no parent resolution, no `meta` wrapping. All of that
lives in `DimensionService`/domain functions, so a future Mongo adapter gets
correct behavior for free instead of re-implementing it (this is exactly
where v1's adapters went wrong).

### Result-based error handling, no `exit()`/`panic!` in domain/core/infra

`AppError` (in `cubtera-core::error`) is the only error type services return.
It is mapped to a process exit code in `crates/cubtera/src/error.rs`
(`exit_code_for`) and to an RFC 7807 `application/problem+json` HTTP response
in `crates/cubtera-api/src/error.rs` (`ApiError`). Nowhere else should you see
`std::process::exit`, `panic!`, or `.unwrap()` used as control flow -  v1 had
12 `exit()` call sites plus ~30 `exit_with_error` helper calls; v2 has zero
outside the CLI/API binaries' own `main`/handler edges.

```rust
// CORRECT
pub async fn get_by_name(&self, org: &str, dim_type: &str, name: &str) -> AppResult<Dimension> {
    let raw = self.inventory.get_raw(org, dim_type, name).await?
        .ok_or_else(|| AppError::not_found(format!("dimension not found: {dim_type}:{name}")))?;
    // ...
}

// WRONG (this was v1's access-denied pattern - never do this)
if !allowed {
    eprintln!("Access denied");
    std::process::exit(0);
}
```

Unit access policy (`allowList`/`denyList`/`affinityTags`) is a pure function,
`AccessPolicy::evaluate(...) -> AccessDecision`, in `cubtera-domain::access`.
`UnitService::build_unit` calls it and returns `AppError::AccessDenied` on
`Denied` - the CLI/API decide what to do with that (exit code 3 in the CLI,
403 in the API), the domain function itself never touches `stdout`/`exit`.

### Async discipline

All ports are `async_trait`. Adapters must not block the tokio reactor:
filesystem calls in `cubtera-persistence` go through `tokio::fs`, and process
execution in `cubtera-runners` goes through `tokio::process` **with
inherited stdio** (this preserves interactive prompts and terraform's color
output - dropping this would regress CLI UX relative to v1). If you must call
blocking code (e.g. some third-party blocking client), wrap it in
`tokio::task::spawn_blocking`.

### Composition root

Nothing is wired via global state. `App::new(...)` (or `AppBuilder`) in
`crates/cubtera-core/src/app.rs` takes every port implementation as an
argument and builds `DimensionService`/`UnitService`/`RunService`. Both the
CLI (`crates/cubtera/src/commands/run.rs`) and the API
(`crates/cubtera-api/src/server.rs`) construct their own `App` this way -
copy one of those two call sites when wiring a new entry point rather than
inventing a new construction path.

```rust
let app = App::new(
    repos.inventory,       // Arc<dyn InventoryRepository>
    hierarchy,              // DimHierarchy (dim_relations from Config)
    repos.units,            // Arc<dyn UnitRepository>
    runner_factory,         // Arc<dyn RunnerFactory>
    workspace,               // Arc<dyn Workspace>
    process,                 // Arc<dyn ProcessRunner>
    copy_config,             // CopyConfig (modules/plugins paths, always_copy_files, clean_cache)
    Some(repos.deployment_log), // Option<Arc<dyn DeploymentLogRepository>> - fs-jsonl unless [deploymentLog] is set
);
```

---

## Core concepts

### Dimension

A logical grouping for infrastructure, resolved through the configured
`dimRelations` chain (default `["dome", "env", "dc"]`):

```
dome:prod
  └── env:prod
      └── dc:prod-use1
```

Each assembled `Dimension` (in `cubtera-domain::dimension`) carries: `meta`
plus any other named sections, a resolved `parent` (`type:name`), the full
ancestor `key_path`, a `data_sha` (SHA-256 of the canonical JSON), and `kids`
(computed via `DimHierarchy` + `InventoryRepository::list_names`, not stored).

### Unit

An atomic IaC operation, described by `manifest.toml` (`cubtera-domain::Manifest`):
`dimensions`/`optDims` (required/optional dimension types), `allowList`/`denyList`/`affinityTags`
(access policy, `type:name` entries), `type` (runner type: `tf`/`tofu`/`bash`),
`spec.files`/`spec.envVars` (required/optional file and env-var mappings), and
`runner`/`state` overrides (arbitrary key-value maps, gap-filled by the org's
global `config.toml` `[runner.<type>]`/`[state.<backend>]`).

`spec.envVars` is parsed into the manifest schema but not yet wired into the
run pipeline - `Unit::materialize` only acts on `spec.files`. Don't add it to
example manifests as if it worked; wiring it up (host env var -> child
process env, with the same required/optional semantics as `spec.files`) is a
`cubtera-core`/`cubtera-domain` change, not yet done.

`allowList`/`denyList` entries **must** be `type:name` (e.g. `"dome:mgmt"`,
`"env:stg1"`), not a bare name - `AccessPolicy::evaluate` matches against the
full `key_path` set of the resolved dimensions, so a bare `"stg1"` will never
match and every run gets denied. See `example/units/*/manifest.toml` for
correct examples.

### Cross-unit state (`[inputs]`/`[outputs]`)

A unit can publish its outputs for other units to consume, without a DAG or
any auto-run of the producer:

- **Producer**: `[outputs] publish = true` - explicit opt-in, `false` by
  default. Publishing only fires when the command's first word is `apply`
  or `destroy` (`RunService::should_log_command` - same gate as the
  deployment log) *and* that run exited `0`; `init`/`plan`/anything else
  never publishes. When it fires, `RunService` calls
  `RunnerStrategy::collect_outputs` (for `tf`/`tofu`, this runs `<binary>
  output -json > cubtera_outputs.json` as a fresh process, after the run's
  own process has already exited), reads the file back via
  `Workspace::read_file`, normalizes it (`flatten_tf_outputs` for tf/tofu -
  `{name: {value, type, sensitive}}` -> `{name: value}`), and `put`s it into
  the configured `UnitStateRepository`, keyed by the producer's own
  `dimensions`/extensions (filtered to the manifest's *required* dims -
  optional dims/extra `-d` overrides passed on the CLI are not part of the
  key). Best-effort: a publish failure (or a missing file) only warns, it
  never fails the run. `RunnerStrategy::collect_outputs`/`normalize_outputs`
  default to a no-op/passthrough - `BashRunner`/`HelmRunner` don't
  auto-generate `cubtera_outputs.json`, so a bash/helm producer must write
  it itself (already flat `{name: value}` JSON), typically via
  `[runner] outlet_command = "..."` in its manifest; if `publish = true`
  but the file never shows up, you'll see a `... was not written` warning
  and nothing gets stored. `destroy` re-runs the same collection and
  *overwrites* (upserts) the existing record - it does not delete it; use
  `cubtera state rm` if you want the record gone after a teardown.
- **Consumer**: `[inputs.<alias>]` names a producer unit and, optionally,
  explicit `dims`/`ext`; left unset, `UnitService` projects the consumer's
  own resolved `dim_key_path` onto the producer's required `dimensions`
  (`cubtera_domain::project_state_key`) to find the exact key the producer
  published under. A producer dimension type the consumer never resolved,
  or an ambiguous match, is a hard error - never a guess. `required`
  (default `true`) controls whether a missing producer state fails the
  consumer's run. Resolved inputs are materialized as
  `cubtera_in_<alias>.json` (`{"in_<alias>": {...}}`) plus an aggregate
  `cubtera_inputs.json` (`{<alias>: {...}}`) in the consumer's temp folder;
  `BashRunner` additionally exposes each alias as a `CUBTERA_IN_<ALIAS>`
  env var (JSON-encoded), and `HelmRunner`'s `values.yaml.tpl` sees them for
  free since it merges every `cubtera_*.json` file in the temp folder.
  See `example/units/tf_unit02` (producer) and `example/units/bash_unit01`
  (consumer, `[inputs.infra]`) for a working example.
- Read-only inspection outside a run: `cubtera state get/ls/rm`, `GET
  /v1/{org}/units/{name}/state`, MCP `get_unit_state` - all read the exact
  `dims`/`ext` key you give them, not a consumer's projected key.

### MaterializationPlan

`Unit::materialize(modules_path, ext)` returns a `MaterializationPlan`: a
list of copy/symlink/write operations (unit files, `.default` includes,
`cubtera_dim_{type}.json`, `cubtera_ext.json`, module symlink, resolved
`[inputs]` as `cubtera_in_*.json`/`cubtera_inputs.json`) computed with
**zero I/O**. `Workspace::apply` (implemented by `FsWorkspace`) is the only
thing that actually touches disk. `cubtera run --dry-run` prints the
resolved inputs (if any) and the plan without applying it - use this to
debug unit wiring without touching a temp folder.

### Runner pipeline

`RunService::run` (in `cubtera-core::services::run`) owns the pipeline:

```
prepare (clean+materialize, or require existing temp folder)
  -> strategy.transform_files   (e.g. cubtera_*.json -> *.auto.tfvars.json)
  -> inlet hook                  (manifest.runner.inlet_command)
  -> strategy.execute            (binary + args + env via ProcessRunner)
  -> outlet hook                 (manifest.runner.outlet_command)
  -> deployment log              (only for apply/destroy; best-effort, never fails the run)
  -> publish [outputs]           (only for apply/destroy, only if publish = true; best-effort)
  -> optional cache cleanup
```

`RunnerStrategy` (implemented by `TerraformRunner`/`OpenTofuRunner`/`BashRunner`/`HelmRunner`)
only expresses *differences*: which binary to resolve, how to build args/env,
what file transforms are needed, and (for tf/tofu) that `prepare_mode` must
require an existing temp folder unless the command is `init`. Every other
pipeline step is implemented once, in `RunService`, so a new runner type only
has to implement `RunnerStrategy::binary` (everything else has a sane
default). This is the direct fix for v1's `Runner` god-trait, whose default
methods did I/O and pipeline orchestration inside what should have been a
strategy interface.

---

## Inventory on-disk format

This is the one v1 contract that must not break - it's user data, not an
interface, and it's pinned by golden tests
(`crates/cubtera-persistence/tests/golden_inventory.rs`) against
`example/inventory`:

- `{type}/{name}.json` or `{type}/{name}{sep}meta.json` → the dimension's `meta` section (`sep` = `fileNameSeparator`, default `:`).
- `{type}/{name}{sep}{section}.json` → an additional named section (e.g. `service/admin:manifest.json` → section `manifest` of `service:admin`).
- `{type}/.default{sep}meta.json`, `.default{sep}{section}.json` → per-type defaults, merged **gap-fill** style (the dimension's own data wins; nested objects merge recursively) - see `gap_fill_merge`.
- `{type}/.schema{sep}meta.json` → a JSON Schema that validates the type's `meta` section (used by `cubtera im validate` and `DimensionService::validate_schema`).
- `{type}/{name}{sep}{file}` (non-`.json` extension) → an include file, copied into the unit's temp folder as `{file}`; a trailing `/` after the name marks an include directory. Same rule applies under `.default{sep}...`.
- Names with a leading `.` (other than `.default`/`.schema`) are excluded from listings. A leading `#` is reserved/ignored.
- `meta.parent = "{type}:{name}"` drives the parent chain; `key_path` is the `type:name` chain from the root down to the dimension itself.
- `data_sha` = SHA-256 of the canonically-ordered JSON of all sections.

If you need to change this convention, it's a one-adapter change
(`crates/cubtera-persistence/src/fs/dimension.rs`) plus updated golden tests -
that's the entire point of putting this behind `InventoryRepository`.

---

## CLI commands

```bash
# Show effective configuration (add --json for machine-readable output)
cubtera config

# Inventory management
cubtera im get-types <org>
cubtera im get-all <dim_type>
cubtera im get <dim_type> <name>
cubtera im get-defaults <dim_type>
cubtera im get-schema <dim_type>
cubtera im get-parent <dim_type> <name>
cubtera im get-children <dim_type> <name>
cubtera im validate <dim_type> <name>   # existence check + JSON Schema validation
cubtera im sync-defaults <dim_type>            # FS -> MongoDB (requires CUBTERA_DB)
cubtera im sync-all <dim_type>                 # FS -> MongoDB, every dimension of that type
cubtera im sync <dim_type> <name>              # FS -> MongoDB, a single dimension

# Run a unit
cubtera run -u <unit> -d <type:name> [-d <type:name>...] [-e <type:name>] [--dry-run] [--auto-approve] -- <command...>

# Deployment log
cubtera log get -q <key:value> [-q ...] [--limit N]

# Unit state (cross-unit outputs) - exact dims/ext key, not a consumer's projection
cubtera state get -u <unit> [-d <type:name>...] [-e <type:name>...]
cubtera state ls -u <unit>
cubtera state rm -u <unit> [-d <type:name>...] [-e <type:name>...]
```

`im sync*` always reads from the FS inventory at `inventoryPath` and writes
to MongoDB (`CUBTERA_DB`), independent of which backend `run`/`im get*` are
currently pointed at - this mirrors v1's `syncDefaults`/`syncAll`/`sync`,
minus `deleteContext` (deliberately not ported; see "What's gone for good"
in the migration guide).

Global flags: `--config <path>`, `--log-level <level>`, `--json` (machine-readable
output where supported). Exit codes are mapped from `AppError` in
`crates/cubtera/src/error.rs` (`EXIT_GENERAL_ERROR=1`, `EXIT_ACCESS_DENIED=3`,
`EXIT_NOT_FOUND=4`, `EXIT_VALIDATION=5`, `EXIT_CONFIG=6`; a runner's own exit
code, if the pipeline reached execution, is propagated as-is instead of being
remapped).

## REST API

See [`.github/docs/api.md`](.github/docs/api.md) for the full reference.
Summary: every `/v1/*` route requires an `x-api-key` header matching
`CUBTERA_API_KEY` (unset = unauthenticated, with a startup warning); errors
are `application/problem+json`; there is no `{status, id, data}` envelope.

```
GET /health
GET /v1/orgs
GET /v1/{org}/dim-types
GET /v1/{org}/dims/{dim_type}
GET /v1/{org}/dims/{dim_type}/defaults
GET /v1/{org}/dims/{dim_type}/schema
GET /v1/{org}/dims/{dim_type}/{name}
GET /v1/{org}/dims/{dim_type}/{name}/parent
GET /v1/{org}/dims/{dim_type}/{name}/children
GET /v1/{org}/units
GET /v1/{org}/units/{name}
GET /v1/{org}/units/{name}/state?dims=<type:name>[,...]&ext=<type:name>[,...]
GET /v1/{org}/dlog?q=<key:value>[,<key:value>...]&limit=<N>
```

## MCP server

`cubtera-mcp` is a proper MCP server (via the [`rmcp`](https://docs.rs/rmcp)
SDK, stdio transport) - not the REST-shaped prototype `test1` had. It exposes
the same read-only queries as the REST API (`list_orgs`, `list_dim_types`,
`list_dimension_names`, `get_dimension`, `get_dimension_defaults`,
`get_dimension_schema`, `get_dimension_parent`, `get_dimension_children`,
`validate_dimension`, `list_units`, `get_unit_manifest`, `get_unit_state`,
`get_deployment_log`) as MCP tools, through `DimensionService`/`UnitService`/
`DeploymentLogRepository`/`UnitStateRepository` directly (no `App`/
`RunService` - it never runs infrastructure commands, deliberately: giving
an MCP client the ability to apply changes is a product decision this
migration doesn't make for you).

```bash
cubtera-mcp --config example/config.toml   # speaks MCP over stdio
```

`AppError` maps to `rmcp::ErrorData` in `crates/cubtera-mcp/src/error.rs`
(`to_mcp_error`), the same boundary-mapping pattern as the CLI's exit codes
and the API's `problem+json`.

## Configuration

`config.toml` has a `[default]` table plus one table per org (matched by
name), whose fields override `[default]` for that org; `[runner.<type>]` and
`[state.<backend>]` are maps of arbitrary keys merged entry-by-entry (not
whole-table replacement). Keys accept camelCase or snake_case. See
`example/config.toml` for a fully-annotated example and
`crates/cubtera-config/src/config.rs` for the loader
(`ConfigProvider`/`ConfigSource` - injectable so tests don't touch real env vars).

`unitStatePath` (default `~/.cubtera/state`) is the fs-json
`UnitStateRepository` root, used unless `[unitState]` (a `MongoUnitStateRepository`
connection string/database/collection, same shape as `[deploymentLog]`) is
set - independent of which backend `run`/`im get*` are pointed at, same as
`deploymentLogPath`/`[deploymentLog]`.

---

## Development commands

```bash
# Build / test / lint the whole workspace (this is what CI runs)
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check

# Run one crate's tests only
cargo test -p cubtera-domain
cargo test -p cubtera-persistence --test golden_inventory

# MongoDB contract tests (inventory + deployment log) need a real server;
# they skip themselves (without failing) if this isn't set
CUBTERA_TEST_MONGO_URL=mongodb://127.0.0.1:27017 \
  cargo test -p cubtera-persistence --features mongodb \
  --test inventory_contract_mongo --test deployment_log_contract_mongo \
  --test unit_state_contract_mongo

# Run the CLI / API / MCP server against the example fixture
cargo run -p cubtera -- -c example/config.toml im get-all dc
CUBTERA_CONFIG=example/config.toml cargo run -p cubtera-api
cargo run -p cubtera-mcp -- -c example/config.toml
```

The `cubtera`/`cubtera-api` binaries always compile in the `mongodb` feature
of `cubtera-persistence` (it's a hard `Cargo.toml` dependency feature, not a
cargo feature of their own) - `cargo build --workspace` alone is enough to
build Mongo support in, `CUBTERA_DB=mongodb://... cubtera ...` switches
`InventoryRepository` to it at runtime, and setting `[deploymentLog]`/
`[unitState]` in `config.toml` does the same for `DeploymentLogRepository`/
`UnitStateRepository`.

---

## Rust conventions

### Module file structure (Rust 2018+ style)

Use `module_name.rs` alongside a `module_name/` directory instead of
`module_name/mod.rs`:

```
# CORRECT
src/
├── lib.rs
├── terraform.rs        # `mod runner; mod switch; pub use runner::TerraformRunner;`
└── terraform/
    ├── runner.rs
    └── switch.rs

# AVOID
src/
└── terraform/
    ├── mod.rs
    ├── runner.rs
    └── switch.rs
```

### Testing conventions

- Domain logic (`cubtera-domain`) is pure - test it with plain `#[test]`, no
  fixtures or async runtime needed.
- Adapter behavior against the real inventory fixture goes in
  `crates/cubtera-persistence/tests/golden_inventory.rs` - if you touch the
  FS naming convention, add or update a golden test there.
- Service-level logic that needs a fake port (e.g. `RunService`,
  `DimensionService`) uses in-file `#[cfg(test)] mod tests` with hand-rolled
  fakes (see `FakeWorkspace`/`FakeProcess`/`FakeFactory` in
  `crates/cubtera-core/src/services/run.rs`) rather than a mocking framework.
- CLI end-to-end behavior against `example/` uses `assert_cmd` (see
  `crates/cubtera/tests/`).

## Notes for AI agents

1. **Follow the dependency rule** - `cubtera-domain` has no I/O, no `tokio`, no `async_trait`.
2. **Everything external goes through a port** - if you're adding a new backend, add/extend a trait in `cubtera-core::ports` first, implement it in an adapter crate, wire it in `App::new` (Mongo and Helm are done this way already - `MongoInventoryRepository`/`MongoDeploymentLogRepository`, `HelmRunner`).
3. **No global state, no `exit()`/`panic!` outside CLI/API edges** - return `AppResult<T>` and let `crates/cubtera/src/error.rs` / `crates/cubtera-api/src/error.rs` translate it.
4. **`v1/` is reference-only** - read it to understand intended behavior, never build against it or copy code from it verbatim (its architecture is exactly what this rewrite is undoing).
5. **The inventory naming convention is a contract** - changes need a golden test update in the same PR.
6. **`allowList`/`denyList` entries are `type:name`**, always - a common mistake when writing example manifests.
7. **English only in code** - comments, docs, identifiers.
8. **Rust 2018+ module style** - `module.rs` + `module/`, not `module/mod.rs`.
9. **Runner pipeline** - add a new runner type by implementing `RunnerStrategy`, not by touching `RunService`.
10. **New MCP tools go in `crates/cubtera-mcp/src/server.rs`** as `#[tool]` methods on `CubteraMcp`, calling `DimensionService`/`UnitService`/`DeploymentLogRepository`/`UnitStateRepository` - never add a `run`/write tool there without an explicit, separate decision to do so.
11. **`[inputs.<alias>]`/`[outputs]` are not a DAG** - a consumer's `[inputs]` never auto-runs the producer, and `project_state_key` never invents a dimension the consumer didn't resolve or guesses on ambiguity - both are hard `AppError`s, not silent fallbacks. Keep it that way when touching `cubtera-domain::unit_state` or `UnitService::resolve_inputs`.
