# Terraform Unit Management

## Prerequisites for tf-commands usage:
1. configure your AWS credentials with `awscli config` or env vars
2. Set required configuration file `export CUBTERA_CONFIG=example/config.toml`
3. Set required organization name `export CUBTERA_ORG=cubtera`
4. `cubtera tf -d dc:staging1-us-e1 -d app:dashboard -u unit1 -- init`

if you want to use your own config file, you can create it in `~/.cubtera/config.toml` or set env var `CUBTERA_CONFIG` with any config file path

[More details about config.toml structure](.github/docs/config.md)

Example cli command:
`cargo run -- tf -d dc:staging1-us-e1 -d app:dashboard -u unit1 -- init`

Every dimension should be set as `-d <dimension_type>:<dimension_name>`.
Every unit should be set as `-u <unit_name>`.
You can set multiple dimensions, but only one unit.