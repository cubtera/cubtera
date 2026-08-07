# Dim Module Refactoring Plan

## Current Issues

### 1. **Monolithic Structure**
- `mod.rs` contains 692 lines with mixed responsibilities
- `Dim` and `DimBuilder` logic intertwined
- Complex hierarchy building logic

### 2. **Error Handling Problems**
- Uses `unwrap_or_exit()` and `exit_with_error()` in library code
- No proper error propagation with Result types
- Hard to test and handle errors gracefully

### 3. **Legacy Dependencies**
- Still uses old `utils::helper::*` through prelude
- Direct dependency on `GLOBAL_CFG`
- Makes testing and isolation difficult

### 4. **Complex DataSource Design**
- Trait objects with `Box<dyn DataSource>`
- Complex cloning mechanism through `CloneBox`
- Mixed FS and DB logic in single trait

## Refactoring Strategy

### Phase 1: Error Handling Migration
1. **Create `dim::error` module**
   - Define `DimError` enum with specific error types
   - Add `DimResult<T>` type alias
   - Create error conversion traits

2. **Replace exit patterns**
   - Convert `unwrap_or_exit()` to `Result` returns
   - Replace `exit_with_error()` with error propagation
   - Add compatibility layer for CLI usage

### Phase 2: Module Structure Reorganization
```
src/core/dim/
├── mod.rs              # Main exports and re-exports
├── error.rs            # Error types and handling
├── dim.rs              # Core Dim struct and methods
├── builder.rs          # DimBuilder implementation
├── hierarchy.rs        # Parent-child relationship logic
├── data/
│   ├── mod.rs          # DataSource trait and factory
│   ├── source.rs       # Improved DataSource trait
│   ├── jsonfile.rs     # File system implementation
│   ├── mongodb.rs      # MongoDB implementation
│   └── memory.rs       # In-memory implementation for testing
└── tests/
    ├── mod.rs
    ├── dim_tests.rs
    ├── builder_tests.rs
    └── integration_tests.rs
```

### Phase 3: DataSource Simplification
1. **Simplify DataSource trait**
   - Remove complex cloning mechanism
   - Use generic parameters instead of trait objects where possible
   - Separate read and write operations

2. **Improve factory pattern**
   - Type-safe data source creation
   - Better error handling for initialization
   - Configuration injection instead of global access

### Phase 4: Dependency Injection
1. **Remove GLOBAL_CFG dependencies**
   - Pass configuration as parameters
   - Create `DimConfig` struct for dim-specific settings
   - Make functions pure and testable

2. **Improve testability**
   - Add mock implementations
   - Create test utilities
   - Add comprehensive unit tests

## Implementation Steps

### Step 1: Create Error Module
```rust
// src/core/dim/error.rs
use crate::error::{CubteraError, CubteraResult};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DimError {
    #[error("Dimension not found: {name}")]
    NotFound { name: String },
    
    #[error("Invalid dimension format: {input}")]
    InvalidFormat { input: String },
    
    #[error("Data source error: {message}")]
    DataSource { message: String },
    
    #[error("Hierarchy error: {message}")]
    Hierarchy { message: String },
}

pub type DimResult<T> = CubteraResult<T>;
```

### Step 2: Extract Dim Struct
```rust
// src/core/dim/dim.rs
use super::error::{DimError, DimResult};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Dim {
    pub name: String,
    pub dim_type: String,
    pub key_path: PathBuf,
    dim_path: PathBuf,
    pub parent: Option<Box<Dim>>,
    data: Value,
    pub data_sha: String,
    pub kids: Option<Vec<String>>,
}

impl Dim {
    // Clean, focused methods with proper error handling
    pub fn get_data(&self) -> &Value { &self.data }
    pub fn get_data_mut(&mut self) -> &mut Value { &mut self.data }
    
    pub fn get_dim_tree(&self) -> Vec<String> {
        let mut tree = vec![self.name.clone()];
        if let Some(parent) = &self.parent {
            tree.extend(parent.get_dim_tree());
        }
        tree
    }
    
    // File operations with proper error handling
    pub fn save_dim_includes(&self, path: PathBuf) -> DimResult<()> {
        // Implementation with Result return
    }
}
```

### Step 3: Extract DimBuilder
```rust
// src/core/dim/builder.rs
use super::{Dim, DimConfig, DimResult};
use super::data::DataSource;

pub struct DimBuilder<T: DataSource> {
    config: DimConfig,
    data_source: T,
    // ... other fields
}

impl<T: DataSource> DimBuilder<T> {
    pub fn new(config: DimConfig, data_source: T) -> Self {
        // Clean constructor
    }
    
    pub fn with_name(mut self, name: &str) -> Self {
        // Builder pattern methods
    }
    
    pub fn build(self) -> DimResult<Dim> {
        // Build with proper error handling
    }
}
```

### Step 4: Improve DataSource
```rust
// src/core/dim/data/source.rs
use super::super::error::DimResult;
use serde_json::Value;

pub trait DataSource {
    fn get_data_by_name(&self, name: &str) -> DimResult<Value>;
    fn get_all_data(&self) -> DimResult<Vec<Value>>;
    fn get_all_names(&self) -> DimResult<Vec<String>>;
    // ... other methods with DimResult
}

pub trait DataSourceWrite: DataSource {
    fn upsert_data(&self, name: &str, data: Value) -> DimResult<()>;
    fn delete_data(&self, name: &str) -> DimResult<()>;
}
```

## Benefits

1. **Better Error Handling**
   - Proper error propagation
   - Testable error scenarios
   - Graceful error recovery

2. **Improved Modularity**
   - Clear separation of concerns
   - Easier to understand and maintain
   - Better testability

3. **Reduced Coupling**
   - No direct GLOBAL_CFG dependencies
   - Dependency injection
   - Pure functions where possible

4. **Enhanced Testing**
   - Mock implementations
   - Isolated unit tests
   - Integration test support

## Migration Strategy

1. **Backward Compatibility**
   - Keep old API working during transition
   - Add deprecation warnings
   - Provide migration guide

2. **Gradual Migration**
   - Start with error handling
   - Move to structure reorganization
   - Finally remove old code

3. **Testing Strategy**
   - Add tests for new code
   - Ensure existing functionality works
   - Add integration tests 