use cubtera::prelude::*;
use cubtera::core::dim::data::Storage;

use clap::{command, ArgMatches};
mod im_command;
mod log_command;
mod run_command;

// custom result type
type CliResult<T> = Result<T, Box<dyn std::error::Error>>;

pub fn get_matches() -> ArgMatches {
    command!()
        .about("Immersive cubic dimensions experience")
        .propagate_version(true)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .trailing_var_arg(true)
        .subcommand(im_command::get_command())
        .subcommand(log_command::get_command())
        .subcommand(run_command::get_command())
        .subcommand(
            command!("config")
                .about("Show configuration")
                .alias("cfg")
        )
        .get_matches()
}

//#[derive(Debug)]
pub struct Cli {
    pub subcommand: ArgMatches,
    pub storage: Storage,
    executor: CliExecutor,
}

type CliExecutor = fn(subcommand: &ArgMatches, storage: &Storage) -> ();

#[allow(clippy::unnecessary_wraps)]
pub fn get_args() -> CliResult<Cli> {
    let matches = get_matches(); // Take arguments from CLI

    // STORAGE TYPE: if CUBTERA_DB env var is set -> DB, else -> FS
    let storage = match &GLOBAL_CFG.db_client.is_some() {
        true => Storage::DB,
        false => Storage::FS,
    };

    if GLOBAL_CFG.org.is_empty() {
        exit_with_error(
            "Organization name is not set!\nDefine a proper name with CUBTERA_ORG env var."
                .to_string(),
        );
    };

    Ok(match matches.subcommand() {
        // legacy command support
        Some(("tf", sub_matches)) => Cli {
            subcommand: sub_matches.clone(),
            executor: run_command::run,
            storage,
        },
        Some(("im", sub_matches)) => Cli {
            subcommand: sub_matches.clone(),
            executor: im_command::run,
            storage,
        },
        Some(("log", sub_matches)) => Cli {
            subcommand: sub_matches.clone(),
            executor: log_command::run,
            storage,
        },
        Some(("run", sub_matches)) => Cli {
            subcommand: sub_matches.clone(),
            executor: run_command::run,
            storage,
        },
        Some(("config", _)) => {
            println!("{}", &GLOBAL_CFG.get_json());
            std::process::exit(0);
        }
        _ => unreachable!(),
    })
}

//#[allow(clippy::unnecessary_wraps)]
pub fn run(cli: Cli) -> CliResult<()> {
    (cli.executor)(&cli.subcommand, &cli.storage);
    Ok(())
}

// Value parser for cli arguments
use clap::builder::ValueParser;
use clap::error::{Error, ErrorKind};

fn if_contains(r: &'static str) -> ValueParser {
    ValueParser::from(move |s: &str| -> Result<String, Error> {
        if s.contains(r) {
            Ok(s.to_owned())
        } else {
            Err(Error::raw(
                ErrorKind::ValueValidation,
                format!("Argument must contain '{r}'"),
            ))
        }
    })
}