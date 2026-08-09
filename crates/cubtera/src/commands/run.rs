//! Run command

use clap::Args;
use cubtera_config::Config;
use cubtera_core::ports::CopyConfig;
use cubtera_core::App;
use cubtera_domain::{gap_fill_merge, render_state_backend_config, RunParams, Unit};
use cubtera_persistence::fs::FsWorkspace;
use cubtera_persistence::Repositories;
use cubtera_runners::{DefaultRunnerFactory, TokioProcessRunner};
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Args)]
pub struct RunArgs {
    /// Unit name
    #[arg(short, long)]
    pub unit: String,

    /// Dimension (format: type:name), can be specified multiple times
    #[arg(short, long = "dim")]
    pub dimensions: Vec<String>,

    /// Extension (format: type:name), can be specified multiple times.
    /// Runs the same unit/dimensions with a different state (e.g. `-e index:0`).
    #[arg(short = 'e', long = "ext")]
    pub extensions: Vec<String>,

    /// Auto-approve (skip confirmation)
    #[arg(long)]
    pub auto_approve: bool,

    /// Print the materialization plan instead of executing it
    #[arg(long)]
    pub dry_run: bool,

    /// Command to run (after --)
    #[arg(last = true)]
    pub command: Vec<String>,
}

pub async fn run(config: &Config, args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
    let repos = Repositories::from_config(config).await?;
    let mut runner_factory = DefaultRunnerFactory::new();
    if let Some(v) = config.runner.get("tf").and_then(|m| m.get("version")) {
        runner_factory = runner_factory.with_tf_version(v.clone());
    }
    if let Some(v) = config.runner.get("tofu").and_then(|m| m.get("version")) {
        runner_factory = runner_factory.with_tofu_version(v.clone());
    }
    let runner_factory = Arc::new(runner_factory);

    // Build CopyConfig from global config
    let copy_config = CopyConfig {
        modules_path: config.modules_path.clone(),
        plugins_path: config.plugins_path.clone(),
        always_copy_files: config.always_copy_files,
        clean_cache: config.clean_cache,
    };

    let hierarchy = cubtera_persistence::Repositories::hierarchy(config);
    let workspace = Arc::new(FsWorkspace::new());
    let process = Arc::new(TokioProcessRunner::new());

    let app = App::new(
        repos.inventory,
        hierarchy,
        repos.units,
        runner_factory,
        workspace,
        process,
        copy_config.clone(),
        Some(repos.deployment_log),
        Some(repos.unit_state),
    );

    // Build unit
    let mut unit = app
        .units
        .build_unit_with_extensions(&config.org, &args.unit, &args.dimensions, &args.extensions)
        .await?;

    // Calculate and set temp folder
    let temp_folder = unit.calculate_temp_folder(&config.temp_folder_path);
    unit = unit.with_temp_folder(temp_folder);

    if args.dry_run {
        if !unit.resolved_inputs.is_empty() {
            println!("Resolved inputs:");
            println!("{}", serde_json::to_string_pretty(&unit.resolved_inputs)?);
            println!();
        }
        let plan = unit.materialize(&copy_config.modules_path, None);
        println!("{plan}");
        return Ok(());
    }

    println!("Running unit: {}", unit.name);
    println!(
        "Dimensions: {:?}",
        unit.dimensions.iter().map(|d| d.key()).collect::<Vec<_>>()
    );
    println!("Command: {:?}", args.command);
    println!("Temp folder: {}", unit.temp_folder.display());
    println!();

    // Build params from CLI flags + manifest `[runner]`/`[state]` overrides,
    // merged with the org's `[state.<backend>]` template and rendered.
    let params = build_run_params(config, &unit, &args)?;

    // Execute
    let result = app.runners.run(&unit, args.command, Some(params)).await?;

    // Output results
    if let Some(output) = &result.output {
        print!("{}", output);
    }

    let exit_code = result.exit_code.unwrap_or(0);

    if result.is_success() {
        println!("\nCompleted successfully");
        Ok(())
    } else {
        eprintln!("\nFailed with exit code {}", exit_code);
        std::process::exit(exit_code);
    }
}

/// Build [`RunParams`] for `unit`: the raw command plus everything sourced
/// from `manifest.runner`/`manifest.state` (inlet/outlet commands, version,
/// custom binary, extra args) and the rendered state backend config (manifest
/// overrides gap-filled from the org's `[state.<backend>]` template, then
/// handlebars-rendered against `{org, unit_name, dim_tree}`).
fn build_run_params(
    config: &Config,
    unit: &Unit,
    args: &RunArgs,
) -> Result<RunParams, Box<dyn std::error::Error>> {
    let mut params = RunParams::new(&unit.temp_folder)
        .with_commands(args.command.clone())
        .with_auto_approve(args.auto_approve);

    // Global `[runner.<type>]` gap-filled by the manifest's own `[runner]`
    // overrides (manifest wins) - same precedence as the state backend below.
    let runner_type = unit.manifest.runner_type().as_str().to_string();
    let mut runner_cfg = config.runner.get(&runner_type).cloned().unwrap_or_default();
    if let Some(overrides) = &unit.manifest.runner {
        runner_cfg.extend(overrides.clone());
    }

    if !runner_cfg.is_empty() {
        if let Some(v) = runner_cfg.get("version") {
            params = params.with_version(v.clone());
        }
        if let Some(v) = runner_cfg.get("runner_command") {
            params = params.with_runner_command(v.clone());
        }
        if let Some(v) = runner_cfg
            .get("extra_args")
            .or_else(|| runner_cfg.get("extra_params"))
        {
            params = params.with_extra_args(v.clone());
        }
        if let Some(v) = runner_cfg.get("inlet_command") {
            params = params.with_inlet_command(v.clone());
        }
        if let Some(v) = runner_cfg.get("outlet_command") {
            params = params.with_outlet_command(v.clone());
        }
    }

    // Manifest's own `[state]` table wins; else fall back to the org's
    // `[runner.<type>]` default (e.g. `state_backend = "s3"` for `tf`).
    let backend_name = unit
        .manifest
        .state_backend()
        .or_else(|| runner_cfg.get("state_backend").map(|s| s.as_str()));

    if let Some(backend_name) = backend_name {
        let template = config
            .state
            .get(backend_name)
            .map(|c| Value::Object(c.options.clone().into_iter().collect()))
            .unwrap_or_else(|| json!({}));

        let mut data = unit
            .manifest
            .state
            .as_ref()
            .map(|overrides| {
                Value::Object(
                    overrides
                        .iter()
                        .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                        .collect(),
                )
            })
            .unwrap_or_else(|| json!({}));
        gap_fill_merge(&mut data, &template);

        let context = json!({
            "org": config.org,
            "unit_name": unit.name,
            "dim_tree": unit.dim_tree(),
        });
        let rendered = render_state_backend_config(&data, &context)?;

        // `TerraformRunner::extend_plan` expects the backend name wrapped
        // around its options (`{"local": {"path": ...}}`), matching the HCL
        // shape `backend "local" { path = ... }` - not the flat options
        // object alone.
        params = params
            .with_state_backend(backend_name)
            .with_state_backend_config(json!({ backend_name: rendered }));
    }

    Ok(params)
}
