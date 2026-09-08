//! Unit entity and related types
//!
//! A Unit represents an atomic infrastructure operation.

use crate::dimension::{DimType, Dimension, IncludeEntry};
use crate::error::{DomainError, DomainResult};
use crate::manifest::Manifest;
use crate::materialization::{MaterializationPlan, MaterializationStep};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// A unit of infrastructure operation
#[derive(Debug, Clone)]
pub struct Unit {
    /// Unit name
    pub name: String,
    /// Organization name
    pub org: String,
    /// Unit manifest
    pub manifest: Manifest,
    /// Resolved dimensions for this unit
    pub dimensions: Vec<DimensionRef>,
    /// Dimension data (type -> data JSON)
    pub dimension_data: HashMap<String, Value>,
    /// Non-JSON includes attached to this unit's dimensions (aggregated
    /// across all resolved dimensions), copied into the temp folder at
    /// materialization time.
    pub includes: Vec<IncludeEntry>,
    /// Source path (unit directory)
    pub unit_path: PathBuf,
    /// Temp folder for runner execution
    pub temp_folder: PathBuf,
    /// Extensions (additional dimension-like parameters)
    pub extensions: Vec<String>,
    /// Git SHA of the unit source
    pub git_sha: Option<String>,
    /// Full ancestor chain ("type:name") across every resolved dimension -
    /// used to project a producer's required dims onto this unit's own
    /// chain for `[inputs.<alias>]` resolution (see
    /// `crate::project_state_key`). Populated by `UnitService`, not by
    /// `Unit::new` - a freshly-constructed unit has an empty chain.
    pub dim_key_path: Vec<String>,
    /// Resolved `[inputs.<alias>]` outputs: alias -> producer's published
    /// output blob. A `BTreeMap` (not `HashMap`) so `materialize()`'s
    /// generated file list has a deterministic order. Populated by
    /// `UnitService` before materialization.
    pub resolved_inputs: BTreeMap<String, Value>,
}

/// Reference to a dimension value
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimensionRef {
    /// Dimension type
    pub dim_type: DimType,
    /// Dimension name
    pub name: String,
}

impl DimensionRef {
    /// Create a new dimension reference
    pub fn new(dim_type: impl Into<DimType>, name: impl Into<String>) -> Self {
        Self {
            dim_type: dim_type.into(),
            name: name.into(),
        }
    }

    /// Parse from string (format: "type:name").
    ///
    /// Validated through `cubtera_kernel::DimRef` (v3 seam, see
    /// docs/specs/2026-09-03-cubtera-v3-architecture.md ยง4): rejects `..`,
    /// absolute paths, embedded `/`/`\0`, and anything else that isn't a
    /// safe single filesystem path component in either half - this string
    /// ends up in `Unit::calculate_temp_folder`'s `path.join(...)` and in
    /// `UnitStateKey`'s on-disk layout, so a previously-accepted value like
    /// `"../../etc:passwd"` (only two non-empty halves were required
    /// before) is exactly the shape of the v2 path-escape bug this closes.
    pub fn parse(s: &str) -> Option<Self> {
        let dim_ref = cubtera_kernel::DimRef::parse(s).ok()?;
        Some(Self::new(
            dim_ref.dim_type.into_string(),
            dim_ref.name.into_string(),
        ))
    }

    /// Get the key representation (type:name)
    pub fn key(&self) -> String {
        format!("{}:{}", self.dim_type, self.name)
    }
}

impl From<&Dimension> for DimensionRef {
    fn from(dim: &Dimension) -> Self {
        Self::new(dim.dim_type.clone(), dim.name.clone())
    }
}

impl Unit {
    /// Create a new unit
    pub fn new(name: impl Into<String>, org: impl Into<String>, manifest: Manifest) -> Self {
        Self {
            name: name.into(),
            org: org.into(),
            manifest,
            dimensions: Vec::new(),
            dimension_data: HashMap::new(),
            includes: Vec::new(),
            unit_path: PathBuf::new(),
            temp_folder: PathBuf::new(),
            extensions: Vec::new(),
            git_sha: None,
            dim_key_path: Vec::new(),
            resolved_inputs: BTreeMap::new(),
        }
    }

