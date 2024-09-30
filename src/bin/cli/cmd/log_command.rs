use cubtera::prelude::*;

use clap::{Arg, ArgAction, ArgMatches, Command};
use cubtera::prelude::data::*;

pub fn get_command() -> Command {
    Command::new("log")
        .about("Unit deployment log commands")
        .subcommand_required(true)
        .subcommands([Command::new("get")
            .about("Get logs of unit deployments")
            .args([
                Arg::new("query")
                    .action(ArgAction::Append)
                    .help("search query")
                    .short('q')
                    .long("query")
                    .value_name("key:value")
                    .number_of_values(1)
                    .value_parser(super::if_contains(":"))
                    .required(true),
                Arg::new("limit")
                    .short('l')
                    .long("limit")
                    .value_name("limit")
                    .help("limit of returned logs")
                    .required(false)
                    .number_of_values(1),
            ])])
}

#[allow(clippy::needless_pass_by_value)]
pub fn run(subcommand: &ArgMatches, _: &Storage) {
    match subcommand.subcommand() {
        Some(("get", matches)) => {
            let keys = matches
                .get_many::<String>("query")
                .unwrap()
                .map(ToString::to_string)
                .collect::<Vec<String>>();
            let keys = keys.as_slice();

            let limit = matches
                .get_one::<String>("limit")
                .unwrap_or(&"10".to_string())
                .parse()
                .unwrap_or(10);
            let res = get_dlog_by_keys(&GLOBAL_CFG.org, keys.to_vec(), Some(limit));
            println!("{res}");
        }
        _ => unreachable!(),
    }
}
