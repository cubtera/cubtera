# Configuration Module (cfg)

## Overview

The `cfg` module provides centralized configuration management for Cubtera with proper error handling and validation.

## Architecture

```
src/core/cfg/
├── mod.rs          # Module exports and global config
├── config.rs       # Main CubteraConfig struct
├── loader.rs       # ConfigLoader for loading from multiple sources
├── error.rs        # Configuration-specific error types
└── README.md       # This file
```

## Key Components

### CubteraConfig
Main configuration structure with:
- **Path management**: workspace, inventory, units, modules, plugins, temp paths
- **Organization settings**: org, orgs list, dim_relations
- **Database configuration**: MongoDB connection strings and client
- **Runtime settings**: clean_cache, always_copy_files, file_name_separator
- **Runner/state configuration**: Custom runner and state backend settings

### ConfigLoader
Handles loading configuration from multiple sources with priority:
1. **Environment variables** (highest priority) - `CUBTERA_*`
2. **Organization-specific config section** - `[myorg]` in config file
3. **Default config section** - `[default]` in config file (lowest priority)

### Error Handling
- **ConfigError**: Specific error types for different failure modes
- **ConfigResult<T>**: Type alias for `CubteraResult<T>`
- **ConfigResultExt**: Extension trait for easy error conversion

## Usage

### Basic Usage
```rust
use cubtera::core::cfg::CubteraConfig;

// Load configuration from all sources
let config = CubteraConfig::load()?;

// Access configuration values
println!("Organization: {}", config.org);
println!("Workspace: {}", config.workspace_path);
```

### Global Configuration
```rust
use cubtera::prelude::GLOBAL_CFG;

// Access global configuration (initialized once)
let storage = match &GLOBAL_CFG.db_client.is_some() {
    true => Storage::DB,
    false => Storage::FS,
};
```

### Custom Configuration
```rust
use cubtera::core::cfg::{ConfigLoader, CubteraConfig};

// Load with custom settings
let loader = ConfigLoader::new()
    .with_env_prefix("MYAPP")
    .with_default_config_path("/custom/config.toml");

let config = CubteraConfig::load_with_loader(loader)?;
```

## Configuration Sources

### Environment Variables
- `CUBTERA_ORG` - Organization name
- `CUBTERA_CONFIG` - Path to config file
- `CUBTERA_WORKSPACE_PATH` - Workspace directory
- `CUBTERA_DB` - MongoDB connection string
- `CUBTERA_DLOG_DB` - Deployment log database
- And more...

### Config File Format (TOML)
```toml
[default]
org = "cubtera"
workspace_path = "~/.cubtera/workspace"
clean_cache = false
always_copy_files = true

[myorg]
org = "myorg"
db = "mongodb://localhost:27017/myorg"
```

## Validation

Configuration is automatically validated on load:
- **Required fields**: workspace_path, org, file_name_separator cannot be empty
- **Path validation**: All paths are resolved to absolute paths
- **Organization consistency**: Current org should be in orgs list (warning if not)
- **Database connection**: Attempts connection if configured (non-fatal)

## Error Handling Strategy

- **Critical errors**: Invalid configuration structure, missing required fields → exit application
- **Non-critical errors**: Database connection failures, missing config file → log warning and continue
- **Path resolution failures**: Log warning and use original path

## Migration from Old Helper

The module has been cleaned up to remove dependencies on the old `utils::helper` module:
- ✅ Uses new `tools` module for path operations and database connections
- ✅ Proper error handling with `ConfigError` types
- ✅ Consolidated validation logic in `CubteraConfig::validate()`
- ✅ Removed unused methods (`get_values()`, `get_toml()`)
- ✅ Clear separation of concerns between loader and config

## Testing

All configuration functionality is thoroughly tested:
- Unit tests for individual components
- Integration tests for complete loading workflow
- Error handling tests for various failure scenarios
- Validation tests for configuration constraints 