# Cubtera configuration

## Configuration file

Cubtera configuration file is a toml file, which should be placed by default path `~/.cubtera/config.toml`. This file contains all cubtera configuration parameters, which could be used to configure cubtera behavior. If you want to use custom configuration file, you can set it with environment variable `CUBTERA_CONFIG`.

### Inventory and terraform folders parameters:
- `workspace_path` - path to your workspace, where all your terraform states will be stored, default is `~/.cubtera/workspace`
- `module_path` - path to your modules, where all your terraform modules will be stored, default is `~/.cubtera/workspace/modules`. This modules folder will be symlinked to every terraform unit folder, so you can use modules in your units without any additional configuration, just install them in this folder, and they will be available in every unit by path `./modules`
- `plugin_path` - path to your plugins, where all your terraform plugins will be stored, default is `~/.cubtera/workspace/plugins`. This plugins folder will be symlinked to every terraform unit folder, so you can use plugins in your units without any additional configuration, just install them in this folder, and they will be available in every unit by path `./plugins`
- `inventory_path` - path to your inventory, where all your terraform inventory will be stored, default is `~/.cubtera/workspace/inventory`. This should contain all your terraform units, which will be used in your infrastructure. Every unit should be placed in separate folder, and should contain `main.tf` file with terraform code, and `unit_manifest.json` file with unit configuration.
- `temp_folder_path` - path to your temp files, where all your terraform temp files will be stored, default is `~/.cubtera/temp`

Path could be set as absolute or relative to your current working directory, if you want to use relative path, you should start it with `./`.

### Dimensions relations parameters:

- `dim_relations` - configure relations between dimensions, default is `[]`. This is array of objects, where every object should contain related `dim_type`s separated by `:`. For example, `[dome:env:dc]` means that `dome`dimension is a parent of `env`, as well as `env` is a parent of `dc`. This relations will be used to provide a right variables set for a unit, depending on a used dimension. If unit uses `dc` dimension as a main, it will get all variables from `dc` dimension, and all variables from `env` dimension, and all variables from `dome` dimension. 
Cubtera currently supports only one level of relations, so you can't use `dome:env:dc` relations, but you can use `dome:env:dc` and `entity:service:app` relations separately.
Dimension parent should be also defined in unit manifest, for example, if you want to use `env` dimension as a parent for `dc` dimension, you should define `env` dimension in unit manifest:
```json
// unit_manifest.json of dc:staging1-us-e2 dimension
{
   ...
   "parent": "env:staging1",
   ...
}
```
more about unit manifest you can read [here](unit.md#unit-manifest)

### Development parameters:
- `clean_cache` - enable or disable cache cleaning cash after successful apply, default is `false`.
- `always_copy_files` - enable or disable unit files copying to cache folder for every command, default is `false`. If enabled, all unit files will be copied to cache folder before every command, if disabled, files will be copied only for `init` command.

### Deployment log parameters:
- `dlog_db` - configures mongo database connection string, if not set, deployment log will be disabled
- `dlog_job_user_name_env` - configures environment variable name, which will be used to store job user name, if not set, will be used local host user name
- `dlog_job_number_env` - configures environment variable name, which will be used to store job number, if not set, will be set to `0`
- `dlog_job_name_env` - configures environment variable name, which will be used to store job name, if not set, will be set to `local`

## Environment variables

Cubtera could be configured with environment variables, which will override configuration file parameters. All environment variables should be started with `CUBTERA_` prefix, and should be in upper case. For example, if you want to override `workspace_path` parameter, you should set environment variable `CUBTERA_WORKSPACE_PATH`.

Only one environment variable is required to start using cubtera - `CUBTERA_ORG`, which should contain name of your organization, and will be used to find your organization related units and dimensions in inventory, as well as for cubtera parameters could be different for every organization and could be set in configuration file.