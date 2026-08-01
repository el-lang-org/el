//! Project discovery and compiler pipeline orchestration.

use el_ast::{Node, Program, Value};
use el_codegen::{CodegenProfile, MetadataWriteError, TargetMetadata};
use el_ir::GenericModule;
use el_span::{Diagnostic, FileId};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

mod package;
pub use package::{
    DependencySource, Manifest, Package, PackageGraph, PackageId, PackageVersion,
    ResolvedDependency, SourceModule, load_package_graph,
};

pub const MANIFEST_FILE_NAME: &str = "el.toml";

/// The two native build profiles in the v1 CLI contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildProfile {
    Development,
    Release,
}

impl BuildProfile {
    #[must_use]
    pub const fn from_release(release: bool) -> Self {
        if release {
            Self::Release
        } else {
            Self::Development
        }
    }

    #[must_use]
    pub const fn directory_name(self) -> &'static str {
        match self {
            Self::Development => "debug",
            Self::Release => "release",
        }
    }

    #[must_use]
    pub const fn codegen_profile(self) -> CodegenProfile {
        match self {
            Self::Development => CodegenProfile::Development,
            Self::Release => CodegenProfile::Release,
        }
    }
}

/// Deterministic locations for one host-native build.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BuildOutputPaths {
    directory: PathBuf,
    object: PathBuf,
    executable: PathBuf,
    metadata: PathBuf,
}

impl BuildOutputPaths {
    #[must_use]
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    #[must_use]
    pub fn object(&self) -> &Path {
        &self.object
    }

    #[must_use]
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    #[must_use]
    pub fn metadata(&self) -> &Path {
        &self.metadata
    }
}

/// Creates the target/profile build directory and records its target metadata.
pub fn prepare_build_output(
    project_root: &Path,
    package_name: &str,
    target: &TargetMetadata,
    profile: BuildProfile,
) -> Result<BuildOutputPaths, BuildOutputError> {
    if !is_package_id(package_name) {
        return Err(BuildOutputError::InvalidPackageName(
            package_name.to_owned(),
        ));
    }

    let directory = project_root
        .join("build")
        .join(target.llvm_target_triple())
        .join(profile.directory_name());
    std::fs::create_dir_all(&directory).map_err(|source| BuildOutputError::CreateDirectory {
        path: directory.clone(),
        kind: source.kind(),
        message: source.to_string(),
    })?;

    let executable_name = format!("{package_name}{}", std::env::consts::EXE_SUFFIX);
    let paths = BuildOutputPaths {
        object: directory.join(format!("{package_name}.o")),
        executable: directory.join(executable_name),
        metadata: directory.join("el-build-metadata.toml"),
        directory,
    };
    target
        .write_reproducibility_file(paths.metadata())
        .map_err(BuildOutputError::WriteMetadata)?;
    Ok(paths)
}

fn is_package_id(name: &str) -> bool {
    let mut components = name.split('_');
    components.next().is_some_and(is_package_component) && components.all(is_package_component)
}

fn is_package_component(component: &str) -> bool {
    let mut bytes = component.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BuildOutputError {
    InvalidPackageName(String),
    CreateDirectory {
        path: PathBuf,
        kind: std::io::ErrorKind,
        message: String,
    },
    WriteMetadata(MetadataWriteError),
}

impl fmt::Display for BuildOutputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPackageName(name) => {
                write!(
                    formatter,
                    "invalid lowercase snake_case package name `{name}`"
                )
            }
            Self::CreateDirectory { path, message, .. } => write!(
                formatter,
                "could not create build directory `{}`: {message}",
                path.display()
            ),
            Self::WriteMetadata(error) => write!(formatter, "{error}"),
        }
    }
}

impl Error for BuildOutputError {}

/// Runs the implemented, target-independent compiler stages for one source module.
///
/// This is the Milestone 2 vertical slice. Project graph loading remains owned by
/// the later manifest milestone, and this function never initializes LLVM.
pub fn analyze_source(file: FileId, source: &str) -> Result<GenericModule, Vec<Diagnostic>> {
    analyze_package_sources(&[(file, source)])
}

