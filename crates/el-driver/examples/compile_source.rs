use el_codegen::{emit_host_object_with_profile, host_target_metadata, link_host_executable};
use el_driver::{BuildProfile, analyze_source, prepare_build_output};
use el_ir::{executable_reachability_roots, monomorphize};
use el_span::SourceMap;
use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = env::args_os().skip(1);
    let usage = "usage: compile_source <source.ell> <project-root> <package-name> [--release]";
    let source_path = arguments.next().map(PathBuf::from).ok_or(usage)?;
    let project_root = arguments.next().map(PathBuf::from).ok_or(usage)?;
    let package_name = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or("package name must be valid UTF-8")?;
    let profile = match arguments.next() {
        None => BuildProfile::Development,
        Some(value) if value == "--release" => BuildProfile::Release,
        Some(_) => return Err("only the optional `--release` argument is accepted".into()),
    };
    if arguments.next().is_some() {
        return Err("too many arguments".into());
    }

    let source = fs::read_to_string(&source_path)?;
    let mut sources = SourceMap::new();
    let file = sources.add_file(source_path.display().to_string(), &source);
    let generic = analyze_source(file, &source).map_err(|diagnostics| {
        diagnostics
            .into_iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let roots = executable_reachability_roots(&generic)
        .map_err(|error| format!("entry point error: {error:?}"))?;
    let concrete = monomorphize(&generic, &roots)
        .map_err(|error| format!("monomorphization error: {error:?}"))?;
    let target = host_target_metadata()?;
    let output = prepare_build_output(&project_root, &package_name, &target, profile)?;
    let emitted =
        emit_host_object_with_profile(&concrete, output.object(), profile.codegen_profile())?;
    if emitted != target {
        return Err("host target changed during compilation".into());
    }
    link_host_executable(output.object(), output.executable())?;

    println!("{}", output.executable().display());
    Ok(())
}