    /// Add a dimension reference
    pub fn with_dimension(mut self, dim_ref: DimensionRef) -> Self {
        self.dimensions.push(dim_ref);
        self
    }

    /// Set unit source path
    pub fn with_unit_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.unit_path = path.into();
        self
    }

    /// Set temp folder path
    pub fn with_temp_folder(mut self, path: impl Into<PathBuf>) -> Self {
        self.temp_folder = path.into();
        self
    }

    /// Add extensions
    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.extensions = extensions;
        self
    }

    /// Add dimension data for a dimension type
    pub fn with_dimension_data(mut self, dim_type: impl Into<String>, data: Value) -> Self {
        self.dimension_data.insert(dim_type.into(), data);
        self
    }

    /// Set all dimension data at once
    pub fn with_all_dimension_data(mut self, data: HashMap<String, Value>) -> Self {
        self.dimension_data = data;
        self
    }

    /// Set aggregated dimension includes
    pub fn with_includes(mut self, includes: Vec<IncludeEntry>) -> Self {
        self.includes = includes;
        self
    }

    /// Set the full resolved dimension ancestor chain ("type:name"), used
    /// for `[inputs.<alias>]` projection (see `crate::project_state_key`).
    pub fn with_dim_key_path(mut self, dim_key_path: Vec<String>) -> Self {
        self.dim_key_path = dim_key_path;
        self
    }

    /// Set resolved `[inputs.<alias>]` outputs (alias -> producer's output
    /// blob), materialized as `cubtera_in_<alias>.json` by [`Self::materialize`].
    pub fn with_resolved_inputs(mut self, resolved_inputs: BTreeMap<String, Value>) -> Self {
        self.resolved_inputs = resolved_inputs;
        self
    }

    /// Calculate temp folder path based on org, unit name, dimensions and extensions
    pub fn calculate_temp_folder(&self, base_temp_path: &Path) -> PathBuf {
        let mut path = base_temp_path.join(&self.org).join(&self.name);

        // Add dimensions to path
        for dim in &self.dimensions {
            path = path.join(dim.key());
        }

        // Add extensions to path
        for ext in &self.extensions {
            path = path.join(ext);
        }

        path
    }

    /// Calculate the state path based on dimensions
    pub fn state_path(&self) -> String {
        let dim_parts: Vec<String> = self.dimensions.iter().map(|d| d.key()).collect();
        if dim_parts.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", dim_parts.join("/"), self.name)
        }
    }

    /// Calculate the dimension tree (for state path templates)
    pub fn dim_tree(&self) -> String {
        let mut parts: Vec<String> = self.dimensions.iter().map(|d| d.key()).collect();
        parts.extend(self.extensions.clone());
        parts.join("/")
    }

    /// Get dimension reference by type
    pub fn get_dimension(&self, dim_type: &str) -> Option<&DimensionRef> {
        self.dimensions
            .iter()
            .find(|d| d.dim_type.as_str() == dim_type)
    }

    /// Check if unit has all required dimensions from manifest
    pub fn has_all_required_dimensions(&self) -> bool {
        self.manifest.dimensions.iter().all(|required| {
            self.dimensions
                .iter()
                .any(|d| d.dim_type.as_str() == required)
        })
    }

    /// Get missing required dimensions
    pub fn missing_dimensions(&self) -> Vec<&str> {
        self.manifest
            .dimensions
            .iter()
            .filter(|required| {
                !self
                    .dimensions
                    .iter()
                    .any(|d| d.dim_type.as_str() == required.as_str())
            })
            .map(|s| s.as_str())
            .collect()
    }

    /// Check if temp folder exists
    pub fn temp_folder_exists(&self) -> bool {
        self.temp_folder.exists()
    }

    /// Build this unit's [`MaterializationPlan`]: modules symlink, the
    /// generic (org-less) unit's files if `manifest.overwrite` is set, this
    /// unit's own files, per-dimension `cubtera_dim_{type}.json` (including
    /// null placeholders for declared-but-unprovided `optDims`, matching
    /// v1), dimension includes, `cubtera_ext.json` for extensions, and
    /// `spec.files`. Pure - no I/O, fully testable and `--dry-run`-printable.
    ///
    /// Fallible since v3's kernel seam (docs/specs/2026-09-03-cubtera-v3-architecture.md
    /// ยง4): every `spec.files` destination is validated through
    /// `cubtera_kernel::SafeSegment::split_relative_path` and joined from
    /// the validated segments, not the raw manifest string, rejecting a
    /// `dst = "../../../etc/cron.d/x"` at plan-build time (before any
    /// `Workspace` ever touches disk) instead of silently writing outside
    /// `temp_folder`.
    pub fn materialize(
        &self,
        modules_path: &Path,
        generic_unit_path: Option<&Path>,
    ) -> DomainResult<MaterializationPlan> {
        let mut plan = MaterializationPlan::new(self.temp_folder.clone());

        plan.push(MaterializationStep::Symlink {
            target: modules_path.to_path_buf(),
            link: self.temp_folder.join("modules"),
        });

        if self.manifest.overwrite {
            if let Some(generic) = generic_unit_path {
                plan.push(MaterializationStep::CopyDir {
                    src: generic.to_path_buf(),
                    dst: self.temp_folder.clone(),
                });
            }
        }

        plan.push(MaterializationStep::CopyDir {
            src: self.unit_path.clone(),
            dst: self.temp_folder.clone(),
        });

        for dim_ref in &self.dimensions {
            let dim_type = dim_ref.dim_type.as_str();
            plan.push(MaterializationStep::WriteFile {
                path: self
                    .temp_folder
                    .join(format!("cubtera_dim_{dim_type}.json")),
                content: dim_vars_json(dim_type, &dim_ref.name, self.dimension_data.get(dim_type)),
            });
        }

        // v1 parity: every declared-but-unprovided optional dimension still
        // gets a placeholder file with a null name, so unit code can
        // unconditionally reference `dim_{type}_name` without an `optDims`
        // it happens not to need this time producing a missing-var error.
        if let Some(opt_dims) = &self.manifest.opt_dims {
            for dim_type in opt_dims {
                if self
                    .dimensions
                    .iter()
                    .any(|d| d.dim_type.as_str() == dim_type.as_str())
                {
                    continue;
                }
                plan.push(MaterializationStep::WriteFile {
                    path: self
                        .temp_folder
                        .join(format!("cubtera_dim_{dim_type}.json")),
                    content: dim_vars_json_null(dim_type),
                });
            }
        }

        for include in &self.includes {
            let dst = self.temp_folder.join(&include.name);
            if include.is_dir {
                plan.push(MaterializationStep::CopyDir {
                    src: include.source.clone(),
                    dst,
                });
            } else {
                plan.push(MaterializationStep::CopyFile {
                    src: include.source.clone(),
                    dst,
                    required: true,
                });
            }
        }

        if !self.extensions.is_empty() {
            plan.push(MaterializationStep::WriteFile {
                path: self.temp_folder.join("cubtera_ext.json"),
                content: extensions_json(&self.extensions),
            });
        }

        if let Some(files) = self
            .manifest
            .spec
            .as_ref()
            .and_then(|spec| spec.files.as_ref())
        {
            for (src, dst) in files.required.iter().flatten() {
                plan.push(MaterializationStep::CopyFile {
                    src: PathBuf::from(src),
                    dst: safe_join(&self.temp_folder, dst)?,
                    required: true,
                });
            }
            for (src, dst) in files.optional.iter().flatten() {
                plan.push(MaterializationStep::CopyFile {
                    src: PathBuf::from(src),
                    dst: safe_join(&self.temp_folder, dst)?,
                    required: false,
                });
            }
        }

        // `resolved_inputs` is a `BTreeMap`, so this iterates (and the
        // aggregate file below serializes) in a deterministic alias order.
        for (alias, value) in &self.resolved_inputs {
            plan.push(MaterializationStep::WriteFile {
                path: self.temp_folder.join(format!("cubtera_in_{alias}.json")),
                content: serde_json::to_string_pretty(&json!({
                    format!("in_{alias}"): value
                }))
                .unwrap_or_default(),
            });
        }
        if !self.resolved_inputs.is_empty() {
            plan.push(MaterializationStep::WriteFile {
                path: self.temp_folder.join("cubtera_inputs.json"),
                content: serde_json::to_string_pretty(&Value::Object(
                    self.resolved_inputs
                        .iter()
                        .map(|(alias, value)| (alias.clone(), value.clone()))
                        .collect(),
                ))
                .unwrap_or_default(),
            });
        }

        Ok(plan)
    }
}

