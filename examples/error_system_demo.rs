use cubtera::error::{CubteraError, CubteraResult, ErrorSeverity, CubteraResultExt};
use cubtera::tools::{ToolsError, CubteraCompat};

/// Demo of the new centralized error system
fn main() -> CubteraResult<()> {
    println!("🔧 Cubtera Centralized Error System Demo");
    
    // Example 1: Error creation and categorization
    println!("\n📋 Example 1: Error Types and Severity");
    demo_error_types();
    
    // Example 2: Error conversion and context
    println!("\n🔄 Example 2: Error Conversion and Context");
    demo_error_conversion()?;
    
    // Example 3: Smart error handling by severity
    println!("\n🧠 Example 3: Smart Error Handling");
    demo_smart_handling();
    
    // Example 4: Module-specific error handling
    println!("\n🏗️ Example 4: Module-Specific Errors");
    demo_module_errors()?;
    
    println!("\n✅ Error system demo completed successfully!");
    Ok(())
}

/// Demo: Different error types and their severity levels
fn demo_error_types() {
    let errors = vec![
        CubteraError::config_error("Invalid configuration file"),
        CubteraError::dimension_error("Unknown dimension type"),
        CubteraError::unit_error("Unit validation failed"),
        CubteraError::runner_error("terraform", "Plan execution failed"),
        CubteraError::cli_error("deploy", "Missing required arguments"),
        CubteraError::api_error("/api/units", "Authentication failed"),
        CubteraError::validation_error("name", "Field cannot be empty"),
        CubteraError::external_error("docker", "Container not found"),
        CubteraError::critical_error("Database connection lost"),
    ];
    
    for error in errors {
        println!("  {} | {} | {}", 
            error.category(), 
            format!("{:?}", error.severity()),
            error
        );
    }
}

/// Demo: Error conversion and context addition
fn demo_error_conversion() -> CubteraResult<()> {
    // Simulate reading a config file
    fn read_config_file() -> std::result::Result<String, std::io::Error> {
        Err(std::io::Error::new(std::io::ErrorKind::NotFound, "config.toml not found"))
    }
    
    // Convert std::io::Error to CubteraError with context
    match read_config_file().to_config_error("Failed to read configuration") {
        Ok(content) => println!("  ✅ Config loaded: {}", content),
        Err(e) => println!("  ⚠️  Config error handled: {}", e),
    }
    
    // Demonstrate automatic conversion from ToolsError
    let tools_error = ToolsError::file_not_found("missing.json");
    let cubtera_error: CubteraError = tools_error.into();
    println!("  🔄 Tools error converted: {}", cubtera_error);
    
    Ok(())
}

/// Demo: Smart error handling based on severity
fn demo_smart_handling() {
    // Simulate different operations with different error severities
    
    // Low severity - should warn and continue
    let low_severity_result: CubteraResult<Config> = Err(CubteraError::Tools(
        ToolsError::operation_failed("Cache file corrupted")
    ));
    let config = low_severity_result.handle_by_severity();
    println!("  📝 Low severity handled, using default config: {:?}", config);
    
    // Medium severity - should log error and use default
    let medium_severity_result: CubteraResult<Config> = Err(
        CubteraError::unit_error("Unit validation failed")
    );
    let config = medium_severity_result.handle_by_severity();
    println!("  ⚠️  Medium severity handled, using default config: {:?}", config);
    
    // High severity would exit (commented out to avoid terminating demo)
    // let high_severity_result: CubteraResult<Config> = Err(
    //     CubteraError::config_error("Critical configuration missing")
    // );
    // let config = high_severity_result.handle_by_severity(); // Would exit here
    
    println!("  ✅ Smart handling completed (high/critical errors would exit)");
}

/// Demo: Module-specific error handling patterns
fn demo_module_errors() -> CubteraResult<()> {
    // Configuration module pattern
    fn load_config() -> CubteraResult<Config> {
        // Simulate config loading failure
        Err(CubteraError::config_error("Configuration file is corrupted"))
    }
    
    // Dimension module pattern
    fn validate_dimension(name: &str) -> CubteraResult<Dimension> {
        if name.is_empty() {
            return Err(CubteraError::dimension_error("Dimension name cannot be empty"));
        }
        Ok(Dimension { name: name.to_string() })
    }
    
    // Runner module pattern
    fn execute_terraform_plan() -> CubteraResult<String> {
        // Simulate terraform failure
        Err(CubteraError::runner_error("terraform", "Invalid terraform configuration"))
    }
    
    // CLI module pattern
    fn handle_deploy_command(args: &[String]) -> CubteraResult<()> {
        if args.is_empty() {
            return Err(CubteraError::cli_error("deploy", "No deployment target specified"));
        }
        Ok(())
    }
    
    // Demonstrate error handling patterns
    println!("  📋 Config loading:");
    match load_config() {
        Ok(config) => println!("    ✅ Config: {:?}", config),
        Err(e) => println!("    ❌ Error: {} (severity: {:?})", e, e.severity()),
    }
    
    println!("  📐 Dimension validation:");
    match validate_dimension("") {
        Ok(dim) => println!("    ✅ Dimension: {:?}", dim),
        Err(e) => println!("    ❌ Error: {} (category: {})", e, e.category()),
    }
    
    println!("  🏃 Runner execution:");
    match execute_terraform_plan() {
        Ok(output) => println!("    ✅ Output: {}", output),
        Err(e) => println!("    ❌ Error: {} (critical: {})", e, e.is_critical()),
    }
    
    println!("  💻 CLI command:");
    match handle_deploy_command(&[]) {
        Ok(()) => println!("    ✅ Command executed"),
        Err(e) => println!("    ❌ Error: {} (should exit: {})", e, e.severity().should_exit()),
    }
    
    Ok(())
}

// Supporting types for demo
#[derive(Debug, Default)]
struct Config {
    name: String,
    debug: bool,
}

#[derive(Debug)]
struct Dimension {
    name: String,
} 