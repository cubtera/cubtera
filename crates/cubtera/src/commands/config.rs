//! Config command

use cubtera_config::Config;

pub fn run(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    println!("Cubtera Configuration");
    println!("=====================");
    println!();
    println!("Organization: {}", config.org);
    println!("Log Level: {}", config.log_level);
    println!();
    println!("Storage: {:?}", config.storage);
    println!();
    println!("Paths:");
    println!("  Units: {:?}", config.units_path);
    println!("  Modules: {:?}", config.modules_path);
    println!("  Plugins: {:?}", config.plugins_path);
    println!();
    println!("Dimension Relations: {:?}", config.dim_relations);

    if let Some(dlog) = &config.deployment_log {
        println!();
        println!("Deployment Log:");
        println!("  Database: {}", dlog.database);
        println!("  Collection: {}", dlog.collection);
    }

    Ok(())
}

