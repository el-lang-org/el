mod support;

use std::fs;
use std::process::{Command, Output};
use support::{TempDir, assert_golden};

fn run(arguments: &[&str], directory: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_el"))
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("run el")
}

#[test]
fn help_uses_stdout_and_status_zero() {
    let temp = TempDir::new();
    let output = run(&["--help"], temp.path());

    assert!(output.status.success());
    assert_golden("help.stdout", &output.stdout);
    assert!(output.stderr.is_empty());
}

#[test]
fn version_uses_stdout_and_status_zero() {
    let temp = TempDir::new();
    let output = run(&["--version"], temp.path());

    assert!(output.status.success());
    assert_golden("version.stdout", &output.stdout);
    assert!(output.stderr.is_empty());
}

#[test]
fn malformed_invocations_use_stderr_and_status_two() {
    let temp = TempDir::new();
    let cases: &[&[&str]] = &[
        &[],
        &["unknown"],
        &["check", "--locked", "--locked"],
        &["check", "--release"],
        &["build", "--release", "--release"],
        &["emit", "llvm-ir", "--module"],
    ];

    for arguments in cases {
        let output = run(arguments, temp.path());
        assert_eq!(output.status.code(), Some(2), "arguments: {arguments:?}");
        assert!(output.stdout.is_empty(), "arguments: {arguments:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Usage:"),
            "arguments: {arguments:?}"
        );
    }
}

#[test]
fn project_command_without_manifest_is_status_one() {
    let temp = TempDir::new();
    let output = run(&["check"], temp.path());

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("could not find el.toml"));
}

#[test]
fn known_unimplemented_command_is_status_one_without_panicking() {
    let temp = TempDir::new();
    fs::write(temp.path().join("el.toml"), "").expect("write manifest");
    let output = run(&["build", "--locked", "--release"], temp.path());

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not implemented in Milestone 0"));
    assert!(!stderr.contains("panicked"));
}
