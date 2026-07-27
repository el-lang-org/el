#![cfg(feature = "llvm")]

use el_codegen::{emit_host_object_with_profile, link_host_objects};
use el_driver::{BuildProfile, analyze_source};
use el_ir::{executable_reachability_roots, monomorphize};
use el_span::SourceMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "el-native-e2e-test-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create native test directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove native test directory");
    }
}

fn compile_runtime_failure_stub(directory: &Path) -> PathBuf {
    let source = directory.join("runtime.c");
    let object = directory.join("runtime.o");
    fs::write(
        &source,
        "#include <stdint.h>\n#include <stdlib.h>\nvoid __el_runtime_fail(uint32_t category, uint32_t file, uint64_t start, uint64_t end) {\n  (void)file; (void)start; (void)end;\n  _Exit((int)(100u + category));\n}\n",
    )
    .expect("write runtime test support");
    let compiler = std::env::var_os("CC").unwrap_or_else(|| OsString::from("cc"));
    let output = Command::new(&compiler)
        .arg("-std=c11")
        .arg("-c")
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("start host C compiler");
    assert!(
        output.status.success(),
        "runtime support compilation failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    object
}

fn build_and_run(
    directory: &Path,
    runtime: &Path,
    name: &str,
    source: &str,
    profile: BuildProfile,
) -> ExitStatus {
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.el", source);
    let generic = analyze_source(file, source).expect("source reaches Generic Core");
    let roots = executable_reachability_roots(&generic).expect("select executable entry");
    let concrete = monomorphize(&generic, &roots).expect("monomorphize executable");
    let object = directory.join(format!("{name}.o"));
    let executable = directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));

    emit_host_object_with_profile(&concrete, &object, profile.codegen_profile())
        .expect("emit host object");
    link_host_objects(&[object.as_path(), runtime], &executable).expect("link native executable");
    Command::new(executable)
        .status()
        .expect("run native executable")
}

#[test]
fn development_and_release_preserve_arithmetic_exit_semantics() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let arithmetic = "defmodule Main do\n  def multiply(value: i32, factor: i32) -> i32 do\n    value * factor\n  end\n  def main() -> i32 do\n    multiply(6, 7)\n  end\nend\n";
    let overflow = "defmodule Main do\n  def main() -> i32 do\n    2147483647 + 1\n  end\nend\n";

    for (label, profile) in [
        ("debug", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-arithmetic"),
                arithmetic,
                profile,
            )
            .code(),
            Some(42)
        );
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-overflow"),
                overflow,
                profile,
            )
            .code(),
            Some(101),
            "integer_overflow must retain runtime category 1 in both profiles"
        );
    }
}

#[test]
fn native_control_flow_preserves_loops_short_circuiting_and_early_returns() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let factorial = "defmodule Main do\n  def main() -> i32 do\n    mut n: i32 = 5\n    mut result: i32 = 1\n    while n > 1 do\n      result := result * n\n      n := n - 1\n    end\n    if false and 1 / 0 == 0 do\n      return 1\n    end\n    if true or 1 / 0 == 0 do\n      return result\n    end\n    0\n  end\nend\n";

    for (label, profile) in [
        ("debug", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-control-flow"),
                factorial,
                profile,
            )
            .code(),
            Some(120),
            "short-circuited division must not run and nested return must preserve factorial"
        );
    }
}

#[test]
fn native_tagged_union_match_preserves_discriminants_and_payloads() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let tagged = "defmodule Main do\n  @type Parsed = {:ok, i32} | :error\n  def parse(valid: bool) -> Parsed do\n    if valid do\n      {:ok, 40}\n    else\n      :error\n    end\n  end\n  def payload(value: {:ok, i32}) -> i32 do\n    match value do\n      {:ok, number} -> number\n    end\n  end\n  def atom_value(value: :error) -> i32 do\n    match value do\n      :error -> 2\n    end\n  end\n  def unwrap(value: Parsed) -> i32 do\n    match value do\n      ok: {:ok, i32} -> payload(ok)\n      _ -> atom_value(:error)\n    end\n  end\n  def main() -> i32 do\n    unwrap(parse(true)) + unwrap(parse(false))\n  end\nend\n";

    for (label, profile) in [
        ("debug", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-tagged-union"),
                tagged,
                profile,
            )
            .code(),
            Some(42),
            "union tags and tagged-tuple payloads must survive calls and exhaustive matches"
        );
    }
}
