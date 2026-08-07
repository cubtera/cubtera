use cubtera::tools::{Result, ToolsError, LegacyCompat, OptionCompat};
use std::path::PathBuf;

/// Example of old vs new error handling patterns
fn main() -> Result<()> {
    println!("🔄 Cubtera Migration Demo - Error Handling Patterns");
    
    // Example 1: Configuration loading
    println!("\n📋 Example 1: Configuration Loading");
    demo_config_loading()?;
    
    // Example 2: File operations
    println!("\n📁 Example 2: File Operations");
    demo_file_operations()?;
    
    // Example 3: Optional operations
    println!("\n🔧 Example 3: Optional Operations");
    demo_optional_operations()?;
    
    // Example 4: CLI-style error handling
    println!("\n💻 Example 4: CLI Error Handling");
    demo_cli_error_handling();
    
    println!("\n✅ Migration demo completed successfully!");
    Ok(())
}

/// Demo: Configuration loading patterns
fn demo_config_loading() -> Result<()> {
    println!("  Old pattern: config.unwrap_or_exit()");
    println!("  New pattern: Proper error propagation with fallbacks");
    
    // New approach - library function returns Result
    fn load_config_new() -> Result<Config> {
        let config_path = "config.toml";
        
        // Try to read config file
        match std::fs::read_to_string(config_path) {
            Ok(content) => {
                // Parse TOML (simulated)
                if content.trim().is_empty() {
                    return Err(ToolsError::config_error("Config file is empty"));
                }
                Ok(Config { 
                    name: "cubtera".to_string(),
                    version: "1.0.15".to_string(),
                    debug: true,
                })
            }
            Err(_) => {
                // Config file not found - this is OK, use defaults
                println!("    ⚠️  Config file not found, using defaults");
                Ok(Config::default())
            }
        }
    }
    
    // Usage in library code
    let config = load_config_new()?;
    println!("    ✅ Loaded config: {} v{}", config.name, config.version);
    
    // Usage in CLI code (with legacy compat)
    let _config_cli = load_config_new()
        .unwrap_or_exit_with_log("Failed to load configuration");
    
    Ok(())
}

/// Demo: File operations patterns
fn demo_file_operations() -> Result<()> {
    println!("  Old pattern: read_file().unwrap_or_exit()");
    println!("  New pattern: Proper error handling with context");
    
    // New approach - with proper error context
    fn process_file_new(path: &str) -> Result<String> {
        let content = std::fs::read_to_string(path)
            .with_context(&format!("Failed to read file: {}", path))?;
        
        if content.is_empty() {
            return Err(ToolsError::validation_error("File is empty"));
        }
        
        // Process content (simulated)
        let processed = content.to_uppercase();
        Ok(processed)
    }
    
    // Usage - graceful error handling
    match process_file_new("nonexistent.txt") {
        Ok(content) => println!("    ✅ Processed: {}", content),
        Err(e) => println!("    ⚠️  Processing failed: {}", e),
    }
    
    // Create a test file for successful case
    std::fs::write("temp_test.txt", "hello world").unwrap();
    
    match process_file_new("temp_test.txt") {
        Ok(content) => println!("    ✅ Processed: {}", content),
        Err(e) => println!("    ❌ Unexpected error: {}", e),
    }
    
    // Cleanup
    let _ = std::fs::remove_file("temp_test.txt");
    
    Ok(())
}

/// Demo: Optional operations patterns
fn demo_optional_operations() -> Result<()> {
    println!("  Old pattern: operation.check_with_warn()");
    println!("  New pattern: warn_and_continue() or warn_and_default()");
    
    // Simulate loading cache
    fn load_cache() -> Result<Cache> {
        Err(ToolsError::operation_failed("Cache file corrupted"))
    }
    
    // New pattern - continue without cache
    if let Some(cache) = load_cache()
        .warn_and_continue("Cache unavailable, proceeding without cache") {
        println!("    ✅ Using cache with {} entries", cache.entries);
    } else {
        println!("    ⚠️  Proceeding without cache");
    }
    
    // New pattern - use default cache
    let cache = load_cache()
        .warn_and_default("Cache unavailable, using empty cache");
    println!("    ✅ Using cache with {} entries", cache.entries);
    
    Ok(())
}

/// Demo: CLI-style error handling
fn demo_cli_error_handling() {
    println!("  CLI pattern: Exit on critical errors, warn on non-critical");
    
    // Simulate CLI main function
    fn cli_main() -> Result<()> {
        // Critical operation - should exit on failure
        let _config = load_critical_config()
            .unwrap_or_exit_with_log("Critical configuration missing");
        
        // Optional operation - warn and continue
        let _cache = load_optional_cache()
            .warn_and_continue("Cache not available");
        
        // Business logic here...
        println!("    ✅ CLI operation completed successfully");
        
        Ok(())
    }
    
    fn load_critical_config() -> Result<Config> {
        Ok(Config::default()) // Simulate success
    }
    
    fn load_optional_cache() -> Result<Cache> {
        Err(ToolsError::operation_failed("Cache not found")) // Simulate failure
    }
    
    // Run CLI simulation
    if let Err(e) = cli_main() {
        eprintln!("CLI Error: {}", e);
        std::process::exit(1);
    }
}

// Supporting types for demo
#[derive(Debug)]
struct Config {
    name: String,
    version: String,
    debug: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            name: "cubtera".to_string(),
            version: "1.0.15".to_string(),
            debug: false,
        }
    }
}

#[derive(Debug)]
struct Cache {
    entries: usize,
}

impl Default for Cache {
    fn default() -> Self {
        Self { entries: 0 }
    }
} 