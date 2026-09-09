# Inventory management (`cubtera im`)

`cubtera im` is the read-only CLI surface over the filesystem inventory
(`ResolveUseCase` / `FsInventoryPort` - see
[AGENTS.md](../../AGENTS.md#dimension) for the on-disk format and
[api.md](api.md#dimensions) for the equivalent REST routes). There is no
`sync*`/database-backed variant in v3 - Mongo support was dropped
entirely; the filesystem inventory *is* the inventory.

```bash
cubtera im get-types <org>              # list dimension types configured for an org
cubtera im get-all <dim_type>           # list dimension names of a type (e.g. `cubtera im get-all dc`)
cubtera im get <dim_type> <name>        # a single dimension, fully assembled (defaults gap-filled, parent resolved)
cubtera im get-defaults <dim_type>      # the type's `.default` record
cubtera im get-schema <dim_type>        # the type's `.schema:meta.json` JSON Schema, if any
cubtera im get-parent <dim_type> <name> # the assembled parent dimension
cubtera im get-children <dim_type> <name> # assembled child dimensions (per dimRelations)
cubtera im validate <dim_type> <name>   # existence check + JSON Schema validation against .schema:meta.json
```

Every command accepts `--json` (via the global CLI flag) for
machine-readable output. `<org>` defaults to `CUBTERA_ORG`/the first entry
in `config.toml`'s `orgs` where a command doesn't take it explicitly.

For a fleet-wide check (every dimension of every type, plus dim-graph
structural validation), use `cubtera validate` instead of looping
`cubtera im validate` yourself - see the top-level
[AGENTS.md](../../AGENTS.md#dimgraph). For "what's actually deployed vs.
what the inventory says should exist", use `cubtera fleet ls`/`cubtera
fleet status`, not `im` (which only ever reads static inventory data, never
`Store`).

## Example

```bash
export CUBTERA_CONFIG=example/config.toml

cubtera im get-types cubtera
# ["dc", "dome", "env"]

cubtera im get-all dc
# ["prod-use1", "stg1-use2", ...]

cubtera im get dc prod-use1
# { "key": "dc:prod-use1", "key_path": [...], "meta": {...}, "content_hash": "...", ... }
```
