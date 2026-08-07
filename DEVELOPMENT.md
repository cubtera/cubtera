# Cubtera Development Guide

## Getting Started

### Prerequisites
- Rust 1.70+
- Cargo
- Git

### Setup
1. Clone repository
2. Install dependencies
3. Run tests

## Development Workflow

### 1. Code Structure
- Follow existing patterns
- Use clear naming
- Add documentation
- Write tests

### 2. Testing
- Write unit tests
- Add integration tests
- Test edge cases
- Verify error handling

### 3. Documentation
- Update doc comments
- Add examples
- Document changes
- Keep README current

## Common Tasks

### Adding New Runner
1. Create new module
2. Implement Runner trait
3. Add tests
4. Update documentation

### Adding New Extension
1. Add to dim/ext
2. Implement interface
3. Add tests
4. Update docs

### Modifying Logger
1. Check existing implementation
2. Add new functionality
3. Update tests
4. Verify both DB and file logging

## Best Practices

### Code Style
- Use rustfmt
- Follow clippy
- Keep functions focused
- Use clear names

### Error Handling
- Use Result type
- Add context
- Log errors
- Handle edge cases

### Testing
- Test happy path
- Test error cases
- Test edge cases
- Mock external deps

### Performance
- Minimize allocations
- Use references
- Cache expensive ops
- Profile when needed

## Common Issues

### Test Failures
1. Check test output
2. Verify test data
3. Check mocks
4. Look for timing issues

### Build Errors
1. Check dependencies
2. Verify imports
3. Check types
4. Look for traits

### Runtime Errors
1. Check logs
2. Verify paths
3. Check permissions
4. Look for missing files

## Tools

### Cargo Commands
```bash
# Build
cargo build

# Test
cargo test

# Format
cargo fmt

# Lint
cargo clippy

# Run
cargo run
```

### Environment Variables
```bash
# Debug logging
RUST_LOG=debug

# Test specific
RUST_BACKTRACE=1
```

## Contributing

### Pull Requests
1. Create feature branch
2. Make changes
3. Add tests
4. Update docs
5. Submit PR

### Code Review
1. Check style
2. Verify tests
3. Review docs
4. Check performance

### Merging
1. All tests pass
2. Docs updated
3. No warnings
4. PR approved 