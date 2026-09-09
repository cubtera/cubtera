# Cubtera V3 - AI Agent Context Guide

## Project Overview

**Cubtera** is an instance-centric Infrastructure Manager: a CLI, a
long-running server, and an MCP server for running Terraform, OpenTofu,
Bash, or Helm against the same unit code across different "dimensions"
(data centers, environments, "domes", services, ...) - with a reviewable
`plan`/`apply` gate, a durable `Run`/`Plan`/`Instance` ledger (SQLite), a
cross-unit output mesh with staleness tracking, and desired-state/drift
reporting over selector expressions.

This is v3: a second rewrite on top of the (now-deleted) v2 hexagonal
codebase. v1 is gone entirely - there is no `v1/` directory left in this
repo. The full target design lives in
[`docs/specs/2026-09-03-cubtera-v3-architecture.md`](docs/specs/2026-09-03-cubtera-v3-architecture.md);
treat it as intent, not as ground truth for what's implemented today -
several things below are simplifications or deviations from that spec
(noted inline). **The one thing that must not break** is the on-disk
inventory format under `example/inventory` - see
[Inventory on-disk format](#inventory-on-disk-format).

If you're migrating a pre-v3 (`deploymentLogPath`/`unitStatePath`/
`[deploymentLog]`/`[unitState]`/`CUBTERA_DB`) `config.toml`/data directory,
run `cubtera migrate` - see [Migration](#migration-cubtera-migrate) below.

### What's actually built (P0-P7, all done)

- **P0-P2: kernel + model.** `cubtera-kernel` (zero-I/O `Ident`/`DimRef`/
  `InstanceId`/`Digest`/`SafeSegment` - the only place path-traversal-safe
  validation lives) and `cubtera-model` (pure domain types: `Dimension`,
  `Unit`, `Manifest`, `Binding`/`Selector`, `Policy`, `Plan`, `Run`,
  `OutputSet`, `MaterializationPlan`, `DimGraph`).
- **P3: read-only fleet visibility.** `cubtera-app`'s `ResolveUseCase`/
  `AssembleUseCase`/`ValidateUseCase` over `cubtera-inventory`'s
  `FsInventoryPort`/`FsUnitPort`; `cubtera validate`, `cubtera fleet ls`.
- **P4: plan/apply/explain.** `cubtera-exec`'s `RunnerStrategy` impls
  (`TfLikeRunner` for tf/tofu, `BashRunner`, `HelmRunner`) + `cubtera-store`'s
  `SqliteStore` (`Instance`/`Plan`/`Run`/lease tables); `RunUseCase::plan`/
  `apply`/`explain`, `cubtera plan`/`apply`/`explain run`.
- **P5: desired state + drift.** `Binding`/`Selector` over the inventory,
  `BindingUseCase::expand`/`status`; `cubtera fleet status`, `cubtera drift`
  (CI-facing, exits `EXIT_DRIFT_DETECTED` on real drift).
- **P6: cross-unit output mesh.** `OutputSet`/`OutputValue::{Plain,Secret}`,
  revision-stamped, staleness-tracked (`Store::list_stale_consumers`);
  `[inputs]`/`[outputs]` in `manifest.toml`, materialized as
  `cubtera_in_<alias>.json`/`cubtera_inputs.json` (same on-disk shape as v2).
- **P7: server + retirement + hardening.** `cubtera-server` (Axum, one
  binary that both reads inventory *and* runs `plan`/`apply`, unlike v2's
  split `cubtera-api`), async `apply` + SSE log streaming (`LogHub`), the
  legacy `cubtera-domain`/`cubtera-core`/`cubtera-persistence`/
  `cubtera-runners`/`cubtera-api` crates deleted, `cubtera-mcp` turned into
  a pure HTTP client of `cubtera-server`, `Ident::parse` validation added at
  every use-case entry point (closing a path-traversal hole), `cubtera
  migrate`, and an adversarial test suite
  (`crates/cubtera-inventory/tests/adversarial.rs`).

---

## Architecture

### Project structure

```
cubtera/
├── Cargo.toml                # workspace root, single version across all members
├── crates/
│   │
│   │ ─────────── KERNEL LAYER (zero I/O, zero business logic) ───────────
│   ├── cubtera-kernel/        # Ident, DimRef, InstanceId, Digest, SafeSegment, KernelError
│   │
│   │ ─────────── MODEL LAYER (pure domain types, zero I/O) ───────────
│   ├── cubtera-model/
│   │   └── src/
│   │       ├── dimension.rs        # Dimension::assemble (provenance-aware gap-fill + parent chain + content hash)
│   │       ├── unit.rs             # Unit entity: temp folder, materialize(), IncludeEntry
│   │       ├── manifest.rs         # Manifest (TOML schema), RunnerType, Spec, InputSpec/OutputsSpec
│   │       ├── access.rs           # legacy AccessPolicy::evaluate (allowList/denyList/affinityTags) - still used by AssembleUseCase
│   │       ├── policy.rs           # unified Policy/PolicyRule/PolicyDecision over Selector - used by cubtera-server's authz gate
│   │       ├── binding.rs          # Binding + Selector AST (desired-state selector grammar)
│   │       ├── dim_graph.rs        # DimGraph/DimEdge/DimTypeDef/SchemaSpec - typed inventory graph, replaces hardcoded dimRelations checks
│   │       ├── materialization.rs  # MaterializationPlan/MaterializationStep (copy/symlink/write), --dry-run printing
│   │       ├── plan.rs             # Plan, ResolutionManifest (the pin set a plan freezes and apply re-checks)
│   │       ├── run.rs              # Run, RunOp, RunStatus, RunPatch, RunFilter
│   │       ├── output_set.rs       # OutputSet, OutputValue::{Plain,Secret(SecretRef)}, StaleConsumer
│   │       ├── state_projection.rs # project_state_key() for [inputs] dim projection (cross-unit state mesh)
│   │       ├── unit_package.rs     # content-addressed UnitPackage, PinnedModule
│   │       ├── provenance.rs       # gap_fill_merge_with_provenance, FieldProvenance, ProvenanceSource
│   │       ├── revision.rs         # monotonic Revision(u64)
│   │       ├── lease.rs            # Lease (mutual-exclusion, fencing token)
│   │       ├── ids.rs              # opaque RunId/PlanId newtypes
│   │       ├── instance.rs         # durable Instance record (what Store persists)
│   │       └── error.rs            # ModelError
│   │
│   │ ─────────── APPLICATION LAYER ───────────
│   ├── cubtera-app/           # Ports (traits) + use cases (no "services"/"App" composition-root class - see below)
│   │   └── src/
│   │       ├── ports.rs        # InventoryPort, UnitPort, Executor, Clock, IdentityProvider, LogSink
│   │       ├── resolve.rs      # ResolveUseCase - Dimension resolution/schema/parent/children/defaults
│   │       ├── assemble.rs     # AssembleUseCase - builds a cubtera_model::Unit from inventory + manifest + AccessPolicy
│   │       ├── validate.rs     # ValidateUseCase - schema + dim-graph validation across the whole fleet
│   │       ├── run.rs          # RunUseCase - plan/apply/apply_direct/explain, the runner pipeline, output publish
│   │       ├── binding.rs      # BindingUseCase - Binding expansion + drift diff against Store
│   │       ├── dim_graph_loader.rs # load_dim_graph() - builds a DimGraph from an InventoryPort
│   │       └── error.rs        # AppError + AppResult, mapped to exit codes/HTTP status at the edges
│   │
│   │ ─────────── INFRASTRUCTURE LAYER ───────────
│   ├── cubtera-inventory/     # FsInventoryPort + FsUnitPort - the only InventoryPort/UnitPort impls that exist (no Mongo in v3)
│   ├── cubtera-store/         # SqliteStore (rusqlite) implementing the Store trait; also legacy dlog/unit-state tables (see below)
│   ├── cubtera-exec/          # RunnerStrategy impls + process execution
│   │   └── src/
│   │       ├── runner.rs      # RunnerStrategy trait, RunnerCapabilities, RunnerContext
│   │       ├── tf_like.rs     # TfLikeRunner - covers both tf and tofu (prepare() writes *.auto.tfvars.json + cubtera_vars.tf, collects `<bin> output -json`)
│   │       ├── bash.rs        # BashRunner - executes the single *.sh file in the temp folder, exposes CUBTERA_IN_<ALIAS> env vars
│   │       ├── helm.rs        # HelmRunner - renders values.yaml.tpl (handlebars) from every cubtera_*.json, then `helm ...`
│   │       ├── process.rs     # TokioProcessRunner/TokioCapturingProcessRunner, ChunkSink-based streaming capture
│   │       ├── workspace.rs   # Workspace/RootedPath - a path-safe handle on a unit's temp folder
│   │       └── materialize.rs # apply(&MaterializationPlan) - the only thing that actually touches disk for materialization
│   ├── cubtera-identity/      # IdentityProvider port + EnvIdentityProvider (env:/literal: secret refs)
│   ├── cubtera-source/        # SourceRepo port + GitSource/FsSource (module pinning/resolution)
│   ├── cubtera-config/        # Config, ConfigProvider/ConfigSource (injectable path+env sources) - the one v2-era crate kept as-is
│   │
│   │ ─────────── INTERFACE LAYER ───────────
│   ├── cubtera/                # CLI (binary: `cubtera`) - commands/{config,im,run,log,state,validate,fleet,plan,apply,explain,drift,migrate}.rs
│   ├── cubtera-server/         # binary: `cubtera-server` (Axum) - reads inventory AND runs plan/apply (v2's cubtera-api only did the former, and is gone)
│   └── cubtera-mcp/            # MCP server (binary: `cubtera-mcp`), rmcp SDK, stdio transport - a pure HTTP client of cubtera-server (P7)
│
├── docs/specs/                 # architecture spec(s) - design intent, read for "why", not "what's implemented"
└── example/                     # fixtures used by golden tests, e2e tests, and local dev (config.toml, inventory/, units/)
```

A web UI is **not implemented** - don't create files for it unless you're
explicitly starting that work. `v1/` and the v2-era `cubtera-domain`/
`cubtera-core`/`cubtera-persistence`/`cubtera-runners`/`cubtera-api` crates
have all been **deleted** - don't reference them, don't resurrect their
names for new code, and don't expect them in `Cargo.toml` workspace members.

### Dependency rule

Dependencies still point inward, enforced by `Cargo.toml`:

- `cubtera-kernel` depends on nothing but `std`/`sha2`/`serde` - **no
  `tokio`, no filesystem, no process spawning.**
- `cubtera-model` depends on `cubtera-kernel` + `serde_json`/`jsonschema`/
  `semver` - still zero I/O, zero async.
- `cubtera-app` depends on `cubtera-kernel` + `cubtera-model` only. Ports in
  `ports.rs` use plain `async fn` in traits (`#[allow(async_fn_in_trait)]`,
  not `#[async_trait]` - see `cubtera_exec::process::CapturingProcessRunner`
  for why: `dyn Fn` lifetimes across `#[async_trait]`'s boxed futures were
  the actual problem it avoids).
- `cubtera-inventory` / `cubtera-store` / `cubtera-exec` / `cubtera-identity`
  / `cubtera-source` / `cubtera-config` depend on `cubtera-app` +
  `cubtera-model` + `cubtera-kernel` and implement the ports (or, for
  `cubtera-store`, define the `Store` port *and* its own SQLite
  implementation in the same crate - there's no separate "port crate" for
  storage).
- `cubtera` (CLI) / `cubtera-server` depend on everything and wire it
  together per-command/per-request (there is **no composition-root
  `App`/`AppBuilder` struct** in v3 - see [Composition](#composition)).
- `cubtera-mcp` depends on **none** of the above `cubtera-*` crates except
  transitively through `reqwest` - it's a pure HTTP client of
  `cubtera-server`'s REST API (`crates/cubtera-mcp/src/client.rs`). This is
  a deliberate P7 change from v2, where `cubtera-mcp` linked
  `cubtera-core`/`cubtera-persistence` directly.

```rust
// CORRECT: adapter implements an app port
impl InventoryPort for FsInventoryPort { /* ... */ }

// WRONG: kernel/model reaching into infrastructure or async runtimes
use tokio::fs; // never in cubtera-kernel or cubtera-model
```

### Ports and adapters actually in the tree

| Port (trait, in `cubtera-app::ports`) | Adapter(s) |
| --- | --- |
| `InventoryPort` | `FsInventoryPort` (`cubtera-inventory`) - the only impl, no Mongo in v3 |
| `UnitPort` | `FsUnitPort` (`cubtera-inventory`) |
| `Executor` | `ExecutorBridge` (CLI, `crates/cubtera/src/exec_bridge.rs`), `ServerExecutor` (server) - both wrap `cubtera-exec`'s `RunnerStrategy` impls behind the port |
| `IdentityProvider` | `EnvIdentityProvider` (`cubtera-identity`) |
| `Clock` | `SystemClock` (default impl in `cubtera-app`) |
| `Store` (in `cubtera-store`, not `cubtera-app::ports`) | `SqliteStore` - the only impl |

`InventoryPort` is deliberately "dumb": `get_raw`/`get_raw_defaults`/
`get_raw_schema`/`list_names`/`list_types`/`list_orgs`/`list_includes`/
`list_default_includes` return raw records with **zero** business logic -
no defaults gap-fill, no parent resolution. All of that lives in
`ResolveUseCase`/domain functions in `cubtera-model::dimension`, exactly the
same "keep adapters dumb" principle v2 had - it just doesn't need a second
(Mongo) adapter to prove the point anymore, since v3 dropped Mongo
entirely (`InventoryPort`/`Store` are both FS/SQLite-only; there is no
`--features mongodb` anywhere in this workspace).

**`FsInventoryPort` and `FsUnitPort` have zero built-in path-traversal
protection on their own** - they'll happily read `../../../etc/passwd` if
you hand them a raw string with `..` in it. The validation choke point is
one layer up: every public `ResolveUseCase`/`AssembleUseCase` method calls
`Ident::parse(org)?` (and `Ident::parse(unit_name)?` where relevant) *before*
touching the port. See `crates/cubtera-inventory/tests/adversarial.rs` for
the test suite that specifically proves this (both the port's raw
vulnerability and the use case's fix).

### Result-based error handling, no `exit()`/`panic!` in app/infra

`AppError` (`cubtera-app::error`) is the only error type use cases return.
It's mapped to a CLI exit code in `crates/cubtera/src/error.rs`
(`exit_code_for`) and to an RFC 7807 `application/problem+json` HTTP
response in `crates/cubtera-server/src/error.rs` (`ApiError`). Nowhere else
should you see `std::process::exit`, `panic!`, or `.unwrap()` used as
control flow outside the CLI/server binaries' own edges.

```rust
// CORRECT
pub async fn resolve(&self, org: &str, dim_type: &str, name: &str) -> AppResult<Dimension> {
    let org = Ident::parse(org)?; // choke point - closes path traversal
    let raw = self.inventory.get_raw(org.as_str(), dim_type, name).await?
        .ok_or_else(|| AppError::not_found(format!("dimension not found: {dim_type}:{name}")))?;
    // ...
}
```

Unit access control is a **legacy `AccessPolicy::evaluate`** (allowList/
denyList/affinityTags, `cubtera-model::access`) inside `AssembleUseCase`,
plus a **separate, newer `Policy::evaluate`** (`cubtera-model::policy`,
`Selector`-based) that `cubtera-server`'s `apply` route runs on top of it
(`crate::policy::check`, compiling the same manifest allow/deny lists into
`Selector`s via `Policy::from_allow_deny_lists` and adding pseudo-dimensions
`actor.name`/`op.name` to the evaluation context). **These two policy
checks are not unified yet** - the CLI's `apply_direct` path only runs the
`AccessPolicy` check inside `AssembleUseCase::build_unit_with_extensions`;
the server's `apply` route runs both. Don't assume parity between CLI and
server authz without checking both call sites if you touch this.

### Async discipline

Ports use `async fn` directly in the trait (not boxed via `#[async_trait]`)
so that a `dyn Fn(&[u8]) + Send + Sync` sink type (`LogSink`) can be passed
through without lifetime gymnastics - see `cubtera_app::ports::LogSink` and
`cubtera_exec::process::ChunkSink`. Filesystem calls go through `tokio::fs`
(materialization) or `rusqlite` behind `tokio::task::spawn_blocking` (the
store - `rusqlite` itself is synchronous). Process execution goes through
`tokio::process` **with inherited stdio for interactive commands** and a
separate `CapturingProcessRunner::exec_captured_streaming` path (mpsc
channel + spawned reader tasks) when a run needs its output captured *and*
live-streamed at the same time (`cubtera apply`/`cubtera-server`'s `apply`
route) - this dual mode is a deliberate v3 addition to preserve v2's CLI
UX (colored, prompt-capable output) while also supporting SSE tailing.

### Composition (no `App`/`AppBuilder` struct)

Unlike v2, there is **no single composition-root type** that wires every
port and hands back pre-built use cases. Each CLI command and each server
route builds exactly the ports/use cases it needs, inline, via shared
helpers:

- CLI: `crates/cubtera/src/commands/run_support.rs` - `inventory_port(config)`,
  `unit_port(config)`, `build_unit(...)`, `prepare(...)` (resolves +
  builds the `Unit` + wires `ExecutorBridge` + constructs `RunUseCase`),
  `build_binding_use_case(config)`, `build_use_case(config, temp_root)`,
  `config_digest(config)`, `default_actor()`. Every command module
  (`plan.rs`, `apply.rs`, `run.rs`, `fleet.rs`, `drift.rs`, `explain.rs`)
  calls into these rather than constructing ports itself.
- Server: `crates/cubtera-server/src/run_support.rs` mirrors the CLI
  version (same shape, `AppState`/`Config`-driven instead of CLI-arg-driven);
  `crates/cubtera-server/src/exec_bridge.rs`'s `ServerExecutor` is the
  server-side `Executor` port impl (registers `HelmRunner`/`BashRunner`/
  `TfLikeRunner` the same way `ExecutorBridge` does for the CLI).

If you're wiring a new entry point, copy one of those two `run_support.rs`
files rather than inventing a new construction path or trying to resurrect
an `App::new(...)`-style constructor.

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

`Dimension::assemble` (`cubtera-model::dimension`) builds each dimension
from raw inventory records + `.default` gap-fill + parent chain, tracking
**field-level provenance** (`FieldProvenance`/`ProvenanceSource` -
`provenance.rs`) - a v3 addition over v2's plain gap-fill, so callers can
tell whether a field came from the dimension's own record, a `.default`,
or an ancestor. Each assembled `Dimension` carries `key` (its own
`type:name`), `key_path` (root-to-self ancestor chain), `parent_ref`, and
`content_hash` (a `Digest`, replacing v2's raw SHA-256 hex string).

### DimGraph

`cubtera-model::dim_graph::DimGraph` is a typed graph over dimension types
(`DimTypeDef`/`DimEdge`) built by `cubtera_app::load_dim_graph` from
`InventoryPort::list_types` + each type's schema/defaults. `cubtera
validate` calls `graph.validate()` to catch structural problems (duplicate
edges, cycles) *before* checking any individual dimension's data - this is
new in v3; v2 never validated more than one dimension's schema at a time.

### Unit

An atomic IaC operation, described by `manifest.toml` (`cubtera-model::
Manifest`) - **the on-disk filename and schema are unchanged from v2**:
`dimensions`/`optDims` (required/optional dimension types), `allowList`/
`denyList`/`affinityTags` (legacy access policy, `type:name` entries),
`type` (runner type: `tf`/`tofu`/`bash`/`helm`), `spec.files`/`spec.envVars`
(required/optional file and env-var mappings - `envVars` still parsed but
not wired into the run pipeline, same caveat as v2), `runner`/`state`
overrides, and `[inputs.<alias>]`/`[outputs]` for the cross-unit mesh. See
`example/units/tf_unit02/manifest.toml` (producer) and
`example/units/bash_unit01/manifest.toml` (consumer) for real, working
examples with `[outputs]`/`[inputs]`.

`allowList`/`denyList` entries **must** be `type:name` (e.g. `"dome:mgmt"`,
`"env:stg1"`), never a bare name - same v2 rule, same failure mode
(silent deny) if you get it wrong.

### RunnerStrategy and capabilities

`RunnerStrategy` (`cubtera-exec::runner`) replaces v2's `RunnerFactory`
pattern with a flatter trait: `name()`, `capabilities()`,
`binary(ctx)`, `prepare(ctx)` (default no-op), `build_args(ctx)`,
`env_vars(ctx)`, `collect_outputs(ctx, process)` (default no-op),
`normalize_outputs(raw)` (default passthrough). `RunnerCapabilities` is a
new v3 concept with **no v2 equivalent**: `supports_plan_artifact` (only
`tf`/`tofu`), `collects_outputs` (only `tf`/`tofu` - `bash`/`helm` never
auto-generate `cubtera_outputs.json`), `pins_version` (only `tf`, via
tfswitch-equivalent version resolution - `tofu` doesn't force a pin),
`needs_identity` (none of the current runners set this). `cubtera validate`
statically cross-checks `[outputs] publish = true` against
`collects_outputs` - a misconfigured bash/helm producer is now a
**validation error**, not a silent no-op discovered only after `apply`
(exactly the OpenTofu-shaped trap the v3 spec calls out).

### Plan / Apply / the pin-drift gate

This is the biggest behavioral addition over v2, only available for
`tf`/`tofu` (bash/helm have no plan concept - `RunUseCase::plan` rejects
them):

```
cubtera plan -u <unit> -d <type:name>... [--ttl-seconds N]
  -> RunUseCase::plan: resolves the Unit, builds a ResolutionManifest
     (package/module/inventory/config digests + runner version + consumed
     input revisions), runs the runner's plan-equivalent command, persists
     a Plan row (Store::put_plan) with an artifact_digest + expiry
  -> prints the Plan id

cubtera apply --plan <plan_id> -u <unit> -d <type:name>...
  -> RunUseCase::apply: loads the Plan, re-computes the current
     ResolutionManifest, calls Plan::pins_match(current) - any drift
     (different module digest, different inventory revision, expired TTL)
     is a hard error, not a warning
  -> only if pins match: acquires a Lease on the InstanceId (mutual
     exclusion, fencing token via Store::acquire_lease), runs the real
     apply, releases the lease, persists the Run row, publishes
     [outputs] if the manifest opts in
```

`cubtera run` (see below) bypasses this gate entirely via
`RunUseCase::apply_direct` - it's the "just run it" escape hatch v2's
`cubtera run` always was, kept specifically because bash/helm units have
no plan artifact to gate on and because forcing every unit through
`plan`+`apply` would be a UX regression for units nobody wants reviewed.

### Async apply + live log streaming (P7)

`cubtera-server`'s `apply` route no longer blocks on the run: it calls
`RunUseCase::queue_apply`/`queue_apply_direct` (synchronously inserts a
`Run` row in `Queued` status and returns it - HTTP 200 with the row, not
202, despite the "queues and returns immediately" framing in the route's
own doc comment), registers a broadcast channel in `crate::log_hub::LogHub`
keyed by the `Run` id, and `tokio::spawn`s `RunUseCase::run_and_finish` in
the background, which streams every process output chunk into that
channel as it arrives *and* buffers it for persistence. `GET
/v1/{org}/runs/{run_id}/log/stream` (SSE) subscribes to the live channel
if the run is still in flight, or falls back to replaying the finished
run's stored artifact (`Run::logs_ref` → `Store::get_artifact`) as a
single frame if it isn't (already finished, or this server process
restarted mid-run and lost the in-memory `LogHub` entry). The CLI's
`cubtera run`/`apply` don't need this - they inherit stdio directly and
print to the terminal in real time already; SSE streaming exists purely
for server clients (a UI, `curl`, etc.) that can't attach to a CLI's stdout.

### Cross-unit output mesh (`[inputs]`/`[outputs]`, `OutputSet`)

Producers opt in with `[outputs] publish = true` (`false` by default).
Publishing fires only on a successful `apply`/`destroy` (`RunOp::
publishes()`), never `plan`/`init`/anything else. `RunUseCase` then calls
`RunnerStrategy::collect_outputs`, reads the result back, normalizes it
(`normalize_outputs` - flattens tf/tofu's `{name: {value, type,
sensitive}}` shape to `{name: value}`, wrapping sensitive values as
`OutputValue::Secret(SecretRef(...))` rather than inlining them), and
`Store::put_output_set`s it - **revision-stamped** (`Revision(u64)`, unlike
v2's unversioned overwrite-in-place unit state). Best-effort: a publish
failure only warns, never fails the run.

Consumers declare `[inputs.<alias>]` (producer unit name, optional explicit
`dims`/`ext`, `required` default `true`); left unset, `dims`/`ext` are
projected from the consumer's own resolved chain onto the producer's
required dimensions (`cubtera_model::state_projection::project_state_key` -
still deliberately not a DAG, no auto-run of the producer, no fan-out
fallback, same hard-error-on-ambiguity semantics as v2). Resolved inputs
materialize as `cubtera_in_<alias>.json` (`{"in_<alias>": {...}}`) plus an
aggregate `cubtera_inputs.json`, exactly like v2's on-disk shape;
`BashRunner` also exposes `CUBTERA_IN_<ALIAS>` env vars, and `HelmRunner`'s
`values.yaml.tpl` sees them for free.

`Store::mark_consumed`/`list_stale_consumers` track, per (consumer,
producer) pair, which `Revision` a consumer last read vs. the producer's
current `Revision` - surfaced by `cubtera state ls --stale` (org-wide, not
scoped to one unit) and `GET /v1/{org}/state/stale`.

### Binding / Selector / drift

`Binding` (`cubtera-model::binding`) = a unit name + a `Selector` boolean
expression over the inventory (`<dim_type>.<field> == <literal>`, `in
[...]`, `&&`/`||`/`!`, quoted string/bool/number literals; empty selector =
`Selector::All`) + an `exclude: Vec<InstanceId>` list + a `wave: u32` (wave
grouping exists on the struct - `group_by_wave` - but nothing schedules
waves yet). **There is no `bindings/*.toml` file loader** - every CLI
command builds a `Binding` ad hoc from `-u`/`-s`/`--exclude` flags
(`crates/cubtera/src/commands/fleet.rs::parse_binding`); a real
`bindings/*.toml` fixture/loader is a documented gap, not a hidden one.

`BindingUseCase::expand` resolves every dimension combination the
selector matches into `InstanceId`s; `::status` diffs each against `Store`
into a `DriftState`: `Desired` (matches, never applied), `UpToDate`,
`PackageDrifted` (applied, but the current package/module digest differs),
`Orphaned` (an `Instance` exists in `Store` but no longer matches the
selector/inventory). `cubtera fleet status` prints the full report;
`cubtera drift` filters to only `PackageDrifted`/`Orphaned` and exits
`EXIT_DRIFT_DETECTED` (7) if anything shows up - built for CI gating
without parsing text.

### MaterializationPlan

`Unit::materialize(modules_path, ext)` (`cubtera-model::unit`) returns a
`MaterializationPlan` - copy/symlink/write ops for unit files, `.default`
includes, `cubtera_dim_{type}.json`, `cubtera_ext.json`, module symlink,
and resolved `[inputs]` files - computed with **zero I/O**, same as v2.
`cubtera_exec::materialize::apply(&plan)` is the only thing that actually
touches disk. `cubtera run --dry-run` prints the plan without applying it.

---

## Inventory on-disk format

Unchanged from v2 - still the one contract that must not break, still
pinned by golden tests (search `crates/cubtera-inventory/tests/` for the
golden fixture test against `example/inventory`):

- `{type}/{name}.json` or `{type}/{name}{sep}meta.json` → the dimension's `meta` section (`sep` = `fileNameSeparator`, default `:`).
- `{type}/{name}{sep}{section}.json` → an additional named section.
- `{type}/.default{sep}meta.json`, `.default{sep}{section}.json` → per-type defaults, merged gap-fill style.
- `{type}/.schema{sep}meta.json` → a JSON Schema validating the type's `meta` section.
- `{type}/{name}{sep}{file}` (non-`.json` extension) → an include file, copied into the unit's temp folder as `{file}`; trailing `/` marks an include directory.
- Names with a leading `.` (other than `.default`/`.schema`) are excluded from listings. A leading `#` is reserved/ignored.
- `meta.parent = "{type}:{name}"` drives the parent chain; `key_path` is the `type:name` chain from root to self.
- `content_hash` = digest of the canonically-ordered JSON of all sections (a `cubtera_kernel::Digest`, not a raw SHA-256 hex string - same input, different wrapper type).

If you need to change this convention, it's a one-adapter change
(`crates/cubtera-inventory/src/`) plus updated golden tests.

---

## CLI commands

```bash
# Show effective configuration (add --json for machine-readable output)
cubtera config

# Inventory management (unchanged surface from v2 - no more sync* subcommands, Mongo is gone)
cubtera im get-types <org>
cubtera im get-all <dim_type>
cubtera im get <dim_type> <name>
cubtera im get-defaults <dim_type>
cubtera im get-schema <dim_type>
cubtera im get-parent <dim_type> <name>
cubtera im get-children <dim_type> <name>
cubtera im validate <dim_type> <name>   # existence check + JSON Schema validation

# Static fleet-wide validation: schemas + dim-graph edges + [outputs]/runner-capability contracts
cubtera validate [--dim-type <type>]

# Fleet visibility
cubtera fleet ls [--dim-type <type>]                                  # every resolvable dimension + content hash
cubtera fleet status -u <unit> [-s <selector>] [--exclude <type:name>...]  # Binding expansion diffed against Store

# Reviewable plan/apply (tf/tofu only)
cubtera plan -u <unit> [-d <type:name>...] [-e <type:name>] [--ttl-seconds N] -- <command...>
cubtera apply --plan <plan_id> -u <unit> [-d <type:name>...] [-e <type:name>] [--auto-approve] \
              [--lease-ttl-seconds N] [--outputs-schema-version X.Y.Z] -- <command...>

# Explain a past Run by id
cubtera explain run <run_id>

# CI-facing drift gate - exits 7 (EXIT_DRIFT_DETECTED) if anything is PackageDrifted/Orphaned
cubtera drift -u <unit> [-s <selector>] [--exclude <type:name>...]

# "Just run it" - no plan artifact, no pin-drift gate (only path for bash/helm)
cubtera run -u <unit> [-d <type:name>...] [-e <type:name>] [--dry-run] [--auto-approve] \
            [--lease-ttl-seconds N] [--outputs-schema-version X.Y.Z] -- <command...>

# Deployment log (v2-shaped, backed by SqliteStore's legacy tables - see below)
cubtera log get -q <key:value> [-q ...] [--limit N]

# Unit state (cross-unit outputs) - v2-shaped legacy read path, not the same table as OutputSet
cubtera state get -u <unit> [-d <type:name>...] [-e <type:name>...]
cubtera state ls -u <unit>
cubtera state ls --stale          # org-wide: every [inputs] consumer behind its producer's latest OutputSet revision (P6, real OutputSet table, not the legacy one)
cubtera state rm -u <unit> [-d <type:name>...] [-e <type:name>...]

# Migrate a pre-v3 config.toml + fs-jsonl/fs-json data into v3's SQLite store
cubtera migrate [--dlog-path <dir>] [--unit-state-path <dir>] [--apply]
```

Global flags: `--config <path>`, `--log-level <level>`, `--json`
(machine-readable output where supported). Exit codes
(`crates/cubtera/src/error.rs`): `EXIT_GENERAL_ERROR=1`,
`EXIT_ACCESS_DENIED=3`, `EXIT_NOT_FOUND=4`, `EXIT_VALIDATION=5`
(`AppError::Validation`/`AppError::Model`), `EXIT_CONFIG=6`,
`EXIT_DRIFT_DETECTED=7` (`cubtera drift` only - not a failure, a CI
signal). A runner's own exit code, if the pipeline reached execution, is
propagated as-is instead of being remapped.

**`cubtera log get`/`cubtera state get/ls/rm` read v2-era "legacy" SQLite
tables** (`legacy_deployment_log`/`legacy_unit_state` in
`cubtera-store::legacy`) directly - they are **not** the same storage as
`OutputSet`/`Store::put_output_set` (the P6 cross-unit mesh). Nothing in
the current `apply`/`run` pipeline writes to the legacy tables anymore
except `cubtera migrate` (importing old fs-jsonl/fs-json data) - they exist
purely to keep old query commands working against migrated history. Don't
assume `cubtera state get` will show you a freshly-`apply`'d unit's
outputs; check `[outputs]`/`OutputSet` via the mesh instead (there's
currently no CLI command that prints an `OutputSet` directly - only the
consumer-side materialized `cubtera_in_*.json` files and, on the server,
`GET /v1/{org}/state`).

---

## REST API (`cubtera-server`)

One binary now does what v2 split across `cubtera-api` (read-only) and
`cubtera` (`run`) - see `.github/docs/api.md` for the fuller reference (a
`cubtera-server` doc that's still catching up to this route list; verify
against `crates/cubtera-server/src/routes/` if it disagrees). Every `/v1/*`
route requires an `x-api-key` header matching `CUBTERA_API_KEY`/config
`apiKey` (unset → unauthenticated, with a startup warning, **not** a
refusal to start); errors are `application/problem+json`. An optional
`x-actor` header (default `"anonymous"`) feeds the `apply` route's authz
check.

```
GET  /health
GET  /v1/orgs
GET  /v1/{org}/dim-types
GET  /v1/{org}/dims/{dim_type}
GET  /v1/{org}/dims/{dim_type}/defaults
GET  /v1/{org}/dims/{dim_type}/schema
GET  /v1/{org}/dims/{dim_type}/{name}
GET  /v1/{org}/dims/{dim_type}/{name}/parent
GET  /v1/{org}/dims/{dim_type}/{name}/children
GET  /v1/{org}/units
GET  /v1/{org}/units/{name}
GET  /v1/{org}/dlog
GET  /v1/{org}/validate
GET  /v1/{org}/fleet/status
POST /v1/{org}/units/{unit}/plan
POST /v1/{org}/units/{unit}/apply
GET  /v1/{org}/runs/{run_id}
GET  /v1/{org}/runs/{run_id}/log
GET  /v1/{org}/runs/{run_id}/log/stream    # SSE - live tail if in flight, one-shot replay of the artifact if finished
GET  /v1/{org}/state
GET  /v1/{org}/state/stale
```

`apply` queues the run (`RunUseCase::queue_apply`/`queue_apply_direct`),
registers it with `crate::log_hub::LogHub`, spawns `run_and_finish` in the
background, and returns the `Run` row (in `Queued` status) immediately -
poll `GET /v1/{org}/runs/{run_id}` for the final status, or open
`.../log/stream` for a live tail. `plan` is read-only and is **not**
policy-gated (matches v2's behavior: `AccessPolicy`/`Policy` only ever
blocks the run pipeline, never a plan/dry-run step); `apply` runs
`crate::policy::check` (built from the unit's `allowList`/`denyList` +
`x-actor`/first command word) before doing anything.

Default bind address: `CUBTERA_SERVER_ADDR` env var, default
`0.0.0.0:8081` (note: `Dockerfile.server` still says `EXPOSE 8000` - a
stale mismatch worth fixing if you touch the Dockerfile).

## MCP server

`cubtera-mcp` (`rmcp` SDK, stdio transport) is, since P7, **a pure HTTP
client of a running `cubtera-server`** - it has no `cubtera-app`/
`cubtera-inventory`/`cubtera-store` dependency and never touches the
filesystem or a store directly. It exposes the same read-only queries the
server does as MCP tools (`list_orgs`, `list_dim_types`,
`list_dimension_names`, `get_dimension`, `get_dimension_defaults`,
`get_dimension_schema`, `get_dimension_parent`, `get_dimension_children`,
`validate_dimension`, `list_units`, `get_unit_manifest`, `get_unit_state`,
`get_deployment_log`) by calling `crates/cubtera-mcp/src/client.rs`'s
`CubteraApiClient`, which mirrors the server's route surface one-to-one. No
`run`/`plan`/`apply` tool exists here, deliberately - giving an MCP client
the ability to apply changes is a product decision this project still
hasn't made.

```bash
cubtera-server --config example/config.toml &      # cubtera-mcp needs a running server to talk to
cubtera-mcp --server-url http://127.0.0.1:8081 --api-key <key>   # speaks MCP over stdio (env: CUBTERA_SERVER_URL/CUBTERA_API_KEY)
```

`ClientError` (network failure or a non-2xx `application/problem+json`
response) maps to `rmcp::ErrorData` in `crates/cubtera-mcp/src/error.rs`.

---

## Configuration

`config.toml` has a `[default]` table plus one table per org, whose fields
override `[default]` for that org; `[runner.<type>]`/`[state.<backend>]`
are maps of arbitrary keys merged entry-by-entry. Keys accept camelCase or
snake_case. See `example/config.toml` for a fully-annotated example and
`crates/cubtera-config/src/config.rs` for the loader.

Key fields on `Config` (`crates/cubtera-config/src/config.rs`): `org`,
`orgs`, `inventory_path`, `units_path`, `modules_path`, `plugins_path`,
`temp_folder_path`, **`store_path`** (the SQLite file backing everything -
`Instance`/`Plan`/`Run`/`OutputSet`/leases/legacy dlog/unit-state tables;
replaces v2's separate `deploymentLogPath`/`unitStatePath`/
`[deploymentLog]`/`[unitState]`/`CUBTERA_DB`, all of which are now
**silently ignored** by the loader), `dim_relations`, `file_name_separator`,
`always_copy_files`, `clean_cache`, `api_key` (not serialized - comes from
`CUBTERA_API_KEY` env, not `config.toml`).

**Mongo support does not exist in v3** - there is no `--features mongodb`
anywhere in this workspace; `InventoryPort`/`Store` are FS/SQLite-only.

### Migration (`cubtera migrate`)

`cubtera migrate [--dlog-path <dir>] [--unit-state-path <dir>] [--apply]`
(`crates/cubtera/src/commands/migrate.rs`) automates moving a pre-v3
install onto v3:

- **Config cleanup**: strips `deploymentLogPath`/`unitStatePath`/
  `[deploymentLog]`/`[unitState]` out of `config.toml`, backing up the
  original file first.
- **Deployment log import**: reads old fs-jsonl deployment log files and
  writes them into `SqliteStore`'s `legacy_deployment_log` table
  (`LegacyDeploymentLogRow`), **append-only**.
- **Unit state import**: reads old fs-json unit-state files and writes
  them into `legacy_unit_state` (`LegacyUnitStateRow`), **idempotent**
  (keyed by `state_key`, upserted).
- **Dry-run by default** - prints the plan (config diff + row counts)
  without writing anything; pass `--apply` to actually write.

This only backfills the **legacy** tables that `cubtera log get`/`cubtera
state get/ls/rm` read - it does not synthesize `OutputSet`/`Plan`/`Run`
rows for the new P4-P6 mesh; there's no historical data to migrate there
since those are new v3 concepts with no v2 on-disk equivalent.

---

## Development commands

```bash
# Build / test / lint the whole workspace (this is what CI runs)
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check

# Run one crate's tests only
cargo test -p cubtera-model
cargo test -p cubtera-inventory --test adversarial

# Run the CLI / server / MCP server against the example fixture
cargo run -p cubtera -- -c example/config.toml im get-all dc
CUBTERA_CONFIG=example/config.toml cargo run -p cubtera-server
cargo run -p cubtera-mcp -- --server-url http://127.0.0.1:8081
```

CI (`.github/workflows/code_pr_test.yaml`) has **no `paths:` filter** on
`pull_request` - every PR runs the full build/test/clippy/fmt pass,
including changes limited to `example/**` or docs, so fixture-only changes
still get tested against the golden/e2e suites.

---

## Rust conventions

### Module file structure (Rust 2018+ style)

Use `module_name.rs` alongside a `module_name/` directory instead of
`module_name/mod.rs`:

```
# CORRECT
src/
├── lib.rs
├── tf_like.rs          # `mod switch;` etc.
└── tf_like/
    └── switch.rs

# AVOID
src/
└── tf_like/
    ├── mod.rs
    └── switch.rs
```

### Testing conventions

- Kernel/model logic (`cubtera-kernel`, `cubtera-model`) is pure - test it
  with plain `#[test]`, no fixtures or async runtime needed.
- Adapter behavior against the real inventory fixture goes in
  `crates/cubtera-inventory/tests/` (golden fixture test +
  `adversarial.rs` for path-traversal/injection coverage) - if you touch
  the FS naming convention or add a new user-controlled string parameter,
  add or update tests there.
- Use-case logic that needs a fake port (e.g. `RunUseCase`) uses in-file
  `#[cfg(test)] mod tests` with hand-rolled fakes (see `FakeExecutor` in
  `crates/cubtera-app/src/run.rs`) rather than a mocking framework.
- CLI end-to-end behavior against `example/` uses `assert_cmd` (see
  `crates/cubtera/tests/`); tests that spin up their own `SqliteStore`
  use an isolated temp DB per test (not a shared one) to avoid lease
  contention under parallel test execution - see `RunId::mint_id`'s
  pid+counter uniqueness scheme if you're adding a new concurrent test.

## Notes for AI agents

1. **Validate every user-controlled `org`/`unit_name`/dim identifier with
   `cubtera_kernel::Ident::parse` at the use-case entry point** -
   `FsInventoryPort`/`FsUnitPort` have no traversal protection of their
   own by design (keeps adapters dumb); `ResolveUseCase`/`AssembleUseCase`
   are the choke point. If you add a new use case or a new public method
   that takes a raw string destined for a filesystem path, parse it first.
2. **Everything external goes through a port in `cubtera-app::ports`** -
   if you're adding a new backend, extend/add a trait there first,
   implement it in an adapter crate, wire it in the CLI's and server's
   `run_support.rs` (there's no `App::new(...)` composition root to update
   instead).
3. **No global state, no `exit()`/`panic!` outside CLI/server edges** -
   return `AppResult<T>` and let `crates/cubtera/src/error.rs` /
   `crates/cubtera-server/src/error.rs` translate it.
4. **v1 and the deleted v2 crates (`cubtera-domain`/`cubtera-core`/
   `cubtera-persistence`/`cubtera-runners`/`cubtera-api`) no longer exist
   in this repo** - don't reference them in new code or docs; if you find
   a stale reference, it's a doc bug, fix it.
5. **The inventory naming convention is a contract** - changes need a
   golden test update in the same PR.
6. **`allowList`/`denyList` entries are `type:name`**, always.
7. **English only in code** - comments, docs, identifiers.
8. **Rust 2018+ module style** - `module.rs` + `module/`, not
   `module/mod.rs`.
9. **Runner pipeline** - add a new runner type by implementing
   `RunnerStrategy` in `cubtera-exec`, not by touching `RunUseCase`.
10. **`cubtera-mcp` has no direct dependency on inventory/store crates** -
    it only ever talks to `cubtera-server` over HTTP
    (`crates/cubtera-mcp/src/client.rs`). Don't add a direct
    `cubtera-app`/`cubtera-inventory` dependency to it; add a server route
    plus a client method plus an MCP tool instead.
11. **`[inputs.<alias>]`/`[outputs]` are not a DAG** - a consumer's
    `[inputs]` never auto-runs the producer, and `project_state_key` never
    invents a dimension the consumer didn't resolve or guesses on
    ambiguity - both are hard `AppError`s, not silent fallbacks.
12. **Two separate policy checks exist** (`AccessPolicy::evaluate` in
    `AssembleUseCase`, `Policy::evaluate` in `cubtera-server`'s `apply`
    route) - they are not unified; if you change access-control semantics,
    check both.
13. **`cubtera log get`/`cubtera state get/ls/rm` read legacy SQLite
    tables, not `OutputSet`** - don't conflate the P6 output mesh with
    these v2-compat read paths when debugging "why doesn't my apply's
    output show up in `cubtera state get`".
