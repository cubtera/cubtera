# Phase 3: Structure Reorganization Plan

## Current State Analysis

### File Structure:
```
src/core/dim/
├── mod.rs              # 517 lines - MONOLITHIC
├── error.rs            # 139 lines - ✅ Already well organized
├── data/               # Data source implementations
│   ├── mod.rs
│   ├── jsonfile.rs
│   └── mongodb.rs
├── tests/              # Test modules - ✅ Well organized
│   ├── mod.rs
│   ├── hierarchy_tests.rs
│   ├── file_operations_tests.rs
│   ├── file_operations_advanced_tests.rs
│   ├── data_operations_tests.rs
│   ├── legacy_tests.rs
│   └── mock_helpers.rs
└── documentation files
```

### Current mod.rs Content Analysis:
- **Lines 1-30**: Imports and module declarations
- **Lines 31-160**: `Dim` struct and its methods (~130 lines)
  - Core data access methods
  - File operations (save_dim_includes, save_dim_folders)
  - JSON variable generation
  - Tree traversal methods
- **Lines 161-517**: `DimBuilder` struct and its methods (~356 lines)
  - Builder pattern implementation
  - Data persistence operations
  - Default data handling
  - CLI integration methods

## Reorganization Strategy

### Phase 3.1: Extract Dim Struct
**Goal**: Move `Dim` struct to separate module
**Files to create**:
- `src/core/dim/dim.rs` - Core Dim struct and basic methods
- `src/core/dim/file_ops.rs` - File operation methods

### Phase 3.2: Extract DimBuilder
**Goal**: Move `DimBuilder` to separate module
**Files to create**:
- `src/core/dim/builder.rs` - DimBuilder struct and methods

### Phase 3.3: Extract Hierarchy Logic
**Goal**: Separate hierarchy-related functionality
**Files to create**:
- `src/core/dim/hierarchy.rs` - Parent-child relationship logic

### Phase 3.4: Refactor mod.rs
**Goal**: Clean main module file
**Result**: `mod.rs` becomes a clean public API interface

## Detailed Implementation Plan

### Step 1: Create dim.rs (Dim struct)
```rust
// src/core/dim/dim.rs
use crate::prelude::*;
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct Dim {
    pub dim_name: String,
    pub dim_type: String,
    pub key_path: PathBuf,
    dim_path: PathBuf,
    pub parent: Option<Box<Dim>>,
    data: Value,
    pub data_sha: String,
    pub kids: Option<Vec<String>>,
}

impl Dim {
    // Basic data access methods
    pub fn get_data(&self) -> &Value { ... }
    pub fn get_data_mut(&mut self) -> &mut Value { ... }
    pub fn get_dim_data(&self) -> Value { ... }
    
    // Tree traversal methods
    pub fn get_dim_tree(&self) -> Vec<String> { ... }
    
    // JSON variable generation
    pub fn get_json_dim_vars(&self) -> Value { ... }
    pub fn save_json_dim_vars(&self, path: PathBuf) -> Result<String, std::io::Error> { ... }
}
```

### Step 2: Create file_ops.rs (File Operations)
```rust
// src/core/dim/file_ops.rs
use super::Dim;
use std::path::{Path, PathBuf};

impl Dim {
    pub fn save_dim_includes(&self, path: PathBuf) -> Result<(), std::io::Error> { ... }
    pub fn save_dim_folders(&self, path: PathBuf) -> Result<(), std::io::Error> { ... }
    
    // Private helper methods
    fn get_filtered_entries<F>(&self, prefix: &str, entry_filter: F) -> std::io::Result<Vec<PathBuf>> { ... }
    fn process_dim_entries<F>(&self, path: PathBuf, entry_filter: F) -> Result<(), std::io::Error> { ... }
    fn copy_entry(&self, src: &Path, dest: &Path, is_dir: bool) -> std::io::Result<()> { ... }
}
```

