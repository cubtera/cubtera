# Migrating from Cubtera v1 to v2

v2 (`v2-rewrite`) is a breaking major release: new `config.toml` schema, new
CLI flags/subcommand names, new REST API shape. There is no compatibility
shim - this guide is the closest thing to one. The legacy v1 source lives at
[`v1/`](../../v1/) for reference during migration; it will be removed once
v2 reaches feature parity with wave 2.

The one thing that **has not** changed is the on-disk inventory format
(`{type}/{name}{sep}{section}.json`, `.default`/`.schema` records, gap-fill
defaults, parent chains). If your `inventory/` directory works with v1, it
works with v2 unmodified - see [`AGENTS.md`](../../AGENTS.md#inventory-on-disk-format)
for the exact rules.

## Config file

v1 read `~/.cubtera/config.toml` (or `$CUBTERA_CONFIG`) with a flat
`[default]`/`[<org>]` table structure and separate `CUBTERA_*` env var
overrides for nearly every field. v2 keeps the `[default]` + per-org table
shape but the field set changed:

| v1 field | v2 field | Notes |
| --- | --- | --- |
| `workspace_path` | *(removed)* | v2 has no single workspace root; set `inventoryPath`/`unitsPath`/`modulesPath`/`pluginsPath` independently. |
| `inventory_path` | `inventoryPath` | camelCase preferred; `inventory_path` still accepted as an alias. |
| `units_path` | `unitsPath` | same alias behavior. |
| `modules_path` | `modulesPath` | |
| `plugins_path` | `pluginsPath` | |
| `temp_folder_path` | `tempFolderPath` | |
| `org` | *(removed from file)* | set via `CUBTERA_ORG` env var or `--config`'s `orgs` list (first entry is the default). |
| `orgs` (colon-separated string) | `orgs` (TOML array) | `orgs = ["cubtera", "teracub"]`, not `"cubtera:teracub"`. |
| `dim_relations` (colon-separated string) | `dimRelations` (TOML array) | same array-instead-of-colon-string change. |
| `db` (MongoDB URL) | *(still env-var only)* `CUBTERA_DB` | Selects the `InventoryRepository` backend (Mongo vs FS); requires a build with `cubtera-persistence`'s `mongodb` feature (the default for the `cubtera`/`cubtera-api` binaries). |
| `dlog_db` | `[deploymentLog]` (`connectionString`/`database`/`collection`) | Selects `MongoDeploymentLogRepository`; unset means the fs-jsonl backend (`deploymentLogPath`, default `~/.cubtera/dlog`). |
| `dlog_job_*_env` | *(not ported)* | v1's per-CI-provider job-env autodetection for dlog metadata isn't carried over; deployment log entries record `command`/`exit_code`/`duration_ms`/`dimensions` but not CI job context. |
| `clean_cache`, `always_copy_files` | `cleanCache`, `alwaysCopyFiles` | unchanged semantics. |
| `file_name_separator` | `fileNameSeparator` | unchanged semantics, default `:`. |
| `[runner.<type>]` / `[state.<backend>]` | same shape | still arbitrary `HashMap<String, String>` per runner type/state backend; merged entry-by-entry between `[default]` and the org table, not replaced wholesale. |
| *(new)* | `apiKey` / `CUBTERA_API_KEY` | required for `cubtera-api` auth (v1's `ApiKey` guard existed but was never actually applied to a route). |

`CUBTERA_<FIELD>` env var overrides still work, but the field names follow
the new camelCase/snake_case-alias set above, and array fields are real env
var lists (`CUBTERA_ORGS=cubtera,teracub` still parses as a list - the file
format is what changed, not the env var convention).

See `example/config.toml` for a fully-annotated v2 example.

## CLI

Binary name is unchanged (`cubtera`), but subcommand/flag names changed:

| v1 | v2 | Notes |
| --- | --- | --- |
| `cubtera run -u <unit> -d <dim> [-e <ext>] [-c <context>] -- <cmd>` | `cubtera run -u <unit> -d <dim> [-e <ext>] [--auto-approve] [--dry-run] -- <cmd>` | `-c/--context` (v1's loosely-defined "advanced feature") is gone; `--dry-run` is new (prints the `MaterializationPlan` without touching disk); `--auto-approve` is now an explicit flag instead of being inferred. |
| `cubtera tf ...` (alias for `run`) | *(removed)* | use `cubtera run` directly. |
| `cubtera im getAll <type>` | `cubtera im get-all <type>` | kebab-case subcommands throughout. |
| `cubtera im getAllData <type>` | *(removed - use `get-all` + `get`)* | v1's "list of names" and "list of full data" were separate v1 calls; v2's `im get-all` lists names, `im get <type> <name>` returns full data for one. |
| `cubtera im getByName <type> <name> [-c <context>]` | `cubtera im get <type> <name>` | `-c/--context` removed along with the rest of the "context" feature. |
| `cubtera im getByParent <type> <name>` | `cubtera im get-children <type> <name>` | renamed for clarity. |
| `cubtera im getParent <type> <name>` | `cubtera im get-parent <type> <name>` | |
| `cubtera im getDefaults <type>` | `cubtera im get-defaults <type>` | |
| `cubtera im getOrgs` | *(removed - use `cubtera config` or `GET /v1/orgs`)* | |
| *(new)* | `cubtera im get-types <org>` | lists dimension types under an org (v1's FS mode had a known bug here - returned `orgs` instead of types; not carried over). |
| *(new)* | `cubtera im get-schema <type>` | fetches `.schema:meta.json` for a type. |
| `cubtera im validate <type> <name>` (stub, printed "not implemented") | `cubtera im validate <type> <name>` | now actually validates: checks existence, then runs the dimension's `meta` section against `.schema:meta.json` if one exists. |
| `cubtera im syncDefaults <type>` | `cubtera im sync-defaults <type>` | Same behavior: reads FS defaults, writes to MongoDB (`CUBTERA_DB` must be set). |
| `cubtera im syncAll <type> [-c <context>]` | `cubtera im sync-all <type>` | `-c/--context` removed along with the rest of the "context" feature. |
| `cubtera im sync <type> <name> [-c <context>]` | `cubtera im sync <type> <name>` | |
| `cubtera im deleteContext <context>` | *(removed)* | deliberately not ported - see "What's gone for good". |
| `cubtera config` | `cubtera config [--json]` | `--json` prints the raw `Config` struct; without it, output is a human-readable summary. |
| `cubtera log get -q <k:v> [--limit N]` | unchanged shape | now backed by a real `DeploymentLogRepository` (fs-jsonl by default, Mongo if `[deploymentLog]` is set) instead of being a stub. |

Global flags: `--config <path>` (was env-var-only in v1's typical flow, now
also a proper `-c/--config` CLI flag), `--log-level`, and the new `--json`
for machine-readable output on `config`/`im`.

### Exit codes

v1 used `std::process::exit(0)` for access-denied and returned `1` for
essentially every other failure via `unwrap_or_exit`. v2 maps `AppError`
variants to distinct exit codes (`crates/cubtera/src/error.rs`):

| Code | Meaning |
| --- | --- |
| `0` | success |
| `1` | general/unclassified error |
| `3` | access denied (`allowList`/`denyList`/`affinityTags`) - **not** `0` like v1 |
| `4` | not found |
| `5` | validation error (including `im validate` schema failures) |
| `6` | configuration error |
| *(runner's own code)* | once the pipeline reaches `execute`, the underlying `terraform`/`tofu`/`bash` exit code is propagated as-is |

If any script depended on v1's "access denied = exit 0", it needs updating -
that behavior was called out in the migration plan as something we
deliberately do not carry forward (`exit(0)` meaning "denied" is
indistinguishable from success to any caller).

## Unit manifests

- `allowList`/`denyList` entries must now be `type:name` (e.g. `"dome:mgmt"`,
  `"env:stg1"`) rather than a bare dimension name. v1's access check compared
  against bare names; v2's `AccessPolicy::evaluate` matches against the full
  resolved `key_path` (`type:name` strings), so update any existing
  manifests - a bare `"mgmt"` will silently deny every run in v2.
- `spec.tf_version` is dropped (was already deprecated in v1); use
  `[runner] version = "..."` instead.
- Everything else in `manifest.toml` (`dimensions`, `optDims`, `type`,
  `affinityTags`, `spec.files.{required,optional}`,
  `spec.envVars.{required,optional}`, `[runner]`, `[state]`) is unchanged in
  shape.
- `type = "helm"` is now a supported runner (ported from `test2`'s helm
  runner prototype): if the unit directory has a `values.yaml.tpl`, it's
  rendered with handlebars against the merged `cubtera_*.json` dimension
  data and written to `values.yaml` before `helm <command...>` runs. Not a
  v1/`main` feature - new in v2 wave 2.

## REST API

- Auth is now actually enforced: set `CUBTERA_API_KEY` and send it as the
  `x-api-key` header on every `/v1/*` request. v1 defined an `ApiKey` guard
  but never attached it to a route - **v1's API was unauthenticated
  regardless of configuration.**
- No more `{status, id, data}` envelope and no more query-string routing
  (`/v1/{org}/dim?type=&name=`). Routes are now REST-shaped path segments
  (`/v1/{org}/dims/{dim_type}/{name}`) and responses are the resource
  itself.
- Errors are `application/problem+json` (RFC 7807) instead of ad-hoc JSON
  bodies with varying shapes.
- v1's API only worked against a MongoDB-backed inventory ("API feature
  works only with DB storage"). v2's API works against the FS adapter too
  (through the same `App`/services the CLI uses) - MongoDB support for the
  API is wave 2, additive, not a prerequisite.
- New unit endpoints (`GET /v1/{org}/units`, `GET /v1/{org}/units/{name}`)
  didn't exist in v1's API at all.
- `GET /v1/{org}/dlog` replaces v1's (Mongo-only) dlog query surface - see
  [`api.md`](api.md#deployment-log). Filters are `q=key:value,...` instead
  of repeated query params, since axum's query deserializer can't collect
  repeated keys into a list.

See [`.github/docs/api.md`](api.md) for the full v2 endpoint reference.

## What's gone for good (not "not yet ported")

These are intentional removals, not gaps to fill in later - see the
migration plan's "что осознанно не переносим из v1":

- `GLOBAL_CFG` / any process-wide static config, and `exit()`/`unwrap_or_exit`
  as control flow in business logic.
- The `{status, id, data}` response envelope and query-param REST style.
- Unconditional copying of plugins into `$HOME/.terraform.d/plugins` -
  it's opt-in via config now, not a silent side effect.
- `deleteContext` (scanned every Mongo database) and the `{data: ...}`
  wrapper around stored defaults.
- `spec.tf_version` and the legacy `tf_state_s3*` config fields.
- Postgres as a storage backend option (it was a dead branch in v1's
  `StorageBackend` enum with no real implementation).

## Wave 2 (done)

Everything planned for wave 2 has landed: MongoDB inventory adapter,
`DeploymentLogRepository` fs-jsonl + Mongo with a working `cubtera log get`
and `GET /v1/{org}/dlog`, `im sync-defaults`/`im sync-all`/`im sync`, a Helm
runner, and `cubtera-mcp` - a real MCP server (via the `rmcp` SDK, stdio
transport), not the REST-prototype approach explored on `test1`. It exposes
read-only inventory/unit/deployment-log queries as MCP tools; it does not
expose a `run` tool, so it cannot apply infrastructure changes on its own.
