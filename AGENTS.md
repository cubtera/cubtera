# Cubtera - AI Agent Context Guide

## Project Overview

**Cubtera** is a Multi-dimensional Infrastructure Manager - a CLI and API tool for managing Infrastructure as Code (IaC) across multiple dimensions (environments, regions, data centers, etc.). It enables running Terraform, OpenTofu, or Bash scripts with context-aware configuration.

### Key Value Proposition

- Run the same IaC code across different "dimensions" without duplication
- Hierarchical dimension relationships (e.g., `dome` → `env` → `dc`)
- Automatic state path generation based on dimensions
- Deployment logging with full audit trail
- REST API for programmatic access to inventory data

## Architecture

```
cubtera/
├── src/
│   ├── lib.rs              # Library entry point, exports prelude
│   ├── core/               # Core business logic
│   │   ├── cfg/            # Configuration (GLOBAL_CFG)
│   │   ├── dim/            # Dimension management
│   │   │   └── data/       # DataSource trait (FS/MongoDB)
│   │   ├── dlog/           # Deployment logging
│   │   ├── im/             # Inventory Management API
│   │   ├── runner/         # Execution engines (TF, Bash, Tofu)
│   │   └── unit/           # Unit & Manifest handling
│   ├── utils/              # Helper functions
│   └── bin/
│       ├── cli/            # CLI binary (cubtera)
│       └── api/            # API binary (cubtera-api)
├── tests/                  # Integration tests
│   ├── cli_tests.rs        # CLI command tests
│   └── api_tests.rs        # API endpoint tests
└── example/                # Test fixtures & example configs
```

## Core Concepts

### 1. Dimensions (`core/dim/`)

A **Dimension** is a logical grouping for infrastructure organization.

```
dome:prod          # Top-level dimension (e.g., production dome)
  └── env:prod     # Environment within dome
      └── dc:us-east-1  # Data center within env
```

**Key Files:**
- `dim/mod.rs` - `Dim` struct and `DimBuilder` for loading dimension data
- `dim/data/mod.rs` - `DataSource` trait, `Storage` enum (FS/DB)
- `dim/data/jsonfile.rs` - Filesystem JSON data source
- `dim/data/mongodb.rs` - MongoDB data source

**Dimension Data Structure:**
```json
{
  "name": "prod",
  "parent": "dome:prod",
  "meta": { "affinity_tags": ["production"] },
  "vpc_cidr": "10.0.0.0/16"
}
```

### 2. Units (`core/unit/`)

A **Unit** is an atomic infrastructure operation defined by a manifest.

**Manifest Structure (`manifest.toml`):**
```toml
dimensions = ["dome", "env", "dc"]  # Required dimensions
type = "tf"                          # Runner type: tf, bash, tofu
overwrite = true                     # Merge with generic unit
allowList = ["prod", "staging"]      # Allowed dimension values
denyList = ["dev"]                   # Denied dimension values
optDims = ["region"]                 # Optional dimensions

[runner]
state_backend = "s3"
version = "1.5.0"

[state]
bucket = "{{ org }}-tfstate"
key = "{{ dim_tree }}/{{ unit_name }}.tfstate"
```

**Key Files:**
- `unit/mod.rs` - `Unit` struct, file copying, dimension handling
- `unit/manifest.rs` - `Manifest` parsing from TOML

### 3. Runners (`core/runner/`)

Runners execute infrastructure code.

**Types:**
- `TF` - Terraform runner (with tfswitch version management)
- `TOFU` - OpenTofu runner (wraps TF runner)
- `BASH` - Bash script runner

**Lifecycle:**
```
copy_files → change_files → inlet → runner → outlet → logger
```

**Key Files:**
- `runner/mod.rs` - `Runner` trait, `RunnerBuilder`, `RunnerType`
- `runner/params.rs` - `RunnerParams` for version, state backend, etc.
- `runner/tf/mod.rs` - Terraform-specific implementation
- `runner/tf/tfswitch.rs` - Terraform version management
- `runner/bash/mod.rs` - Bash runner
- `runner/tofu/mod.rs` - OpenTofu runner

### 4. Configuration (`core/cfg/`)

Configuration is loaded from:
1. Default values (hardcoded)
2. Config file (`~/.cubtera/config.toml`)
3. Environment variables (`CUBTERA_*`)

**Important Environment Variables:**
```bash
CUBTERA_ORG          # Organization name (required)
CUBTERA_CONFIG       # Config file path
CUBTERA_WORKSPACE_PATH
CUBTERA_DB           # MongoDB connection string
CUBTERA_DLOG_DB      # Deployment log DB connection
CUBTERA_LOG          # Log level (error, warn, info, debug)
```

**Global Config Access:**
```rust
use crate::globals::GLOBAL_CFG;

let org = &GLOBAL_CFG.org;
let db_client = GLOBAL_CFG.db_client.clone();
```

### 5. Inventory Management (`core/im/`)

API functions for querying dimensions:

```rust
get_dim_by_name(dim_type, dim_name, org, storage, context)
get_dim_names_by_type(dim_type, org, storage)
get_dims_data_by_type(dim_type, org, storage)
get_dim_defaults_by_type(dim_type, org, storage)
get_dim_kids(dim_type, dim_name, org, storage)
get_dim_parent(dim_type, dim_name, org, storage)
get_all_orgs(storage)
get_dlog(org, filter, limit)
```

### 6. Deployment Logging (`core/dlog/`)

Tracks every deployment with:
- Unit name and dimensions
- Git SHAs (unit, inventory, dims)
- Terraform command and exit code
- Timestamp and job info
- Optional extended log data

## CLI Interface

