use cubtera::prelude::*;
use clap::{Arg, ArgAction, ArgMatches, Command};
use cubtera::core::dim::data::Storage;

pub fn get_command() -> Command {
    Command::new("run")
        .alias("tf")
        .about("Run terraform unit with dimension values")
        .arg(
            Arg::new("dim")
                .action(ArgAction::Append)
                .help("Dimension type and name")
                .short('d')
                .long("dim")
                .value_name("dim_type:dim_name")
                .number_of_values(1)
                .value_parser(super::if_contains(":"))
                .required(true),
        )
        .arg(
            Arg::new("ext")
                .action(ArgAction::Append)
                .help("Extension type and name (opt)")
                .long_help("Extension type and name (opt)\nUsed for run a unit with the same dimensions but with different states\nExample: -e index:0")
                .short('e')
                .long("ext")
                .value_name("ext_type:ext_name")
                .number_of_values(1)
                .value_parser(super::if_contains(":"))
                .required(false),
        )
        .arg(
            Arg::new("unit")
                .short('u')
                .long("unit")
                .value_name("name")
                .help("Unit name")
                .required(true)
                .number_of_values(1)
        )
        .arg(
            Arg::new("context")
                .help("Context")
                .value_name("context")
                .required(false)
                .short('c')
                .long("context")
        )
        .arg(
            Arg::new("command")
                .last(true)
                .help("Terraform command")
                .required(false)
                .action(ArgAction::Append)
                .allow_hyphen_values(true)
        )
}

#[allow(clippy::needless_pass_by_value)]
pub fn run(sub_matches: &ArgMatches, storage: &Storage) {
    let dimensions = sub_matches
        .get_many::<String>("dim")
        .unwrap()
        .map(std::string::ToString::to_string)
        .collect::<Vec<String>>();
    let dimensions = dimensions.as_slice();

    let unit_name = sub_matches.get_one::<String>("unit").unwrap().clone();

    let command = sub_matches
        .get_many::<String>("command")
        .unwrap_or_default()
        .map(std::string::ToString::to_string)
        .collect::<Vec<String>>();
        // .join(" ");

    let context = sub_matches.get_one::<String>("context").cloned();

    let extensions = sub_matches
        .get_many::<String>("ext")
        .unwrap_or_default()
        .map(std::string::ToString::to_string)
        .collect::<Vec<String>>();

    let extensions = extensions.as_slice();
    let unit = Unit::new(unit_name, dimensions, extensions, storage, context).build();

    let res = RunnerBuilder::new(unit, command)
        .build()
        .run()
        .unwrap_or_exit("Unit runner failed".to_string());

    let exit_code = res["exit_code"].as_i64().unwrap_or(0);
    std::process::exit(exit_code as i32);
}