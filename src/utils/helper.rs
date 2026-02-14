use log::{debug, error, warn};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn get_blob_sha_by_path(path: &PathBuf) ->  Result<String, git2::Error> {
    let abs_path =  path_to_absolute(path);

    // Open the repository
    let repo = git2::Repository::discover(&abs_path)?;

    // Get the repository's workdir (root directory)
    let repo_root = repo.workdir().ok_or_else(|| {
        git2::Error::from_str("Could not get repository working directory")
    })?;

    // Convert the absolute path to a path relative to the repository root
    let relative_path = abs_path.strip_prefix(repo_root).map_err(|_| {
        git2::Error::from_str("Failed to get relative path")
    })?;

    // Get the HEAD commit
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    
    // Get the tree from the commit
    let tree = commit.tree()?;
    
    // Get the tree entry for the relative_path
    let entry = tree.get_path(relative_path)?;

    // Get the object ID (SHA) of the entry
    let entry_sha = entry.id().to_string();

    Ok(entry_sha)
}

fn path_to_absolute(path: &PathBuf) -> PathBuf {
    if path.is_absolute() {
        path.clone()
    } else {
        std::env::current_dir().unwrap().join(path)
    }
}

pub fn get_commit_sha_by_path(path_buf: &PathBuf) -> Result<String, git2::Error> {
    let abs_path = path_to_absolute(path_buf);

    // Open the repository
    let repo = git2::Repository::discover(abs_path)?;

    // Get the HEAD commit
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    // Get the commit ID (SHA)
    let commit_sha = commit.id().to_string();

    Ok(commit_sha)
}

