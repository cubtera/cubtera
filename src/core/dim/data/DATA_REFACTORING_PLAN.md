# Data Submodule Refactoring Plan

## Overview
Progressive refactoring of the `data` submodule to improve error handling, reduce global dependencies, and enhance maintainability while preserving backward compatibility.

## Phase 1: Error Handling Foundation ✅ COMPLETED

### Accomplished:
- **Created specialized error system**: Added `DataSourceError` enum with specific error types
- **Added safe method variants**: All DataSource trait methods now have `_safe` counterparts  
- **Preserved backward compatibility**: Legacy methods remain unchanged
- **Comprehensive testing**: Added 5 new tests for safe methods in JsonDataSource
- **Proper error propagation**: Replaced `unwrap_or_exit()` with `Result` returns in safe methods

### Key Changes:
```rust
// New error types
pub enum DataSourceError {
    PathNotFound { path: String },
    FileNotFound { filename: String }, 
    ParseError { message: String },
    DatabaseError { message: String },
    ConfigurationError { message: String },
    IOError { message: String },
    ValidationError { message: String },
}

// Safe method examples
fn get_data_by_name_safe(&self, name: &str) -> DataResult<Value>
fn get_all_names_safe(&self) -> DataResult<Vec<String>>
```

### Files Modified:
- `src/core/dim/data/mod.rs` - Added error module and safe trait methods
- `src/core/dim/data/jsonfile.rs` - Implemented safe methods with proper error handling

### Benefits:
- **Graceful error handling**: No more process crashes on data errors
- **Better debugging**: Specific error types with contextual information
- **Backward compatibility**: Existing code continues to work unchanged
- **Type safety**: Structured error handling instead of generic `Box<dyn Error>`

## Phase 2: Configuration Abstraction ✅ COMPLETED

### Accomplished:
- **Unified configuration system**: Created `DataSourceConfig` that works on top of GLOBAL_CFG, not parallel to it
- **Backward compatibility**: All existing code continues to work unchanged
- **Configuration flexibility**: New `data_src_init_with_config()` allows custom configurations
- **Dependency injection**: DataSource constructors now accept configuration objects
- **Zero breaking changes**: Legacy functions remain as thin wrappers

```rust
// Unified configuration that defaults to GLOBAL_CFG
#[derive(Debug, Clone)]
pub struct DataSourceConfig {
    pub inventory_path: String,
    pub file_name_separator: String,
    pub db_client: Option<::mongodb::sync::Client>,
}

impl DataSourceConfig {
    pub fn from_global() -> Self { /* uses GLOBAL_CFG */ }
    pub fn with_inventory_path(path: String) -> Self { /* override path only */ }
    pub fn with_file_name_separator(sep: String) -> Self { /* override separator only */ }
}

// Backward compatible factory function
pub fn data_src_init(org: &str, dim_type: &str, storage: Storage) -> Box<dyn DataSource> {
    data_src_init_with_config(org, dim_type, storage, &DataSourceConfig::from_global())
}

// New configurable factory function
pub fn data_src_init_with_config(
    org: &str, dim_type: &str, storage: Storage, config: &DataSourceConfig
) -> Box<dyn DataSource>
```

### Files Modified:
- `src/core/dim/data/mod.rs` - Added DataSourceConfig and new factory function
- `src/core/dim/data/jsonfile.rs` - Updated to use config instead of direct GLOBAL_CFG access
- `src/core/dim/data/mongodb.rs` - Updated to use config instead of direct GLOBAL_CFG access

### Benefits:
- **Single source of truth**: GLOBAL_CFG remains the default, no configuration conflicts
- **Testability**: Easy to create custom configurations for testing
- **Flexibility**: Can override specific config values while keeping others from GLOBAL_CFG
- **Zero migration effort**: Existing code works without any changes

## Phase 3: Structure Simplification 🔴 PLANNED

### Goals:
- Simplify the DataSource trait design
- Remove the CloneBox trait workaround
- Split sync/async DataSource variants
- Improve MongoDB async handling

### Planned Changes:

#### 3.1 Trait Simplification
```rust
// Split into separate traits
pub trait DataSourceRead {
    fn get_data_by_name(&self, name: &str) -> DataResult<Value>;
    fn get_all_data(&self) -> DataResult<Vec<Value>>;
    fn get_all_names(&self) -> DataResult<Vec<String>>;
    fn get_all_types(&self) -> DataResult<Vec<String>>;
}

pub trait DataSourceWrite: DataSourceRead {
    fn upsert_data_by_name(&self, name: &str, data: Value) -> DataResult<()>;
    fn delete_data_by_name(&self, name: &str) -> DataResult<()>;
    fn upsert_all_data(&self, data: Vec<Value>) -> DataResult<()>;
    fn delete_all_by_context(&self, context: &str) -> DataResult<()>;
}

pub trait DataSourceContext {
    fn set_context(&mut self, context: Option<String>);
    fn get_context(&self) -> Option<String>;
}
```

#### 3.2 Async Support
```rust
#[async_trait]
pub trait AsyncDataSource {
    async fn get_data_by_name(&self, name: &str) -> DataResult<Value>;
    async fn upsert_data_by_name(&self, name: &str, data: Value) -> DataResult<()>;
    // ... other async methods
}
```

#### 3.3 Remove CloneBox Workaround
Replace trait object cloning with proper design patterns or Arc<dyn DataSource>.

### Files to Modify:
- `src/core/dim/data/mod.rs` - Redesign trait structure
- `src/core/dim/data/mongodb.rs` - Add proper async support
- `src/core/dim/builder.rs` - Update to use new trait design

## Phase 4: Testing and Documentation 🔴 PLANNED

### Goals:
- Comprehensive test coverage for all phases
- Performance benchmarks
- Updated documentation and examples
- Migration guide for existing code

### Deliverables:
- Unit tests for all new functionality
- Integration tests with real data scenarios
- Performance comparison benchmarks
- Updated README with examples
- Migration guide from legacy API to new API

## Risk Mitigation

### Backward Compatibility Strategy:
1. **Deprecation warnings** rather than immediate removal
2. **Wrapper functions** that bridge old and new APIs
3. **Gradual migration path** - users can adopt new features incrementally
4. **Comprehensive testing** to ensure no regression in existing functionality

### Testing Strategy:
1. **Phase-by-phase testing** - each phase thoroughly tested before proceeding
2. **Integration tests** to verify compatibility with existing dim module
3. **Error scenario testing** - verify proper error handling in edge cases
4. **Performance regression tests** - ensure new code doesn't degrade performance

## Success Metrics

### Phase 1 ✅:
- [x] All legacy tests continue to pass
- [x] New safe methods have 100% test coverage
- [x] No breaking changes to existing API
- [x] Proper error propagation without process exits

### Phase 2 ✅:
- [x] Zero direct GLOBAL_CFG dependencies in data sources (replaced with config abstraction)
- [x] Flexible configuration system with override capabilities
- [x] All existing functionality works through new configuration system
- [x] Backward compatibility maintained - no migration needed

### Phase 3 Targets:
- [ ] Simplified trait hierarchy
- [ ] Async MongoDB operations
- [ ] Removed CloneBox workaround
- [ ] Improved type safety

### Overall Goals:
- **Maintainability**: Cleaner, more modular code structure
- **Reliability**: Better error handling and testing coverage  
- **Performance**: No regression, potential improvements in async operations
- **Developer Experience**: Easier to understand and extend data sources 