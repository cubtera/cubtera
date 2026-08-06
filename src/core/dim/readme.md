Here's a markdown documentation for the `data` module and its two submodules `mongodb` and `jsonfile`:

# Data Module Documentation

The `data` module provides an abstraction layer for different data storage mechanisms used in the Cubtera project. It defines a common interface (`DataSource`) that can be implemented by various storage backends.

## Main Components

### Storage Enum

```rust
pub enum Storage {
    FS,
    DB,
}
```

This enum represents the available storage types:
- `FS`: File System storage
- `DB`: Database storage

### DataSource Trait

```rust
pub trait DataSource: CloneBox + 'static {
    fn get_data_by_name(&self, name: &str) -> Result<Value, Box<dyn std::error::Error>>;
    fn get_all_data(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>>;
    fn get_all_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>>;
    fn get_all_types(&self) -> Result<Vec<String>, Box<dyn std::error::Error>>;
    fn upsert_all_data(&self, _data: Vec<Value>) -> Result<(), Box<dyn std::error::Error>>;
    fn upsert_data_by_name(&self, name: &str, data: Value) -> Result<(), Box<dyn std::error::Error>>;
    fn delete_data_by_name(&self, name: &str) -> Result<(), Box<dyn std::error::Error>>;
    fn delete_all_by_context(&self, context: &str) -> Result<(), Box<dyn std::error::Error>>;
    fn set_context(&mut self, context: Option<String>);
    fn get_context(&self) -> Option<String>;
}
```

This trait defines the common interface for all data sources. It includes methods for retrieving, updating, and deleting data, as well as managing context.

### data_src_init Function

```rust
pub fn data_src_init(org: &str, dim_type: &str, storage: Storage) -> Box<dyn DataSource>
```

This function initializes and returns a `DataSource` implementation based on the provided `storage` type.

## Submodules

### mongodb

This submodule provides a MongoDB implementation of the `DataSource` trait.

#### MongoDBDataSource Struct

```rust
pub struct MongoDBDataSource {
    client: Client,
    db_name: String,
    col_name: String,
    col: Collection<Bson>,
    db: Database,
    context: Option<String>,
}
```

This struct represents a MongoDB data source and implements the `DataSource` trait.

### jsonfile

This submodule provides a JSON file-based implementation of the `DataSource` trait.

#### JsonDataSource Struct

```rust
pub struct JsonDataSource {
    path: PathBuf,
    col_name: String,
    context: Option<String>,
}
```

This struct represents a JSON file-based data source and implements the `DataSource` trait.

## Usage

To use a specific data source, you can create an instance of the appropriate struct (`MongoDBDataSource` or `JsonDataSource`) and use it through the `DataSource` trait interface. Alternatively, you can use the `data_src_init` function to create the appropriate data source based on the desired storage type.

Example:

```rust
let storage = Storage::DB;
let data_source = data_src_init("my_org", "my_dim_type", storage);
let all_data = data_source.get_all_data()?;
```

This code creates a MongoDB data source (since `Storage::DB` is specified) and retrieves all data using the common `DataSource` interface.