### Step 3: Create builder.rs (DimBuilder)
```rust
// src/core/dim/builder.rs
use super::{Dim, data::*};
use crate::prelude::*;
use serde_json::Value;
use std::path::PathBuf;
use std::collections::HashMap;

pub struct DimBuilder {
    dim_name: String,
    dim_type: String,
    org: String,
    dim_path: PathBuf,
    data: Value,
    default_data: Value,
    datasource: Box<dyn DataSource>,
    storage: Storage,
}

impl DimBuilder {
    // Creation methods
    pub fn new(dim_type: &str, org: &str, storage: &Storage) -> Self { ... }
    pub fn new_from_cli(dim: &str, org: &str, storage: &Storage, context: Option<String>) -> Dim { ... }
    pub fn new_undefined(dim_type: &str) -> Self { ... }
    
    // Builder pattern methods
    pub fn with_name(mut self, dim_name: &str) -> Self { ... }
    pub fn with_context(mut self, context: Option<String>) -> Self { ... }
    
    // Data operations
    pub fn save_data(&self) { ... }
    pub fn delete_data(&self) { ... }
    pub fn read_data(mut self) -> Self { ... }
    
    // Build methods
    pub fn build(mut self) -> Dim { ... }
    pub fn full_build(self) -> Dim { ... }
}
```

### Step 4: Create hierarchy.rs (Hierarchy Logic)
```rust
// src/core/dim/hierarchy.rs
use super::Dim;
use std::collections::HashMap;

impl Dim {
    // Hierarchy-specific methods that might be extracted
    pub fn get_parent_chain(&self) -> Vec<&Dim> { ... }
    pub fn find_root(&self) -> &Dim { ... }
}

// Hierarchy utility functions
pub fn validate_parent_format(parent_ref: &str) -> bool { ... }
pub fn build_hierarchy_path(dim_type: &str, dim_name: &str, parent: Option<&Dim>) -> PathBuf { ... }
```

### Step 5: Refactor mod.rs (Clean Public API)
```rust
// src/core/dim/mod.rs
pub mod data;
pub mod error;
pub mod dim;
pub mod builder;
pub mod file_ops;
pub mod hierarchy;

#[cfg(test)]
pub mod tests;

// Re-export main types for backward compatibility
pub use dim::Dim;
pub use builder::DimBuilder;
pub use error::{DimError, DimResult, DimResultExt};

// Re-export data types
pub use data::*;
```

## Migration Strategy

### Backward Compatibility
- All public APIs remain unchanged
- Existing imports continue to work
- Tests should pass without modification

### Implementation Order
1. **Step 1**: Create `dim.rs` with basic Dim struct
2. **Step 2**: Move file operations to `file_ops.rs`
3. **Step 3**: Create `builder.rs` with DimBuilder
4. **Step 4**: Extract hierarchy logic to `hierarchy.rs`
5. **Step 5**: Clean up `mod.rs` to be a public API interface
6. **Step 6**: Update imports and ensure all tests pass

### Validation Steps
After each step:
1. Run `cargo test --lib core::dim` to ensure no regressions
2. Check that all imports still work
3. Verify backward compatibility

## Benefits Expected

### Code Organization
- **Smaller files**: Each file focuses on specific functionality
- **Better separation of concerns**: Clear boundaries between different aspects
- **Easier navigation**: Developers can find relevant code faster

### Maintainability
- **Focused modules**: Each module has a single responsibility
- **Easier testing**: Smaller, focused units are easier to test
- **Better documentation**: Each module can have focused documentation

### Future Development
- **Easier to extend**: New functionality can be added to appropriate modules
- **Better for teams**: Multiple developers can work on different modules
- **Clearer dependencies**: Module boundaries make dependencies explicit

## Risk Mitigation

### Potential Issues
1. **Import conflicts**: Circular dependencies between modules
2. **Test failures**: Tests might break due to import changes
3. **API changes**: Accidental breaking changes to public API

### Mitigation Strategies
1. **Careful import design**: Use re-exports in mod.rs for backward compatibility
2. **Incremental approach**: Move code gradually, testing after each step
3. **Comprehensive testing**: Run full test suite after each change

## Success Criteria

### Phase 3 Complete When:
- ✅ `mod.rs` is under 100 lines (currently 517)
- ✅ Each new module is under 200 lines
- ✅ All 95 tests continue to pass
- ✅ No breaking changes to public API
- ✅ Clear module boundaries and responsibilities
- ✅ Improved code organization and maintainability

This reorganization will provide a solid foundation for future development and make the codebase much more maintainable. 