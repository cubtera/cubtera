# Migrating to Cubtera v3

v3 is a breaking release on top of the previous (v2-shaped) codebase: a
single SQLite `Store` (`storePath`) replaces the old three-way
deployment-log/unit-state backend choice (fs-jsonl, fs-json, MongoDB), and
`cubtera-server` replaces the old split `cubtera-api` (read-only) + `cubtera
run` (execution) with one binary that does both, plus a reviewable
`plan`/`apply` pipeline and cross-unit output mesh that didn't exist
before. There is no compatibility shim - old `config.toml` keys are
silently ignored, not translated - so run `cubtera migrate` once to bring
an existing install forward.

The one thing that **has not** changed, ever, across any of these
rewrites: the on-disk inventory format
(`{type}/{name}{sep}{section}.json`, `.default`/`.schema` records,
gap-fill defaults, parent chains). If your `inventory/` directory works
today, it works with v3 unmodified - see
[AGENTS.md](../../AGENTS.md#inventory-on-disk-format) for the exact rules.
`units/*/manifest.toml`'s schema is also unchanged (same field names,
same `type:name` access-list convention) - see [unit.md](unit.md).

## Step 1: run `cubtera migrate`

```bash
# Dry run first - reports what it would do, writes nothing
cubtera migrate --dlog-path ~/.cubtera/dlog --unit-state-path ~/.cubtera/state

# Then actually do it
cubtera migrate --dlog-path ~/.cubtera/dlog --unit-state-path ~/.cubtera/state --apply
```

`--dlog-path`/`--unit-state-path` should point at whatever your old
`deploymentLogPath`/`unitStatePath` config values were (defaults were
`~/.cubtera/dlog`/`~/.cubtera/state`) - omit either flag to skip importing
that data source. Without `--apply`, the command only prints a report;
nothing is written until you pass it.

What it does:

- **`config.toml` cleanup**: finds now-dead keys (`deploymentLogPath`,
  `unitStatePath`, `[deploymentLog]`, `[unitState]`) in every org table,
  backs up the original file to `config.toml.bak`, removes those keys,
  and makes `storePath` explicit in `[default]`. If a MongoDB-based
  install used `CUBTERA_DB` for the inventory backend, note that this env
  var (and Mongo support generally) is gone entirely in v3 - there's
  nothing to migrate there, since `InventoryPort` only has a filesystem
  implementation now; re-point `inventoryPath` at an exported filesystem
  copy of your inventory if you relied on `CUBTERA_DB` for that.
- **Deployment log import**: reads `{dlog-path}/{org}.jsonl` (one JSON
  object per line, the old fs-jsonl format) and appends each entry into
  the SQLite store's `legacy_deployment_log` table - the same table
  `cubtera log get` reads. **Append-only**, matching the old backend's own
  semantics - re-running against a store that already has this data will
  duplicate rows, so migrate once per store.
- **Unit state import**: walks `{unit-state-path}/{org}/{unit}/{dims...}/
  {ext...}/outputs.json` (the old fs-json layout) and upserts each into
  `legacy_unit_state` - the same table `cubtera state get/ls/rm` reads.
  **Idempotent** (keyed by org+unit+dims+ext), safe to re-run.
- Unparseable lines/files are reported and skipped, never a hard failure
  for the whole migration.

