# Cubtera examples

Fixtures for local development, golden/e2e tests
(`crates/cubtera-persistence/tests/golden_inventory.rs`,
`crates/cubtera/tests/e2e.rs`, `crates/cubtera/tests/e2e_run.rs`), and a
hands-on walkthrough of every runner type (`bash`, `tf`, `tofu`, `helm`).
Every unit here runs to a real, meaningful result with **zero logins and
zero cloud credentials** - `bash` just runs a script, `tf`/`tofu` only touch
local files (`hashicorp/local`/`hashicorp/random` providers, `local` state
backend), and `helm template` never contacts a cluster.

All commands below assume you're at the repo root. `terraform` self-downloads
the pinned version via `tfswitch` the first time it's used (needs network,
no local install required); `tofu` and `helm` must already be on `PATH` (see
"Installing tofu/helm" below).

## Inventory

`example/inventory/` is the dimension data every unit below resolves
against - `dome` -> `env` -> `dc` (see `dimRelations` in
[`config.toml`](config.toml)), plus a `service` dimension used as an
optional dimension. It's pinned by golden tests; don't restructure it
without updating those.

## `bash_unit01` (runner: `bash`)

Runs a script that prints its resolved dimension data and included files -
no external tool needed beyond `bash` itself.

```bash
cargo run -p cubtera -- -c example/config.toml run -u bash_unit01 -d dc:stg1-use2 -- deploy
```

Expected output: the `dc:stg1-use2` dimension's JSON (region, VPC CIDR),
a note that the `service` dimension is declared as optional (`optDims`)
but wasn't supplied, the contents of the unit's required/optional included
files, and a demonstration of a missing optional file being skipped
gracefully instead of failing the run.

Supply the optional `service` dimension by passing it like any other `-d`:

```bash
cargo run -p cubtera -- -c example/config.toml run -u bash_unit01 -d dc:stg1-use2 -d service:admin -- deploy
```

(Note: `-e`/`--ext` is a *different* feature - "extensions" like a shard
index, e.g. `-e index:0` - not how you supply an optional dimension.)

## `tf_unit02` (runner: `tf`, dimension: `dc`)

Creates a local file, a directory, and a random-named pet file - purely
local state (`state_backend = "local"`), no cloud resources.

```bash
cargo run -p cubtera -- -c example/config.toml run -u tf_unit02 -d dc:stg1-use2 -- init
cargo run -p cubtera -- -c example/config.toml run -u tf_unit02 -d dc:stg1-use2 --auto-approve -- apply
```

Check the printed `Temp folder: ...` path - it should now contain
`example.txt`, `pet.txt`, and a `terraform_created_dir/` directory. Clean up
with:

```bash
cargo run -p cubtera -- -c example/config.toml run -u tf_unit02 -d dc:stg1-use2 --auto-approve -- destroy
```

`example/units/cubtera/tf_unit02/` is the same unit overridden for the
`cubtera` org (`overwrite = true`) - run it with `-c example/config.toml`
as-is (its `[default].orgs` list already puts `cubtera` first), and note
its `random_pet.tf` produces different content (`hello.txt`/`pet.txt` from
`hello_world`/`random_pet`) than the generic unit's (`example.txt` from
`main.tf`) - `overwrite = true` means files with the same name in the
org-specific unit replace the generic unit's, not merge with them.

`example/units/tf_unit02/tests/` is a native `terraform test` fixture
(`cd example/units/tf_unit02 && tofu test`, or `terraform test`) - it's
independent of cubtera and exercises the module directly.

## `tf_unit01` (runner: `tofu`, dimension: `dome`)

Same idea with `tofu` instead of `terraform`, and a `local_file.user_data`
resource that writes admin/user lists to JSON:

```bash
cargo run -p cubtera -- -c example/config.toml run -u tf_unit01 -d dome:mgmt -- init
cargo run -p cubtera -- -c example/config.toml run -u tf_unit01 -d dome:mgmt --auto-approve -- apply
cargo run -p cubtera -- -c example/config.toml run -u tf_unit01 -d dome:mgmt --auto-approve -- destroy
```

Check the temp folder for `user_data.json` (`admins`: gary/wendy, `users`:
bill/jenny - matching `example/units/tf_unit01/user_data.json`, kept here
purely for reference).

## `helm_unit01` (runner: `helm`, dimension: `dc`)

Renders `values.yaml.tpl` from the resolved dimension data into
`values.yaml`, then runs `helm template` against a minimal chart - no
cluster, no kubeconfig:

```bash
cargo run -p cubtera -- -c example/config.toml run -u helm_unit01 -d dc:stg1-use2 -- template .
```

Expected output: a `ConfigMap` manifest with `region`/`environment` set
from the `dc:stg1-use2` dimension (`us-east-2`/`stg1-use2`).

## Installing tofu/helm locally

```bash
brew install opentofu helm   # macOS
```

CI installs both via `opentofu/setup-opentofu` and `azure/setup-helm` (see
`.github/workflows/code_pr_test.yaml`); `crates/cubtera/tests/e2e_run.rs`
skips its `tofu`/`helm` tests (without failing) when the binary isn't found.

## Known gap: `spec.env_vars`

`[spec.env_vars.optional]`/`[spec.env_vars.required]` are parsed into the
manifest schema, but nothing in the run pipeline consumes them yet -
`Unit::materialize` only acts on `spec.files`. The example manifests
document this in a comment rather than presenting non-functional config as
if it worked; wiring it up is a `cubtera-core`/`cubtera-domain` change, not
an examples one.
