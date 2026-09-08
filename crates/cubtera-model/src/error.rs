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
        }
    }
}

impl std::error::Error for ModelError {}
