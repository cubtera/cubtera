# Cubtera V2 - AI Agent Context Guide

## Project Overview

**Cubtera** is a Multi-dimensional Infrastructure Manager - a CLI and API tool for managing Infrastructure as Code (IaC) across multiple dimensions (environments, regions, data centers, etc.). It enables running Terraform, OpenTofu, or Bash scripts with context-aware configuration.

### Key Features

- Run the same IaC code across different "dimensions" without duplication
- Hierarchical dimension relationships (e.g., `dome` → `env` → `dc`)
- Automatic state path generation based on dimensions
- Deployment logging with full audit trail
- REST API for programmatic access to inventory data
- Multiple persistence backends (FileSystem, MongoDB, PostgreSQL)

---

## Architecture (Hexagonal / Ports & Adapters)

### Project Structure

```
cubtera/
├── Cargo.toml                 # Workspace root
│
├── crates/
│   │
│   │ ─────────── DOMAIN LAYER ───────────
│   ├── cubtera-domain/        # Pure business logic (zero dependencies)
│   │   └── src/
│   │       ├── dimension.rs   # Entity: Dimension, DimType, Hierarchy
│   │       ├── unit.rs        # Entity: Unit, StatePath
│   │       ├── manifest.rs    # Value Object: Manifest, Spec
│   │       ├── runner.rs      # Value Object: RunnerType, RunResult
│   │       └── error.rs       # Domain errors
│   │
│   │ ─────────── APPLICATION LAYER ───────────
│   ├── cubtera-core/          # Application Services + Ports
│   │   └── src/
│   │       ├── ports/         # Traits (interfaces)
│   │       │   ├── repository.rs
│   │       │   ├── runner.rs
│   │       │   └── deployment_log.rs
│   │       ├── services/      # Use cases
│   │       │   ├── dimension.rs
│   │       │   ├── unit.rs
│   │       │   └── runner.rs
│   │       └── app.rs         # Composition root
│   │
│   │ ─────────── INFRASTRUCTURE LAYER ───────────
│   ├── cubtera-persistence/   # Repository implementations
│   │   └── src/
│   │       ├── fs/            # FileSystem adapter
│   │       ├── mongodb/       # MongoDB adapter
│   │       ├── postgres/      # PostgreSQL adapter
│   │       └── factory.rs     # Repository factory
│   │
│   ├── cubtera-runners/       # Runner implementations
│   │   └── src/
│   │       ├── terraform.rs   # Module declaration (Rust 2018+ style)
│   │       ├── terraform/     # Terraform runner implementation
│   │       │   ├── runner.rs  # TerraformRunner
│   │       │   └── switch.rs  # tfswitch (version manager)
│   │       ├── opentofu.rs    # OpenTofu module
│   │       ├── opentofu/
│   │       │   └── runner.rs
│   │       ├── bash.rs        # Bash module
│   │       ├── bash/
│   │       │   └── runner.rs
│   │       └── factory.rs     # DefaultRunnerFactory
│   │
│   ├── cubtera-config/        # Configuration loading
│   │
│   │ ─────────── INTERFACE LAYER ───────────
│   ├── cubtera/               # CLI (binary: cubtera)
│   ├── cubtera-api/           # REST API server
│   ├── cubtera-mcp/           # MCP Server (future)
│   └── cubtera-web/           # Web UI (future)
│
├── v1/                        # Legacy code (reference only)
└── example/                   # Test fixtures
```

### Dependency Graph

```
                    ┌─────────────────────────────────────────────┐
                    │           INTERFACE LAYER                    │
                    │  ┌─────────┐ ┌─────────┐ ┌─────┐ ┌─────┐   │
                    │  │ cubtera │ │  api    │ │ mcp │ │ web │   │
                    │  │  (CLI)  │ │ server  │ │     │ │     │   │
                    │  └────┬────┘ └────┬────┘ └──┬──┘ └──┬──┘   │
                    └───────┼──────────┼─────────┼───────┼───────┘
                            │          │         │       │
                            ▼          ▼         ▼       ▼
                    ┌─────────────────────────────────────────────┐
                    │         APPLICATION LAYER                    │
                    │              cubtera-core                    │
                    │   ┌─────────────────────────────────────┐   │
                    │   │  Services    │    Ports (traits)    │   │
                    │   │  - Dimension │    - Repository      │   │
                    │   │  - Unit      │    - Runner          │   │
                    │   │  - Runner    │    - DeploymentLog   │   │
                    │   └──────────────┴──────────────────────┘   │
                    └──────────────────┬──────────────────────────┘
                                       │
                            ▼          ▼          ▼
                    ┌─────────────────────────────────────────────┐
                    │         INFRASTRUCTURE LAYER                 │
                    │  ┌────────────┐ ┌───────────┐ ┌──────────┐  │
                    │  │persistence │ │  runners  │ │  config  │  │
                    │  │ - fs       │ │ - tf      │ │          │  │
                    │  │ - mongodb  │ │ - tofu    │ │          │  │
                    │  │ - postgres │ │ - bash    │ │          │  │
                    │  └────────────┘ └───────────┘ └──────────┘  │
                    └─────────────────────────────────────────────┘
                                       │
                                       ▼
                    ┌─────────────────────────────────────────────┐
                    │            DOMAIN LAYER                      │
                    │            cubtera-domain                    │
                    │   Dimension, Unit, Manifest, Error          │
                    │   (zero external dependencies)              │
                    └─────────────────────────────────────────────┘
```

