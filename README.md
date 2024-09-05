[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg)](.github/CODE_OF_CONDUCT.md)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
# Cubtera
## Multi-dimensional infrastructure manager

Cli tool for multi-layer and multi-dimension infrastructures management.


## Features:
- [Terraform Units management](.github/docs/tf.md) with different dimensions from single inventory
- [Inventory management with CLI](.github/docs/im.md) using MongoDB or File (read-only) storage types
- [API inventory management](.github/docs/api.md) using MongoDB storage (read-only)
- [Unit's deployment logging](.github/docs/dlog.md) using separate MongoDB storage
- support of local files and/or DB storage for dimension's inventory persistence
- docker image for live API service

Cubtera could be configured with [config file](.github/docs/config.md) or with [environment variables](.github/docs/config.md#environment-variables).

## How it works

Cubtera is a tool for managing terraform units with different dimensions from single inventory. It allows you to separate your infrastructure into different dimensions, and manage it with single inventory.

Each unit is a separate terraform code, which could be applied with defined set of dimensions. Dimensions are a set of values, which could be used for infrastructure separation.

For example, you have a unit `aws_network_vpc` which is creating vpc network for your infrastructure, and you want to use it for different environments, like staging and production. You can create dimension, `dc` (data center), and inside this dimension folder create two files `staging.json` and `production.json` with the same set of variables, but with different values. 
production.json
```json
{ 
  "account_id": "123456789012",
  "cidr_block": "10.0.0.0/16",
  "availability_zones": ["us-east-1a", "us-east-1b", "us-east-1c"]
}
```
staging.json
```json
{ 
  "account_id": "9834895838923",
  "cidr_block": "10.100.0.0/16",
  "availability_zones": ["us-east-1a", "us-east-1b"]
}
```
And use this dimension to create vpc network for your staging and production environments, with the same unit, but with different values for variables. 
```bash
cubtera tf -d dc:production -u aws_network_vpc -- init
cubtera tf -d dc:production -u aws_network_vpc -- plan
cubtera tf -d dc:production -u aws_network_vpc -- apply
```
Your vpc network will be created in production environment with values from `production.json` file. Defined values will be provided to terraform unit as variables:
- `var.dim_dc_meta` - object from `production.json` file
- `var.dim_dc_name` - string `production

Cubtera takes responsibility for providing these variables to your terraform unit, and you can use them in your terraform code as any other variables.

### What is Terraform Unit?
Every terraform `unit` should be placed in separate folder in `inventory_path` folder, and should contain `main.tf` file with terraform code, and `unit_manifest.json` file with unit configuration. Folder name will be used as unit name.
Unit manifest file should contain following fields:
- `dimensions` - required dimensions which are required to apply this unit (required)
- `optDims` - optional dimensions which could be used to apply this unit (optional)
- `allowList` - allowed dimensions which could be used to apply this unit (optional)
- `denyList` - denied dimensions which could not be used to apply this unit (optional)
- `spec` - unit specification:
  - `spec.tfVersion` - terraform version which will be used to apply this unit (required)
  - `spec.envVars` - environment variables which will be used to apply this unit (optional)
    - `spec.envVars.optional` - optional environment variables which could be used to apply this unit
    - `spec.envVars.required` - required environment variables which are required to apply this unit
  - `spec.files` - files which will be used to apply this unit (optional)
    - `spec.files.optional` - optional files which could be used to apply this unit
    - `spec.files.required` - required files which are required to apply this unit

### What is Dimension?

Dimension is a set of values, which could be used for infrastructure separation. 
Every dimension type represented by separate folder in org inventory and configured by `config.toml`
*Dimension type* value is a folder name.

Each *dimension name* could be represented by set of files in dimension type folder starting with `<dimension_name>`, and separated with `:`.
Required file is `<dimension_name>.json`, which contains dimension values. If you want to add additional logic for your dimension, you can add `<dimension_name>:<any_name>.json`.

Values from `<dimension_name>.json` file will be provided to terraform unit as tf variable with name `dim_<dimension_type>_meta`, as object from json file.

Values from `<dimension_name>:<any_name>.json` file will be provided to terraform unit as tf variable with name `var.dim_<dimension_type>_<any_name>`, as object from manifest json file.

### What is Dimension Type?
Dimension type is a set of dimensions, which could be used for infrastructure separation.

<!-- Every dimension entry is represented with three separate files, which are:
- `<dimension_name>.json` - dimension values file (required)
- `<dimension_name>:manifest.json` - dimension manifest file (optional), could be used for ownership separation or other purposes
- `<dimension_name>:defaults.json` - dimension defaults file (optional), could be used to set default values for dimension -->

Every *dimension type* set of dimensions should be placed as separate files in `inventory_path/dimensions/<dimension_type>` folder, and should contain `<dimension_name>.json` file for each. 
File name will be used as `dimension name`, folder name will be used as `dimension type`.

Dimension json files could contain any json data, which will be provided to any terraform unit, started with this dimension name, as variables:
- Data from `dimension_name.json` will be provided with `dim_<dimension_type>_meta` variable in terraform unit, as object from json file
- Data from `dimension_name:manifest.json` will be provided with `var.dim_<dimension_type>_manifest` variable in terraform unit, as object from manifest json file
- Data from `dimension_name:defaults.json` will be provided with `var.dim_<dimension_type>_defaults` variable in terraform unit, as object from defaults json file
- `dimension_name` will be provided with `var.dim_<dimension_type>_name` variable in terraform unit, as string