/// Join `dst` (a `spec.files` destination from the manifest) onto
/// `temp_folder`, rejecting anything that isn't a plain relative path -
/// no `..`, no absolute path, no embedded NUL. See
/// [`Unit::materialize`]'s doc comment for why this exists.
fn safe_join(temp_folder: &Path, dst: &str) -> DomainResult<PathBuf> {
    let segments = cubtera_kernel::SafeSegment::split_relative_path(dst).map_err(|e| {
        DomainError::InvalidManifest {
            reason: format!("invalid spec.files destination {dst:?}: {e}"),
        }
    })?;
    let mut path = temp_folder.to_path_buf();
    for segment in segments {
        path.push(segment.as_str());
    }
    Ok(path)
}

/// Build the `cubtera_dim_{type}.json` content for a resolved dimension:
/// `dim_{type}_name` and `dim_{type}_{field}` for every field/section in its
/// data (`dim_{type}_meta` is just the "meta" section, like every other
/// section - it is NOT the full per-dimension data blob; see v1's
/// `get_json_dim_vars`, which this mirrors).
fn dim_vars_json(dim_type: &str, dim_name: &str, data: Option<&Value>) -> String {
    let mut vars = serde_json::Map::new();
    vars.insert(
        format!("dim_{dim_type}_name"),
        Value::String(dim_name.to_string()),
    );

    if let Some(data) = data {
        if let Some(obj) = data.as_object() {
            for (key, value) in obj {
                vars.insert(format!("dim_{dim_type}_{key}"), value.clone());
            }
        }
    }

    serde_json::to_string_pretty(&Value::Object(vars)).unwrap_or_default()
}