---

## Architecture Principles

### 1. Dependency Rule

Dependencies point inward only:
- **Domain** has ZERO external dependencies (only std)
- **Core** depends on Domain
- **Infrastructure** depends on Core + Domain
- **Interface** depends on all layers

```rust
// CORRECT: Infrastructure implements Core trait
impl DimensionRepository for FsDimensionRepository { ... }

// WRONG: Domain importing infrastructure
use mongodb::Client;  // Never in domain!
```

### 2. Ports and Adapters

All external systems are accessed through traits (ports):

```rust
// Port (in cubtera-core/src/ports/repository.rs)
pub trait DimensionRepository: Send + Sync {
    async fn find_by_name(&self, dim_type: &str, name: &str) -> Result<Option<Dimension>>;
    async fn find_all(&self, dim_type: &str) -> Result<Vec<Dimension>>;
    async fn save(&self, dim: &Dimension) -> Result<()>;
}

// Adapter (in cubtera-persistence/src/fs/dimension.rs)
pub struct FsDimensionRepository { /* ... */ }
impl DimensionRepository for FsDimensionRepository { /* ... */ }

// Adapter (in cubtera-persistence/src/mongodb/dimension.rs)  
pub struct MongoDimensionRepository { /* ... */ }
impl DimensionRepository for MongoDimensionRepository { /* ... */ }
```

### 3. Result-Based Error Handling

No `exit()` calls. All errors returned as `Result<T, Error>`:

```rust
// CORRECT
pub fn load_manifest(path: &Path) -> Result<Manifest, ManifestError> {
    let content = fs::read_to_string(path)?;
    let manifest: Manifest = toml::from_str(&content)?;
    Ok(manifest)
}

// WRONG
pub fn load_manifest(path: &Path) -> Manifest {
    let content = fs::read_to_string(path).unwrap_or_exit("Failed to read");
    // ...
}
```

### 4. Explicit Dependencies (No Global State)

All dependencies passed explicitly via constructors:

```rust
// CORRECT
pub struct DimensionService {
    repository: Arc<dyn DimensionRepository>,
    config: Arc<Config>,
}

impl DimensionService {
    pub fn new(repository: Arc<dyn DimensionRepository>, config: Arc<Config>) -> Self {
        Self { repository, config }
    }
}

// WRONG
pub fn get_dimension(name: &str) -> Dimension {
    let path = GLOBAL_CFG.inventory_path;  // No global state!
    // ...
}
```

### 5. Composition Root

All wiring happens in one place (App struct):

```rust
// cubtera-core/src/app.rs
pub struct App {
    pub dimension_service: DimensionService,
    pub unit_service: UnitService,
    pub runner_service: RunnerService,
}

impl App {
    pub fn new(config: Config) -> Result<Self> {
        // Create repositories based on config
        let dim_repo: Arc<dyn DimensionRepository> = match config.storage {
            StorageType::Fs => Arc::new(FsDimensionRepository::new(&config)),
            StorageType::MongoDB(ref conn) => Arc::new(MongoDimensionRepository::new(conn)?),
            StorageType::Postgres(ref conn) => Arc::new(PostgresDimensionRepository::new(conn)?),
        };
        
        // Create services
        let dimension_service = DimensionService::new(dim_repo.clone(), config.clone());
        // ...
        
        Ok(Self { dimension_service, /* ... */ })
    }
}
```

### 6. Test-First Development

Every module must have tests. Domain logic is easily testable:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn dimension_hierarchy_resolves_correctly() {
        let parent = Dimension::new("dome", "prod");
        let child = Dimension::new("env", "prod").with_parent(parent);
        
        assert_eq!(child.state_path(), "dome:prod/env:prod");
    }
}
```

---

## Core Concepts

### Dimension

A logical grouping for infrastructure organization:

```
dome:prod          # Top-level
  └── env:prod     # Environment within dome
      └── dc:us-east-1  # Data center within env
