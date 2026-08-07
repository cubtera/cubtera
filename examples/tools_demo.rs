use cubtera::tools::{Result, ToolsError};
use std::path::PathBuf;

fn main() -> Result<()> {
    println!("🚀 Cubtera Tools Demo");
    
    // String utilities
    println!("\n📝 String Utilities:");
    let capitalized = cubtera::tools::capitalize_first("hello world");
    println!("Capitalized: {}", capitalized);
    
    // Path operations
    println!("\n📁 Path Operations:");
    let path = cubtera::tools::string_to_path("/usr/bin")?;
    println!("Path: {:?}", path);
    
    // JSON operations
    println!("\n📄 JSON Operations:");
    let mut target = serde_json::json!({"name": "cubtera"});
    let source = serde_json::json!({"version": "1.0.15"});
    cubtera::tools::json::merge_values(&mut target, &source);
    println!("Merged JSON: {}", target);
    
    // Crypto operations
    println!("\n🔐 Crypto Operations:");
    let test_value = serde_json::json!({"test": "data", "number": 42});
    let hash = cubtera::tools::crypto::get_sha_by_value(&test_value)?;
    println!("SHA256 hash: {}", hash);
    
    // Collections
    println!("\n📊 Collections:");
    let vec1 = vec!["rust".to_string(), "go".to_string()];
    let vec2 = vec!["rust".to_string(), "python".to_string()];
    let intersects = cubtera::tools::collections::if_intersect(vec1, vec2);
    println!("Vectors intersect: {}", intersects);
    
    // Git operations (if in a git repo)
    println!("\n🔧 Git Operations:");
    match cubtera::tools::git::get_commit_sha(&PathBuf::from(".")) {
        Ok(sha) => println!("Current commit SHA: {}", sha),
        Err(e) => println!("Not in a git repo or error: {}", e),
    }
    
    println!("\n✅ Demo completed successfully!");
    Ok(())
} 