```bash
# Configuration
cubtera config                    # Show current config

# Inventory Management
cubtera im getAll <dim_type>      # List dimension names
cubtera im getByName <type> <name># Get dimension data
cubtera im getDefaults <type>     # Get defaults for type
cubtera im getByParent <type> <name>  # Get children
cubtera im getParent <type> <name>    # Get parent
cubtera im getOrgs                # List organizations
cubtera im validate <type> <name> # Validate dimension

# Run Units
cubtera run -u <unit_name> -d <dim:value> [-d <dim:value>...] [-- <command>]
cubtera tf -u network -d dome:prod -d env:prod -d dc:us-east-1 -- plan

# Deployment Logs
cubtera log get -q <key:value> [-l <limit>]
```

## API Interface

```
GET  /health                        # Health check
GET  /v1/orgs                       # List organizations
GET  /v1/{org}/dimTypes             # List dimension types
GET  /v1/{org}/dims?type=           # List dims by type
GET  /v1/{org}/dim?type=&name=      # Get dimension by name
GET  /v1/{org}/dimDefaults?type=    # Get defaults
GET  /v1/{org}/dimParent?type=&name=    # Get parent
GET  /v1/{org}/dimsByParent?type=&name= # Get children
GET  /v1/{org}/dimsData?type=       # Get all data by type
```

**Response Format:**
```json
{
  "status": "ok",
  "id": "dimByName",
  "type": "env",
  "name": "prod",
  "data": { ... }
}
```

## Data Storage

### Filesystem (Storage::FS)

```
inventory/
└── {org}/
    ├── {dim_type}/
    │   ├── {dim_name}.json
    │   └── .defaults:{name}.json
    └── ...
```

### MongoDB (Storage::DB)

- Database per organization
- Collection per dimension type
- Documents with `name`, optional `context` field

## Key Patterns

### Error Handling

```rust
use crate::utils::helper::*;

// Exit on error
value.unwrap_or_exit("Error message".to_string());

// Log warning and continue
result.check_with_warn("Warning message");
```

### Handlebars Templating

State backend configs support templating:
```toml
[state.s3]
bucket = "{{ org }}-tfstate"
key = "{{ dim_tree }}/{{ unit_name }}.tfstate"
region = "us-east-1"
```

Available variables: `org`, `unit_name`, `dim_tree`

### Dimension Relations

Configured via `dim_relations` (colon-separated):
```toml
dim_relations = "dome:env:dc"
```

This defines the hierarchy: `dome` → `env` → `dc`

## Testing

### Run All Tests
```bash
cargo test
```

### Test Coverage Summary

| Module | Tests | Focus |
|--------|-------|-------|
| `core::cfg` | 20 | Config defaults, paths, serialization |
| `core::unit::manifest` | 24 | TOML parsing, validation |
| `core::unit` | 17 | Unit state path, dimensions |
| `core::runner` | 12 | Runner types, templating |
| `core::runner::params` | 14 | Params initialization |
| `core::dlog` | 14 | Log serialization |
| `core::im` | 22 | Dot notation, filters |
| `core::dim` | 14 | Dimension building |
| `core::dim::data` | 31 | Storage, DataSource |
| CLI integration | 31 | All CLI commands |
| API integration | 22 | Endpoints, formats |

### Test Fixtures

The `example/` directory contains test fixtures:
- `config.toml` - Example configuration
- `inventory/cubtera/` - Sample dimensions
- `units/` - Sample unit manifests

## Development Guidelines

### Adding a New Runner

1. Create `runner/{type}/mod.rs`
2. Implement `Runner` trait
3. Add to `RunnerType` enum in `runner/mod.rs`
4. Update `runner_create()` function

### Adding a New Dimension Operation

1. Add function in `core/im/mod.rs`
2. Add CLI subcommand in `bin/cli/cmd/im_command.rs`
3. Add API endpoint in `bin/api/api.rs`
4. Add tests

### Configuration Changes

1. Add field to `CubteraConfig` in `core/cfg/mod.rs`
2. Add default function if needed
3. Update `Default` implementation
4. Add tests

## Dependencies

Key crates:
- `clap` - CLI argument parsing
- `config` - Configuration loading
- `rocket` - HTTP API framework
- `mongodb` - MongoDB driver
- `serde` / `serde_json` / `toml` - Serialization
- `handlebars` - Templating
- `git2` - Git operations for SHA retrieval
- `walkdir` - Directory traversal

Dev dependencies:
- `assert_cmd` - CLI testing
- `predicates` - Assertion helpers
- `tempfile` - Temporary directories for tests
- `mockall` - Mocking (available but not heavily used)

## Common Tasks

### Get dimension data programmatically
```rust
use cubtera::prelude::*;

let dim = DimBuilder::new("env", "cubtera", &Storage::FS)
    .with_name("prod")
    .full_build();

let data = dim.get_dim_data();
```

### Create a unit and run it
```rust
let unit = Unit::new(
    "network".to_string(),
    &["dome:prod", "env:prod", "dc:us-east-1"],
    &[],
    &Storage::FS,
    None
).build();

let runner = RunnerBuilder::new(unit, vec!["plan".to_string()]).build();
runner.run()?;
```

### Query deployment logs
```rust
let logs = get_dlog_by_keys(
    "cubtera",
    vec!["env:prod".to_string(), "dc:us-east-1".to_string()],
    Some(10)
);
```

## Notes for AI Agents

1. **Always check GLOBAL_CFG** - Many operations depend on global config
2. **Storage type matters** - FS vs DB affects data source behavior
3. **Dimension hierarchy** - Parent relationships are crucial
4. **Exit vs Error** - Code often uses `exit_with_error()` for fatal errors
5. **Test fixtures** - Use `example/` directory for testing
6. **MongoDB optional** - Most features work with FS storage
7. **Context parameter** - Used for branching/PR-specific dimension data