pub fn get_sha_by_value(value: &Value) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    let ordered = order_json(value);
    let canonical_json = serde_json::to_string(&ordered).unwrap_or_default();
    hasher.update(canonical_json.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn order_json(value: &Value) -> Value {
    use std::collections::BTreeMap;
    match value {
        Value::Object(map) => {
            let ordered: BTreeMap<_, _> = map
                .iter()
                .map(|(k, v)| (k.clone(), order_json(v)))
                .collect();
            Value::Object(ordered.into_iter().collect())
        }
        Value::Array(arr) => {
            let mut ordered: Vec<Value> = arr.iter().map(order_json).collect();
            if ordered.iter().all(|v| v.is_number() || v.is_string()) {
                ordered.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
            }
            Value::Array(ordered)
        }
        _ => value.clone(),
    }
}

pub fn db_connect(db: &str) -> mongodb::sync::Client {
    let options = match mongodb::options::ClientOptions::parse(db).run() {
        Ok(client) => client,
        Err(e) => exit_with_error(format!("DB connection error: {e}")),
    };
    let client = mongodb::sync::Client::with_options(options);
    match client {
        Ok(client) => client,
        Err(e) => exit_with_error(format!("DB connection error: {e}")),
    }
}

/// Exits the program with an error message and a status code of 1.
///
/// # Arguments
///
/// * `error` - A string slice that holds the error message to be logged.
#[allow(clippy::needless_pass_by_value)]
pub fn exit_with_error(error: String) -> ! {
    error!(target: "", "{}", error);
    std::process::exit(1);
}

// helper trait for unwrapping Result to value or exit with error message if Error
pub trait ResultExtUnwrap<T, E> {
    fn unwrap_or_exit(self, error: String) -> T;
}

impl<T, E: std::fmt::Display> ResultExtUnwrap<T, E> for Result<T, E> {
    fn unwrap_or_exit(self, error: String) -> T {
        match self {
            Ok(value) => value,
            Err(e) => {
                error!(target: "", "{}: {}", error, e);
                std::process::exit(1);
            }
        }
    }
}

pub trait ResultExtWarn<T, E> {
    fn check_with_warn(self, warning: &str) -> Result<T, E>;
}

impl<T, E: std::fmt::Display> ResultExtWarn<T, E> for Result<T, E> {
    fn check_with_warn(self, warning: &str) -> Result<T, E> {
        match self {
            Ok(value) => Ok(value),
            Err(e) => {
                warn!(target: "", "{}: {}", warning, e);
                Err(e)
            }
        }
    }
}

// helper trait for unwrapping Option to value or exit with error message if None
pub trait OptionExtUnwrap<T> {
    fn unwrap_or_exit(self, error: String) -> T;
}

impl<T> OptionExtUnwrap<T> for Option<T> {
    fn unwrap_or_exit(self, error: String) -> T {
        if let Some(t) = self {
            t
        } else {
            error!(target: "", "{}", error);
            std::process::exit(1);
        }
    }
}

pub fn convert_path_to_absolute(s: String) -> Option<String> {
    s.starts_with('~')
        .then(|| s.replacen("~", &std::env::var("HOME").unwrap(), 1))
        .or(s
            .starts_with('.')
            .then(|| s.replacen(".", &std::env::var("PWD").unwrap(), 1)))
        .or(s.starts_with('/').then(|| s.clone()))
        .or(Path::new(&s).is_relative().then(|| {
            std::env::current_dir()
                .unwrap()
                .join(&s)
                .to_str()
                .unwrap()
                .to_string()
        }))
}

/// Reads a JSON file from the given path and returns its content as a `serde_json::Value` object.
///
/// # Arguments
///
/// * `path` - A `PathBuf` object representing the path to the JSON file.
///
/// # Returns
///
/// An `Option` containing the `serde_json::Value` object if the file exists and is a valid JSON file,
/// otherwise `None`.
pub fn read_json_file(path: &PathBuf) -> Option<Value> {
    match std::fs::read_to_string(path) {
        Ok(json) => {
            let meta_data = serde_json::from_str::<Value>(&json).unwrap_or_exit(format!(
                "Sorry, but file {:?} is not a valid JSON file",
                &path
            ));
            Some(meta_data)
        }
        Err(_) => {
            debug!(target:"", "File {:?} doesn't exist. Pass empty data...", &path.file_name().unwrap());
            None
        }
    }
}

/// Merges the values from the `source` JSON object into the `target` JSON object.
/// If a key exists in both objects, the values are merged recursively.
/// If a key exists in the `target` object but not in the `source` object, the value is added to the `target` object.
/// If the values are arrays, the `source` array is appended to the `target` array.
///
/// # Arguments
///
/// * `target` - A mutable reference to the target JSON object.
/// * `source` - A reference to the source JSON object.
///
/// # Example
///
/// ```
/// use serde_json::json;
/// use cubtera::prelude::merge_values;
///
/// let mut target = json!({
///     "name": "John",
///     "age": 30,
///     "address": {
///         "street": "123 Main St",
///         "city": "New York"
///     },
///     "hobbies": ["reading", "gaming"]
/// });
///
/// let source = json!({
///     "age": 31,
///     "address": {
///         "city": "San Francisco",
///         "state": "CA"
///     },
///     "hobbies": ["traveling", "cooking"],
///     "gender": "male"
/// });
///
/// merge_values(&mut target, &source);
/// ```
// pub fn merge_values(target: &mut serde_json::Value, source: &serde_json::Value) {
//     match (target, source) {
//         (serde_json::Value::Object(target_obj), serde_json::Value::Object(source_obj)) => {
//             for (key, source_value) in source_obj.iter() {
//                 if !target_obj.contains_key(key) {
//                     target_obj.insert(key.clone(), source_value.clone());
//                 } else {
//                     // Recursively merge if the value is an object
//                     merge_values(target_obj.get_mut(key).unwrap(), &source_value.clone());
//                 }
//             }
//         }
//         (serde_json::Value::Array(target_arr), serde_json::Value::Array(source_arr)) => {
//             // Merge arrays by extending the target array
//             for source_value in source_arr {
//                 if !target_arr.contains(source_value) {
//                     target_arr.push(source_value.clone());
//                 }
//             }
//         }
//         _ => {}
//     }
// }
pub fn merge_values(data: &mut serde_json::Value, with: &serde_json::Value) {
    if let (serde_json::Value::Object(data_obj), serde_json::Value::Object(with_obj)) = (data, with)
    {
        for (key, with_value) in with_obj {
            if let serde_json::map::Entry::Vacant(entry) = data_obj.entry(key) {
                entry.insert(with_value.clone());
            } else if let serde_json::map::Entry::Occupied(mut entry) = data_obj.entry(key) {
                if with_value.is_object() && entry.get().is_object() {
                    merge_values(entry.get_mut(), with_value);
                }
            }
        }
    }
}

use std::collections::HashSet;
use std::process::ExitStatus;

pub fn if_intersect(vec1: Vec<String>, vec2: Vec<String>) -> bool {
    let set1: HashSet<String> = vec1.into_iter().collect();
    let set2: HashSet<String> = vec2.into_iter().collect();
    let intersect: HashSet<String> = set1.intersection(&set2).cloned().collect();
    !intersect.is_empty()
}

pub fn value_intersection(value1: Value, value2: Value) -> Option<HashSet<String>> {
    let vec1 = value_to_vec(&value1);
    let vec2 = value_to_vec(&value2);
    let set1: HashSet<_> = vec1?.into_iter().collect();
    let set2: HashSet<_> = vec2?.into_iter().collect();

    match set1.intersection(&set2).cloned().collect::<HashSet<_>>() {
        set if set.is_empty() => None,
        set => Some(set),
    }
}

fn value_to_vec(value: &Value) -> Option<Vec<String>> {
    value
        .as_array()?
        .iter()
        .map(|v| v.as_str().map(|s| s.to_string()))
        .collect()
}

pub fn group_tuples(tuples: Vec<(String, String)>) -> HashMap<String, Vec<String>> {
    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    tuples.into_iter().for_each(|(key, value)| {
        map.entry(key).or_default().push(value);
    });
    map
}

/// Validates a JSON object against a JSON schema.
///
/// # Arguments
///
/// * `json` - A JSON object to be validated.
/// * `schema` - A JSON schema against which the `json` object is validated.
///
/// # Returns
///
/// Returns a cloned `json` object if it is valid against the `schema`. Otherwise, returns `None`.
///
/// # Example
///
/// ```
/// use serde_json::json;
/// use cubtera::prelude::validate_json_by_schema;
///
/// let json_obj = json!({
///     "name": "John Doe",
///     "age": 30,
///     "city": "New York"
/// });
///
/// let schema = json!({
///     "type": "object",
///     "properties": {
///         "name": {"type": "string"},
///         "age": {"type": "number"},
///         "city": {"type": "string"}
///     },
///     "required": ["name", "age", "city"]
/// });
///
/// let result = validate_json_by_schema(&json_obj, &schema);
/// assert_eq!(result, Some(json_obj));
/// ```
pub fn validate_json_by_schema(json: &Value, schema: &Value) -> Option<Value> {
    let validator = jsonschema::validator_for(schema).unwrap();
    validator.validate(json).ok().map(|_| json.clone())
}

/// Reads and validates a JSON file against a JSON schema file.
///
/// # Arguments
///
/// * `json_path` - A `PathBuf` representing the path to the JSON file.
/// * `schema_path` - A `PathBuf` representing the path to the JSON schema file.
///
/// # Returns
///
/// An `Option<Value>` containing the parsed JSON data if validation succeeds, otherwise `None`.
///
/// # Panics
///
/// This function will panic if it is unable to read the schema file or if it is unable to parse the schema file.
/// It will also panic if it is unable to read the JSON file or if it is unable to parse the JSON file.
/// If validation fails, it will exit with an error message.
pub fn read_and_validate_json(json_path: PathBuf, schema_path: PathBuf) -> Option<Value> {
    let json_schema = std::fs::read_to_string(&schema_path)
        .unwrap_or_exit(format!("Can't read schema: {:?}.", &schema_path));
    let json_schema = serde_json::from_str(&json_schema)
        .unwrap_or_exit(format!("Can't parse schema: {:?}.", schema_path));

    let validator = jsonschema::validator_for(&json_schema).unwrap();

    let json_data = std::fs::read_to_string(&json_path)
        .unwrap_or_exit(format!("Can't read json: {:?}", &json_path));
    let json = serde_json::from_str::<Value>(&json_data)
        .unwrap_or_exit(format!("Can't parse json: {:?}", json_path));
    
    if ! validator.is_valid(&json) {
        let errors = validator.iter_errors(&json);
        for error in errors {
            println!("Validation error: {}", error);
        }
        exit_with_error(format!("File: {:?}", json_path)); 
    }
    
    Some(json)
}

/// # Panics
///
/// Will panic if there is a problem with files i/o
/// Copies all files in a folder from the source path to the destination path.
/// If the destination path does not exist, it will be created.
///
/// # Arguments
///
/// * `src` - A `PathBuf` representing the source folder path.
/// * `dst` - A reference to a `PathBuf` representing the destination folder path.
pub fn copy_all_files_in_folder(src: PathBuf, dst: &PathBuf, overwrite_existing: bool) {
    if !dst.exists() {
        std::fs::create_dir_all(dst)
            .unwrap_or_exit(format!("Failed to create folder {}", dst.to_str().unwrap()));
    }
    walkdir::WalkDir::new(src)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file())
        //.filter(|_| override_existing)
        .filter(|path| !(dst.join(path.file_name()).exists()) || overwrite_existing)
        .for_each(|e| {
            let src_path = e.clone().into_path();
            let dst_path = dst.join(e.into_path().file_name().unwrap());
            std::fs::copy(&src_path, &dst_path)
                .unwrap_or_exit(format!("Failed to copy file {src_path:?} to {dst_path:?}"));
        });
}