/// Runs the target-independent frontend for all modules in one source package.
///
/// Inputs must already be in deterministic package-relative path order. Manifest,
/// dependency, and lockfile loading remain part of the later package milestone.
pub fn analyze_package_sources(
    sources: &[(FileId, &str)],
) -> Result<GenericModule, Vec<Diagnostic>> {
    let mut parsed = Vec::with_capacity(sources.len());
    let mut diagnostics = Vec::new();
    for (file, source) in sources {
        match el_parser::parse(*file, source) {
            Ok(program) => parsed.push(program),
            Err(error) => diagnostics.push(Diagnostic::error("E1000", error.span, error.message)),
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let resolved = el_resolve::resolve_package(&parsed)?;
    let typed = el_types::check_package(&resolved)?;
    Ok(el_ir::lower(&typed))
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectCommand {
    Check { locked: bool },
    Build { release: bool, locked: bool },
    EmitLlvmIr { module: String },
}

impl ProjectCommand {
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Check { .. } => "check",
            Self::Build { .. } => "build",
            Self::EmitLlvmIr { .. } => "emit llvm-ir",
        }
    }
}

/// Establishes the project boundary for a known command.
///
/// Language compilation is deliberately unavailable until its first vertical
/// slice; reaching this error proves that invocation and project discovery
/// completed without crossing an unimplemented compiler stage.
pub fn run_project_command(
    command: ProjectCommand,
    start: &Path,
) -> Result<ProjectOutput, ProjectError> {
    let project = discover_project(start)?;
    let locked = match &command {
        ProjectCommand::Check { locked } | ProjectCommand::Build { locked, .. } => *locked,
        ProjectCommand::EmitLlvmIr { .. } => false,
    };
    let graph = load_package_graph(project.directory(), locked).map_err(ProjectError::Package)?;
    let mut source_map = el_span::SourceMap::new();
    let mut programs = Vec::new();
    for package in graph.packages() {
        let package_modules = package
            .modules()
            .iter()
            .map(|module| module.name().to_owned())
            .collect::<Vec<_>>();
        for module in package.modules() {
            let display = if package.id() == graph.root().id() {
                module.relative_path().to_path_buf()
            } else {
                PathBuf::from(package.namespace()).join(module.relative_path())
            };
            let file = source_map.add_file(display, module.source().to_owned());
            let mut program = el_parser::parse(file, module.source()).map_err(|error| {
                ProjectError::Diagnostics(vec![Diagnostic::error(
                    "E1000",
                    error.span,
                    error.message,
                )])
            })?;
            qualify_dependency_program(&mut program, package.namespace(), &package_modules);
            programs.push(program);
        }
    }
    let resolved = el_resolve::resolve_package(&programs).map_err(ProjectError::Diagnostics)?;
    let typed = el_types::check_package(&resolved).map_err(ProjectError::Diagnostics)?;
    let core = el_ir::lower(&typed);

    match command {
        ProjectCommand::Check { .. } => Ok(ProjectOutput::default()),
        ProjectCommand::Build { release, .. } => {
            if graph.root().manifest().target().is_none() {
                return Err(ProjectError::NoExecutableTarget {
                    package: graph.root().id().as_str().to_owned(),
                });
            }
            build_project(&core, graph.root(), BuildProfile::from_release(release))?;
            Ok(ProjectOutput::default())
        }
        ProjectCommand::EmitLlvmIr { module } => {
            if !graph
                .root()
                .modules()
                .iter()
                .any(|candidate| candidate.name() == module)
            {
                return Err(ProjectError::UnknownEmitModule(module));
            }
            Ok(ProjectOutput {
                stdout: emit_module(&core, graph.root().namespace(), &module)?,
            })
        }
    }
}

#[cfg(feature = "llvm")]
fn build_project(
    core: &GenericModule,
    package: &Package,
    profile: BuildProfile,
) -> Result<(), ProjectError> {
    let target_module = package.manifest().target().expect("build target checked");
    let qualified_target = format!("{}.{}", package.namespace(), target_module);
    let roots = el_ir::executable_reachability_roots_for(core, &qualified_target)
        .map_err(|error| ProjectError::Backend(format!("invalid executable target: {error:?}")))?;
    let concrete = el_ir::monomorphize(core, &roots)
        .map_err(|error| ProjectError::Backend(format!("monomorphization failed: {error:?}")))?;
    let target = el_codegen::host_target_metadata()
        .map_err(|error| ProjectError::Backend(error.to_string()))?;
    let paths = prepare_build_output(package.root(), package.id().as_str(), &target, profile)
        .map_err(|error| ProjectError::Backend(error.to_string()))?;
    el_codegen::emit_host_object_with_profile(&concrete, paths.object(), profile.codegen_profile())
        .map_err(|error| ProjectError::Backend(error.to_string()))?;
    #[cfg(feature = "managed-runtime")]
    el_codegen::link_host_managed_executable(&[paths.object()], paths.executable())
        .map_err(|error| ProjectError::Backend(error.to_string()))?;
    #[cfg(not(feature = "managed-runtime"))]
    el_codegen::link_host_executable(paths.object(), paths.executable())
        .map_err(|error| ProjectError::Backend(error.to_string()))?;
    Ok(())
}

#[cfg(not(feature = "llvm"))]
fn build_project(_: &GenericModule, _: &Package, _: BuildProfile) -> Result<(), ProjectError> {
    Err(ProjectError::BackendUnavailable)
}

#[cfg(feature = "llvm")]
fn emit_module(
    core: &GenericModule,
    namespace: &str,
    module: &str,
) -> Result<String, ProjectError> {
    let qualified_module = format!("{namespace}.{module}");
    let functions = core
        .functions
        .iter()
        .filter(|function| {
            function.module_name == qualified_module
                && function.type_parameters.is_empty()
                && function.constraints.is_empty()
        })
        .map(|function| function.id)
        .collect::<Vec<_>>();
    if functions.is_empty() {
        return Err(ProjectError::Backend(format!(
            "module `{module}` has no monomorphic functions to emit"
        )));
    }
    let concrete = el_ir::monomorphize(core, &el_ir::ReachabilityRoots { functions })
        .map_err(|error| ProjectError::Backend(format!("monomorphization failed: {error:?}")))?;
    el_codegen::lower_module_to_llvm_ir(&concrete)
        .map(|llvm| llvm.as_str().to_owned())
        .map_err(|error| ProjectError::Backend(error.to_string()))
}

#[cfg(not(feature = "llvm"))]
fn emit_module(_: &GenericModule, _: &str, _: &str) -> Result<String, ProjectError> {
    Err(ProjectError::BackendUnavailable)
}

fn qualify_dependency_program(program: &mut Program, namespace: &str, modules: &[String]) {
    qualify_dependency_node(&mut program.root, namespace, modules, true);
}

fn qualify_dependency_node(
    node: &mut Node,
    namespace: &str,
    modules: &[String],
    mut module_declaration: bool,
) {
    if matches!(node.kind.as_str(), "type_path" | "qualified_value") {
        let path = node
            .children
            .iter()
            .filter_map(|component| match &component.value {
                Some(Value::Text(text)) => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let written = path.join(".");
        let internal = module_declaration
            || modules
                .iter()
                .any(|module| written == *module || written.starts_with(&format!("{module}.")));
        if internal && path.first().copied() != Some(namespace) {
            let template = node.children.first().cloned();
            if let Some(mut component) = template {
                component.value = Some(Value::Text(namespace.to_owned()));
                node.children.insert(0, component);
            }
        }
        module_declaration = false;
    }
    for child in &mut node.children {
        qualify_dependency_node(child, namespace, modules, module_declaration);
        module_declaration = false;
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ProjectOutput {
    stdout: String,
}

impl ProjectOutput {
    #[must_use]
    pub fn stdout(&self) -> &str {
        &self.stdout
    }
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
    Package(package::PackageError),
    Diagnostics(Vec<Diagnostic>),
    NoExecutableTarget {
        package: String,
    },
    UnknownEmitModule(String),
    BackendUnavailable,
    Backend(String),
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
            Self::Package(error) => write!(formatter, "{error}"),
            Self::Diagnostics(diagnostics) => {
                write!(
                    formatter,
                    "compilation failed with {} diagnostic(s)",
                    diagnostics.len()
                )?;
                for diagnostic in diagnostics {
                    write!(
                        formatter,
                        "\nerror[{}]: {}",
                        diagnostic.code, diagnostic.message
                    )?;
                }
                Ok(())
            }
            Self::NoExecutableTarget { package } => write!(
                formatter,
                "package `{package}` has no executable target; use `el check`"
            ),
            Self::UnknownEmitModule(module) => {
                write!(formatter, "package does not contain module `{module}`")
            }
            Self::BackendUnavailable => formatter.write_str(
                "native project builds require an EL compiler built with the LLVM backend",
            ),
            Self::Backend(message) => formatter.write_str(message),
        }
    }
}

impl Error for ProjectError {}

#[cfg(test)]
mod tests {
    use super::*;
    use el_span::SourceMap;
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
    fn prepares_target_specific_development_and_release_outputs() {
        let temp = TempDir::new();
        let target = TargetMetadata::new("arm64-apple-test", 64).unwrap();

        let development = prepare_build_output(
            temp.path(),
            "sample_app",
            &target,
            BuildProfile::from_release(false),
        )
        .expect("prepare development output");
        let release = prepare_build_output(
            temp.path(),
            "sample_app",
            &target,
            BuildProfile::from_release(true),
        )
        .expect("prepare release output");

        assert_eq!(
            development.directory(),
            temp.path().join("build/arm64-apple-test/debug")
        );
        assert_eq!(
            release.directory(),
            temp.path().join("build/arm64-apple-test/release")
        );
        assert!(development.directory().is_dir());
        assert!(release.directory().is_dir());
        assert_eq!(
            development.object(),
            development.directory().join("sample_app.o")
        );
        assert_eq!(
            development.executable(),
            development
                .directory()
                .join(format!("sample_app{}", std::env::consts::EXE_SUFFIX))
        );
        assert_eq!(
            fs::read_to_string(development.metadata()).unwrap(),
            target.reproducibility_text()
        );
    }

    #[test]
    fn rejects_unsafe_package_output_names_before_creating_directories() {
        let temp = TempDir::new();
        let target = TargetMetadata::new("arm64-apple-test", 64).unwrap();

        for name in ["", "Bad", "bad-name", "bad_", "_bad", "bad__name"] {
            assert_eq!(
                prepare_build_output(temp.path(), name, &target, BuildProfile::Development),
                Err(BuildOutputError::InvalidPackageName(name.to_owned()))
            );
        }
        assert!(!temp.path().join("build").exists());
    }

    #[test]
    fn reports_build_directory_creation_failures_without_panicking() {
        let temp = TempDir::new();
        fs::write(temp.path().join("build"), "not a directory").unwrap();
        let target = TargetMetadata::new("arm64-apple-test", 64).unwrap();

        let error = prepare_build_output(temp.path(), "sample", &target, BuildProfile::Development)
            .expect_err("directory failure must be structured");

        assert!(matches!(error, BuildOutputError::CreateDirectory { .. }));
        assert!(
            error
                .to_string()
                .contains("could not create build directory")
        );
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
    fn malformed_manifest_fails_as_an_ordinary_project_error() {
        let temp = TempDir::new();
        fs::write(temp.path().join(MANIFEST_FILE_NAME), "").expect("write manifest");

        let error = run_project_command(ProjectCommand::Check { locked: false }, temp.path())
            .expect_err("invalid project manifest must fail");

        assert!(matches!(error, ProjectError::Package(_)));
    }

    #[test]
    fn analyzes_the_milestone_two_slice_without_a_backend() {
        let source = "defmodule Main do\n  def main() -> i32 do\n    value: i32 = 40\n    value + 2\n  end\nend\n";
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/main.el", source);

        let core = analyze_source(file, source).expect("source reaches verified Generic Core IR");

        assert!(core.debug_text().contains("checked.Add"));
    }

    #[test]
    fn invalid_source_never_reaches_lowering() {
        let source = "defmodule Main do\n  def main() -> i32 do\n    value = true\n    value + 2\n  end\nend\n";
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/main.el", source);

        let diagnostics = analyze_source(file, source).expect_err("type error stops the pipeline");

        assert!(
            diagnostics
                .iter()
                .all(|diagnostic| diagnostic.primary.file() == file)
        );
    }

    #[test]
    fn orchestrates_a_multi_module_package_through_generic_core() {
        let library = "defmodule Library do\n  def answer() -> i32 do\n    42\n  end\nend\n";
        let main = "defmodule Main do\n  def main() -> i32 do\n    Library.answer()\n  end\nend\n";
        let mut sources = SourceMap::new();
        let library_file = sources.add_file("src/library.el", library);
        let main_file = sources.add_file("src/main.el", main);

        let core = analyze_package_sources(&[(library_file, library), (main_file, main)])
            .expect("package reaches Generic Core IR");

        assert_eq!(core.functions.len(), 2);
        assert!(core.debug_text().contains("call f0"));
    }

    #[test]
    fn package_orchestration_preserves_cross_module_diagnostic_spans() {
        let library = "defmodule Library do\n  defp hidden() -> i32 do\n    1\n  end\nend\n";
        let main = "defmodule Main do\n  def main() -> i32 do\n    Library.hidden()\n  end\nend\n";
        let mut sources = SourceMap::new();
        let library_file = sources.add_file("src/library.el", library);
        let main_file = sources.add_file("src/main.el", main);

        let diagnostics = analyze_package_sources(&[(library_file, library), (main_file, main)])
            .expect_err("private cross-module call is rejected");
        let private = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "E2138")
            .expect("private visibility diagnostic");
        assert_eq!(private.primary.file(), main_file);
        assert_eq!(
            private.primary.start(),
            main.find("Library.hidden").unwrap()
        );
    }

    #[test]
    fn checks_a_manifest_project_with_a_namespaced_path_dependency_and_stable_lock() {
        let temp = TempDir::new();
        let dependency = temp.path().join("dep");
        fs::create_dir_all(temp.path().join("src")).unwrap();
        fs::create_dir_all(dependency.join("src")).unwrap();
        fs::write(
            temp.path().join("el.toml"),
            "[package]\nname = \"app\"\nnamespace = \"App\"\nversion = \"1.0.0\"\n\n[deps.dep]\npath = \"dep\"\nversion = \"2.0.0\"\n\n[target]\nmain = \"Main\"\n",
        ).unwrap();
        fs::write(
            temp.path().join("src/main.el"),
            "defmodule Main do\n  def main() -> i32 do\n    Dep.Utility.answer()\n  end\nend\n",
        )
        .unwrap();
        fs::write(
            dependency.join("el.toml"),
            "[package]\nname = \"dep\"\nnamespace = \"Dep\"\nversion = \"2.0.0\"\n\n[deps]\n",
        )
        .unwrap();
        fs::write(
            dependency.join("src/utility.el"),
            "defmodule Utility do\n  def answer() -> i32 do\n    42\n  end\nend\n",
        )
        .unwrap();

        run_project_command(ProjectCommand::Check { locked: false }, temp.path())
            .expect("multi-package check succeeds");
        let first = fs::read_to_string(temp.path().join("el.lock")).unwrap();
        run_project_command(ProjectCommand::Check { locked: true }, temp.path())
            .expect("generated lock verifies");
        assert_eq!(
            fs::read_to_string(temp.path().join("el.lock")).unwrap(),
            first
        );
    }

    #[test]
    fn locked_check_never_creates_a_missing_lockfile() {
        let temp = TempDir::new();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(
            temp.path().join("el.toml"),
            "[package]\nname = \"app\"\nnamespace = \"App\"\nversion = \"1.0.0\"\n\n[deps]\n",
        )
        .unwrap();
        fs::write(
            temp.path().join("src/main.el"),
            "defmodule Main do\n  def main() -> i32 do\n    0\n  end\nend\n",
        )
        .unwrap();

        let error = run_project_command(ProjectCommand::Check { locked: true }, temp.path())
            .expect_err("missing locked file fails");
        assert!(matches!(
            error,
            ProjectError::Package(package::PackageError::Lockfile(_))
        ));
        assert!(!temp.path().join("el.lock").exists());
    }

    #[cfg(feature = "llvm")]
    #[test]
    fn manifest_build_and_emit_use_the_selected_target_module() {
        let temp = TempDir::new();
        fs::create_dir(temp.path().join("src")).unwrap();
        fs::write(
            temp.path().join("el.toml"),
            "[package]\nname = \"app\"\nnamespace = \"App\"\nversion = \"1.0.0\"\n\n[deps]\n\n[target]\nmain = \"Command\"\n",
        ).unwrap();
        fs::write(
            temp.path().join("src/command.el"),
            "defmodule Command do\n  def main() -> i32 do\n    42\n  end\nend\n",
        )
        .unwrap();

        run_project_command(
            ProjectCommand::Build {
                release: false,
                locked: false,
            },
            temp.path(),
        )
        .expect("manifest target builds");
        let target = el_codegen::host_target_metadata().unwrap();
        let executable = temp
            .path()
            .join("build")
            .join(target.llvm_target_triple())
            .join("debug")
            .join(format!("app{}", std::env::consts::EXE_SUFFIX));
        assert!(executable.is_file());
        assert_eq!(
            std::process::Command::new(executable)
                .status()
                .unwrap()
                .code(),
            Some(42)
        );

        let output = run_project_command(
            ProjectCommand::EmitLlvmIr {
                module: "Command".to_owned(),
            },
            temp.path(),
        )
        .expect("module emits LLVM IR");
        assert!(output.stdout().contains("define i32 @el.f"));
        assert!(!output.stdout().contains("define i32 @main"));
    }
}
