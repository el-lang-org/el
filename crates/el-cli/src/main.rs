use el_cli::{HELP, Invocation, InvocationError, VERSION, parse};
use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(run())
}

fn run() -> u8 {
    let arguments = match utf8_arguments(std::env::args_os().skip(1)) {
        Ok(arguments) => arguments,
        Err(error) => return report_invocation_error(&error),
    };
    let invocation = match parse(&arguments) {
        Ok(invocation) => invocation,
        Err(error) => return report_invocation_error(&error),
    };

    match invocation {
        Invocation::Help => write_stdout(HELP),
        Invocation::Version => write_stdout(VERSION),
        Invocation::Project(command) => {
            let current_directory = match std::env::current_dir() {
                Ok(directory) => directory,
                Err(error) => {
                    eprintln!("error: could not read the current directory: {error}");
                    return 1;
                }
            };
            match el_driver::run_project_command(command.driver_command(), &current_directory) {
                Ok(()) => 0,
                Err(error) => {
                    eprintln!("error: {error}");
                    1
                }
            }
        }
    }
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
    let _ = writeln!(stderr, "error: {error}\n\n{HELP}");
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
