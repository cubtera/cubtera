# Cubtera API (v3)

`cubtera-server` is the one HTTP server v3 ships - it replaces both of
v2's `cubtera-api` (read-only inventory) and `cubtera run`'s server-side
equivalent: it can both read the inventory *and* drive `plan`/`apply`/
`explain` through `cubtera-app`'s use cases directly (no
`cubtera-domain`/`cubtera-core`/`cubtera-persistence`/`cubtera-runners`
dependency at all - those crates were deleted in P7). There's still no
`{status, id, data}` envelope - responses are the resource itself, plain
JSON.

## Running

```bash
CUBTERA_CONFIG=/path/to/config.toml \
CUBTERA_SERVER_ADDR=127.0.0.1:8081 \
CUBTERA_API_KEY=change-me \
cubtera-server
```

| Env var | Purpose | Default |
| --- | --- | --- |
| `CUBTERA_CONFIG` | Path to `config.toml` | see `cubtera-config` |
| `CUBTERA_SERVER_ADDR` | Listen address | `0.0.0.0:8081` |
| `CUBTERA_API_KEY` | Shared secret required on every `/v1/*` request | none (server starts unauthenticated, with a startup warning) |

If `CUBTERA_API_KEY` is unset, the server logs a warning at startup and
serves `/v1/*` without authentication - fine for local dev, not for anything
reachable over a network. `Dockerfile.server` currently `EXPOSE`s `8000`
while the process itself defaults to `:8081` - override
`CUBTERA_SERVER_ADDR` (or fix the Dockerfile) if you rely on the exposed
port matching the listen port.

## Authentication

Every route under `/v1` requires the `x-api-key` header to match
`CUBTERA_API_KEY`. `/health` is always open, for liveness/readiness probes.

```bash
curl -H "x-api-key: $CUBTERA_API_KEY" http://localhost:8081/v1/orgs
```

Missing or invalid keys get a `401` `problem+json` body.

