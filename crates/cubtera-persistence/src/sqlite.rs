//! SQLite-backed adapters for the deployment log and unit state ports,
//! built on `cubtera-store`'s `SqliteStore` (see `cubtera_store::legacy`).
//! Both share one `Arc<SqliteStore>`/one SQLite file - see
//! `crate::factory::Repositories::from_config`.

mod deployment_log;
mod unit_state;

pub use deployment_log::SqliteDeploymentLogRepository;
pub use unit_state::SqliteUnitStateRepository;
