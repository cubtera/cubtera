use cubtera::prelude::*;
use cubtera::core::dim::data::Storage;
use clap::{Arg, ArgAction, ArgMatches, Command};

fn get_dim_type_arg() -> Arg {
    Arg::new("dim_type")
        .action(ArgAction::Set)
        .help("Dimension type")
        .value_name("dim_type")
        .required(true)
}

fn get_dim_name_arg() -> Arg {
    Arg::new("dim_name")
        .action(ArgAction::Set)
        .help("Dimension name")
        .value_name("dim_name")
        .required(true)
}

fn get_context_arg() -> Arg {
    Arg::new("context")
        .action(ArgAction::Set)
        .help("Context")
        .value_name("context")
        .required(false)
        .short('c')
}

pub fn get_command() -> Command {
    Command::new("im")
        .about("Run inventory management commands")
        .subcommand_help_heading("Available commands")
        .subcommand_value_name("COMMAND")
        .subcommand_required(true)
        .subcommands([
            Command::new("getAll")
                .about("Get all dim_names of a dim_type")
                .arg(get_dim_type_arg()),
            Command::new("getAllData")
                .about("Get all dim's data by dim_type")
                .arg(get_dim_type_arg()),
            Command::new("getDefaults")
                .about("Get all defaults by dim_type")
                .arg(get_dim_type_arg()),
            Command::new("getByName")
                .about("Get data by dim_type:dim_name")
                .arg(get_dim_type_arg())
                .arg(get_dim_name_arg())
                .arg(
                    Arg::new("context")
                        .action(ArgAction::Set)
                        .help("Context")
                        .value_name("context")
                        .required(false)
                        .short('c')
                ),
            Command::new("getByParent")
                .about("Get all kids of a dim_type:dim_name")
                .arg(get_dim_type_arg())
                .arg(get_dim_name_arg()),
            Command::new("getParent")
                .about("Get parent data by dim_type:dim_name")
                .arg(get_dim_type_arg())
                .arg(get_dim_name_arg()),
            Command::new("getOrgs").about("Get all Orgs names from config file"),
            Command::new("validate")
                .about("Validate json for dim_name of dim_type")
                .arg(get_dim_type_arg())
                .arg(get_dim_name_arg()),
            Command::new("syncDefaults")
                .about("Sync dim_type defaults with DB from files (Required CUBTERA_DB)")
                .arg(get_dim_type_arg()),
            Command::new("syncAll")
                .about("Sync all entries of dim_type with DB from files (Required CUBTERA_DB)")
                .arg(get_dim_type_arg())
                .arg(get_context_arg()),
            Command::new("sync")
                .about("Sync dim_name with DB from files (Required CUBTERA_DB)")
                .arg(get_dim_type_arg())
                .arg(get_dim_name_arg())
                .arg(get_context_arg()),
            Command::new("deleteContext")
                .about("Sync all entries of dim_type with DB from files (Required CUBTERA_DB)")
                .arg(
                    Arg::new("context")
                        .help("Context")
                        .value_name("context")
                        .required(true)
                        //.short('c')
                )
        ])
}

#[allow(clippy::needless_pass_by_value, clippy::too_many_lines)]
pub fn run(subcommand: &ArgMatches, storage: &Storage) {
    match subcommand.subcommand() {
        Some(("getAll", sub_sub_matches)) => {
            let dim_type = sub_sub_matches
                .get_one::<String>("dim_type")
                .unwrap()
                .to_string();
            println!("{}", get_dim_names_by_type(&dim_type, &GLOBAL_CFG.org, storage));
        }
        Some(("getAllData", sub_sub_matches)) => {
            let dim_type = sub_sub_matches
                .get_one::<String>("dim_type")
                .unwrap()
                .to_string();
            println!("{}", get_dims_data_by_type(&dim_type, &GLOBAL_CFG.org, storage));
        }
        Some(("getDefaults", sub_sub_matches)) => {
            let dim_type = sub_sub_matches
                .get_one::<String>("dim_type")
                .unwrap()
                .to_string();
            println!("{}", get_dim_defaults_by_type(&dim_type, &GLOBAL_CFG.org, storage));
        },
        Some(("getByName", sub_sub_matches)) => {
            let dim = get_dim_by_name(
                sub_sub_matches.get_one::<String>("dim_type").unwrap(),
                sub_sub_matches.get_one::<String>("dim_name").unwrap(),
                &GLOBAL_CFG.org,
                storage,
                sub_sub_matches.get_one::<String>("context").cloned()
            );
            println!("{dim}");
        }
        Some(("getByParent", sub_sub_matches)) => {
            let dim_type = sub_sub_matches.get_one::<String>("dim_type").unwrap();
            let dim_name = sub_sub_matches.get_one::<String>("dim_name").unwrap();
            let dims = get_dim_kids(dim_type, dim_name, &GLOBAL_CFG.org, storage);

            println!("{dims}");
        }
        Some(("getParent", sub_sub_matches)) => {
            let parent = get_dim_parent(
                sub_sub_matches.get_one::<String>("dim_type").unwrap(),
                sub_sub_matches.get_one::<String>("dim_name").unwrap(),
                &GLOBAL_CFG.org,
                storage,
            );

            println!("{parent}");
        }
        Some(("getOrgs", _)) => {
            println!("{}", get_all_orgs(storage));
        }
        Some(("syncDefaults", sub_sub_matches)) => {
            let dim_type = sub_sub_matches
                .get_one::<String>("dim_type")
                .unwrap()
                .to_string();

            DimBuilder::new(&dim_type, &GLOBAL_CFG.org, &Storage::FS)
                .read_default_data()
                .switch_datasource(&Storage::DB)
                .save_default_data();
        }

        Some(("syncAll", sub_sub_matches)) => {
            let dim_type = sub_sub_matches
                .get_one::<String>("dim_type")
                .unwrap()
                .to_string();

            DimBuilder::new(&dim_type, &GLOBAL_CFG.org, &Storage::FS)
                .with_context(sub_sub_matches.get_one::<String>("context").cloned())
                .save_all_data_by_type();
        }

        Some(("sync", sub_sub_matches)) => {
            let dim_type = sub_sub_matches
                .get_one::<String>("dim_type")
                .unwrap();
            DimBuilder::new(dim_type, &GLOBAL_CFG.org, &Storage::FS)
                .with_name(sub_sub_matches.get_one::<String>("dim_name").unwrap())
                .read_data()
                //.read_default_data()
                .switch_datasource(&Storage::DB)
                .with_context(sub_sub_matches.get_one::<String>("context").cloned())
                .save_data();
        }

        Some(("deleteContext", sub_sub_matches)) => {
            let context = sub_sub_matches
                .get_one::<String>("context").cloned();
            DimBuilder::new("", &GLOBAL_CFG.org,&Storage::DB)
                .with_context(context)
                .delete_all_data_by_context();
        }

        Some(("validate", sub_sub_matches)) => {
            let dim_type = sub_sub_matches
                .get_one::<String>("dim_type")
                .unwrap();
            let dim_name = sub_sub_matches
                .get_one::<String>("dim_name")
                .unwrap();
            let _ = DimBuilder::new(dim_type, &GLOBAL_CFG.org,&Storage::FS)
                .with_name(dim_name)
                .read_data()
                .build();

            println!("Sorry, validate command was not implemented yet...");
        }
        _ => unreachable!(),
    };
}