/// Recursively copies all files and subfolders from the source folder to the destination folder.
///
/// # Arguments
///
/// * `src` - A `PathBuf` representing the source folder to copy from.
/// * `dst` - A reference to a `PathBuf` representing the destination folder to copy to.
pub fn copy_folder(src: PathBuf, dst: &PathBuf, overwrite_existing: bool) {
    copy_all_files_in_folder(src.clone(), dst, overwrite_existing);

    walkdir::WalkDir::new(src)
        .max_depth(1)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|path| path.path().is_dir())
        .skip(1)
        .for_each(|folder| {
            copy_folder(
                folder.clone().into_path(),
                &dst.join(folder.into_path().file_name().unwrap()),
                overwrite_existing,
            );
        });
}

pub fn check_path(path: PathBuf) -> Option<PathBuf> {
    match std::fs::metadata(&path) {
        Ok(_) => Some(path),
        Err(_) => None,
    }
}

pub fn string_to_path(s: &str) -> PathBuf {
    let mut path = s.to_string();

    // Expand tilde to home directory
    if path.starts_with("~") {
        if let Ok(home) = std::env::var("HOME") {
            path = path.replacen("~", &home, 1);
        }
    }

    // Expand relative path to absolute
    if path.starts_with("./") {
        if let Ok(pwd) = std::env::var("PWD") {
            path = path.replacen(".", &pwd, 1);
        }
    }

    // Expand environment variables in path string
    // Replace ${VAR} and $VAR with actual values
    let with_env = std::env::vars().fold(path, |s, (k, v)| {
        s.replace(&format!("${}", k), &v)
            .replace(&format!("${{{}}}", k), &v)
    });

    // Check if path is relative and convert to absolute
    // if Path::new(&with_env).is_relative() {
    //     with_env = std::env::current_dir().unwrap()
    //         .join(&with_env).to_str().unwrap().to_string();
    // }

    // Convert to PathBuf
    PathBuf::from(with_env)
}

