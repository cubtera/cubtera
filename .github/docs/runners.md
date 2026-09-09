# Runners

A **runner** is what actually executes a unit's command inside its
materialized temp folder. v3 expresses each runner as a
`cubtera_exec::runner::RunnerStrategy` implementation - a small trait
(`name`, `capabilities`, `binary`, `prepare`, `build_args`, `env_vars`,
`collect_outputs`, `normalize_outputs`) rather than the larger "owns the
whole pipeline" abstraction v1 had. `RunUseCase` (`cubtera-app`) owns the
actual pipeline (prepare → transform → inlet hook → execute → outlet hook
→ deployment log → publish outputs) once, for every runner type; a new
runner only has to implement the differences.

## Runner types

| Type (`manifest.toml`'s `type`) | Implementation | Plan artifact | Auto-collects `[outputs]` | Version pin |
| --- | --- | --- | --- | --- |
| `tf` | `TfLikeRunner::terraform()` | yes | yes | yes (tfswitch-equivalent) |
| `tofu` | `TfLikeRunner::opentofu()` | yes | yes | no |
| `bash` | `BashRunner` | no | no | no |
| `helm` | `HelmRunner` | no | no | no |

These booleans are `RunnerCapabilities` (`cubtera_exec::runner`) - a v3
addition with no v1/v2 equivalent, checked statically by `cubtera validate`
(a `bash`/`helm` unit with `[outputs] publish = true` is a validation
error, not a silent post-`apply` no-op) and used by `RunUseCase::plan` to
reject `plan`/`apply --plan` for runner types that don't support a plan
artifact (`bash`/`helm` - use `cubtera run` for those).

### `tf` / `tofu`

Both are the same `TfLikeRunner` implementation, parameterized by which
binary it resolves and whether it enforces a version pin. On `prepare()`:
every `cubtera_*.json` file already materialized into the temp folder
(dimension data, extension data, resolved `[inputs]`) gets mirrored into a
`*.auto.tfvars.json` file, and a `cubtera_vars.tf` is generated declaring
each top-level key as a Terraform `variable` block - so a unit's `.tf`
code can reference `var.dc`/`var.in_infra`/etc. without hand-writing
`variable` declarations.

`[runner] version = "1.6.6"` resolves/downloads that exact version (tf
only - tofu has no forced pin here); `runner_command = "..."` bypasses
version resolution entirely and runs that binary as-is. `[runner]
state_backend = "..."` picks which `[state.<backend>]` table (from
`config.toml`, gap-filled per-org) gets handlebars-rendered into a backend
config file for this unit.

After a successful `apply`/`destroy` with `[outputs] publish = true`,
`RunUseCase` invokes `RunnerStrategy::collect_outputs`, which runs `<tf or
tofu binary> output -json` as a **separate process** after the run's own
process has exited, reads the result, and `normalize_outputs` flattens
Terraform's `{name: {value, type, sensitive}}` shape into `{name: value}`
- wrapping any `sensitive = true` value as `OutputValue::Secret` instead
of inlining it. See [unit.md](unit.md#tftofu-tflikerunner) for the
manifest-level detail.

### `bash`

Executes the single `*.sh` file found in the unit's materialized temp
folder - there's no version/binary resolution concept for this runner, so
`[runner] version`/`runner_command`/`extra_args` are all ignored.
`[inputs.<alias>]` values are exposed both as `cubtera_in_<alias>.json`
files (every runner gets these) and as `CUBTERA_IN_<ALIAS>` environment
variables (JSON-encoded), which is the one thing `bash` does that `tf`/
`tofu`/`helm` don't.

Because `collects_outputs = false`, a `bash` producer that sets `[outputs]
publish = true` must write `cubtera_outputs.json` itself (flat `{name:
value}` JSON) - typically from `[runner] outlet_command = "..."` after the
script's own work is done.

### `helm`

Renders `values.yaml.tpl` (if the unit directory has one) with handlebars
against every `cubtera_*.json` file merged together, writes the result to
`values.yaml`, then runs `helm <command...>`. Same "doesn't auto-collect
outputs" caveat as `bash`.

## Inlet/outlet hooks

Every runner supports `[runner] inlet_command`/`outlet_command` - shell
commands run (via the same `ProcessRunner`) before/after the runner's own
process, inside the temp folder. `inlet_command` runs unconditionally
before `execute`; `outlet_command` only runs after a **successful**
execution. These are the mechanism `bash`/`helm` producers use to hand-roll
`cubtera_outputs.json` for the output mesh, and are also handy for
debugging (`inlet_command = "ls -la"`).

## Adding a new runner type

Implement `RunnerStrategy` in `cubtera-exec` (one new module, following
the `module.rs` + `module/` layout convention), set its
`RunnerCapabilities` honestly, and register it in both the CLI's
`ExecutorBridge` (`crates/cubtera/src/exec_bridge.rs`) and the server's
`ServerExecutor` (`crates/cubtera-server/src/exec_bridge.rs`) - **not** by
touching `RunUseCase`, which is deliberately runner-agnostic.
