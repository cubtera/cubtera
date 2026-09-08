//! Config command

use super::Ctx;
use cubtera_config::Config;

pub fn run(config: &Config, ctx: &Ctx) -> Result<(), Box<dyn std::error::Error>> {
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(config)?);
        return Ok(());
    }

    println!("Cubtera Configuration");
    println!("=====================");
    println!();
    println!("Organization: {}", config.org);
    if !config.orgs.is_empty() {
        println!("Known Orgs: {:?}", config.orgs);
    }
    println!("Log Level: {}", config.log_level);
    println!();
    println!("Inventory: FS ({:?})", config.inventory_path);
    println!("Store: SQLite ({:?})", config.store_path);
    println!();
    println!("Paths:");
    println!("  Units: {:?}", config.units_path);
    println!("  Modules: {:?}", config.modules_path);
    println!("  Plugins: {:?}", config.plugins_path);
    println!();
    println!("Dimension Relations: {:?}", config.dim_relations);
    if !config.runner.is_empty() {
        println!("Runner Config: {:?}", config.runner);
    }
    if !config.state.is_empty() {
        println!(
            "State Backends: {:?}",
            config.state.keys().collect::<Vec<_>>()
        );
    }

    Ok(())
}
