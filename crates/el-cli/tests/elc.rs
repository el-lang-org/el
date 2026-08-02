mod support;

use std::fs;
use std::process::{Command, Output};
use support::{TempDir, assert_golden};

fn run(arguments: &[&str], directory: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_elc"))
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("run elc")
}

#[test]
fn help_and_version_use_stdout_and_status_zero() {
    let temp = TempDir::new();
    for (arguments, golden) in [
        (["--help"], "elc-help.stdout"),
        (["--version"], "elc-version.stdout"),
    ] {
        let output = run(&arguments, temp.path());
        assert!(output.status.success());
        assert_golden(golden, &output.stdout);
        assert!(output.stderr.is_empty());
    }
}

#[test]
fn malformed_invocations_use_stderr_and_status_two() {
    let temp = TempDir::new();
    for arguments in [vec![], vec!["source.el"], vec!["one.ell", "two.ell"]] {
        let output = run(&arguments, temp.path());
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Usage:"));
    }
}

#[test]
fn source_failures_use_stderr_and_status_one() {
    let temp = TempDir::new();
    fs::write(temp.path().join("bad.ell"), "defmodule Main do\n").expect("write source");

    let output = run(&["bad.ell"], temp.path());

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("compilation failed"));
    assert!(!temp.path().join("bad").exists());
}

#[test]
fn refuses_to_overwrite_the_source_through_an_equivalent_path() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def main() -> i32 do\n    0\n  end\nend\n";
    fs::write(temp.path().join("safe.ell"), source).expect("write source");

    let output = run(&["-o", "./safe.ell", "safe.ell"], temp.path());

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("would overwrite source file"));
    assert_eq!(
        fs::read_to_string(temp.path().join("safe.ell")).expect("read preserved source"),
        source
    );
}

#[cfg(feature = "llvm")]
#[test]
fn compiles_and_runs_one_source_file_without_project_artifacts() {
    let temp = TempDir::new();
    fs::write(
        temp.path().join("answer.ell"),
        "defmodule Main do\n  def main() -> i32 do\n    42\n  end\nend\n",
    )
    .expect("write source");

    let compilation = run(&["answer.ell"], temp.path());

    assert!(compilation.status.success());
    assert!(compilation.stdout.is_empty());
    assert!(compilation.stderr.is_empty());
    assert!(!temp.path().join("el.toml").exists());
    assert!(!temp.path().join("build").exists());
    assert!(
        fs::read_dir(temp.path())
            .expect("read output directory")
            .all(|entry| !entry
                .expect("read output entry")
                .file_name()
                .to_string_lossy()
                .contains(".elc-"))
    );

    let execution = Command::new(temp.path().join("answer"))
        .output()
        .expect("run compiled program");
    assert_eq!(execution.status.code(), Some(42));
}
