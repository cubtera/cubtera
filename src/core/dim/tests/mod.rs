// Test module organization for dim module
pub mod mock_helpers;
pub mod legacy_tests;
pub mod hierarchy_tests;
pub mod data_operations_tests;
pub mod file_operations_tests;
pub mod file_operations_advanced_tests;
// pub mod builder_tests; // Disabled due to GLOBAL_CFG dependencies

// Re-export test utilities for convenience
pub use mock_helpers::*; 