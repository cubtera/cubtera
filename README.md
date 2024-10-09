[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg)](.github/CODE_OF_CONDUCT.md)
[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Cubtera version release](https://github.com/cubtera/cubtera/actions/workflows/release_please.yaml/badge.svg?branch=main)](https://github.com/cubtera/cubtera/actions/workflows/release_please.yaml)
# Cubtera
## Multi-dimensional infrastructure manager

Cli tool for multi-layer and multi-dimension infrastructures management.

### Install
MacOS and Linux:
```bash
brew tap cubtera/cubtera
brew install cubtera
```
or download binary from [releases](https://ginhub.com/cubtera/cubtera/releases) and put it to your PATH.

Configure cli with [config file](.github/docs/config.md) or with [environment variables](.github/docs/config.md#environment-variables).

## Core Concepts

### Dimensions

Cubtera uses the concept of "dimensions" to organize and manage infrastructure. Dimensions can represent any logical grouping or layer of your infrastructure. Examples include:

- Cloud accounts (e.g., management, production, staging)
- Environments (e.g., dev, test, staging, production)
- Data centers (e.g., different regions or types of data centers related to one env)
- Applications (e.g., different applications or services with the same flows)
- And more (user-definable: databases, storages, domains, etc.)

Dimensions could be hierarchical and can have parent-child relationships, as well as flat dimensions with no relationships.

### Participants

Each dimension type can have multiple "participants," which are specific instances or configurations within that dimension. Participants are defined in JSON files and stored in the inventory.

### Units

Units are the smallest operational entities in Cubtera, representing specific scripts or actions that can be performed on your infrastructure by different tools like Terraform, Opentofu, Bash Scripts, Helm,  etc. 
Units are stored in the inventory and can be run with different dimensions values and using cubtera variables defined in the inventory and named following simple convention.

### Features
- [Units management](.github/docs/unit.md): the same IaC code module (tf, otf, bash, etc.) runs with different dimensions values
- [Inventory management](.github/docs/im.md): manage your inventory content with CLI commands
- [Runners plugins](.github/docs/runners.md): run your units with different runners (terraform, bash, etc.)
- [Inventory API server](.github/docs/api.md): using MongoDB storage (read-only)
- [Unit's deployment logging](.github/docs/dlog.md): log, view and monitor your deployments (BOM)
- support of local files and/or DB storage for dimension's inventory persistence
- docker image for live API service
- GH action for units runs in CI pipelines (WIP)

## How it works

Cubtera is a tool for managing IaC units with different dimensions from single inventory. It allows you to separate your infrastructure by different dimensions, and manage it with single inventory without code duplication (DRY).

Each unit is a separate bunch of code for chosen type of runner, such as terraform or bash script, which could be applied with defined set of dimensions. 
Dedicated dimension is a set of values, which could be used for infrastructure separation by different environments, regions, accounts, etc.

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
And use this dimension to create vpc network for your staging and production environments, with the same terraform unit, but with different values for variables. 
```bash
cubtera run -d dc:production -u aws_network_vpc -- init
cubtera run -d dc:production -u aws_network_vpc -- plan
cubtera run -d dc:production -u aws_network_vpc -- apply
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
- `type` - unit type, which will define which runner be used to run this unit (required). Currently supported types are `tf` and `bash`.
- `runner` - runner settings section (optional):
  - `runner.bin_path` - path to runner script or binary, which will be used to run this unit (optional). If not set, will be used default runner for this type.
  - `runner.version` - version of runner, which will be used to run this unit (optional). If not set, will be used `latest` version of runner for this type or default version from config.
  - 
- `spec` - unit specification:
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


# Cubtera Application Documentation

Cubtera is a Rust-based application designed to manage and orchestrate infrastructure deployments using Terraform. It provides a flexible architecture for executing various runner implementations, managing data sources, handling unit manifests, and offering a comprehensive CLI for interacting with different functionalities.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Runners](#runners)
    - [Runner Trait](#runner-trait)
    - [Terraform Runner (`TfRunner`)](#terraform-runner-tfrunner)
    - [Bash Runner (`BashRunner`)](#bash-runner-bashrunner)
    - [Tofu Runner (`TofuRunner`)](#tofu-runner-tofurnanner)
3. [Data Sources](#data-sources)
    - [DataSource Trait](#datasource-trait)
    - [MongoDB Data Source (`MongoDBDataSource`)](#mongodb-data-source-mongodbdatasource)
    - [JSON File Data Source (`JsonDataSource`)](#json-file-data-source-jsondatasource)
4. [Unit Manifest](#unit-manifest)
5. [Command-Line Interface (CLI)](#command-line-interface-cli)
    - [Inventory Management Commands (`im`)](#inventory-management-commands-im)
    - [Log Commands (`log`)](#log-commands-log)
    - [Run Commands (`run`/`tf`)](#run-commands-runtf)
6. [Terraform Version Switching](#terraform-version-switching)
7. [Testing](#testing)
8. [Utilities](#utilities)
9. [Conclusion](#conclusion)

---

## Architecture Overview

Cubtera follows a modular architecture, separating concerns into different components for flexibility and maintainability. The core components include:

- **Runners**: Define how different tasks are executed.
- **Data Sources**: Manage data retrieval and storage mechanisms.
- **Unit Manifest**: Defines the configuration and metadata for units.
- **CLI Commands**: Provides a command-line interface for interacting with the application.
- **Utilities**: Helper functions and tools supporting the main components.

This separation allows for easy extension and adaptation to various deployment scenarios and data management strategies.

---

## Runners

Runners in Cubtera implement the `Runner` trait, defining how specific tasks are executed. The application includes multiple runner implementations, each tailored for different execution environments or requirements.

### Runner Trait

```rust:path/to/src/core/runner/mod.rs
pub trait Runner {
    fn new(load: RunnerLoad) -> Self;
    fn get_load(&self) -> &RunnerLoad;
    fn get_ctx(&self) -> &Value;
    fn get_ctx_mut(&mut self) -> &mut Value;
    fn copy_files(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn runner(&mut self) -> Result<(), Box<dyn std::error::Error>>;
}
```

The `Runner` trait defines the essential methods that any runner must implement:

- **Initialization**: Creating a new runner instance with the necessary load parameters.
- **Context Management**: Accessing and modifying the runner's context.
- **File Operations**: Handling file copying or manipulation required before execution.
- **Execution**: The core method that runs the defined task.

### Terraform Runner (`TfRunner`)

```rust:path/to/src/core/runner/tf/mod.rs
impl Runner for TfRunner {
    fn new(load: RunnerLoad) -> Self { /* ... */ }
    fn get_load(&self) -> &RunnerLoad { /* ... */ }
    fn get_ctx(&self) -> &Value { /* ... */ }
    fn get_ctx_mut(&mut self) -> &mut Value { /* ... */ }
    fn copy_files(&mut self) -> Result<(), Box<dyn std::error::Error>> { /* ... */ }
    fn runner(&mut self) -> Result<(), Box<dyn std::error::Error>> { /* ... */ }
}
```

The `TfRunner` handles Terraform-specific operations, including:

- **Initialization**: Setting up the Terraform backend and state management.
- **Command Execution**: Running Terraform commands such as `init`, `plan`, `apply`, and `destroy`.
- **Variable Management**: Handling Terraform variables from environment variables or manifests.
- **Logging**: Integrating with the application's logging system to record deployment actions.

#### Terraform Version Switching

Cubtera includes functionality to manage different Terraform versions, ensuring compatibility and stability across deployments.

```rust:path/to/src/core/runner/tf/tfswitch.rs
pub fn tf_switch(tf_version: &str) -> Result<PathBuf, Box<dyn std::error::Error>> { /* ... */ }
```

The `tf_switch` function automates the downloading and switching of Terraform binaries based on the specified version, handling tasks like:

- **Version Parsing**: Validating and parsing the requested Terraform version.
- **Binary Management**: Downloading the appropriate Terraform binary if not already present.
- **Concurrency Handling**: Managing parallel downloads and avoiding race conditions.

### Bash Runner (`BashRunner`)

```rust:path/to/src/core/runner/bash/mod.rs
impl Runner for BashRunner {
    fn new(load: RunnerLoad) -> Self { /* ... */ }
    fn get_load(&self) -> &RunnerLoad { /* ... */ }
    fn get_ctx(&self) -> &Value { /* ... */ }
    fn get_ctx_mut(&mut self) -> &mut Value { /* ... */ }
}
```

The `BashRunner` provides a simple implementation to execute bash scripts or commands. It leverages the default methods provided by the `Runner` trait, enabling:

- **Script Execution**: Running predefined bash commands within the context of a unit.
- **Environment Management**: Setting up the necessary environment variables and context for script execution.

### Tofu Runner (`TofuRunner`)

```rust:path/to/src/core/runner/tofu/mod.rs
impl Runner for TofuRunner {
    fn new(load: RunnerLoad) -> Self { /* ... */ }
    fn get_load(&self) -> &RunnerLoad { /* ... */ }
    fn get_ctx(&self) -> &Value { /* ... */ }
    fn get_ctx_mut(&mut self) -> &mut Value { /* ... */ }
    fn copy_files(&mut self) -> Result<(), Box<dyn Error>> { /* ... */ }
    fn run(&mut self) -> Result<Value, Box<dyn std::error::Error>> { /* ... */ }
}
```

The `TofuRunner` is an extension of the `TfRunner`, tailored for OpenTofu operations. It wraps the `TfRunner` and provides additional functionalities or overrides specific behaviors as needed.

---

## Data Sources

Data sources in Cubtera manage how data is retrieved and stored. The application supports multiple data sources, allowing flexibility in choosing between different storage mechanisms.

### DataSource Trait

```rust:path/to/src/core/dim/data/mod.rs
pub trait DataSource: CloneBox + 'static {
    fn get_data_by_name(&self, name: &str) -> Result<Value, Box<dyn std::error::Error>>;
    fn get_all_data(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>>;
    fn get_all_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>>;
    fn get_all_types(&self) -> Result<Vec<String>, Box<dyn std::error::Error>>;
    fn upsert_all_data(&self, _data: Vec<Value>) -> Result<(), Box<dyn std::error::Error>>;
    // Additional methods...
}
```

The `DataSource` trait defines methods for data operations, including retrieval, insertion, and deletion.

### MongoDB Data Source (`MongoDBDataSource`)

```rust:path/to/src/core/dim/data/mongodb.rs
impl DataSource for MongoDBDataSource {
    fn get_data_by_name(&self, name: &str) -> Result<Value, Box<dyn std::error::Error>> { /* ... */ }
    fn get_all_data(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>> { /* ... */ }
    fn get_all_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> { /* ... */ }
    fn get_all_types(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> { /* ... */ }
    fn upsert_all_data(&self, data: Vec<Value>) -> Result<(), Box<dyn std::error::Error>> { /* ... */ }
    fn upsert_data_by_name(&self, name: &str, data: Value) -> Result<(), Box<dyn std::error::Error>> { /* ... */ }
    fn delete_data_by_name(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> { /* ... */ }
    fn delete_all_by_context(&self, context: &str) -> Result<(), Box<dyn std::error::Error>> { /* ... */ }
    fn set_context(&mut self, context: Option<String>) { /* ... */ }
    fn get_context(&self) -> Option<String> { /* ... */ }
    // Additional methods...
}
```

The `MongoDBDataSource` interacts with a MongoDB database to perform data operations. Key functionalities include:

- **Data Retrieval**: Fetching individual or all data entries.
- **Upsertion**: Inserting or updating data entries efficiently.
- **Deletion**: Removing data entries by name or context.
- **Context Management**: Handling contextual data to support multi-tenancy or scoped data access.

### JSON File Data Source (`JsonDataSource`)

```rust:path/to/src/core/dim/data/jsonfile.rs
impl DataSource for JsonDataSource {
    fn get_data_by_name(&self, name: &str) -> Result<Value, Box<dyn std::error::Error>> { /* ... */ }
    fn get_all_data(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>> { /* ... */ }
    fn get_all_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> { /* ... */ }
    fn get_all_types(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> { /* ... */ }
    fn set_context(&mut self, context: Option<String>) { /* ... */ }
    fn get_context(&self) -> Option<String> { /* ... */ }
    // Additional methods...
}
```

The `JsonDataSource` manages data stored in JSON files, allowing for:

- **File-Based Data Management**: Reading and writing JSON files to manage data entries.
- **Schema Handling**: Parsing and structuring JSON data into usable formats.
- **Contextual Data Access**: Similar to MongoDB, managing data based on context.

#### Testing `JsonDataSource`

Cubtera includes tests to ensure the reliability of the `JsonDataSource`:

```rust:path/to/src/core/dim/data/jsonfile.rs
#[cfg(test)]
mod tests {
    #[test]
    fn test_get_data_by_name() { /* ... */ }

    #[test]
    fn test_get_all_data() { /* ... */ }

    #[test]
    fn test_get_all_names() { /* ... */ }

    #[test]
    fn test_get_all_types() { /* ... */ }
    // Additional tests...
}
```

These tests validate the core functionalities of data retrieval, upsertion, and deletion, ensuring that the data source behaves as expected.

---

## Unit Manifest

The unit manifest defines the configuration, metadata, and specifications for a unit within Cubtera.

```rust:path/to/src/core/unit/manifest.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub dimensions: Vec<String>,
    pub overwrite: bool,
    pub opt_dims: Option<Vec<String>>,
    pub allow_list: Option<Vec<String>>,
    pub deny_list: Option<Vec<String>>,
    pub affinity_tags: Option<Vec<String>>,
    pub unit_type: String,
    pub spec: Option<Spec>,
    pub runner: Option<HashMap<String, String>>,
    pub state: Option<HashMap<String, String>>,
}
```

Key components of the `Manifest`:

- **Dimensions**: Defines the dimensions associated with the unit.
- **Overwrite & Lists**: Configuration flags and lists to control behavior.
- **Affinity Tags**: Tags to associate units with specific resources or environments.
- **Unit Type**: Specifies the type of unit, influencing how it is executed.
- **Specifications (`Spec`)**: Additional configurations like Terraform version, environment variables, and file specifications.
- **Runner Configuration**: Defines the runner to be used for the unit.
- **State Management**: Handles the state backend and other state-related configurations.

### Manifest Loading

```rust:path/to/src/core/unit/manifest.rs
impl Manifest {
    pub fn load(path: &Path) -> Result<Self> { /* ... */ }
}
```

The `load` function reads the manifest from a specified path, supporting both TOML and legacy JSON configurations.

---

## Command-Line Interface (CLI)

Cubtera offers a comprehensive CLI to interact with various functionalities, including inventory management, logging, and executing units.

### Inventory Management Commands (`im`)

Handles operations related to managing inventory data.

```rust:path/to/src/bin/cli/cmd/im_command.rs
pub fn get_command() -> Command { /* ... */ }

pub fn run(subcommand: &ArgMatches, storage: &Storage) { /* ... */ }
```

Key Subcommands:

- **getAll**: Retrieve all dimension names of a specified type.
- **getAllData**: Fetch all data entries for a given dimension type.
- **getDefaults**: Obtain default configurations for a dimension type.
- **getByName**: Retrieve data for a specific dimension (type and name).
- **getByParent / getParent**: Manage hierarchical relationships between dimensions.
- **getOrgs**: List all organizations from the configuration.
- **syncDefaults / syncAll / sync**: Synchronize data between storage backends (e.g., JSON and MongoDB).
- **deleteContext**: Remove data associated with a specific context.
- **validate**: Validate JSON data for a dimension (Note: Implementation pending).

### Log Commands (`log`)

Manage and retrieve deployment logs.

```rust:path/to/src/bin/cli/cmd/log_command.rs
pub fn get_command() -> Command { /* ... */ }

pub fn run(subcommand: &ArgMatches, _: &Storage) { /* ... */ }
```

#### Subcommand

- **get**: Fetch logs based on query parameters and limit the number of returned logs.

##### Usage Example

```sh
cubtera log get -q key1:value1 -q key2:value2 -l 20
```

### Run Commands (`run`/`tf`)

Execute units with specified dimensions and parameters.

```rust:path/to/src/bin/cli/cmd/run_command.rs
pub fn get_command() -> Command { /* ... */ }

pub fn run(sub_matches: &ArgMatches, storage: &Storage) { /* ... */ }
```

#### Arguments

- **dim** (`-d`): Specify dimension type and name (e.g., `-d dc:stg1-use1`).
- **ext** (`-e`): Define extensions for context-specific executions.
- **unit** (`-u`): Name of the unit to execute.
- **context** (`-c`): Optional context for scoped executions.
- **command**: Runner-specific commands (e.g., Terraform commands like `init`, `apply`).

##### Usage Example

```sh
cubtera run -d dc:stg1-use1 -u my-unit apply
```

---

## Terraform Version Switching

Cubtera manages Terraform versions to ensure compatibility and flexibility.

```rust:path/to/src/core/runner/tf/tfswitch.rs
pub fn tf_switch(tf_version: &str) -> Result<PathBuf, Box<dyn std::error::Error>> { /* ... */ }

fn get_latest() -> String { /* ... */ }
fn get_os() -> String { /* ... */ }
// Additional helper functions...
```

### Features

- **Automatic Download**: Downloads the specified Terraform version if not already present.
- **Version Parsing**: Validates and parses semantic versions.
- **Concurrency Handling**: Ensures thread-safe operations when switching versions.
- **OS Compatibility**: Supports different operating systems and architectures.

### Usage in `TfRunner`

The `TfRunner` utilizes the `tf_switch` function to set up the appropriate Terraform binary before executing any commands.

---

## Testing

Cubtera includes comprehensive tests to ensure the reliability and correctness of its components.

### Example: Testing `JsonDataSource`

```rust:path/to/src/core/dim/data/jsonfile.rs
#[cfg(test)]
mod tests {
    #[test]
    fn test_get_data_by_name() { /* ... */ }

    #[test]
    fn test_get_all_data() { /* ... */ }

    #[test]
    fn test_get_all_names() { /* ... */ }

    #[test]
    fn test_get_all_types() { /* ... */ }

    // Additional tests...
}
```

#### Test Cases

- **Data Retrieval**: Ensures that data can be correctly fetched by name.
- **Bulk Data Operations**: Validates retrieving all data entries.
- **Name Extraction**: Confirms that all dimension names are correctly identified.
- **Type Listing**: Checks that all dimension types are accurately listed.

### Running Tests

Execute the tests using the following command:

```sh
cargo test
```

---

## Utilities

Cubtera includes various helper functions and utilities to support its main functionalities.

### Helper Functions

- **Reading JSON Files**: Functions to read and parse JSON data.
- **Error Handling**: Standardized methods to handle and report errors.
- **Path Management**: Utilities for constructing and manipulating file system paths.
- **Environment Variable Management**: Handling environment variables for Terraform and other processes.

### Example: Error Handling

```rust:path/to/src/core/runner/tf/mod.rs
fn exit_with_error(message: String) -> ! {
    eprintln!("Error: {}", message);
    std::process::exit(1);
}
```

---

## Conclusion

Cubtera is a robust and flexible application designed to streamline infrastructure deployments using Terraform. By leveraging a modular architecture with distinct runners and data sources, it provides a versatile platform adaptable to various deployment scenarios. The comprehensive CLI further enhances its usability, allowing users to manage inventory, execute deployments, and handle logs with ease.

For further details and contributions, refer to the [Cubtera Repository](#).