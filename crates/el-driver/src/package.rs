//! Strict v1 manifest, package graph, source discovery, and lockfile support.

use el_ast::Value;
use el_span::SourceMap;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::LOCKFILE_FORMAT_VERSION;

const LOCK_FILE_NAME: &str = "el.lock";

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PackageId(String);

impl PackageId {
    pub fn parse(value: &str) -> Result<Self, PackageError> {
        let valid = !value.is_empty()
            && value.split('_').all(|part| {
                let mut bytes = part.bytes();
                bytes.next().is_some_and(|byte| byte.is_ascii_lowercase())
                    && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            });
        valid.then(|| Self(value.to_owned())).ok_or_else(|| {
            PackageError::Manifest(format!(
                "invalid package ID `{value}`; expected lowercase snake_case"
            ))
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PackageVersion {
    major: u64,
    minor: u64,
    patch: u64,
    suffix: String,
}

impl PackageVersion {
    pub fn parse(value: &str) -> Result<Self, PackageError> {
        let (without_build, build) = value
            .split_once('+')
            .map_or((value, None), |(core, build)| (core, Some(build)));
        if build.is_some_and(|identifiers| !valid_semver_identifiers(identifiers, false))
            || without_build.matches('+').count() != 0
        {
            return Err(PackageError::Manifest(format!(
                "invalid semantic version `{value}`"
            )));
        }
        let (core, prerelease) = without_build
            .split_once('-')
            .map_or((without_build, None), |(core, prerelease)| {
                (core, Some(prerelease))
            });
        if prerelease.is_some_and(|identifiers| !valid_semver_identifiers(identifiers, true)) {
            return Err(PackageError::Manifest(format!(
                "invalid semantic version `{value}`"
            )));
        }
        let numbers = core.split('.').collect::<Vec<_>>();
        if numbers.len() != 3
            || numbers.iter().any(|part| {
                part.is_empty()
                    || (part.len() > 1 && part.starts_with('0'))
                    || !part.bytes().all(|byte| byte.is_ascii_digit())
            })
        {
            return Err(PackageError::Manifest(format!(
                "invalid semantic version `{value}`; expected MAJOR.MINOR.PATCH"
            )));
        }
        let parse = |part: &str| {
            part.parse::<u64>().map_err(|_| {
                PackageError::Manifest(format!(
                    "semantic version component in `{value}` is too large"
                ))
            })
        };
        Ok(Self {
            major: parse(numbers[0])?,
            minor: parse(numbers[1])?,
            patch: parse(numbers[2])?,
            suffix: value[core.len()..].to_owned(),
        })
    }
}

impl fmt::Display for PackageVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{}.{}.{}{}",
            self.major, self.minor, self.patch, self.suffix
        )
    }
}

fn valid_semver_identifiers(value: &str, reject_numeric_leading_zero: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && !(reject_numeric_leading_zero
                    && identifier.len() > 1
                    && identifier.starts_with('0')
                    && identifier.bytes().all(|byte| byte.is_ascii_digit()))
        })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DependencySource {
    Path(PathBuf),
    Git { url: String, revision: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedDependency {
    id: PackageId,
    version: PackageVersion,
    source: DependencySource,
}

impl ResolvedDependency {
    #[must_use]
    pub fn id(&self) -> &PackageId {
        &self.id
    }
    #[must_use]
    pub fn version(&self) -> &PackageVersion {
        &self.version
    }
    #[must_use]
    pub fn source(&self) -> &DependencySource {
        &self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Manifest {
    name: PackageId,
    namespace: String,
    version: PackageVersion,
    dependencies: Vec<ResolvedDependency>,
    target: Option<String>,
}

impl Manifest {
    #[must_use]
    pub fn name(&self) -> &PackageId {
        &self.name
    }
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
    #[must_use]
    pub fn version(&self) -> &PackageVersion {
        &self.version
    }
    #[must_use]
    pub fn dependencies(&self) -> &[ResolvedDependency] {
        &self.dependencies
    }
    #[must_use]
    pub fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceModule {
    name: String,
    relative_path: PathBuf,
    source: String,
}

impl SourceModule {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub fn relative_path(&self) -> &Path {
        &self.relative_path
    }
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Package {
    manifest: Manifest,
    root: PathBuf,
    source_identity: String,
    modules: Vec<SourceModule>,
}

impl Package {
    #[must_use]
    pub fn id(&self) -> &PackageId {
        self.manifest.name()
    }
    #[must_use]
    pub fn namespace(&self) -> &str {
        self.manifest.namespace()
    }
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
    #[must_use]
    pub fn modules(&self) -> &[SourceModule] {
        &self.modules
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PackageGraph {
    root: usize,
    packages: Vec<Package>,
}

impl PackageGraph {
    #[must_use]
    pub fn root(&self) -> &Package {
        &self.packages[self.root]
    }
    #[must_use]
    pub fn packages(&self) -> &[Package] {
        &self.packages
    }
    fn lock_text(&self) -> String {
        let mut packages = self.packages.iter().collect::<Vec<_>>();
        packages.sort_by(|left, right| left.id().cmp(right.id()));
        let mut output = format!("version = {LOCKFILE_FORMAT_VERSION}\n");
        for package in packages {
            output.push_str("\n[[package]]\n");
            push_quoted(&mut output, "name", package.id().as_str());
            push_quoted(&mut output, "namespace", package.namespace());
            push_quoted(
                &mut output,
                "version",
                &package.manifest.version().to_string(),
            );
            push_quoted(&mut output, "source", &package.source_identity);
            let mut dependencies = package
                .manifest
                .dependencies
                .iter()
                .map(|dependency| dependency.id.as_str())
                .collect::<Vec<_>>();
            dependencies.sort_unstable();
            output.push_str("dependencies = [");
            for (index, dependency) in dependencies.iter().enumerate() {
                if index != 0 {
                    output.push_str(", ");
                }
                output.push('"');
                output.push_str(dependency);
                output.push('"');
            }
            output.push_str("]\n");
        }
        output
    }
}

fn push_quoted(output: &mut String, key: &str, value: &str) {
    output.push_str(key);
    output.push_str(" = \"");
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            _ => output.push(character),
        }
    }
    output.push_str("\"\n");
}

pub fn load_package_graph(root: &Path, locked: bool) -> Result<PackageGraph, PackageError> {
    let root = root
        .canonicalize()
        .map_err(|error| io_error("canonicalize project root", root, error))?;
    let mut loader = GraphLoader::new(root.clone());
    let root_id = loader.load(&root, "root".to_owned())?;
    loader.validate()?;
    let root_index = loader
        .packages
        .iter()
        .position(|package| package.id() == &root_id)
        .expect("loaded root package is present");
    let graph = PackageGraph {
        root: root_index,
        packages: loader.packages,
    };
    let expected = graph.lock_text();
    let lock_path = root.join(LOCK_FILE_NAME);
    let existing = fs::read_to_string(&lock_path).ok();
    if locked {
        if existing.as_deref() != Some(expected.as_str()) {
            return Err(PackageError::Lockfile(if existing.is_some() {
                "el.lock is stale; rerun without --locked".to_owned()
            } else {
                "el.lock is missing; rerun without --locked".to_owned()
            }));
        }
    } else if existing.as_deref() != Some(expected.as_str()) {
        fs::write(&lock_path, expected)
            .map_err(|error| io_error("write lockfile", &lock_path, error))?;
    }
    Ok(graph)
}

struct GraphLoader {
    project_root: PathBuf,
    packages: Vec<Package>,
    states: BTreeMap<PackageId, VisitState>,
    stack: Vec<PackageId>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum VisitState {
    Visiting,
    Complete,
}

impl GraphLoader {
    fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            packages: Vec::new(),
            states: BTreeMap::new(),
            stack: Vec::new(),
        }
    }

    fn load(&mut self, root: &Path, source_identity: String) -> Result<PackageId, PackageError> {
        let manifest_path = root.join("el.toml");
        let text = fs::read_to_string(&manifest_path)
            .map_err(|error| io_error("read manifest", &manifest_path, error))?;
        let manifest = parse_manifest(&text).map_err(|error| error.at(&manifest_path))?;
        let id = manifest.name.clone();
        if self.states.get(&id) == Some(&VisitState::Visiting) {
            let mut cycle = self.stack.iter().map(PackageId::as_str).collect::<Vec<_>>();
            cycle.push(id.as_str());
            return Err(PackageError::Dependency(format!(
                "dependency cycle: {}",
                cycle.join(" -> ")
            )));
        }
        if self.states.get(&id) == Some(&VisitState::Complete) {
            let existing = self
                .packages
                .iter()
                .find(|package| package.id() == &id)
                .expect("complete package exists");
            if existing.source_identity != source_identity
                || existing.manifest.version != manifest.version
            {
                return Err(PackageError::Dependency(format!(
                    "conflicting sources or versions for package `{}`",
                    id.as_str()
                )));
            }
            return Ok(id);
        }
        self.states.insert(id.clone(), VisitState::Visiting);
        self.stack.push(id.clone());
        let modules = discover_sources(root)?;
        if let Some(target) = manifest.target()
            && !modules.iter().any(|module| module.name == target)
        {
            return Err(PackageError::Manifest(format!(
                "target module `{target}` is not owned by package `{}`",
                id.as_str()
            ))
            .at(&manifest_path));
        }
        let dependencies = manifest.dependencies.clone();
        self.packages.push(Package {
            manifest,
            root: root.to_path_buf(),
            source_identity,
            modules,
        });
        for dependency in dependencies {
            let (dependency_root, identity) = match &dependency.source {
                DependencySource::Path(path) => {
                    let resolved = root.join(path).canonicalize().map_err(|error| {
                        io_error("resolve path dependency", &root.join(path), error)
                    })?;
                    (resolved.clone(), format!("path:{}", resolved.display()))
                }
                DependencySource::Git { url, revision } => {
                    let checkout = self.checkout_git(dependency.id.as_str(), url, revision)?;
                    (checkout, format!("git:{url}#{revision}"))
                }
            };
            let loaded = self.load(&dependency_root, identity)?;
            let package = self
                .packages
                .iter()
                .find(|package| package.id() == &loaded)
                .expect("loaded dependency exists");
            if loaded != dependency.id {
                return Err(PackageError::Dependency(format!(
                    "dependency key `{}` resolved package `{}`",
                    dependency.id.as_str(),
                    loaded.as_str()
                )));
            }
            if package.manifest.version != dependency.version {
                return Err(PackageError::Dependency(format!(
                    "dependency `{}` requires version {} but resolved {}",
                    loaded.as_str(),
                    dependency.version,
                    package.manifest.version
                )));
            }
        }
        self.stack.pop();
        self.states.insert(id.clone(), VisitState::Complete);
        Ok(id)
    }

    fn checkout_git(&self, id: &str, url: &str, revision: &str) -> Result<PathBuf, PackageError> {
        let directory = self
            .project_root
            .join("build/.el/git")
            .join(format!("{id}-{revision}"));
        if !directory.join(".git").is_dir() {
            fs::create_dir_all(directory.parent().expect("git cache has parent"))
                .map_err(|error| io_error("create Git cache", &directory, error))?;
            let output = Command::new("git")
                .arg("clone")
                .arg("--no-checkout")
                .arg(url)
                .arg(&directory)
                .output()
                .map_err(|error| io_error("launch git clone", &directory, error))?;
            if !output.status.success() {
                return Err(PackageError::Dependency(format!(
                    "git clone failed for `{url}`: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                )));
            }
        }
        let output = Command::new("git")
            .arg("-C")
            .arg(&directory)
            .arg("checkout")
            .arg("--detach")
            .arg(revision)
            .output()
            .map_err(|error| io_error("launch git checkout", &directory, error))?;
        if !output.status.success() {
            return Err(PackageError::Dependency(format!(
                "git revision `{revision}` is unavailable for `{url}`"
            )));
        }
        let output = Command::new("git")
            .arg("-C")
            .arg(&directory)
            .arg("rev-parse")
            .arg("HEAD")
            .output()
            .map_err(|error| io_error("verify Git revision", &directory, error))?;
        if !output.status.success() || String::from_utf8_lossy(&output.stdout).trim() != revision {
            return Err(PackageError::Dependency(format!(
                "Git checkout did not resolve exact revision `{revision}`"
            )));
        }
        Ok(directory)
    }

    fn validate(&self) -> Result<(), PackageError> {
        let mut namespaces = BTreeMap::<&str, &PackageId>::new();
        let reserved = [
            "Array", "Bits", "Buffer", "Bytes", "Enum", "File", "IO", "List", "Map", "Process",
            "Rune", "Slice", "String", "I8", "I16", "I32", "I64", "Isize", "U8", "U16", "U32",
            "U64", "Usize",
        ];
        for package in &self.packages {
            if reserved.contains(&package.namespace()) {
                return Err(PackageError::Dependency(format!(
                    "package namespace `{}` is reserved by the prelude",
                    package.namespace()
                )));
            }
            if let Some(previous) = namespaces.insert(package.namespace(), package.id()) {
                return Err(PackageError::Dependency(format!(
                    "packages `{}` and `{}` share namespace `{}`",
                    previous.as_str(),
                    package.id().as_str(),
                    package.namespace()
                )));
            }
        }
        Ok(())
    }
}

fn parse_manifest(text: &str) -> Result<Manifest, PackageError> {
    let mut section = String::new();
    let mut tables = BTreeSet::new();
    let mut package = BTreeMap::new();
    let mut target = BTreeMap::new();
    let mut dependencies = BTreeMap::<String, BTreeMap<String, String>>::new();
    for (line_index, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') || line.starts_with("[[") {
                return Err(manifest_line(line_index, "invalid table header"));
            }
            section = line[1..line.len() - 1].trim().to_owned();
            let valid = section == "package"
                || section == "deps"
                || section == "target"
                || section
                    .strip_prefix("deps.")
                    .is_some_and(|name| PackageId::parse(name).is_ok());
            if !valid {
                return Err(manifest_line(
                    line_index,
                    &format!("unknown manifest table `[{section}]`"),
                ));
            }
            if !tables.insert(section.clone()) {
                return Err(manifest_line(
                    line_index,
                    &format!("duplicate manifest table `[{section}]`"),
                ));
            }
            continue;
        }
        let (key, raw_value) = line
            .split_once('=')
            .ok_or_else(|| manifest_line(line_index, "expected `key = value`"))?;
        let key = key.trim();
        if key.is_empty()
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        {
            return Err(manifest_line(line_index, "invalid manifest key"));
        }
        let value =
            parse_string(raw_value.trim()).map_err(|message| manifest_line(line_index, message))?;
        let table = match section.as_str() {
            "package" => &mut package,
            "target" => &mut target,
            value if value.starts_with("deps.") => {
                dependencies.entry(value[5..].to_owned()).or_default()
            }
            "deps" => {
                return Err(manifest_line(
                    line_index,
                    "dependencies use `[deps.name]` tables",
                ));
            }
            _ => {
                return Err(manifest_line(
                    line_index,
                    "key appears before a table header",
                ));
            }
        };
        if table.insert(key.to_owned(), value).is_some() {
            return Err(manifest_line(line_index, &format!("duplicate key `{key}`")));
        }
    }
    reject_unknown(&package, &["name", "namespace", "version"], "package")?;
    reject_unknown(&target, &["main"], "target")?;
    let name = PackageId::parse(required(&package, "name", "package")?)?;
    let namespace = required(&package, "namespace", "package")?.to_owned();
    if !is_namespace(&namespace) {
        return Err(PackageError::Manifest(format!(
            "invalid root namespace `{namespace}`; expected one PascalCase component"
        )));
    }
    let version = PackageVersion::parse(required(&package, "version", "package")?)?;
    let target = if target.is_empty() {
        None
    } else {
        Some(required(&target, "main", "target")?.to_owned())
    };
    if target.as_deref().is_some_and(|name| !is_module_name(name)) {
        return Err(PackageError::Manifest(
            "target.main must be a package-relative module name".to_owned(),
        ));
    }
    let mut resolved = Vec::new();
    for (key, values) in dependencies {
        reject_unknown(
            &values,
            &["path", "git", "rev", "version"],
            &format!("deps.{key}"),
        )?;
        let id = PackageId::parse(&key)?;
        let version = PackageVersion::parse(required(&values, "version", &format!("deps.{key}"))?)?;
        let source = match (values.get("path"), values.get("git"), values.get("rev")) {
            (Some(path), None, None) if valid_dependency_path(path) => {
                DependencySource::Path(PathBuf::from(path))
            }
            (Some(_), None, None) => {
                return Err(PackageError::Manifest(format!(
                    "dependency `{key}` has an invalid path"
                )));
            }
            (None, Some(url), Some(revision))
                if revision.len() == 40
                    && revision.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
            {
                DependencySource::Git {
                    url: url.clone(),
                    revision: revision.to_ascii_lowercase(),
                }
            }
            (None, Some(_), Some(_)) => {
                return Err(PackageError::Manifest(format!(
                    "dependency `{key}` requires a full 40-hex commit revision"
                )));
            }
            _ => {
                return Err(PackageError::Manifest(format!(
                    "dependency `{key}` must select exactly one of path or Git with rev"
                )));
            }
        };
        resolved.push(ResolvedDependency {
            id,
            version,
            source,
        });
    }
    Ok(Manifest {
        name,
        namespace,
        version,
        dependencies: resolved,
        target,
    })
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted && character == '\\' {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        } else if character == '#' && !quoted {
            return &line[..index];
        }
    }
    line
}
fn parse_string(value: &str) -> Result<String, &'static str> {
    if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
        return Err("manifest values must be quoted strings");
    }
    let inner = &value[1..value.len() - 1];
    if inner.contains(['"', '\\', '\n', '\r']) {
        return Err("manifest strings do not permit escapes or control characters");
    }
    Ok(inner.to_owned())
}
fn required<'a>(
    values: &'a BTreeMap<String, String>,
    key: &str,
    table: &str,
) -> Result<&'a str, PackageError> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| PackageError::Manifest(format!("missing required `{table}.{key}`")))
}
fn reject_unknown(
    values: &BTreeMap<String, String>,
    allowed: &[&str],
    table: &str,
) -> Result<(), PackageError> {
    if let Some(key) = values.keys().find(|key| !allowed.contains(&key.as_str())) {
        Err(PackageError::Manifest(format!(
            "unknown manifest key `{table}.{key}`"
        )))
    } else {
        Ok(())
    }
}
fn is_namespace(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_uppercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}
fn is_module_name(value: &str) -> bool {
    !value.is_empty() && value.split('.').all(is_namespace)
}
fn valid_dependency_path(value: &str) -> bool {
    !value.is_empty()
        && Path::new(value)
            .components()
            .all(|component| !matches!(component, Component::Prefix(_) | Component::RootDir))
}
fn manifest_line(line: usize, message: &str) -> PackageError {
    PackageError::Manifest(format!("line {}: {message}", line + 1))
}

fn discover_sources(root: &Path) -> Result<Vec<SourceModule>, PackageError> {
    let source_root = root.join("src");
    if !source_root.is_dir() {
        return Err(PackageError::Source(format!(
            "missing source directory `{}`",
            source_root.display()
        )));
    }
    let mut paths = Vec::new();
    collect_el_files(&source_root, &mut paths)?;
    paths.sort();
    let mut names = BTreeMap::<String, PathBuf>::new();
    let mut modules = Vec::new();
    let mut source_map = SourceMap::new();
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .expect("discovered source is below root")
            .to_path_buf();
        let below_src = path
            .strip_prefix(&source_root)
            .expect("discovered source is below src");
        let name = module_name_from_path(below_src)?;
        if let Some(previous) = names.insert(name.clone(), relative.clone()) {
            return Err(PackageError::Source(format!(
                "source paths `{}` and `{}` both map to module `{name}`",
                previous.display(),
                relative.display()
            )));
        }
        let source =
            fs::read_to_string(&path).map_err(|error| io_error("read source", &path, error))?;
        let file = source_map.add_file(&relative, source.clone());
        let program = el_parser::parse(file, &source).map_err(|error| {
            PackageError::Source(format!(
                "{}:{}: {}",
                relative.display(),
                error.span.start(),
                error.message
            ))
        })?;
        let declared = program
            .root
            .children
            .first()
            .and_then(|module| {
                module
                    .children
                    .iter()
                    .find(|node| node.kind.as_str() == "type_path")
            })
            .map(|path| {
                path.children
                    .iter()
                    .filter_map(|node| match &node.value {
                        Some(Value::Text(text)) => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(".")
            });
        if declared.as_deref() != Some(name.as_str()) {
            return Err(PackageError::Source(format!(
                "`{}` must declare `defmodule {name}` but declares `{}`",
                relative.display(),
                declared.as_deref().unwrap_or("<missing>")
            )));
        }
        modules.push(SourceModule {
            name,
            relative_path: relative,
            source,
        });
    }
    if modules.is_empty() {
        return Err(PackageError::Source(format!(
            "`{}` contains no .ell source files",
            source_root.display()
        )));
    }
    Ok(modules)
}

fn collect_el_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), PackageError> {
    let entries = fs::read_dir(directory)
        .map_err(|error| io_error("read source directory", directory, error))?;
    for entry in entries {
        let entry = entry.map_err(|error| io_error("read source entry", directory, error))?;
        let ty = entry
            .file_type()
            .map_err(|error| io_error("inspect source entry", &entry.path(), error))?;
        if ty.is_symlink() {
            return Err(PackageError::Source(format!(
                "source tree contains symbolic link `{}`",
                entry.path().display()
            )));
        }
        if ty.is_dir() {
            collect_el_files(&entry.path(), output)?;
        } else if ty.is_file()
            && entry
                .path()
                .extension()
                .is_some_and(|extension| extension == "ell")
        {
            output.push(entry.path());
        }
    }
    Ok(())
}
fn module_name_from_path(path: &Path) -> Result<String, PackageError> {
    let mut components = Vec::new();
    for component in path.components() {
        let Component::Normal(value) = component else {
            return Err(PackageError::Source(format!(
                "invalid source path `{}`",
                path.display()
            )));
        };
        let text = value.to_str().ok_or_else(|| {
            PackageError::Source(format!("source path `{}` is not Unicode", path.display()))
        })?;
        let stem = text.strip_suffix(".ell").unwrap_or(text);
        if stem.is_empty()
            || stem.split('_').any(|part| {
                part.is_empty()
                    || !part
                        .bytes()
                        .next()
                        .is_some_and(|byte| byte.is_ascii_lowercase())
                    || !part
                        .bytes()
                        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
            })
        {
            return Err(PackageError::Source(format!(
                "source path component `{stem}` is not lowercase snake_case"
            )));
        }
        let mut converted = String::new();
        for part in stem.split('_') {
            let mut chars = part.chars();
            converted.extend(chars.next().into_iter().flat_map(char::to_uppercase));
            converted.extend(chars);
        }
        components.push(converted);
    }
    Ok(components.join("."))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageError {
    Manifest(String),
    Dependency(String),
    Lockfile(String),
    Source(String),
    Io {
        action: &'static str,
        path: PathBuf,
        message: String,
    },
}
impl PackageError {
    fn at(self, path: &Path) -> Self {
        match self {
            Self::Manifest(message) => Self::Manifest(format!("{}: {message}", path.display())),
            other => other,
        }
    }
}
fn io_error(action: &'static str, path: &Path, error: std::io::Error) -> PackageError {
    PackageError::Io {
        action,
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}
impl fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Manifest(message) => write!(formatter, "manifest error: {message}"),
            Self::Dependency(message) => write!(formatter, "dependency error: {message}"),
            Self::Lockfile(message) => write!(formatter, "lockfile error: {message}"),
            Self::Source(message) => write!(formatter, "source error: {message}"),
            Self::Io {
                action,
                path,
                message,
            } => write!(
                formatter,
                "could not {action} `{}`: {message}",
                path.display()
            ),
        }
    }
}
impl std::error::Error for PackageError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_manifest_accepts_normative_shape() {
        let manifest = parse_manifest("[package]\nname = \"demo\"\nnamespace = \"Demo\"\nversion = \"1.2.3\"\n\n[deps]\n\n[target]\nmain = \"Main\"\n").unwrap();
        assert_eq!(manifest.name.as_str(), "demo");
        assert_eq!(manifest.target(), Some("Main"));
    }
    #[test]
    fn strict_manifest_rejects_unknown_keys_and_ranges() {
        assert!(
            parse_manifest("[package]\nname=\"demo\"\nnamespace=\"Demo\"\nversion=\"^1.0\"\n")
                .is_err()
        );
        assert!(
            parse_manifest(
                "[package]\nname=\"demo\"\nnamespace=\"Demo\"\nversion=\"1.0.0\"\nextra=\"x\"\n"
            )
            .is_err()
        );
    }
    #[test]
    fn path_mapping_is_mechanical() {
        assert_eq!(
            module_name_from_path(Path::new("http/json_api.ell")).unwrap(),
            "Http.JsonApi"
        );
        assert!(module_name_from_path(Path::new("Bad.ell")).is_err());
    }
}
