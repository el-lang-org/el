#![cfg(feature = "boehm")]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn unicode_17_grapheme_break_test_matches_every_boundary() {
    let directory = temporary_directory();
    let executable = directory.join("grapheme-conformance");
    let compiler = std::env::var_os("CC").unwrap_or_else(|| OsString::from("cc"));
    let runtime = Path::new(env!("EL_RUNTIME_NATIVE_DIR")).join("libel_runtime.a");
    let collector = Path::new(env!("EL_RUNTIME_BOEHM_LIB_DIR")).join("libgc.a");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/grapheme_conformance.c");

    let compilation = Command::new(compiler)
        .arg("-std=c11")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-Werror")
        .arg(source)
        .arg(runtime)
        .arg(collector)
        .arg("-lpthread")
        .arg("-o")
        .arg(&executable)
        .output()
        .expect("start native compiler");
    assert!(
        compilation.status.success(),
        "grapheme fixture compilation failed: {}{}",
        String::from_utf8_lossy(&compilation.stdout),
        String::from_utf8_lossy(&compilation.stderr)
    );

    let execution = Command::new(&executable)
        .output()
        .expect("run grapheme fixture");
    assert!(
        execution.status.success(),
        "grapheme conformance failed: {}{}",
        String::from_utf8_lossy(&execution.stdout),
        String::from_utf8_lossy(&execution.stderr)
    );
    std::fs::remove_dir_all(directory).expect("remove grapheme fixture directory");
}

fn temporary_directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "el-runtime-grapheme-conformance-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir(&path).expect("create grapheme fixture directory");
    path
}
