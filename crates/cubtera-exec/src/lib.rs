//! `cubtera-exec`: rooted [`Workspace`], [`ProcessRunner`] port, and the
//! capability-aware [`RunnerStrategy`] contract (see
//! `docs/specs/2026-09-03-cubtera-v3-architecture.md` §7).
//!
//! Depends only on `cubtera-kernel`/`cubtera-model` (per the crate table in
//! §3) - it is deliberately unaware of `cubtera-domain`'s `Unit`/`Manifest`
//! or `cubtera-core`'s `RunService` pipeline. Wiring this crate's
//! primitives into a full run (materialize -> transform -> exec -> outlet
//! -> publish) is `cubtera-app`'s job (P4-run), not this crate's - this
//! crate only expresses *what a runner can do* and *how to do it safely in
//! a rooted directory*, matching the "rooted workspace, capability
//! contract, common `TfLike`" scope P4-exec calls for.

mod bash;
mod error;
mod materialize;
mod process;
mod runner;
mod tf_like;
mod version;
mod workspace;

pub use bash::BashRunner;
pub use error::{ExecError, ExecResult};
pub use materialize::{apply as apply_materialization_plan, clean as clean_temp_folder, read_file};
pub use process::{
    CapturingProcessRunner, ProcessOutput, ProcessRunner, ProcessSpec, TokioCapturingProcessRunner,
    TokioProcessRunner,
};
pub use runner::{merged_env, RunnerCapabilities, RunnerContext, RunnerStrategy};
pub use tf_like::TfLikeRunner;
pub use version::{PathVersionResolver, TfSwitchResolver, VersionResolver};
pub use workspace::{RootedPath, Workspace};
