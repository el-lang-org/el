//! Project discovery and compiler pipeline orchestration.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

pub const MANIFEST_FILE_NAME: &str = "el.toml";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectRoot {
    manifest_path: PathBuf,
}

impl ProjectRoot {
    #[must_use]
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    #[must_use]
    pub fn directory(&self) -> &Path {
        self.manifest_path
            .parent()
            .expect("a discovered manifest always has a parent")
    }
}

/// Finds the nearest `el.toml`, starting at `start` and walking to the root.
pub fn discover_project(start: &Path) -> Result<ProjectRoot, ProjectError> {
    for directory in start.ancestors() {
        let candidate = directory.join(MANIFEST_FILE_NAME);
        if candidate.is_file() {
            return Ok(ProjectRoot {
                manifest_path: candidate,
            });
        }
    }

    Err(ProjectError::ManifestNotFound {
        start: start.to_path_buf(),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectCommand {
    Check { locked: bool },
    Build { release: bool, locked: bool },
    EmitLlvmIr,
}

impl ProjectCommand {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Check { .. } => "check",
            Self::Build { .. } => "build",
            Self::EmitLlvmIr => "emit llvm-ir",
        }
    }
}

/// Establishes the project boundary for a known command.
///
/// Language compilation is deliberately unavailable until its first vertical
/// slice; reaching this error proves that invocation and project discovery
/// completed without crossing an unimplemented compiler stage.
pub fn run_project_command(command: ProjectCommand, start: &Path) -> Result<(), ProjectError> {
    let project = discover_project(start)?;
    Err(ProjectError::CommandNotImplemented {
        command: command.name(),
        manifest: project.manifest_path,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectError {
    ManifestNotFound {
        start: PathBuf,
    },
    CommandNotImplemented {
        command: &'static str,
        manifest: PathBuf,
    },
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ManifestNotFound { start } => write!(
                formatter,
                "could not find {MANIFEST_FILE_NAME} in `{}` or any parent directory",
                start.display()
            ),
            Self::CommandNotImplemented { command, manifest } => write!(
                formatter,
                "`el {command}` is not implemented in Milestone 0 (project `{}`)",
                manifest.display()
            ),
        }
    }
}

impl Error for ProjectError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("el-driver-test-{}-{sequence}", std::process::id()));
            fs::create_dir(&path).expect("create temporary test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove temporary test directory");
        }
    }

    #[test]
    fn discovers_manifest_at_project_root() {
        let temp = TempDir::new();
        fs::write(temp.path().join(MANIFEST_FILE_NAME), "").expect("write manifest");

        let project = discover_project(temp.path()).expect("discover root manifest");

        assert_eq!(project.directory(), temp.path());
    }

    #[test]
    fn discovers_nearest_manifest_from_descendant() {
        let temp = TempDir::new();
        let child = temp.path().join("src/nested");
        fs::create_dir_all(&child).expect("create descendant");
        fs::write(temp.path().join(MANIFEST_FILE_NAME), "").expect("write root manifest");
        fs::write(temp.path().join("src").join(MANIFEST_FILE_NAME), "")
            .expect("write nearer manifest");

        let project = discover_project(&child).expect("discover nearest manifest");

        assert_eq!(project.directory(), temp.path().join("src"));
    }

    #[test]
    fn reports_missing_manifest() {
        let temp = TempDir::new();

        assert_eq!(
            discover_project(temp.path()),
            Err(ProjectError::ManifestNotFound {
                start: temp.path().to_path_buf()
            })
        );
    }

    #[test]
    fn known_command_fails_as_an_ordinary_project_error() {
        let temp = TempDir::new();
        fs::write(temp.path().join(MANIFEST_FILE_NAME), "").expect("write manifest");

        let error = run_project_command(ProjectCommand::Check { locked: false }, temp.path())
            .expect_err("Milestone 0 command remains unavailable");

        assert!(matches!(error, ProjectError::CommandNotImplemented { .. }));
    }
}
