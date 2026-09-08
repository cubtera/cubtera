use std::fmt;

/// Errors raised while building/validating model-layer values: the
/// dimension graph, gap-fill, and unit packages. Purely data-level - no
/// I/O has happened by the time one of these can occur.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelError {
    Graph(Vec<crate::dim_graph::GraphError>),
    SchemaViolation {
        dim_type: String,
        errors: Vec<String>,
    },
    /// A `Binding.selector` expression (section 5.4) failed to parse -
    /// malformed syntax, not a runtime evaluation failure (evaluation
    /// against a missing field is `false`, never an error - see
    /// `Selector::evaluate`'s doc comment).
    Selector(String),
    /// `project_state_key` (section 5.5) couldn't unambiguously project a
    /// consumer's resolved dimension chain onto a producer's required
    /// dimension types - never a guess, always a hard error.
    InputResolution(String),
    /// `Manifest::from_toml` failed to parse `manifest.toml` (P7's
    /// v2->v3 manifest port, `manifest.rs`).
    InvalidManifest(String),
    /// A `spec.files` destination in a manifest isn't a safe relative path
    /// (`Unit::materialize`, `unit.rs`) - rejected before any `Workspace`
    /// ever touches disk, per the same kernel-seam rationale as everywhere
    /// else path segments come from user-authored config.
    InvalidPath(String),
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Graph(errors) => {
                write!(f, "invalid dimension graph: ")?;
                for (i, e) in errors.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "{e}")?;
                }
                Ok(())
            }
            Self::SchemaViolation { dim_type, errors } => {
                write!(
                    f,
                    "dimension type {dim_type:?} failed schema validation: {}",
                    errors.join("; ")
                )
            }
            Self::Selector(msg) => write!(f, "invalid selector: {msg}"),
            Self::InputResolution(msg) => write!(f, "{msg}"),
            Self::InvalidManifest(msg) => write!(f, "invalid manifest: {msg}"),
            Self::InvalidPath(msg) => write!(f, "invalid path: {msg}"),
        }
    }
}

impl std::error::Error for ModelError {}
