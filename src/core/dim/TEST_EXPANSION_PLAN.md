# Dim Module Test Expansion Plan

## Current Test Coverage Analysis

### ✅ **Currently Tested (22 tests)**
1. **Basic Dim functionality** (6 tests)
   - `test_dim_data_access()` - Data getter methods
   - `test_get_data()`, `test_get_data_mut()`, `test_get_dim_data()` - Data access
   - `test_dim_tree_generation()`, `test_get_dim_tree()` - Hierarchy traversal
   - `test_dim_json_vars()` - JSON variable generation

2. **DimBuilder functionality** (8 tests)
   - `test_dim_builder_creation()` - Basic builder creation
   - `test_dim_builder_with_name()` - Name setting
   - `test_dim_builder_merge_defaults()` - Default merging
   - `test_dim_builder_with_context()` - Context management
   - `split_by_colon_*` tests (4 tests) - String parsing

3. **DataSource functionality** (4 tests in jsonfile.rs)
   - File system data source operations
   - JSON file reading and parsing

4. **Error handling** (4 tests)
   - Safe function variants with proper error handling

### ❌ **Missing Critical Test Coverage**

## Priority 1: Core Business Logic (High Risk)

### 1. **Hierarchy and Parent-Child Relationships**
```rust
// Missing tests for:
- Parent dimension loading and validation
- Recursive parent traversal
- Parent data inheritance
- Circular dependency detection
- Invalid parent format handling
```

### 2. **Data Merging and Inheritance**
```rust
// Missing tests for:
- Complex default data merging
- Parent data inheritance patterns
- Override behavior validation
- Nested object merging
- Array merging strategies
```

### 3. **File System Operations**
```rust
// Missing tests for:
- save_dim_includes() - File copying with filters
- save_dim_folders() - Directory copying
- process_dim_entries() - Entry processing logic
- copy_entry() - Individual file/directory copying
- get_filtered_entries() - File filtering logic
```

### 4. **Kids/Children Management**
```rust
// Missing tests for:
- get_all_kids_by_name() - Child dimension discovery
- Child relationship validation
- Dimension relation configuration
- Child filtering by parent
```

## Priority 2: Data Operations (Medium Risk)

### 5. **Data Persistence**
```rust
// Missing tests for:
- save_data() - Data saving to storage
- save_all_data_by_type() - Bulk data operations
- delete_data() - Data deletion
- delete_all_data_by_context() - Context-based deletion
```

### 6. **Default Data Handling**
```rust
// Missing tests for:
- read_default_data() - Default data loading
- save_default_data() - Default data persistence
- delete_default_data() - Default data cleanup
- Default data format validation
```

### 7. **Context Management**
```rust
// Missing tests for:
- Context-based data filtering
- Context inheritance
- Context validation
- Multi-context scenarios
```

## Priority 3: Edge Cases and Error Scenarios (Medium Risk)

### 8. **Error Handling Scenarios**
```rust
// Missing tests for:
- Invalid dimension formats
- Missing data files
- Corrupted JSON data
- Permission errors
- Network failures (for DB storage)
- Disk space issues
```

### 9. **Configuration Dependencies**
```rust
// Missing tests for:
- GLOBAL_CFG dependency scenarios
- Missing configuration values
- Invalid configuration formats
- Configuration override behavior
```

### 10. **Storage Backend Switching**
```rust
// Missing tests for:
- FS to DB migration scenarios
- DB to FS fallback
- Storage backend validation
- Data format differences between backends
```

## Priority 4: Performance and Integration (Lower Risk)

### 11. **Performance Tests**
```rust
// Missing tests for:
- Large dimension hierarchies
- Bulk data operations
- Memory usage patterns
- Concurrent access scenarios
```

### 12. **Integration Tests**
```rust
// Missing tests for:
- End-to-end dimension workflows
- Multi-storage backend scenarios
- Real file system operations
- Database integration tests
```

## Implementation Strategy

### Phase 1: Critical Business Logic (Week 1)
1. **Hierarchy Tests** - Parent-child relationships
2. **Data Merging Tests** - Inheritance and overrides
3. **File Operations Tests** - Core file system functionality

### Phase 2: Data Operations (Week 2)
1. **Persistence Tests** - Save/delete operations
2. **Default Data Tests** - Default handling
3. **Context Tests** - Context management

### Phase 3: Error Handling (Week 3)
1. **Error Scenario Tests** - Comprehensive error coverage
2. **Configuration Tests** - Config dependency handling
3. **Storage Backend Tests** - Backend switching

### Phase 4: Performance & Integration (Week 4)
1. **Performance Tests** - Load and stress testing
2. **Integration Tests** - End-to-end scenarios
3. **Regression Tests** - Ensure no breaking changes

## Test Structure Organization

### Proposed Test File Structure
```
src/core/dim/tests/
├── mod.rs                    # Test module organization
├── dim_tests.rs             # Core Dim struct tests
├── builder_tests.rs         # DimBuilder tests
├── hierarchy_tests.rs       # Parent-child relationship tests
├── data_operations_tests.rs # Data persistence tests
├── file_operations_tests.rs # File system operation tests
├── error_handling_tests.rs  # Error scenario tests
├── integration_tests.rs     # End-to-end tests
├── performance_tests.rs     # Performance benchmarks
└── mock_helpers.rs          # Test utilities and mocks
```

## Success Metrics

### Coverage Goals
- **Unit Test Coverage**: 90%+ of public methods
- **Error Scenario Coverage**: 100% of error paths
- **Integration Coverage**: All major workflows
- **Performance Baseline**: Established benchmarks

### Quality Gates
- All tests pass consistently
- No flaky tests
- Clear test documentation
- Maintainable test code
- Fast test execution (<5s for unit tests)

## Benefits of Expanded Testing

1. **Risk Mitigation**: Catch regressions during refactoring
2. **Documentation**: Tests serve as living documentation
3. **Confidence**: Safe refactoring with comprehensive coverage
4. **Quality**: Better error handling and edge case coverage
5. **Maintainability**: Easier to understand and modify code

## Next Steps

1. **Start with Priority 1 tests** - Highest risk areas
2. **Create test utilities** - Mock objects and helpers
3. **Add performance baselines** - Before refactoring
4. **Document test patterns** - For consistency
5. **Set up CI integration** - Automated test running

This expansion will provide a solid foundation for the upcoming refactoring phases while ensuring we don't break existing functionality. 