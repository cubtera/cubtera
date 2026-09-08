//! Materialization plan (P7 v2->v3 port of `cubtera_domain::materialization`).
//!
//! The model only *describes* the work as a [`MaterializationPlan`]: an
//! ordered list of [`MaterializationStep`]s with all content already
//! computed. An adapter (`cubtera-exec`'s materialize module) executes it;
//! `--dry-run` just prints it. Directory copies stay directory-granular
//! (`CopyDir`) rather than being expanded file-by-file, since enumerating a
//! directory's contents is itself I/O the model has no business doing.

use std::fmt;
use std::path::PathBuf;

/// A single filesystem action needed to materialize a unit's temp working
/// directory. Every field is fully resolved data - no further business
/// logic runs at apply time, only I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterializationStep {
    /// Create `link` as a symlink to `target`, unless `link` already exists.
    Symlink { target: PathBuf, link: PathBuf },
    /// Recursively copy the contents of `src` into `dst` (creating `dst` if
    /// needed). Missing `src` is a hard error - the caller only plans copies
    /// of directories it expects to exist.
    CopyDir { src: PathBuf, dst: PathBuf },
    /// Copy a single file from `src` to `dst`. If `src` is missing:
    /// `required = true` is a hard error, `required = false` only warns.
    CopyFile {
        src: PathBuf,
        dst: PathBuf,
        required: bool,
    },
    /// Write `content` verbatim to `path`, overwriting any existing file.
    WriteFile { path: PathBuf, content: String },
}

impl fmt::Display for MaterializationStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Symlink { target, link } => {
                write!(f, "symlink   {} -> {}", link.display(), target.display())
            }
            Self::CopyDir { src, dst } => {
                write!(f, "copy dir  {} -> {}", src.display(), dst.display())
            }
            Self::CopyFile { src, dst, required } => write!(
                f,
                "copy file {} -> {} ({})",
                src.display(),
                dst.display(),
                if *required { "required" } else { "optional" }
            ),
            Self::WriteFile { path, content } => {
                write!(f, "write     {} ({} bytes)", path.display(), content.len())
            }
        }
    }
}

/// An ordered, fully-resolved plan for materializing a unit's temp working
/// directory. Pure data: building one does no I/O, and applying it twice is
/// idempotent by construction (symlinks/copies/writes are all "ensure this
/// exists/matches", not "append").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaterializationPlan {
    /// The unit's temp working directory (created first, before any step runs)
    pub temp_folder: PathBuf,
    /// Steps to execute, in order
    pub steps: Vec<MaterializationStep>,
}

impl MaterializationPlan {
    /// Start a new, empty plan rooted at `temp_folder`
    pub fn new(temp_folder: impl Into<PathBuf>) -> Self {
        Self {
            temp_folder: temp_folder.into(),
            steps: Vec::new(),
        }
    }

    /// Append a step
    pub fn push(&mut self, step: MaterializationStep) -> &mut Self {
        self.steps.push(step);
        self
    }
}

impl fmt::Display for MaterializationPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Materialization plan for {}:",
            self.temp_folder.display()
        )?;
        if self.steps.is_empty() {
            writeln!(f, "  (nothing to do)")?;
        }
        for step in &self.steps {
            writeln!(f, "  {step}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_display_lists_all_steps_in_order() {
        let mut plan = MaterializationPlan::new("/tmp/unit");
        plan.push(MaterializationStep::Symlink {
            target: "/modules".into(),
            link: "/tmp/unit/modules".into(),
        });
        plan.push(MaterializationStep::WriteFile {
            path: "/tmp/unit/cubtera_ext.json".into(),
            content: "{}".to_string(),
        });

        let rendered = plan.to_string();
        assert!(rendered.contains("Materialization plan for /tmp/unit"));
        assert!(rendered.contains("symlink"));
        assert!(rendered.contains("write"));
    }

    #[test]
    fn empty_plan_says_nothing_to_do() {
        let plan = MaterializationPlan::new("/tmp/unit");
        assert!(plan.to_string().contains("nothing to do"));
    }
}
