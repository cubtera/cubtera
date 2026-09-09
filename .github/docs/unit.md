# Unit concept and management

A **unit** is a piece of infrastructure code that can be run against many
different dimension combinations without duplicating the code per target.
A unit can be a Terraform module, an OpenTofu module, a Bash script, or a
Helm chart.

Cubtera resolves the dimensions you pass on the command line, builds a
temp folder containing the unit's files plus generated
`cubtera_dim_<type>.json`/`cubtera_ext.json`/`cubtera_in_<alias>.json`
data files, and runs the configured runner inside it.

## Unit configuration and structure

Every unit lives in its own folder under the configured `unitsPath`, and
must contain a `manifest.toml`. **The folder name is the unit's name.**

```
units/
└── tf_unit02/
    ├── manifest.toml
    ├── main.tf
    └── greeting.txt
```

### Unit manifest

```toml
dimensions = ["dc"]          # required dimension types for this unit
optDims = ["service"]        # optional dimension types - resolved if passed with -d, ignored otherwise
allowList = ["dc:stg1-use2"] # access policy: allow-listed "type:name" refs (matched against the full resolved key_path)
denyList = []                # access policy: deny-listed "type:name" refs - checked before allowList
affinityTags = []            # legacy v1/v2 access-policy field, still parsed and evaluated by AccessPolicy::evaluate
type = "tf"                  # runner type: "tf" | "tofu" | "bash" | "helm"

[spec.files.required]
"main.tf" = "main.tf"                     # project-relative source -> temp-folder-relative dest; missing source is a hard error

[spec.files.optional]
"greeting.txt" = "greeting.txt"           # missing source is silently skipped, not an error

# Parsed into the manifest schema but NOT wired into the run pipeline yet -
# Unit::materialize only acts on spec.files. Don't rely on this working.
# [spec.env_vars.optional]
# some_optional_var = "SOME_OPTIONAL_VAR"
# [spec.env_vars.required]
# some_required_var = "PWD"

[runner]
version = "1.6.6"          # tf only - if unset and runner_command is unset, resolves/downloads the latest tf version
# runner_command = "terraform" # if set, run this binary as-is instead of tfswitch-style version resolution (ignores `version`)
# extra_args = "-json"         # extra CLI args appended after the runner's own args
state_backend = "local"        # which [state.<backend>] table (config.toml, gap-filled by org) to render for this unit
# inlet_command = "echo starting"  # runs before the runner, inside the temp folder
# outlet_command = "echo done"     # runs after a successful run, inside the temp folder

[outputs]
publish = true             # after a successful apply/destroy, publish this unit's outputs for other units to read (see below)
```

Access control notes:

- `allowList`/`denyList` entries **must** be `type:name` (e.g. `"dome:mgmt"`,
  `"env:stg1"`), never a bare name - matching is against the unit's full
  resolved `key_path` set (every ancestor, not just the leaf dimension). A
  bare `"stg1"` will never match, silently denying every run.
