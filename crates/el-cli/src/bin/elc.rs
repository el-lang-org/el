use el_cli::{CompilerInvocation, ELC_HELP, ELC_VERSION, InvocationError, parse_compiler};
use el_driver::BuildProfile;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(run())
}

fn run() -> u8 {
    let arguments = match utf8_arguments(std::env::args_os().skip(1)) {
        Ok(arguments) => arguments,
        Err(error) => return report_invocation_error(&error),
    };
    let invocation = match parse_compiler(&arguments) {
        Ok(invocation) => invocation,
        Err(error) => return report_invocation_error(&error),
    };

    match invocation {
        CompilerInvocation::Help => write_stdout(ELC_HELP),
        CompilerInvocation::Version => write_stdout(ELC_VERSION),
        CompilerInvocation::Compile {
            source,
            output,
            release,
        } => {
            let source = PathBuf::from(source);
            let executable = match output {
                Some(output) => PathBuf::from(output),
                None => match default_output(&source) {
                    Ok(output) => output,
                    Err(error) => return report_invocation_error(&error),
                },
            };
            match el_driver::compile_single_source(
                &source,
                &executable,
                BuildProfile::from_release(release),
            ) {
                Ok(()) => 0,
                Err(error) => {
                    eprintln!("error: {error}");
                    1
                }
            }
        }
    }
}

fn default_output(source: &Path) -> Result<PathBuf, InvocationError> {
    let stem = source
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_suffix(".ell"))
        .filter(|stem| !stem.is_empty())
        .ok_or_else(|| InvocationError::new("source file must have a name before `.ell`"))?;
    let mut output = PathBuf::from(stem);
    if !std::env::consts::EXE_SUFFIX.is_empty() {
        output.set_extension(std::env::consts::EXE_SUFFIX.trim_start_matches('.'));
    }
    Ok(output)
}

fn utf8_arguments(
    arguments: impl Iterator<Item = OsString>,
) -> Result<Vec<String>, InvocationError> {
    arguments
        .map(|argument| {
            argument.into_string().map_err(|_| {
                InvocationError::new("command-line arguments must be valid Unicode text")
            })
        })
        .collect()
}

fn report_invocation_error(error: &InvocationError) -> u8 {
    let mut stderr = io::stderr().lock();
    let _ = writeln!(stderr, "error: {error}\n\n{ELC_HELP}");
    2
}

fn write_stdout(text: &str) -> u8 {
    match io::stdout().lock().write_all(text.as_bytes()) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("error: could not write standard output: {error}");
            1
        }
    }
}