```

### Unit

An atomic infrastructure operation defined by a manifest (`manifest.toml`):

```toml
dimensions = ["dome", "env", "dc"]
type = "tf"

[runner]
version = "1.5.0"
state_backend = "s3"
```

### Runner

Executes infrastructure code. Types: `terraform`, `opentofu`, `bash`, etc.

---

## CLI Commands

```bash
# Configuration
cubtera config

# Inventory Management
cubtera im get-all <dim_type>
cubtera im get <dim_type> <name>
cubtera im get-defaults <dim_type>

# Run Units
cubtera run -u <unit> -d <dim:value> [-d <dim:value>...] [-- <command>]

# Deployment Logs
cubtera log get -q <key:value>
```

## API Endpoints

```
GET  /health
GET  /v1/orgs
GET  /v1/{org}/dimensions/{type}
GET  /v1/{org}/dimensions/{type}/{name}
GET  /v1/{org}/dimensions/{type}/defaults
```

---

## Development Commands

```bash
# Build all
cargo build

# Run CLI
cargo run -p cubtera -- im get-all env

# Run API server
cargo run -p cubtera-api

# Test specific crate
cargo test -p cubtera-domain

# Test all
cargo test --workspace

# Check formatting
cargo fmt --check

# Lint
cargo clippy --workspace
```

---

## Rust Conventions

### Module File Structure (Rust 2018+ style)

Use the modern module style - `module_name.rs` alongside `module_name/` directory instead of `module_name/mod.rs`:

```
# CORRECT (Rust 2018+)
src/
├── lib.rs
├── terraform.rs        # Module declaration
└── terraform/          # Submodules
    ├── runner.rs
    └── switch.rs

# AVOID (old style)
src/
├── lib.rs
└── terraform/
    ├── mod.rs          # Don't use mod.rs
    ├── runner.rs
    └── switch.rs
```

In `terraform.rs`:
```rust
//! Terraform runner module

mod runner;
mod switch;

pub use runner::TerraformRunner;
```

### Async Blocking Calls

When calling blocking code (e.g., `reqwest::blocking`) from async context, use `spawn_blocking`:

```rust
// CORRECT
tokio::task::spawn_blocking(move || blocking_function())
    .await
    .map_err(|e| AppError::runner(format!("Task error: {}", e)))?

// WRONG - will panic "Cannot drop a runtime in a context where blocking is not allowed"
blocking_function()  // Don't call blocking code directly in async fn
```

---

## Runner Pipeline Pattern

Runners use a pipeline pattern with customizable steps. Each runner can override specific steps while inheriting defaults.

### Pipeline Order

```
1. copy_files   → Copy unit files to temp folder
2. change_files → Transform files (e.g., JSON → tfvars)
3. inlet        → Pre-command execution (optional hook)
4. runner       → Main command execution
5. outlet       → Post-command execution (optional hook)
6. logger       → Logging/audit
```

### Key Files

- `Unit.temp_folder` - Persistent working directory per unit+dimensions
- `cubtera_dim_{type}.json` - Dimension data for terraform variables
- `cubtera_vars.tf` - Auto-generated variable declarations

### Terraform Runner Specifics

```rust
// TerraformRunner overrides:
// - copy_files: Removes temp folder only on "init", preserves for plan/apply
// - change_files: Renames cubtera_*.json to .auto.tfvars.json
// - runner: Handles version management via tfswitch
```

### Example Workflow

```bash
# 1. init: Creates temp folder, copies files, generates dim JSON files
cubtera run -u myunit -d dc:prod -- init

# 2. plan: Uses existing temp folder, runs terraform plan
cubtera run -u myunit -d dc:prod -- plan

# 3. apply: Uses existing temp folder, runs terraform apply
cubtera run -u myunit -d dc:prod --auto-approve -- apply
```

### Persistent Temp Folder Path

```
~/.cubtera/temp/{org}/{unit_name}/{dim:value}/{ext:value}
Example: ~/.cubtera/temp/cubtera/network/dc:prod/index:0
```

---

## Notes for AI Agents

1. **Follow the dependency rule** - Domain has no external deps
2. **Use traits for boundaries** - All external systems via ports
3. **No global state** - Pass dependencies explicitly
4. **Result everywhere** - No exit(), only Result<T, E>
5. **v1/ is reference only** - Don't modify, just look at for behavior
6. **Tests are mandatory** - Every PR needs tests
7. **English only in code** - Comments, docs, variable names
8. **Rust 2018+ module style** - Use `module.rs` + `module/` instead of `module/mod.rs`
9. **Runner pipeline** - Override only needed steps, inherit defaults
