# Cubtera API (v2)

The API server (`cubtera-api`) exposes the same read model as the CLI's `im`
subcommands, going through the exact same `App`/service layer
(`DimensionService`, `UnitService`) — no query-parameter envelopes, no
`{status, id, data}` wrapping, just plain JSON resources.

Wave 1 shipped FS-backed inventory only; wave 2 added an opt-in MongoDB
`InventoryRepository` (selected via `CUBTERA_DB`, requires the server binary
to be built with `cubtera-persistence`'s `mongodb` feature, which is the
default for the `cubtera-api` binary) and a deployment log endpoint (see
below).

## Running

```bash
CUBTERA_CONFIG=/path/to/config.toml \
CUBTERA_API_ADDR=127.0.0.1:8080 \
CUBTERA_API_KEY=change-me \
cubtera-api
```

| Env var | Purpose | Default |
| --- | --- | --- |
| `CUBTERA_CONFIG` | Path to `config.toml` | see `cubtera-config` |
| `CUBTERA_API_ADDR` | Listen address | `0.0.0.0:8080` |
| `CUBTERA_API_KEY` | Shared secret required on every `/v1/*` request | none (server starts unauthenticated with a startup warning) |

If `CUBTERA_API_KEY` is unset, the server logs a warning at startup and
serves `/v1/*` without authentication — fine for local dev, not for anything
reachable over a network.

## Authentication

Every route under `/v1` requires the `x-api-key` header to match
`CUBTERA_API_KEY` (constant-time comparison). `/health` is always open, so
it can be used as a liveness/readiness probe without a key.

```bash
curl -H "x-api-key: $CUBTERA_API_KEY" http://localhost:8080/v1/orgs
```

Missing or invalid keys get a `401` `problem+json` body (see below).

## Error format

All errors are returned as `application/problem+json` ([RFC 7807](https://www.rfc-editor.org/rfc/rfc7807)):

```json
{
  "type": "not-found",
  "title": "Not Found",
  "status": 404,
  "detail": "dimension not found: dc:does-not-exist"
}
```

`type`/`status` mapping:

| `AppError` variant | `type` | HTTP status |
| --- | --- | --- |
| `NotFound` | `not-found` | 404 |
| `AccessDenied` | `access-denied` | 403 |
| `Validation` | `validation-error` | 400 |
| `Config` | `configuration-error` | 500 |
| everything else | `internal-error` | 500 |

## Endpoints

### Health

- `GET /health` — liveness probe, no auth. Returns `{"status":"ok","service":"cubtera-api","version":"..."}`.

### Orgs

- `GET /v1/orgs` — list configured orgs.

  ```json
  ["cubtera", "teracub"]
  ```

### Dimensions

All dimension routes are scoped under an org: `/v1/{org}/...`.

- `GET /v1/{org}/dim-types` — list dimension types known to the org (subdirectories of the inventory root).

  ```json
  ["dc", "dome", "env", "mongodb", "service"]
  ```

- `GET /v1/{org}/dims/{dim_type}` — list dimension names of a given type.

- `GET /v1/{org}/dims/{dim_type}/defaults` — the `.default` record for a type (merged meta), or `null` if none exists.

- `GET /v1/{org}/dims/{dim_type}/schema` — the raw JSON Schema from `.schema:meta.json` for the type, or `null` if none exists.

- `GET /v1/{org}/dims/{dim_type}/{name}` — a single dimension, fully assembled (defaults gap-filled, wrapped in `meta`, parent resolved, `key_path`/`data_sha`/`kids` computed).

  ```json
  {
    "name": "prod-use1",
    "type": "dc",
    "meta": { "parent": "env:prod", "region": "us-east-1", "vpc_cidr": "10.11.0.0/16" },
    "parent": "env:prod",
    "key_path": ["dome:prod", "env:prod", "dc:prod-use1"],
    "data_sha": "0e6dd6ec...",
    "kids": []
  }
  ```

- `GET /v1/{org}/dims/{dim_type}/{name}/parent` — the assembled parent dimension, or `null` if there is none.

- `GET /v1/{org}/dims/{dim_type}/{name}/children` — assembled child dimensions (per `dim_relations`).

> Route ordering note: static suffixes (`/defaults`, `/schema`) are declared
> before the dynamic `/{name}` segment so a dimension literally named
> `defaults` or `schema` can't shadow those endpoints. Reserved names should
> be avoided in inventory data regardless.

### Units

- `GET /v1/{org}/units` — list unit names discovered under the configured units path.

- `GET /v1/{org}/units/{name}` — the unit's manifest (parsed `manifest.toml`), as JSON. This is the manifest only — it does not resolve dimensions, run access policy, or materialize anything (that's `cubtera run`'s job).

### Deployment log

- `GET /v1/{org}/dlog?q=<key:value>[,<key:value>...]&limit=<N>` — query
  deployment log entries for an org, newest first. `q` is a comma-separated
  list of `key:value` filters (repeated `q=` params aren't supported by the
  URL-encoded query deserializer, so every filter shares one param); `unit`/
  `unit_name`, `command`, `exit_code` match the corresponding entry field
  exactly, anything else is matched against the dimensions the run was
  against. `limit` defaults to unbounded. Reads through whichever
  `DeploymentLogRepository` the server is configured with (fs-jsonl by
  default, MongoDB if `[deploymentLog]` is set) — same backend, same query
  semantics, as `cubtera log get`.

  ```bash
  curl -H "x-api-key: $CUBTERA_API_KEY" \
    "http://localhost:8080/v1/cubtera/dlog?q=env:prod,command:apply&limit=5"
  ```

  ```json
  [
    {
      "unit_name": "network",
      "org": "cubtera",
      "dimensions": ["dome:prod", "env:prod", "dc:prod-use1"],
      "command": "apply",
      "exit_code": 0,
      "timestamp": 1700000000,
      "duration_ms": 4213,
      "git_shas": {},
      "metadata": {}
    }
  ]
  ```

## What's intentionally not in v2

- No `{status, id, data}` envelope and no query-string routing
  (`/dims?type=`) — see "Что осознанно не переносим из v1" in the migration
  plan.
- No write endpoints — the API is read-only; mutations (`run`, `im sync*`)
  stay CLI-only.