This only backfills the **legacy** tables `cubtera log`/`cubtera state`
read - it does not (and can't) synthesize `Plan`/`Run`/`OutputSet` rows for
the new v3 pipeline, since those are new concepts with no pre-v3 on-disk
equivalent. Your deployment history is preserved for querying; your first
`plan`/`apply`/`run` on each unit after migrating starts a fresh v3
history for it.

## Step 2: update `config.toml`

If you ran `cubtera migrate --apply`, this already happened for you.
Otherwise, by hand:

| Old key | v3 replacement | Notes |
| --- | --- | --- |
| `deploymentLogPath` | *(removed)* | history now lives in `storePath`'s `legacy_deployment_log` table |
| `[deploymentLog]` (Mongo connection) | *(removed)* | Mongo support is gone; there is no database-backed deployment log anymore |
| `unitStatePath` | *(removed)* | history now lives in `storePath`'s `legacy_unit_state` table |
| `[unitState]` (Mongo connection) | *(removed)* | same - Mongo is gone |
| `CUBTERA_DB` | *(removed)* | `InventoryPort` is filesystem-only in v3, no runtime backend switch |
| *(new)* | `storePath` | SQLite file backing `Instance`/`Plan`/`Run`/`OutputSet`/leases *and* the legacy dlog/unit-state tables - default `~/.cubtera/store.sqlite` |

Everything else (`inventoryPath`, `unitsPath`, `modulesPath`,
`pluginsPath`, `tempFolderPath`, `dimRelations`, `orgs`,
`fileNameSeparator`, `alwaysCopyFiles`, `cleanCache`, `[runner.<type>]`,
`[state.<backend>]`) is unchanged. See [config.md](config.md) for the full
current schema and `example/config.toml` for a working reference.

## Step 3: know what's new (and what changed shape)

### New CLI surface

| Command | Purpose |
| --- | --- |
| `cubtera validate` | fleet-wide JSON Schema + dim-graph + `[outputs]`/runner-capability validation (was `im validate`, one dimension at a time) |
| `cubtera fleet ls`/`status` | list every resolvable dimension; diff a `Binding` (unit + selector) against `Store` |
| `cubtera plan` / `cubtera apply --plan` | reviewable plan artifact (tf/tofu only) with pin-drift checking before a real apply |
| `cubtera explain run <run_id>` | look up one `Run` record |
| `cubtera drift` | CI-facing: `fleet status`, filtered to drift only, with a dedicated exit code |
| `cubtera migrate` | this migration itself |

`cubtera run`, `cubtera im *`, `cubtera log get`, `cubtera state
get/ls/rm` all still exist with the same flags as before - `cubtera run`
is now explicitly the "no plan artifact, no pin-drift gate" escape hatch
(and the only path for `bash`/`helm` units, which have no plan concept at
all).

### Exit codes

`EXIT_DRIFT_DETECTED = 7` is new (`cubtera drift` only, when real drift is
found - not a failure signal for anything else). `EXIT_GENERAL_ERROR=1`,
`EXIT_ACCESS_DENIED=3`, `EXIT_NOT_FOUND=4`, `EXIT_VALIDATION=5`,
`EXIT_CONFIG=6` are unchanged.

### REST API

`cubtera-api` (read-only) is gone - `cubtera-server` now serves everything
under one binary, including `plan`/`apply` (async, with SSE log streaming)
and the new `validate`/`fleet/status`/`state`/`state/stale` routes. See
[api.md](api.md) for the full v3 route list; `GET /v1/{org}/units/{name}/
state` (v2's unit-state route) has no direct v3 equivalent - the closest
is `GET /v1/{org}/state?unit=...` over the new `OutputSet` table (a
different, revision-stamped shape - see [api.md](api.md#cross-unit-output-mesh)).

### MCP server

`cubtera-mcp` no longer links `cubtera-app`/`cubtera-inventory`/
`cubtera-store` directly - it's a pure HTTP client of a running
`cubtera-server` (`--server-url`/`CUBTERA_SERVER_URL`, `--api-key`/
`CUBTERA_API_KEY`). You must run `cubtera-server` for `cubtera-mcp` to
work at all now; it can no longer read the inventory standalone.

### Unit manifests

Unchanged in shape from before - `allowList`/`denyList` still `type:name`,
`[outputs] publish = true`/`[inputs.<alias>]` still work the same way.
New: `cubtera validate` now catches a `publish = true` unit whose runner
can't actually collect outputs (`bash`/`helm` without a hand-rolled
`cubtera_outputs.json` via `outlet_command`) *before* you find out the
hard way after an `apply`.

## What's gone for good

- **MongoDB**, everywhere it used to be optional (`InventoryPort`,
  deployment log, unit state). There is no `--features mongodb` in this
  workspace and no `CUBTERA_DB`/`[deploymentLog]`/`[unitState]` config.
- **`cubtera-api`** as a separate binary/crate - folded into
  `cubtera-server`.
- The old `cubtera-domain`/`cubtera-core`/`cubtera-persistence`/
  `cubtera-runners` crates and the `v1/` reference directory - all
  deleted from the repository.

## Full architecture reference

See [AGENTS.md](../../AGENTS.md) for the complete crate map, on-disk
formats, plan/apply pipeline, and cross-unit output mesh design; see
[`docs/specs/2026-09-03-cubtera-v3-architecture.md`](../../docs/specs/2026-09-03-cubtera-v3-architecture.md)
for the original design intent (note: several details there are
simplified or diverge slightly from what's actually implemented - AGENTS.md
tracks the real implementation).