pub fn execute_command(
    command: &str,
    current_dir: &str,
    env_vars: HashMap<String, String>,
) -> Result<ExitStatus, Box<dyn std::error::Error>> {
    let mut command = command.split_whitespace();
    let binary = command.next().unwrap_or_exit("Command is empty".into());
    let path = string_to_path(binary);
    let args = command.collect::<Vec<&str>>();

    let mut process = std::process::Command::new(path)
        .current_dir(current_dir)
        .args(args)
        .envs(env_vars)
        .spawn()?;

    let result = process.wait();

    match result {
        Ok(status) => Ok(status),
        Err(e) => Err(e.into()),
    }
}

pub fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[test]
fn test_copy_all_files_in_folder() {
    use std::fs;

    // Prepare a temporary test directory with some files
    let src_dir = tempfile::tempdir().unwrap();
    let file1_path = src_dir.path().join("file1.txt");
    let file2_path = src_dir.path().join("file2.txt");
    fs::write(file1_path, "Hello, world!").unwrap();
    fs::write(file2_path, "Goodbye, world!").unwrap();

    // Prepare a temporary test destination directory
    let dst_dir = tempfile::tempdir().unwrap();
    let dst_path = dst_dir.path();

    // Copy the files from the source directory to the destination directory
    copy_all_files_in_folder(src_dir.path().to_path_buf(), &dst_path.to_path_buf(), true);

    // Check that the files were copied correctly
    let copied_file1_path = dst_path.join("file1.txt");
    let copied_file2_path = dst_path.join("file2.txt");
    assert!(copied_file1_path.exists());
    assert!(copied_file2_path.exists());
    assert_eq!(
        fs::read_to_string(&copied_file1_path).unwrap(),
        "Hello, world!"
    );
    assert_eq!(
        fs::read_to_string(&copied_file2_path).unwrap(),
        "Goodbye, world!"
    );
}

