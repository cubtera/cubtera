//! `cubtera drift -u <unit> -s <selector>` (v3, P5) - the CI-facing sibling
//! of `cubtera fleet status`: same report, but filtered down to only
//! `PackageDrifted`/`Orphaned` entries, and a distinct non-zero exit code
//! (`EXIT_DRIFT_DETECTED`) when anything shows up, so a pipeline can gate
//! on it without parsing text.

use super::fleet::{excluded_by_dim_ref, parse_binding, print_drift_report};
use super::run_support::build_binding_use_case;
use super::Ctx;
use clap::Args;
use cubtera_app::DriftState;
use cubtera_config::Config;
use cubtera_kernel::DimRef;

#[derive(Args)]
pub struct DriftArgs {
    /// Unit this binding applies to
    #[arg(short, long)]
    pub unit: String,
    /// Selector expression over the inventory - defaults to matching
    /// everything
    #[arg(short, long)]
    pub selector: Option<String>,
    /// Exclude any candidate whose resolved dimensions include this
    /// `type:name` ref (repeatable) - see `fleet status`'s `--exclude`
    #[arg(long = "exclude")]
    pub exclude: Vec<String>,
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    args: DriftArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let binding = parse_binding(&args.unit, args.selector.as_deref())?;
    let exclude: Vec<DimRef> = args
        .exclude
        .iter()
        .map(|s| DimRef::parse(s))
        .collect::<Result<_, _>>()?;

    let binding_uc = build_binding_use_case(config)?;
    let report = binding_uc.status(&config.org, &binding).await?;
    let drifted: Vec<_> = report
        .into_iter()
        .filter(|d| !excluded_by_dim_ref(&d.id, &exclude))
        .filter(|d| matches!(d.state, DriftState::PackageDrifted | DriftState::Orphaned))
        .collect();

    print_drift_report(ctx, &drifted);

    if !drifted.is_empty() {
        std::process::exit(crate::error::EXIT_DRIFT_DETECTED);
    }
    Ok(())
}
