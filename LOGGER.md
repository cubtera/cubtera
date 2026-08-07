# Cubtera Logger System

## Overview

The logger system in Cubtera provides flexible logging capabilities with support for both database and file-based storage. It's designed to be extensible and maintainable.

## Components

### 1. Database Logger (`src/core/dlog/mod.rs`)
- Handles database storage
- Supports multiple DB types
- Provides query interface
- Manages connections

### 2. File Logger (in Runner)
- Stores logs in files
- Uses JSON format
- Maintains directory structure
- Handles file operations

## Storage Locations

### Database
- Configured via `GLOBAL_CFG.dlog_db`
- Supports multiple DB types
- Connection pooling
- Error handling

### File System
- Base path: `~/.cubtera/`
- Structure: `{org}/{unit}/{dims}/dlog.json`
- Fallback to `/tmp` if HOME not set
- Automatic directory creation

## Log Format

### JSON Structure
```json
{
  "timestamp": "ISO8601",
  "command": "string",
  "exit_code": "integer",
  "output": "string",
  "error": "string",
  "metadata": {
    "key": "value"
  }
}
```

## Usage

### In Runner
```rust
fn logger(&mut self, exit_code: i32) -> Result<(), Box<dyn Error>> {
    // Check for DB config
    if let Some(db_config) = &GLOBAL_CFG.dlog_db {
        // Use DB logger
    } else {
        // Use file logger
    }
}
```

### In Tests
```rust
#[test]
fn test_logger() {
    // Setup test environment
    // Run logger
    // Verify output
}
```

## Configuration

### Database
```rust
GLOBAL_CFG.dlog_db = Some(DbConfig {
    // DB settings
});
```

### File System
- Uses `HOME` environment variable
- Falls back to `/tmp`
- Creates directories as needed

## Error Handling

### Database Errors
- Connection failures
- Query errors
- Transaction issues

### File System Errors
- Permission issues
- Path problems
- Disk space
- File operations

## Testing

### Unit Tests
- Mock DB/file operations
- Test error cases
- Verify output format

### Integration Tests
- Real DB connections
- File system operations
- End-to-end flows

## Best Practices

### 1. Error Handling
- Use proper error types
- Add context to errors
- Log error details
- Handle edge cases

### 2. Performance
- Use connection pooling
- Buffer file operations
- Cache when possible
- Monitor resource usage

### 3. Security
- No sensitive data in logs
- Secure file permissions
- Validate input
- Handle paths safely

## Extension Points

### 1. New Storage Backends
- Implement storage trait
- Add configuration
- Update tests
- Document usage

### 2. Custom Formats
- Add format handlers
- Update serialization
- Add validation
- Update tests

## Maintenance

### 1. Log Rotation
- Implement rotation
- Clean old logs
- Monitor disk usage
- Handle errors

### 2. Monitoring
- Track errors
- Monitor performance
- Check disk space
- Alert on issues

## Troubleshooting

### 1. Database Issues
- Check connection
- Verify config
- Look for errors
- Check permissions

### 2. File System Issues
- Check paths
- Verify permissions
- Look for space
- Check operations

### 3. Format Issues
- Validate JSON
- Check encoding
- Verify structure
- Test parsing 