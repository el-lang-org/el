use std::env;
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io;
use std::path::Path;
use std::process::{Command, ExitStatus};

const DEFAULT_HOST_COMPILER: &str = "cc";

/// Invokes the platform C compiler driver to link one host object file.
///
/// `CC` may select a different driver executable. It is interpreted as one
/// executable path, not as a shell command, so linker invocation never depends
/// on shell parsing.
pub fn link_host_executable(object: &Path, executable: &Path) -> Result<(), LinkerError> {
    link_host_objects(&[object], executable)
}

/// Invokes the platform C compiler driver with all objects needed by an EL
/// executable, including private runtime objects.
pub fn link_host_objects(objects: &[&Path], executable: &Path) -> Result<(), LinkerError> {
    let driver = env::var_os("CC").unwrap_or_else(|| OsString::from(DEFAULT_HOST_COMPILER));
    invoke_linker(&driver, objects, executable)
}

/// Links generated code with the matching private runtime and vendored
/// collector. Runtime archives follow generated objects so static symbol
/// resolution is deterministic.
#[cfg(feature = "managed-runtime")]
pub fn link_host_managed_executable(
    objects: &[&Path],
    executable: &Path,
) -> Result<(), LinkerError> {
    let archives = el_runtime::native_runtime_archives();
    let driver = env::var_os("CC").unwrap_or_else(|| OsString::from(DEFAULT_HOST_COMPILER));
    let output = Command::new(&driver)
        .args(objects)
        .arg(archives.wrapper())
        .arg(archives.collector())
        .arg("-lpthread")
        .arg("-o")
        .arg(executable)
        .output()
        .map_err(|source| LinkerError::Launch {
            driver: driver.clone(),
            source: IoError::from(source),
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(LinkerError::Failed {
            driver,
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

fn invoke_linker(driver: &OsStr, objects: &[&Path], executable: &Path) -> Result<(), LinkerError> {
    let output = Command::new(driver)
        .args(objects)
        .arg("-o")
        .arg(executable)
        .output()
        .map_err(|source| LinkerError::Launch {
            driver: driver.to_os_string(),
            source: IoError::from(source),
        })?;

    if output.status.success() {
        return Ok(());
    }

    Err(LinkerError::Failed {
        driver: driver.to_os_string(),
        status: output.status,
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// A host compiler driver could not be started or reported a link failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LinkerError {
    Launch {
        driver: OsString,
        source: IoError,
    },
    Failed {
        driver: OsString,
        status: ExitStatus,
        stdout: String,
        stderr: String,
    },
}

impl fmt::Display for LinkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Launch { driver, source } => write!(
                formatter,
                "could not start host compiler driver `{}`: {source}",
                driver.to_string_lossy()
            ),
            Self::Failed {
                driver,
                status,
                stdout,
                stderr,
            } => {
                write!(
                    formatter,
                    "host compiler driver `{}` failed with {status}",
                    driver.to_string_lossy()
                )?;
                if !stderr.is_empty() {
                    write!(formatter, ": {}", stderr.trim_end())?;
                } else if !stdout.is_empty() {
                    write!(formatter, ": {}", stdout.trim_end())?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for LinkerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Launch { source, .. } => Some(source),
            Self::Failed { .. } => None,
        }
    }
}

/// Cloneable, comparable details from an I/O error, suitable for structured
/// diagnostics without retaining platform-specific error internals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IoError {
    kind: io::ErrorKind,
    message: String,
}

impl IoError {
    #[must_use]
    pub const fn kind(&self) -> io::ErrorKind {
        self.kind
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<io::Error> for IoError {
    fn from(error: io::Error) -> Self {
        Self {
            kind: error.kind(),
            message: error.to_string(),
        }
    }
}

impl fmt::Display for IoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for IoError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
            let path = env::temp_dir().join(format!(
                "el-codegen-linker-test-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("create temporary test directory");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).expect("remove temporary test directory");
        }
    }

    #[test]
    fn reports_a_missing_driver_without_panicking() {
        let temp = TempDir::new();
        let driver = temp.0.join("missing-cc");

        let error = invoke_linker(
            driver.as_os_str(),
            &[&temp.0.join("main.o")],
            &temp.0.join("main"),
        )
        .expect_err("missing driver must be reported");

        assert!(matches!(
            error,
            LinkerError::Launch {
                source: IoError {
                    kind: io::ErrorKind::NotFound,
                    ..
                },
                ..
            }
        ));
        assert!(
            error
                .to_string()
                .contains("could not start host compiler driver")
        );
    }

    #[cfg(feature = "managed-runtime")]
    #[test]
    fn links_the_matching_private_runtime_archives() {
        let temp = TempDir::new();
        let source = temp.0.join("main.c");
        let object = temp.0.join("main.o");
        let executable = temp.0.join("main");
        fs::write(
            &source,
            "void __el_runtime_init(void);\nint main(void) { __el_runtime_init(); return 42; }\n",
        )
        .expect("write managed linker fixture");
        let compiler = env::var_os("CC").unwrap_or_else(|| OsString::from(DEFAULT_HOST_COMPILER));
        let compilation = Command::new(compiler)
            .arg("-c")
            .arg(source)
            .arg("-o")
            .arg(&object)
            .output()
            .expect("compile managed linker fixture");
        assert!(compilation.status.success());

        link_host_managed_executable(&[object.as_path()], &executable)
            .expect("link with private runtime archives");
        let status = Command::new(executable)
            .status()
            .expect("run managed linker fixture");
        assert_eq!(status.code(), Some(42));
    }

    #[cfg(unix)]
    #[test]
    fn preserves_a_failed_linkers_status_and_output() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new();
        let driver = temp.0.join("failing-cc");
        fs::write(
            &driver,
            "#!/bin/sh\nprintf 'link detail' >&2\nprintf 'link stdout'\nexit 23\n",
        )
        .expect("write fake compiler driver");
        fs::set_permissions(&driver, fs::Permissions::from_mode(0o700))
            .expect("make fake compiler driver executable");

        let error = invoke_linker(
            driver.as_os_str(),
            &[&temp.0.join("main.o")],
            &temp.0.join("main"),
        )
        .expect_err("failing driver must be reported");

        let LinkerError::Failed {
            status,
            stdout,
            stderr,
            ..
        } = &error
        else {
            panic!("expected a linker exit failure");
        };
        assert_eq!(status.code(), Some(23));
        assert_eq!(stdout, "link stdout");
        assert_eq!(stderr, "link detail");
        assert!(error.to_string().contains("link detail"));
    }
}
