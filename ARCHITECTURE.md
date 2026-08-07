# Cubtera Architecture

## Core Systems

### 1. Runner System
- **Purpose**: Command execution and logging
- **Key Components**:
  - Base Runner (`src/core/runner/mod.rs`)
  - TF Runner (`src/core/runner/tf/mod.rs`)
  - Logger functionality
- **Features**:
  - Command execution
  - Logging to DB/file
  - State management
  - Error handling

### 2. Dimension System
- **Purpose**: Handle unit dimensions and extensions
- **Key Components**:
  - Core (`src/core/dim/mod.rs`)
  - Extensions (`src/core/dim/ext/`)
  - Tests (`src/core/dim/tests/`)
- **Features**:
  - Dimension parsing
  - Extension handling
  - State management
  - Validation

### 3. Logging System
- **Purpose**: Store execution logs
- **Key Components**:
  - DB Logger (`src/core/dlog/mod.rs`)
  - File Logger (in runner)
- **Features**:
  - DB storage
  - File storage
  - Log formatting
  - Error handling

## Data Flow

1. **Command Execution**
   ```
   CLI -> Runner -> Command -> Logger
   ```

2. **Dimension Processing**
   ```
   Unit -> Dimension Parser -> Extensions -> State
   ```

3. **Logging Flow**
   ```
   Runner -> Logger -> DB/File
   ```

## Configuration

### Global Config (`GLOBAL_CFG`)
- Database settings
- Path configurations
- Feature flags

### Environment Variables
- Secrets
- Paths
- Feature toggles

## Testing Strategy

### 1. Unit Tests
- In-module tests
- Mock dependencies
- Fast execution

### 2. Integration Tests
- End-to-end flows
- Real dependencies
- Full system testing

### 3. Test Data
- Mock data in tests
- Test fixtures
- State management

## Error Handling

### 1. Error Types
- `Result<T, Box<dyn Error>>`
- Custom error types
- Error context

### 2. Error Flow
- Propagation
- Logging
- User feedback

## Security

### 1. Data Protection
- No hardcoded secrets
- Secure file operations
- Input validation

### 2. Access Control
- Permission checks
- Path validation
- Resource limits

## Performance

### 1. Optimization
- Minimal allocations
- Reference usage
- Caching

### 2. Resource Management
- File handles
- DB connections
- Memory usage

## Extension Points

### 1. New Runners
- Implement Runner trait
- Add specific logic
- Register in system

### 2. New Extensions
- Add to dim/ext
- Implement interface
- Register in system

## Maintenance

### 1. Code Quality
- Clippy checks
- Formatting
- Documentation

### 2. Testing
- Coverage
- Performance
- Integration

### 3. Updates
- Dependency updates
- Security patches
- Feature additions 