# Deployment log (`cubtera log`)

`cubtera log get` queries a durable, append-only record of every `apply`/
`destroy` run against a unit - what ran, against which dimensions, with
what exit code, when. It is a **legacy, v2-shaped** read path: rows live
in `cubtera-store`'s `legacy_deployment_log` SQLite table
(`LegacyDeploymentLogRow`), the same table `cubtera migrate` imports old
fs-jsonl deployment logs into. It is **not** the same storage as the P6
cross-unit output mesh (`OutputSet`, `Store::put_output_set`) - see
[AGENTS.md](../../AGENTS.md#cross-unit-output-mesh-inputsoutputs-outputset)
for that.

Nothing in the current `plan`/`apply`/`run` pipeline writes new rows into
`legacy_deployment_log` - as of v3 this table is populated only by
`cubtera migrate` importing history from an older install. It is kept
around, and still queryable, purely so that history survives the
migration; it is not the audit trail for new v3 runs. For that, use
`cubtera explain run <run_id>` (backed by the `Run`/`Plan`/`Instance`
tables `plan`/`apply`/`run` actually write to) or `cubtera fleet
status`/`cubtera drift`.

## Usage

```bash
cubtera log get -q <key:value> [-q <key:value>...] [--limit N]
```

- `-q/--query` is repeatable. `unit`/`unit_name`, `command`, and
  `exit_code` match the corresponding entry field exactly; anything else
  (e.g. `env:prod`) is matched against the dimensions the run was against.
- `--limit` defaults to `10`, most recent first.

```bash
cubtera log get -q unit:network -q command:apply --limit 5
```

## REST equivalent

`GET /v1/{org}/dlog?q=<key:value>[,<key:value>...]&limit=<N>` - see
[api.md](api.md#deployment-log). Note the REST route takes a single
comma-separated `q` parameter (axum's query deserializer can't collect
repeated `q=` keys into a list), while the CLI's `-q` flag is repeatable.

## Related: unit state

`cubtera state get -u <unit> ...`, `cubtera state ls -u <unit>`, and
`cubtera state rm -u <unit> ...` are a separate, also-legacy read/delete
path over a different table (`legacy_unit_state`) for a producer unit's
published outputs, keyed by the exact `dims`/`ext` it ran with.

`cubtera state ls --stale` is the one exception in that same command group
- it does **not** read the legacy table at all. It reads the real (P6)
`OutputSet`/consumed-revision bookkeeping (`Store::list_stale_consumers`)
and lists every `[inputs]` consumer, org-wide, whose recorded revision is
behind its producer's latest published one - the same data
`GET /v1/{org}/state/stale` serves. See
[unit.md](unit.md#cross-unit-outputs-outputsinputs) for the current (P6)
output mesh, and [AGENTS.md](../../AGENTS.md#cli-commands) for the exact
CLI flags.