/// Placeholder `cubtera_dim_{type}.json` content for a declared-but-unprovided
/// optional dimension type.
fn dim_vars_json_null(dim_type: &str) -> String {
    let mut vars = serde_json::Map::new();
    vars.insert(format!("dim_{dim_type}_name"), Value::Null);
    serde_json::to_string_pretty(&Value::Object(vars)).unwrap_or_default()
}

/// Build `cubtera_ext.json` content: `ext_{type}_name` for each `type:name` extension.
fn extensions_json(extensions: &[String]) -> String {
    let mut vars = serde_json::Map::new();
    for ext in extensions {
        if let Some((ext_type, ext_name)) = ext.split_once(':') {
            vars.insert(
                format!("ext_{ext_type}_name"),
                Value::String(ext_name.to_string()),
            );
        }
    }
    serde_json::to_string_pretty(&Value::Object(vars)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn create_test_manifest() -> Manifest {
        Manifest::new(
            vec!["dome".to_string(), "env".to_string(), "dc".to_string()],
            "tf",
        )
    }

    #[test]
    fn test_dimension_ref_parse() {
        let dim_ref = DimensionRef::parse("env:prod").unwrap();
        assert_eq!(dim_ref.dim_type.as_str(), "env");
        assert_eq!(dim_ref.name, "prod");
    }

    #[test]
    fn test_dimension_ref_key() {
        let dim_ref = DimensionRef::new("env", "prod");
        assert_eq!(dim_ref.key(), "env:prod");
    }

    #[test]
    fn test_unit_state_path() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"))
            .with_dimension(DimensionRef::new("env", "prod"))
            .with_dimension(DimensionRef::new("dc", "us-east-1"));

        assert_eq!(unit.state_path(), "dome:prod/env:prod/dc:us-east-1/network");
    }

    #[test]
    fn test_unit_dim_tree() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"))
            .with_dimension(DimensionRef::new("env", "prod"));

        assert_eq!(unit.dim_tree(), "dome:prod/env:prod");
    }

    #[test]
    fn test_unit_dim_tree_with_extensions() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"))
            .with_extensions(vec!["index:0".to_string(), "replica:1".to_string()]);

        assert_eq!(unit.dim_tree(), "dome:prod/index:0/replica:1");
    }

    #[test]
    fn test_unit_missing_dimensions() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"));

        let missing = unit.missing_dimensions();
        assert!(missing.contains(&"env"));
        assert!(missing.contains(&"dc"));
        assert!(!missing.contains(&"dome"));
    }

    #[test]
    fn test_unit_has_all_dimensions() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"))
            .with_dimension(DimensionRef::new("env", "prod"))
            .with_dimension(DimensionRef::new("dc", "us-east-1"));

        assert!(unit.has_all_required_dimensions());
    }

    #[test]
    fn test_calculate_temp_folder() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"))
            .with_dimension(DimensionRef::new("env", "stg"))
            .with_extensions(vec!["index:0".to_string()]);

        let temp_folder = unit.calculate_temp_folder(&PathBuf::from("/tmp/cubtera"));
        assert_eq!(
            temp_folder,
            PathBuf::from("/tmp/cubtera/cubtera/network/dome:prod/env:stg/index:0")
        );
    }

    #[test]
    fn test_temp_folder_exists_false() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_temp_folder("/nonexistent/path/that/does/not/exist");
        assert!(!unit.temp_folder_exists());
    }

    fn base_unit() -> Unit {
        Unit::new("network", "cubtera", create_test_manifest())
            .with_unit_path("/units/network")
            .with_temp_folder("/tmp/cubtera/network")
    }

    #[test]
    fn materialize_symlinks_modules_and_copies_unit_files() {
        let unit = base_unit();
        let plan = unit.materialize(Path::new("/modules"), None).unwrap();

        assert_eq!(plan.temp_folder, PathBuf::from("/tmp/cubtera/network"));
        assert!(plan.steps.iter().any(|s| matches!(
            s,
            MaterializationStep::Symlink { target, link }
                if target == Path::new("/modules") && link == Path::new("/tmp/cubtera/network/modules")
        )));
        assert!(plan.steps.iter().any(|s| matches!(
            s,
            MaterializationStep::CopyDir { src, dst }
                if src == Path::new("/units/network") && dst == Path::new("/tmp/cubtera/network")
        )));
    }

    #[test]
    fn materialize_copies_generic_unit_when_overwrite_is_set() {
        let mut manifest = create_test_manifest();
        manifest.overwrite = true;
        let unit = Unit::new("network", "cubtera", manifest)
            .with_unit_path("/units/network")
            .with_temp_folder("/tmp/cubtera/network");

        let plan = unit
            .materialize(Path::new("/modules"), Some(Path::new("/units/_generic")))
            .unwrap();

        let generic_copy_index = plan.steps.iter().position(|s| {
            matches!(
                s,
                MaterializationStep::CopyDir { src, .. } if src == Path::new("/units/_generic")
            )
        });
        let own_copy_index = plan.steps.iter().position(|s| {
            matches!(
                s,
                MaterializationStep::CopyDir { src, .. } if src == Path::new("/units/network")
            )
        });
        // The unit's own files must be copied after the generic ones, so they win on conflicts.
        assert!(generic_copy_index.unwrap() < own_copy_index.unwrap());
    }

    #[test]
    fn materialize_writes_flattened_dim_vars_json_per_resolved_dimension() {
        // `dimension_data` mirrors `Dimension::to_json()`: sections keyed by
        // name (`meta` plus any other section), not a flat blob.
        let unit = base_unit()
            .with_dimension(DimensionRef::new("env", "prod"))
            .with_dimension_data(
                "env",
                json!({
                    "name": "prod",
                    "meta": {"region": "us-east-1"},
                    "manifest": {"owner": "platform"}
                }),
            );

        let plan = unit.materialize(Path::new("/modules"), None).unwrap();

        let content = plan
            .steps
            .iter()
            .find_map(|s| match s {
                MaterializationStep::WriteFile { path, content }
                    if path == Path::new("/tmp/cubtera/network/cubtera_dim_env.json") =>
                {
                    Some(content)
                }
                _ => None,
            })
            .expect("cubtera_dim_env.json step");
        let parsed: Value = serde_json::from_str(content).unwrap();
        assert_eq!(parsed["dim_env_name"], "prod");
        // `dim_env_meta` must be just the "meta" section, not the whole
        // per-dimension data blob (that was the double-wrapping regression).
        assert_eq!(parsed["dim_env_meta"]["region"], "us-east-1");
        assert!(parsed["dim_env_meta"].get("manifest").is_none());
        assert_eq!(parsed["dim_env_manifest"]["owner"], "platform");
    }

    #[test]
    fn materialize_writes_null_placeholder_for_unprovided_opt_dim() {
        let mut manifest = create_test_manifest();
        manifest.opt_dims = Some(vec!["region".to_string()]);
        let unit = Unit::new("network", "cubtera", manifest)
            .with_unit_path("/units/network")
            .with_temp_folder("/tmp/cubtera/network");

        let plan = unit.materialize(Path::new("/modules"), None).unwrap();

        let content = plan
            .steps
            .iter()
            .find_map(|s| match s {
                MaterializationStep::WriteFile { path, content }
                    if path == Path::new("/tmp/cubtera/network/cubtera_dim_region.json") =>
                {
                    Some(content)
                }
                _ => None,
            })
            .expect("cubtera_dim_region.json placeholder step");
        let parsed: Value = serde_json::from_str(content).unwrap();
        assert!(parsed["dim_region_name"].is_null());
    }

    #[test]
    fn materialize_skips_opt_dim_placeholder_when_dimension_is_provided() {
        let mut manifest = create_test_manifest();
        manifest.opt_dims = Some(vec!["dome".to_string()]);
        let unit = Unit::new("network", "cubtera", manifest)
            .with_unit_path("/units/network")
            .with_temp_folder("/tmp/cubtera/network")
            .with_dimension(DimensionRef::new("dome", "prod"));

        let plan = unit.materialize(Path::new("/modules"), None).unwrap();

        let dome_writes = plan
            .steps
            .iter()
            .filter(|s| {
                matches!(
                    s,
                    MaterializationStep::WriteFile { path, .. }
                        if path == Path::new("/tmp/cubtera/network/cubtera_dim_dome.json")
                )
            })
            .count();
        assert_eq!(
            dome_writes, 1,
            "should not double-write a provided dimension"
        );
    }

    #[test]
    fn materialize_copies_dimension_includes() {
        let unit = base_unit().with_includes(vec![
            IncludeEntry {
                name: "keys".to_string(),
                source: PathBuf::from("/inventory/env/prod:keys"),
                is_dir: true,
            },
            IncludeEntry {
                name: "cert.pem".to_string(),
                source: PathBuf::from("/inventory/env/prod:cert.pem"),
                is_dir: false,
            },
        ]);

        let plan = unit.materialize(Path::new("/modules"), None).unwrap();

        assert!(plan.steps.iter().any(|s| matches!(
            s,
            MaterializationStep::CopyDir { src, dst }
                if src == Path::new("/inventory/env/prod:keys")
                    && dst == Path::new("/tmp/cubtera/network/keys")
        )));
        assert!(plan.steps.iter().any(|s| matches!(
            s,
            MaterializationStep::CopyFile { src, dst, required: true }
                if src == Path::new("/inventory/env/prod:cert.pem")
                    && dst == Path::new("/tmp/cubtera/network/cert.pem")
        )));
    }

    #[test]
    fn materialize_writes_ext_json_only_when_extensions_present() {
        let without_ext = base_unit();
        let plan = without_ext
            .materialize(Path::new("/modules"), None)
            .unwrap();
        assert!(!plan.steps.iter().any(|s| matches!(
            s,
            MaterializationStep::WriteFile { path, .. }
                if path == Path::new("/tmp/cubtera/network/cubtera_ext.json")
        )));

        let with_ext = base_unit().with_extensions(vec!["index:0".to_string()]);
        let plan = with_ext.materialize(Path::new("/modules"), None).unwrap();
        let content = plan
            .steps
            .iter()
            .find_map(|s| match s {
                MaterializationStep::WriteFile { path, content }
                    if path == Path::new("/tmp/cubtera/network/cubtera_ext.json") =>
                {
                    Some(content)
                }
                _ => None,
            })
            .expect("cubtera_ext.json step");
        let parsed: Value = serde_json::from_str(content).unwrap();
        assert_eq!(parsed["ext_index_name"], "0");
    }

    #[test]
    fn materialize_includes_manifest_spec_files() {
        let mut manifest = create_test_manifest();
        manifest.spec = Some(crate::manifest::Spec {
            env_vars: None,
            files: Some(crate::manifest::Files {
                required: Some(HashMap::from([(
                    "/etc/creds.json".to_string(),
                    "creds.json".to_string(),
                )])),
                optional: Some(HashMap::from([(
                    "~/.aws/config".to_string(),
                    "aws_config".to_string(),
                )])),
            }),
        });
        let unit = Unit::new("network", "cubtera", manifest)
            .with_unit_path("/units/network")
            .with_temp_folder("/tmp/cubtera/network");

        let plan = unit.materialize(Path::new("/modules"), None).unwrap();

        assert!(plan.steps.iter().any(|s| matches!(
            s,
            MaterializationStep::CopyFile { src, dst, required: true }
                if src == Path::new("/etc/creds.json") && dst == Path::new("/tmp/cubtera/network/creds.json")
        )));
        assert!(plan.steps.iter().any(|s| matches!(
            s,
            MaterializationStep::CopyFile { src, dst, required: false }
                if src == Path::new("~/.aws/config") && dst == Path::new("/tmp/cubtera/network/aws_config")
        )));
    }

    #[test]
    fn materialize_skips_input_files_when_no_resolved_inputs() {
        let plan = base_unit()
            .materialize(Path::new("/modules"), None)
            .unwrap();
        assert!(!plan.steps.iter().any(|s| matches!(
            s,
            MaterializationStep::WriteFile { path, .. }
                if path == Path::new("/tmp/cubtera/network/cubtera_inputs.json")
        )));
    }

    #[test]
    fn materialize_writes_per_alias_and_aggregate_input_files() {
        let mut resolved_inputs = BTreeMap::new();
        resolved_inputs.insert("network".to_string(), json!({"vpc_id": "vpc-123"}));
        resolved_inputs.insert("certs".to_string(), json!({"cert_arn": "arn:aws:acm:..."}));
        let unit = base_unit().with_resolved_inputs(resolved_inputs);

        let plan = unit.materialize(Path::new("/modules"), None).unwrap();

        let network_content = plan
            .steps
            .iter()
            .find_map(|s| match s {
                MaterializationStep::WriteFile { path, content }
                    if path == Path::new("/tmp/cubtera/network/cubtera_in_network.json") =>
                {
                    Some(content)
                }
                _ => None,
            })
            .expect("cubtera_in_network.json step");
        let parsed: Value = serde_json::from_str(network_content).unwrap();
        assert_eq!(parsed["in_network"]["vpc_id"], "vpc-123");

        let aggregate_content = plan
            .steps
            .iter()
            .find_map(|s| match s {
                MaterializationStep::WriteFile { path, content }
                    if path == Path::new("/tmp/cubtera/network/cubtera_inputs.json") =>
                {
                    Some(content)
                }
                _ => None,
            })
            .expect("cubtera_inputs.json step");
        let parsed: Value = serde_json::from_str(aggregate_content).unwrap();
        assert_eq!(parsed["network"]["vpc_id"], "vpc-123");
        assert_eq!(parsed["certs"]["cert_arn"], "arn:aws:acm:...");
    }
}