#[test]
fn test_copy_folder() {
    use std::fs;
    use std::path::Path;

    // Create a temporary source directory with some files and subdirectories.
    let src_dir = tempfile::tempdir().unwrap();
    let src_path = Path::new(src_dir.path());
    let file1_path = src_path.join("file1.txt");
    let file2_path = src_path.join("file2.txt");
    fs::write(file1_path, "hello").unwrap();
    fs::write(file2_path, "world").unwrap();
    let sub_dir = src_path.join("sub.dir.test");
    fs::create_dir(&sub_dir).unwrap();
    let sub_file_path = sub_dir.join("sub_file.txt");
    fs::write(sub_file_path, "sub").unwrap();

    // Create a temporary destination directory.
    let dst_dir = tempfile::tempdir().unwrap();
    let dst_path = Path::new(dst_dir.path());

    // Call the function to copy the source directory to the destination directory.
    copy_folder(src_path.to_path_buf(), &dst_path.to_path_buf(), true);

    // Assert that the files and subdirectories were copied successfully.
    let dst_file1_path = dst_path.join("file1.txt");
    let dst_file2_path = dst_path.join("file2.txt");
    let dst_sub_dir_path = dst_path.join("sub.dir.test");
    let dst_sub_file_path = dst_sub_dir_path.join("sub_file.txt");
    assert!(dst_file1_path.exists());
    assert!(dst_file2_path.exists());
    assert!(dst_sub_dir_path.exists());
    assert!(dst_sub_file_path.exists());
    assert_eq!(fs::read_to_string(&dst_file1_path).unwrap(), "hello");
    assert_eq!(fs::read_to_string(&dst_file2_path).unwrap(), "world");
    assert_eq!(fs::read_to_string(&dst_sub_file_path).unwrap(), "sub");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::path::PathBuf;

    #[test]
    fn test_merge_values() {
        let mut target = serde_json::json!({
            "name_not_in_defaults": "John",
            "age": 30,
            "list_not_in_defaults" : ["test"],
            "address": {
                "street": "123 Main St",
                "city": "New York"
            },
            "list_differ_from_target": ["reading", "gaming"]
        });
        // default values
        let defaults = serde_json::json!({
            "age": 31,
            "key_not_in_target": "test",
            "address": {
                "city": "San Francisco",
                "state": "CA",
                "nested": {
                    "key1": "value1",
                    "key2": "value2"
                }
            },
            "list_differ_from_target": ["traveling", "cooking"],
            "list_not_in_target" : ["test"],
            "gender": "male"
        });

        merge_values(&mut target, &defaults);

        let expected_result = serde_json::json!({
            "name_not_in_defaults": "John",
            "age": 30,
            "address": {
                "nested": {
                    "key1": "value1",
                    "key2": "value2"
                },
                "street": "123 Main St",
                "city": "New York",
                "state": "CA"
            },
            "list_differ_from_target": ["reading", "gaming"],
            "list_not_in_target" : ["test"],
            "list_not_in_defaults" : ["test"],
            "gender": "male",
            "key_not_in_target": "test",
        });

        assert_eq!(target, expected_result);
    }

    #[test]
    fn test_simple_path() {
        assert_eq!(string_to_path("/usr/bin"), PathBuf::from("/usr/bin"));
    }

    // #[test]
    // fn test_tilde_expansion() {
    //     env::set_var("HOME", "/home/user");
    //     let home = env::var("HOME").expect("HOME environment variable should be set");
    //     assert_eq!(
    //         string_to_path("~/Documents"),
    //         PathBuf::from(format!("{}/Documents", home))
    //     );
    // }

    #[test]
    fn test_env_var_expansion() {
        env::set_var("TEST_VAR", "test_value");
        assert_eq!(
            string_to_path("/$TEST_VAR/path"),
            PathBuf::from("/test_value/path")
        );
        assert_eq!(
            string_to_path("/${TEST_VAR}/path"),
            PathBuf::from("/test_value/path")
        );
    }

    #[test]
    fn test_multiple_env_vars() {
        env::set_var("VAR1", "value1");
        env::set_var("VAR2", "value2");
        assert_eq!(
            string_to_path("/$VAR1/$VAR2"),
            PathBuf::from("/value1/value2")
        );
    }

    // #[test]
    // fn test_tilde_and_env_var() {
    //     env::set_var("HOME", "/home/user");
    //     let home = env::var("HOME").expect("HOME environment variable should be set");
    //     env::set_var("TEST_VAR", "test_value");
    //     assert_eq!(
    //         string_to_path("~/$TEST_VAR"),
    //         PathBuf::from(format!("{}/test_value", home))
    //     );
    // }

    #[test]
    fn test_non_existent_env_var() {
        assert_eq!(
            string_to_path("/$NONEXISTENT_VAR/path"),
            PathBuf::from("/$NONEXISTENT_VAR/path")
        );
    }

    #[test]
    fn test_tilde_in_middle() {
        assert_eq!(
            string_to_path("/path/~/other"),
            PathBuf::from("/path/~/other")
        );
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(string_to_path(""), PathBuf::from(""));
    }

    // #[test]
    // fn test_complex_path() {
    //     env::set_var("USER", "testuser");
    //     env::set_var("PROJECT", "myproject");
    //     env::set_var("HOME", "/home/user");
    //     let home = env::var("HOME").expect("HOME environment variable should be set");
    //     assert_eq!(
    //         string_to_path("~/Documents/${USER}/$PROJECT/src"),
    //         PathBuf::from(format!("{}/Documents/testuser/myproject/src", home))
    //     );
    // }
}
