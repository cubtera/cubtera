use super::Dim;
use crate::prelude::*;
use std::path::{Path, PathBuf};

impl Dim {
    /// Save all non-json files from dimension folder to a path (usually temp folder for a unit)
    pub fn save_dim_includes(&self, path: PathBuf) -> Result<(), std::io::Error> {
        let entry_filter =
            |entry: &Path| entry.is_file() && entry.extension().unwrap_or_default() != "json";

        self.process_dim_entries(path, entry_filter)
    }

    /// Save dimension folders from inventory to a path (usually temp folder for a unit)
    pub fn save_dim_folders(&self, path: PathBuf) -> Result<(), std::io::Error> {
        let entry_filter = |entry: &Path| entry.is_dir();

        self.process_dim_entries(path, entry_filter)
    }

    /// Get filtered entries from dimension directory based on prefix and filter function
    fn get_filtered_entries<F>(
        &self,
        prefix: &str,
        entry_filter: F,
    ) -> std::io::Result<Vec<PathBuf>>
    where
        F: Fn(&Path) -> bool,
    {
        Ok(std::fs::read_dir(&self.dim_path)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(move |entry| entry_filter(entry))
            .filter_map(move |entry| {
                entry
                    .clone()
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .filter(|name| name.starts_with(prefix))
                    .map(|_| entry)
            })
            .collect())
    }

    /// Process dimension entries (files or folders) and copy them to target path
    fn process_dim_entries<F>(&self, path: PathBuf, entry_filter: F) -> Result<(), std::io::Error>
    where
        F: Fn(&Path) -> bool,
    {
        let default_prefix = format!(".default{}", &GLOBAL_CFG.file_name_separator);
        let dim_prefix = format!("{}{}", &self.dim_name, &GLOBAL_CFG.file_name_separator);

        let default_entries = self.get_filtered_entries(&default_prefix, &entry_filter)?;
        let dim_entries = self.get_filtered_entries(&dim_prefix, &entry_filter)?;

        default_entries
            .into_iter()
            .chain(dim_entries)
            .try_for_each(|entry| {
                let entry_name = entry
                    .file_name()
                    .and_then(|f| f.to_str())
                    .and_then(|s| s.split(&GLOBAL_CFG.file_name_separator).last())
                    .filter(|s| !s.is_empty());

                match entry_name {
                    Some(name) => self.copy_entry(&entry, &path.join(name), entry.is_dir()),
                    None => Ok(()),
                }
            })
    }

    /// Copy a single entry (file or directory) from source to destination
    fn copy_entry(&self, src: &Path, dest: &Path, is_dir: bool) -> std::io::Result<()> {
        if is_dir {
            copy_folder(src.to_path_buf(), &dest.to_path_buf(), true);
            Ok(())
        } else {
            std::fs::copy(src, dest).map(|_| ())
        }
    }
} 