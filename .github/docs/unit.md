# Unit Concept and Management

Unit is a set of code, which could be applied to your infrastructure.
Every unit could be run with a different type of runner, like terraform, opentofu, ansible, bash, etc.
Every unit could use any set of dimensions, which could be used to separate your infrastructure.

## Unit configuration and structure

## Unit manifest

## Terraform Unit Type

### Prerequisites for tf-commands usage:
1. configure your AWS credentials with `awscli config` or env vars
2. Set required configuration file `export CUBTERA_CONFIG=example/config.toml`
3. Set required organization name `export CUBTERA_ORG=cubtera`
4. `cubtera run -d dc:staging1-us-e1 -d app:dashboard -u unit1 -- init`

if you want to use your own config file, you can create it in `~/.cubtera/config.toml` or set env var `CUBTERA_CONFIG` with any config file path

[More details about config.toml structure](.github/docs/config.md)

Example cli command:
`cargo run -- tf -d dc:staging1-us-e1 -d app:dashboard -u unit1 -- init`

Every dimension should be set as `-d <dimension_type>:<dimension_name>`.
Every unit should be set as `-u <unit_name>`.
You can set multiple dimensions, but only one unit.