//! Cubtera Runners
//!
//! Runner implementations for executing infrastructure code.

mod bash;
mod factory;
mod opentofu;
mod terraform;
mod tfswitch;

pub use bash::BashRunner;
pub use factory::DefaultRunnerFactory;
pub use opentofu::OpenTofuRunner;
pub use terraform::TerraformRunner;

// Re-export tfswitch for direct usage
pub use tfswitch::tf_switch;

