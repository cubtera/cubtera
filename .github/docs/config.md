# Cubtera configuration

## Configuration file

Cubtera reads a `config.toml` file, by default from `~/.cubtera/config.toml`.
Override the path with `--config <path>` (CLI) or the `CUBTERA_CONFIG` env
var. See `crates/cubtera-config/src/config.rs` for the loader
(`ConfigProvider`/`ConfigSource`) and `example/config.toml` for a
fully-annotated real example.

The file has a `[default]` table plus one table per org (matched by
`orgs`/`CUBTERA_ORG`), whose fields override `[default]`'s for that org.
Field names accept either camelCase or snake_case (`inventoryPath` and
`inventory_path` both work).

### Paths

- `inventoryPath` (default `inventory`) - root of the dimension inventory
  (see [`AGENTS.md`](../../AGENTS.md#inventory-on-disk-format) for the
  on-disk format).
- `unitsPath` (default `units`) - root of unit directories, one
  `manifest.toml` per unit.
- `modulesPath` (default `modules`) - symlinked into every `tf`/`tofu`
  unit's temp folder as `modules/`.
- `pluginsPath` (default `plugins`) - symlinked into every runner's temp
  folder as `plugins/`.
- `tempFolderPath` (default `~/.cubtera/temp`) - root under which every
  unit run materializes its temp folder.
- `storePath` (default `~/.cubtera/store.sqlite`) - the single SQLite file
  backing **everything** persistent: `Instance`/`Plan`/`Run`/`OutputSet`/
  lease rows for the v3 pipeline, plus the legacy `legacy_deployment_log`/
  `legacy_unit_state` tables `cubtera log`/`cubtera state` read (see
  [dlog.md](dlog.md)). There is no separate database or backend choice -
  no MongoDB, no `deploymentLogPath`/`unitStatePath`/`[deploymentLog]`/
  `[unitState]`/`CUBTERA_DB` (those keys are silently ignored if present in
  an old config - run `cubtera migrate` to clean them up and import their
  data).

Paths can be absolute or relative to the current working directory.

### Dimension relations

- `dimRelations` (default `["dome", "env", "dc"]`) - the parent chain used
  to resolve a dimension's ancestors and to decide which dimension types
  `cubtera fleet ls`/`cubtera validate` iterate by default. Each type in
  the list is a parent of the next; a dimension's own `meta.parent =
  "{type}:{name}"` (in its inventory JSON record) must point at a type
  earlier in this chain for the parent link to resolve.
- `orgs` (default `[]`) - known org names; the first one is used when
  `CUBTERA_ORG`/`--org` isn't set. Used by `im`/listing commands and to
  scope the REST API's `/v1/{org}/...` routes.
- `fileNameSeparator` (default `:`) - the separator between an inventory/
  unit file's name and its section/include suffix (`dc/prod-use1:manifest.json`).

### Development

- `alwaysCopyFiles` (default `false`) - copy unit files into the temp
  folder before *every* command, not just `init`. Useful for local dev
  where module contents change between runs without re-running `init`.
- `cleanCache` (default `false`) - remove the run's temp folder after a
  successful `apply`.

### Runner and state backend defaults

`[<org>.runner.<type>]` and `[<org>.state.<backend>]` (and their
`[default.runner.<type>]`/`[default.state.<backend>]` equivalents) are maps
of arbitrary string keys, merged **entry-by-entry** with a unit's own
`[runner]`/`[state]` table (the unit's own value wins per key, not per
table) - see [unit.md](unit.md) for what keys each runner type actually
reads. `[state.<backend>]` values are handlebars templates, rendered per
run with `{{org}}`, `{{dim_tree}}`, `{{unit_name}}`, etc.

```toml
[cubtera.runner.tf]
state_backend = "local"
inlet_command = "echo starting"
outlet_command = "echo done"

[cubtera.state.s3]
bucket = "{{org}}-example-state"
key = "{{dim_tree}}/{{unit_name}}.tfstate"
region = "us-east-1"

[cubtera.state.local]
path = "~/.cubtera/state/{{org}}/{{dim_tree}}/{{unit_name}}.tfstate"
```

### API key (server only)

- `apiKey` - not read from `config.toml` at all (the `Config` struct's
  `api_key` field is `#[serde(skip)]`); set the `CUBTERA_API_KEY`
  environment variable instead. Required by `cubtera-server`'s `/v1/*`
  routes and read by `cubtera-mcp` (as `--api-key`/`CUBTERA_API_KEY`) to
  authenticate against it.

## Environment variables

| Env var | Overrides |
| --- | --- |
| `CUBTERA_CONFIG` | which `config.toml` to load |
| `CUBTERA_ORG` | active org (else the first entry in `orgs`) |
| `CUBTERA_API_KEY` | shared secret for `cubtera-server`'s auth middleware and `cubtera-mcp`'s client |
| `CUBTERA_SERVER_ADDR` | `cubtera-server`'s listen address, default `0.0.0.0:8081` |
| `CUBTERA_SERVER_URL` | which `cubtera-server` `cubtera-mcp` talks to, default `http://127.0.0.1:8081` |

Only `CUBTERA_ORG` is strictly required to get started - without a
`config.toml` at all, every path field falls back to its documented
default under `~/.cubtera/`.

## Example

See `example/config.toml` in this repo for a complete, working, richly
commented reference used by the e2e test suite - it's the most
up-to-date source of truth for the schema if this document and the code
ever disagree.
