Certainly! I'll create a markdown documentation for the `unit` module based on the provided `mod.rs` file. Here's the documentation:

# Unit Module

The `unit` module is responsible for managing and processing units within the application. It provides functionality for loading, building, and handling units with their associated dimensions and extensions.

## Structures

### Unit

The main structure in this module is `Unit`, which represents a unit in the system.

```rust
pub struct Unit {
    pub name: String,
    pub manifest: Manifest,
    pub temp_folder: PathBuf,
    pub extensions: Vec<String>,
    dimensions: Vec<Dim>,
    opt_dims: Option<Vec<Dim>>,
    unit_folder: PathBuf,
    generic_unit_folder: Option<PathBuf>,
}
```

## Main Functions

### Unit::new

Creates a new `Unit` instance.

```rust
pub fn new(
    name: String,
    dimensions: &[String],
    extensions: &[String],
    storage: &Storage,
    context: Option<String>,
) -> Self
```

- Loads the unit manifest from the appropriate folder.
- Validates required dimensions.
- Sorts and processes dimensions.
- Sets up temporary folders and other unit properties.

### Unit::build

Builds the unit, performing various checks and validations.

```rust
pub fn build(self) -> Self
```

- Checks for allowed and denied dimensions.
- Validates affinity tags.

### Unit::copy_files

Copies necessary files for the unit.

```rust
pub fn copy_files(&self)
```

- Creates temporary folders.
- Sets up symlinks for modules.
- Copies plugin folders.
- Copies unit files.
- Generates dimension variable JSON files.
- Copies files specified in the unit manifest.

## Helper Functions

- `get_dims_from_cli`: Retrieves dimensions from CLI arguments.
- `get_unit_state_path`: Generates the unit state path.
- `remove_temp_folder`: Removes the temporary folder for the unit.
- `copy_files_from_manifest`: Copies files specified in the unit manifest.

## Error Handling

The module uses custom error handling mechanisms, including:
- `exit_with_error`: Exits the program with an error message.
- `unwrap_or_exit`: Unwraps a `Result` or exits with an error message.

## Dependencies

This module relies on several external and internal dependencies:
- External: `serde_json`, `yansi`, `anyhow`
- Internal: `crate::globals`, `crate::prelude`, `crate::utils`

## Notes

- The module handles both organization-specific and generic units.
- It processes required and optional dimensions.
- Affinity tags are checked for compatibility.
- The module manages temporary folders and file copying for unit execution.

This documentation provides an overview of the `unit` module's main components and functionality. For more detailed information on specific methods or structures, refer to the inline comments and function signatures in the source code.