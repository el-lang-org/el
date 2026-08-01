//! Strict parsing for the normative EL command-line surface.

use el_driver::ProjectCommand;
use std::error::Error;
use std::fmt;

pub const HELP: &str = "EL compiler and package tool

Usage:
  el --help
  el --version
  el check [--locked]
  el build [--release] [--locked]
  el emit llvm-ir --module <Module>

Options:
  --locked    Require an up-to-date lockfile
  --release   Build with optimizations
  --module    Select a package-relative module
";

pub const VERSION: &str = concat!("el ", env!("CARGO_PKG_VERSION"), "\n");

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Invocation {
    Help,
    Version,
    Project(ProjectInvocation),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProjectInvocation {
    Check { locked: bool },
    Build { release: bool, locked: bool },
    EmitLlvmIr { module: String },
}

impl ProjectInvocation {
    #[must_use]
    pub fn driver_command(&self) -> ProjectCommand {
        match self {
            Self::Check { locked } => ProjectCommand::Check { locked: *locked },
            Self::Build { release, locked } => ProjectCommand::Build {
                release: *release,
                locked: *locked,
            },
            Self::EmitLlvmIr { module } => ProjectCommand::EmitLlvmIr {
                module: module.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvocationError {
    message: String,
}

impl InvocationError {
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for InvocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for InvocationError {}

pub fn parse(arguments: &[String]) -> Result<Invocation, InvocationError> {
    match arguments {
        [flag] if flag == "--help" => Ok(Invocation::Help),
        [flag] if flag == "--version" => Ok(Invocation::Version),
        [command, rest @ ..] if command == "check" => parse_check(rest),
        [command, rest @ ..] if command == "build" => parse_build(rest),
        [command, rest @ ..] if command == "emit" => parse_emit(rest),
        [] => Err(InvocationError::new("no command was provided")),
        [argument, ..] if argument.starts_with('-') => {
            Err(InvocationError::new(format!("unknown option `{argument}`")))
        }
        [command, ..] => Err(InvocationError::new(format!("unknown command `{command}`"))),
    }
}

fn parse_check(arguments: &[String]) -> Result<Invocation, InvocationError> {
    let mut locked = false;
    for argument in arguments {
        match argument.as_str() {
            "--locked" if locked => {
                return Err(InvocationError::new(
                    "option `--locked` was provided more than once",
                ));
            }
            "--locked" => locked = true,
            option if option.starts_with('-') => {
                return Err(InvocationError::new(format!(
                    "unknown option `{option}` for `el check`"
                )));
            }
            value => {
                return Err(InvocationError::new(format!(
                    "unexpected argument `{value}` for `el check`"
                )));
            }
        }
    }
    Ok(Invocation::Project(ProjectInvocation::Check { locked }))
}

fn parse_build(arguments: &[String]) -> Result<Invocation, InvocationError> {
    let mut release = false;
    let mut locked = false;
    for argument in arguments {
        match argument.as_str() {
            "--release" if release => {
                return Err(InvocationError::new(
                    "option `--release` was provided more than once",
                ));
            }
            "--release" => release = true,
            "--locked" if locked => {
                return Err(InvocationError::new(
                    "option `--locked` was provided more than once",
                ));
            }
            "--locked" => locked = true,
            option if option.starts_with('-') => {
                return Err(InvocationError::new(format!(
                    "unknown option `{option}` for `el build`"
                )));
            }
            value => {
                return Err(InvocationError::new(format!(
                    "unexpected argument `{value}` for `el build`"
                )));
            }
        }
    }
    Ok(Invocation::Project(ProjectInvocation::Build {
        release,
        locked,
    }))
}

fn parse_emit(arguments: &[String]) -> Result<Invocation, InvocationError> {
    let [format, options @ ..] = arguments else {
        return Err(InvocationError::new("missing emit format `llvm-ir`"));
    };
    if format != "llvm-ir" {
        return Err(InvocationError::new(format!(
            "unknown emit format `{format}`"
        )));
    }

    let [module_option, module] = options else {
        if options.first().is_some_and(|option| option == "--module") {
            return Err(InvocationError::new("option `--module` requires a value"));
        }
        return Err(InvocationError::new(
            "`el emit llvm-ir` requires `--module <Module>`",
        ));
    };
    if module_option != "--module" {
        return Err(InvocationError::new(format!(
            "unknown option `{module_option}` for `el emit llvm-ir`"
        )));
    }
    if module.starts_with('-') {
        return Err(InvocationError::new("option `--module` requires a value"));
    }
    if !is_module_name(module) {
        return Err(InvocationError::new(format!(
            "invalid package-relative module name `{module}`"
        )));
    }

    Ok(Invocation::Project(ProjectInvocation::EmitLlvmIr {
        module: module.clone(),
    }))
}

fn is_module_name(name: &str) -> bool {
    !name.is_empty() && name.split('.').all(is_module_segment)
}

fn is_module_segment(segment: &str) -> bool {
    let mut bytes = segment.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_uppercase())
        && bytes.all(|byte| byte.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    #[test]
    fn accepts_complete_normative_surface() {
        let accepted = [
            vec!["--help"],
            vec!["--version"],
            vec!["check"],
            vec!["check", "--locked"],
            vec!["build"],
            vec!["build", "--release"],
            vec!["build", "--locked"],
            vec!["build", "--release", "--locked"],
            vec!["build", "--locked", "--release"],
            vec!["emit", "llvm-ir", "--module", "Http.Client"],
        ];

        for invocation in accepted {
            assert!(
                parse(&arguments(&invocation)).is_ok(),
                "rejected {invocation:?}"
            );
        }
    }

    #[test]
    fn rejects_duplicate_unknown_and_misplaced_options() {
        let rejected = [
            vec!["check", "--locked", "--locked"],
            vec!["check", "--release"],
            vec!["build", "--release", "--release"],
            vec!["build", "--module"],
            vec!["--locked", "check"],
            vec!["emit", "llvm-ir", "--module", "Main", "--module", "Other"],
        ];

        for invocation in rejected {
            assert!(
                parse(&arguments(&invocation)).is_err(),
                "accepted {invocation:?}"
            );
        }
    }

    #[test]
    fn rejects_every_audited_post_v1_command_and_global_option() {
        for invocation in [
            vec!["run"],
            vec!["test"],
            vec!["fmt"],
            vec!["repl"],
            vec!["--verbose", "check"],
            vec!["--target", "aarch64-unknown-linux-gnu", "build"],
            vec!["build", "--output", "program"],
        ] {
            assert!(
                parse(&arguments(&invocation)).is_err(),
                "post-v1 surface leaked through {invocation:?}"
            );
        }
    }

    #[test]
    fn rejects_missing_values_and_invalid_modules() {
        let rejected = [
            vec!["emit"],
            vec!["emit", "llvm-ir"],
            vec!["emit", "llvm-ir", "--module"],
            vec!["emit", "llvm-ir", "--module", "--locked"],
            vec!["emit", "llvm-ir", "--module", "main"],
            vec!["emit", "llvm-ir", "--module", "Main."],
        ];

        for invocation in rejected {
            assert!(
                parse(&arguments(&invocation)).is_err(),
                "accepted {invocation:?}"
            );
        }
    }
}