`POST .../apply` additionally reads an optional `x-actor` header (default
`"anonymous"`) and runs it, plus the request's `actor` field if set,
through the unit's `allowList`/`denyList` (see
[Authorization on `apply`](#authorization-on-apply) below).

## Error format

All errors are `application/problem+json` ([RFC 7807](https://www.rfc-editor.org/rfc/rfc7807)):

```json
{
  "type": "not-found",
  "title": "Not Found",
  "status": 404,
  "detail": "dimension not found: dc:does-not-exist"
}
```

`AppError` variant → `type`/status mapping (`crates/cubtera-server/src/error.rs`):

| `AppError` variant | `type` | HTTP status |
| --- | --- | --- |
| `NotFound` | `not-found` | 404 |
| `AccessDenied` | `access-denied` | 403 |
| `Validation` | `validation` | 400 |
| `Model` (schema/graph/selector/manifest errors from `cubtera-model`) | `model` | 400 |
| `Backend` | `backend` | 500 |
| *(a bare `String`, e.g. `SqliteStore::open` failing)* | `config` | 500 |

## Endpoints

### Health

- `GET /health` - liveness probe, no auth. Returns `{"status":"ok"}`.

### Orgs

- `GET /v1/orgs` - list configured orgs.

  ```json
  ["cubtera", "teracub"]
  ```

### Dimensions

All dimension routes are scoped under an org: `/v1/{org}/...`. `org`/
`dim_type`/`name` path segments are validated with `Ident::parse` before
touching the filesystem - a malformed or path-traversal-shaped value gets
a `400 validation` response, never a filesystem read outside the
inventory root.

- `GET /v1/{org}/dim-types` - list dimension types known to the org.

  ```json
  ["dc", "dome", "env"]
  ```

- `GET /v1/{org}/dims/{dim_type}` - list dimension names of a given type.

- `GET /v1/{org}/dims/{dim_type}/defaults` - the `.default` record for a
  type, or `null` if none exists.

- `GET /v1/{org}/dims/{dim_type}/schema` - the raw JSON Schema from
  `.schema:meta.json` for the type, or `null` if none exists.

- `GET /v1/{org}/dims/{dim_type}/{name}` - a single dimension, fully
  assembled (gap-filled defaults, parent resolved, `key_path`/
  `content_hash`/`kids` computed), via `Dimension::to_response_json`:

  ```json
  {
    "key": "dc:prod-use1",
    "key_path": ["dome:prod", "env:prod", "dc:prod-use1"],
    "parent": "env:prod",
    "meta": { "region": "us-east-1", "vpc_cidr": "10.11.0.0/16" },
    "content_hash": "0e6dd6ec...",
    "kids": []
  }
  ```

- `GET /v1/{org}/dims/{dim_type}/{name}/parent` - the assembled parent
  dimension, or `null` if there is none.

- `GET /v1/{org}/dims/{dim_type}/{name}/children` - assembled child
  dimensions (per `dimRelations`).

- `GET /v1/{org}/dims/{dim_type}/{name}/validate` - JSON-Schema-validate
  this one dimension's `meta` section against `.schema:meta.json` (a
  narrower version of `GET /v1/{org}/validate`, scoped to one dimension).

  ```json
  { "valid": true, "errors": [] }
  ```

> Route ordering note: static suffixes (`/defaults`, `/schema`) are
> declared before the dynamic `/{name}` segment so a dimension literally
> named `defaults` or `schema` can't shadow those endpoints.

### Units

- `GET /v1/{org}/units` - list unit names discovered under the configured
  units path.

- `GET /v1/{org}/units/{name}` - the unit's manifest (parsed
  `manifest.toml`) as JSON. Manifest only - it does not resolve
  dimensions, run access policy, or materialize anything (that's `plan`/
  `apply`/`run`'s job).

### Fleet-wide validation

- `GET /v1/{org}/validate` - server-side `cubtera validate`: JSON Schema +
  dim-graph edge validation across every dimension of every type in
  `dimRelations`. (Unlike the CLI, this route does **not** also check the
  `[outputs] publish = true` vs. `RunnerCapabilities::collects_outputs`
  contract - that check lives only in `crates/cubtera/src/commands/
  validate.rs`, since it needs an `Executor`, which the read-only routes
  in this file don't construct.)

  ```json
  {
    "valid": true,
    "results": [
      { "key": "dc:prod-use1", "valid": true, "schema_errors": [], "graph_errors": [] }
    ]
  }
  ```

### Fleet status / drift

- `GET /v1/{org}/fleet/status?unit=<name>&selector=<expr>&exclude=<type:name>[,...]`
  - expands a `Binding` (`unit` + `selector`, same grammar as the CLI's
  `-s`) and diffs it against `Store`. `selector` defaults to matching
  everything; `exclude` is a comma-separated list of `type:name` refs
  (coarser than `Binding.exclude`'s exact-`InstanceId` semantics - see the
  CLI's `fleet status --exclude` doc comment).

  ```json
  [
    { "instance": "cubtera/network/dc:prod-use1", "state": "UpToDate" },
    { "instance": "cubtera/network/dc:stg1-use2", "state": "PackageDrifted" }
  ]
  ```

  `state` is one of `Desired` (matches, never applied), `UpToDate`,
  `PackageDrifted`, `Orphaned`. There is no server-side equivalent of
  `cubtera drift`'s filtered-to-drift-only view or its dedicated exit code
  - a client wanting that should filter this response client-side.

### Plan / apply / explain

- `POST /v1/{org}/units/{unit}/plan` - server-side `cubtera plan`. **Not**
  policy-gated (matches the CLI: plan never touches real infrastructure).

  Request body:
  ```json
  {
    "dims": ["dc:prod-use1"],
    "ext": [],
    "command": ["plan"],
    "actor": "ci-bot",
    "ttl_seconds": 3600
  }
  ```
  All fields except `dims` optional; `command` defaults to `["plan"]`,
  `actor` defaults to the `x-actor` header (or `"anonymous"`),
  `ttl_seconds` defaults to `3600`. Response is the persisted `Plan`
  (`id`, `instance`, `resolution` (`ResolutionManifest`), `artifact_digest`,
  `diff_summary`, `created_at`, `expires_at`).

- `POST /v1/{org}/units/{unit}/apply` - server-side `cubtera apply
  --plan`. **Policy-gated**: `crate::policy::check` evaluates the unit's
  `allowList`/`denyList` (compiled into a `Policy`/`Selector` via
  `Policy::from_allow_deny_lists`) against the actor and the first word of
  `command`, and returns `403 access-denied` before anything runs.

  Request body:
  ```json
  {
    "dims": ["dc:prod-use1"],
    "ext": [],
    "plan_id": "<id returned by /plan>",
    "command": ["apply"],
    "auto_approve": true,
    "actor": "ci-bot",
    "lease_ttl_seconds": 300,
    "outputs_schema_version": "1.0.0"
  }
  ```
  Only `dims` and `plan_id` are required. **This queues the run and
  returns immediately** with the `Run` row in `Queued` status - the actual
  execution happens in a background task
  (`RunUseCase::run_and_finish`, spawned via `tokio::spawn`). Poll `GET
  .../runs/{run_id}` for the final status/exit code, or open
  `.../runs/{run_id}/log/stream` for a live tail. `apply` re-checks the
  `Plan`'s pinned digests against the current state before doing anything
  real - if they've drifted (a module changed, the inventory moved, the
  plan expired), the run fails immediately with a validation error instead
  of running.

- `GET /v1/{org}/runs/{run_id}` - one `Run` record (same shape
  `cubtera explain run` prints): `id`, `instance`, `op`, `status`, `actor`,
  `plan_ref`, `started_at`, `finished_at`, `exit_code`, `logs_ref`,
  `produced_outputs_revision`.

- `GET /v1/{org}/runs/{run_id}/log` - the finished run's captured combined
  stdout+stderr as `text/plain`, read via `Run::logs_ref` (a content digest
  into `Store`'s artifact table). `404` if the run hasn't finished yet or
  never captured output.

- `GET /v1/{org}/runs/{run_id}/log/stream` (SSE, `text/event-stream`) -
  live-tails a run's output while it's in flight (subscribing to the
  server's in-memory `LogHub`); if the run isn't currently in flight
  (already finished, or this server process restarted mid-run), falls
  back to replaying the stored artifact as a single frame. Frames:
  `event: chunk` (one or more, `data:` is a slice of combined stdout+
  stderr) followed by a final `event: done`. A client can always open this
  endpoint rather than choosing between it and `/log` based on whether the
  run *looks* finished.

  ```bash
  curl -N -H "x-api-key: $CUBTERA_API_KEY" \
    "http://localhost:8081/v1/cubtera/runs/<run_id>/log/stream"
  ```

### Cross-unit output mesh

- `GET /v1/{org}/state?unit=<name>&dims=<type:name>[,...]&ext=<type:name>[,...]`
  - a producer's published `OutputSet` for the **exact** `dims`/`ext` key
  it ran with (not a consumer's `[inputs]` projection, which only happens
  inside `plan`/`apply`/`run`). `404` if nothing has been published under
  that key.

  ```json
  {
    "schema_version": "1.0.0",
    "values": { "vpc_id": { "Plain": "vpc-123" } },
    "produced_by": "<run id>",
    "source_hash": "...",
    "revision": 3
  }
  ```

  This reads the real P6 `OutputSet` table (`Store::get_output_set`) - it
  is a **different table** from the legacy `GET /v1/{org}/dlog`-adjacent
  unit-state data `cubtera state get` reads (see
  [AGENTS.md](../../AGENTS.md#cross-unit-output-mesh-inputsoutputs-outputset)).
  There is currently no server route for the legacy unit-state table.

- `GET /v1/{org}/state/stale` - every (consumer, producer) pair where the
  consumer last read an older `Revision` than the producer's current one
  (`Store::list_stale_consumers`). Same data as `cubtera state ls --stale`.

  ```json
  [
    {
      "consumer": "cubtera/bash_unit01/dc:prod-use1",
      "producer": "cubtera/tf_unit02/dc:prod-use1",
      "consumed_revision": 2,
      "current_revision": 3
    }
  ]
  ```

### Deployment log

- `GET /v1/{org}/dlog?q=<key:value>[,<key:value>...]&limit=<N>` - query
  deployment log entries for an org, newest first. `q` is a
  comma-separated list of `key:value` filters (repeated `q=` params aren't
  supported by the URL-encoded query deserializer, so every filter shares
  one param). Reads `cubtera-store`'s `legacy_deployment_log` table
  directly (`LegacyDeploymentLogRow::matches`) - the same table `cubtera
  migrate` imports old fs-jsonl logs into and `cubtera log get` reads.
  `limit` defaults to `10`.

  ```bash
  curl -H "x-api-key: $CUBTERA_API_KEY" \
    "http://localhost:8081/v1/cubtera/dlog?q=env:prod,command:apply&limit=5"
  ```

## Authorization on `apply`

`POST /v1/{org}/units/{unit}/apply` runs `crate::policy::check(unit, actor,
op)` before doing anything: it compiles the unit manifest's `allowList`/
`denyList` into a `Policy` (`Policy::from_allow_deny_lists`), builds a
`SelectorContext` from the resolved unit's dimensions plus pseudo-dimensions
`actor.name`/`op.name` (`op` = the first word of `command`), and evaluates
it. A `Deny` match, or an `Allow`-only policy with no match, returns `403
access-denied`. `POST .../plan` runs no such check (matching v2's
behavior: access policy has always only ever gated the run pipeline, never
a read-only plan/dry-run step).

## What's intentionally not here

- No `{status, id, data}` envelope, no query-string resource routing.
- No write endpoints for inventory or unit manifests - those are files on
  disk, edited out-of-band; the API only ever reads them (except for
  `plan`/`apply`, which run infrastructure commands, not inventory
  writes).
- No Mongo-backed anything - v3 dropped Mongo entirely; `InventoryPort`/
  `Store` are filesystem/SQLite only.
