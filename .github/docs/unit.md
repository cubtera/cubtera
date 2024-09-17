# Unit Concept and Management

**Unit** is a set of code, which could be applied to your infrastructure.
Every unit could be run with a different type of runner, like terraform, opentofu, ansible, bash, etc.
Every unit could use any set of dimensions, which could be used to separate your infrastructure.

Cubtera takes responsibility for providing these variables to your unit, and you can use them in your code as any other variables.
After you run your unit, cubtera will create `temp folder`, copy all required files to this folder, and collect all `dimension variables` to your unit following provided dimension's names, and will run it with a chosen runner.

## Unit configuration and structure
Every unit should be placed in separate folder in `units` folder from your config, and should contain `manifest.toml` file with unit configuration. **Folder name will be used as unit name**.

### Unit manifest
```toml
dimensions = ["dc"] # List of required dimensions for this unit
allow_list = ["stg1"] # List of allowed dimensions names for this unit 
deny_list = ["stg2"] # List of denied dimensions names for this unit
type = "tf" # Type of unit (runner), currently supported types are `tf` and `bash`

[spec.env_vars.optional]
some_optional_var = "SOME_OPTIONAL_VAR"

[spec.env_vars.required]
some_required_var = "PWD"

[spec.files.optional]
"main.tf" = "test_optional.txt"

#[spec.files.required]
#"main.tf" = "test_required.txt"
```

#### TF Runner supported fields
```toml
[runner]
version = "1.6.6" # if not set and bin_path is not set, it will run latest version of runner if supported
bin_path =  "~/.cubtera/tf/1.9.5/terraform" # if defined - version will be ignored
extra_params = "--detailed-output" # extra params for runner
state_backend = "local" # in case of your unit requires local state, default is S3 for tf runner

```

#### Bash Runner supported fields
```toml
[runner]
bin_path =  "./runner.sh" # path to runner script or binary, which will be used to run this unit.
extra_params = "make some noise" # extra params for the binary
```
any other runner fields will be ignored, and will not be used for this runner.

## Terraform Unit Type

### Prerequisites for units test from examples:
1. Set required configuration file `export CUBTERA_CONFIG=example/config.toml`
2. Run `cubtera run -d dc:staging1-us-e1 -d app:dashboard -u unit1 -- init`

if you want to use your own config file, you can create it in `~/.cubtera/config.toml` or set env var `CUBTERA_CONFIG` with any config file path

[More details about config.toml structure](.github/docs/config.md)

Example cli command:
`cargo run -- tf -d dc:staging1-us-e1 -d app:dashboard -u unit1 -- init`

Every dimension should be set as `-d <dimension_type>:<dimension_name>`.
Every unit should be set as `-u <unit_name>`.
You can set multiple dimensions, but only one unit for one run command