- Evaluated by `cubtera_model::access::AccessPolicy::evaluate` inside
  `AssembleUseCase::build_unit_with_extensions` (CLI's `cubtera
  run`/`plan`/`apply` path). `cubtera-server`'s `apply` route additionally
  runs a second, `Selector`-based `Policy` check
  (`cubtera_model::policy::Policy::from_allow_deny_lists`,
  `crate::policy::check` in `cubtera-server`) compiled from the same
  `allowList`/`denyList` fields - see [`api.md`](api.md#authorization-on-apply).
  These are two separate evaluations of the same source lists, not a
  shared code path.

### Runner-specific fields

#### `tf`/`tofu` (`TfLikeRunner`)

```toml
[runner]
version = "1.6.6"        # tf: resolves/downloads via tfswitch-equivalent logic; tofu: has no forced version pin
runner_command = "terraform"  # overrides version resolution entirely
extra_args = "-json"
state_backend = "local"  # picks a [state.<backend>] table to render into a backend config file
```

On `prepare()`, `TfLikeRunner` transforms every `cubtera_*.json` file in
the temp folder into `*.auto.tfvars.json` and generates a `cubtera_vars.tf`
declaring each of those as a Terraform variable, so unit code can
reference dimension/extension/input data as ordinary `var.*` values
without hand-writing `variable` blocks. After a successful `apply`/
`destroy` (only if `[outputs] publish = true`), it runs `<binary> output
-json` as a fresh process and normalizes the result (`{name: {value,
type, sensitive}}` → `{name: value}`, wrapping `sensitive = true` fields
as `OutputValue::Secret`).

`RunnerCapabilities` for this runner: `supports_plan_artifact = true`,
`collects_outputs = true`, `pins_version` = `true` for `tf`, `false` for
`tofu`, `needs_identity = false`.

#### `bash` (`BashRunner`)

```toml
type = "bash"
[runner]
inlet_command = "echo preparing"
outlet_command = "echo finished"
```

`BashRunner` always executes the single `*.sh` file it finds in the unit's
temp folder - `runner_command`/`version`/`extra_args` have no effect for
this runner type. It exposes each resolved `[inputs.<alias>]` value as a
`CUBTERA_IN_<ALIAS>` environment variable (JSON-encoded), in addition to
the `cubtera_in_<alias>.json` files every runner gets.

`bash` never auto-collects outputs (`collects_outputs = false`) - if the
unit sets `[outputs] publish = true`, it must write
`cubtera_outputs.json` itself (flat `{name: value}` JSON), typically via
`[runner] outlet_command = "..."`. `cubtera validate` flags a `publish =
true` bash/helm unit as an error precisely because of this, rather than
letting it fail silently after `apply`.

`RunnerCapabilities`: all fields `false` (no plan artifact, no output
collection, no version pin, no identity requirement) - `cubtera plan`
rejects bash units entirely; use `cubtera run` for them.

#### `helm` (`HelmRunner`)

```toml
type = "helm"
```

If the unit directory has a `values.yaml.tpl`, it's rendered with
handlebars against every merged `cubtera_*.json` file in the temp folder
(dimension data, extension data, and any resolved `[inputs]`) and written
to `values.yaml` before `helm <command...>` runs. Same output-collection
caveat as `bash`: `helm` doesn't auto-generate `cubtera_outputs.json`.

## Cross-unit outputs (`[outputs]`/`[inputs]`)

A unit can publish its outputs for other units to read, without a DAG and
without ever auto-running the producer:

```toml
# Producer (example/units/tf_unit02/manifest.toml)
[outputs]
publish = true
```

```toml
# Consumer (example/units/bash_unit01/manifest.toml)
[inputs.infra]
unit = "tf_unit02"
required = false   # default true - a missing producer state fails the consumer's run
# dims = ["dc:prod-use1"]   # optional explicit override; left unset, projected from the consumer's own resolved dims
# ext = []
```

Publishing only fires after a successful `apply`/`destroy` (never `plan`/
`init`) if the manifest opts in. Left unset, a consumer's `dims`/`ext` are
*projected* from its own resolved dimension chain onto the producer's
required dimensions
(`cubtera_model::state_projection::project_state_key`) - a producer
dimension type the consumer never resolved, or an ambiguous match, is a
hard error, never a guess.

Resolved inputs materialize in the consumer's temp folder as
`cubtera_in_<alias>.json` (`{"in_<alias>": {...}}`) plus an aggregate
`cubtera_inputs.json` (`{<alias>: {...}}`); `BashRunner` also exposes
`CUBTERA_IN_<ALIAS>` env vars. See
[AGENTS.md](../../AGENTS.md#cross-unit-output-mesh-inputsoutputs-outputset)
for the full producer/consumer/staleness model.

## Running a unit

```bash
export CUBTERA_CONFIG=example/config.toml

# Direct execution, no plan artifact (works for every runner type):
cubtera run -u tf_unit02 -d dc:prod-use1 -- init
cubtera run -u tf_unit02 -d dc:prod-use1 -- apply

# Reviewed plan -> apply (tf/tofu only):
cubtera plan -u tf_unit02 -d dc:prod-use1 -- plan
cubtera apply --plan <plan_id> -u tf_unit02 -d dc:prod-use1 -- apply
```

Every dimension is `-d <type>:<name>`; you can pass multiple `-d` flags.
Exactly one unit per run (`-u <name>`). `--dry-run` prints the
`MaterializationPlan` without touching disk or running anything.
