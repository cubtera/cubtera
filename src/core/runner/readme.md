Here's a markdown documentation for the `runner` module based on the provided code:

# Runner Module Documentation

The `runner` module is responsible for managing and executing different types of runners in the project. It provides a flexible framework for handling various runner types and their associated parameters.

## Key Components

### RunnerParams (params.rs)

`RunnerParams` is a struct that holds configuration parameters for runners.

```rust
pub struct RunnerParams {
    pub version: String,
    pub state_backend: String,
    pub runner_command: Option<String>,
    pub extra_args: Option<String>,
    pub inlet_command: Option<String>,
    pub outlet_command: Option<String>,
    pub lock_port: String,
}
```

Key methods:
- `init(params: HashMap<String, String>) -> Self`: Initializes `RunnerParams` from a HashMap.
- `get_params_hashmap(&self) -> HashMap<String, String>`: Converts `RunnerParams` back to a HashMap.
- `get_lock_port(&self) -> u16`: Returns the lock port as a u16.
- `get_version(&self) -> String`: Returns the version.
- `get_state_backend(&self) -> String`: Returns the state backend.

### RunnerType (mod.rs)

An enum representing different types of runners:

```rust
pub enum RunnerType {
    TF,
    BASH,
    TOFU,
    UNKNOWN,
}
```

### RunnerLoad (mod.rs)

A struct containing all necessary data for initializing a runner:

```rust
pub struct RunnerLoad {
    unit: Unit,
    command: Vec<String>,
    params: params::RunnerParams,
    state_backend: Value,
}
```

### Runner Trait (mod.rs)

The `Runner` trait defines the interface for all runner implementations:

```rust
pub trait Runner {
    fn new(load: RunnerLoad) -> Self where Self: Sized;
    fn get_load(&self) -> &RunnerLoad;
    fn get_ctx(&self) -> &Value;
    fn get_ctx_mut(&mut self) -> &mut Value;
    
    // Default implementations provided for:
    fn copy_files(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn change_files(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn inlet(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn runner(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn outlet(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn logger(&mut self) -> Result<(), Box<dyn std::error::Error>>;
    fn run(&mut self) -> Result<Value, Box<dyn std::error::Error>>;
    
    // Helper methods:
    fn update_ctx(&mut self, key: &str, value: Value);
    fn executor(&mut self, step: &str) -> Result<(), Box<dyn std::error::Error>>;
}
```

### RunnerBuilder (mod.rs)

`RunnerBuilder` is responsible for constructing and configuring runners:

```rust
pub struct RunnerBuilder {
    unit: Unit,
    command: Vec<String>,
}
```

Key method:
- `build(&self) -> Box<dyn Runner>`: Constructs and returns a boxed `Runner` trait object.

## Functionality

1. The module supports multiple runner types (TF, BASH, TOFU).
2. Runners can be dynamically created based on the `RunnerType`.
3. Configuration parameters can be loaded from both global config and unit manifest.
4. State backend configuration is flexible and supports templating.
5. The `Runner` trait provides a common interface for all runner types, with default implementations for common operations.

## Usage

To create a new runner:

1. Instantiate a `RunnerBuilder` with a `Unit` and command.
2. Call the `build()` method to get a boxed `Runner`.
3. Use the returned `Runner` to execute the desired operations.

Example:
```rust
let builder = RunnerBuilder::new(unit, command);
let mut runner = builder.build();
let result = runner.run()?;
```

This module provides a flexible and extensible system for managing different types of runners in the project, allowing for easy addition of new runner types and customization of runner behavior.