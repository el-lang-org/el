#![cfg(all(feature = "boehm", not(feature = "allocation-failure-test")))]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn scanned_graph_atomic_storage_and_registered_globals_survive_collection() {
    let directory = temporary_directory();
    let executable = directory.join("native-gc");
    let compiler = std::env::var_os("CC").unwrap_or_else(|| OsString::from("cc"));
    let runtime = Path::new(env!("EL_RUNTIME_NATIVE_DIR")).join("libel_runtime.a");
    let collector = Path::new(env!("EL_RUNTIME_BOEHM_LIB_DIR")).join("libgc.a");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/native_gc.c");

    let mut command = Command::new(compiler);
    command.arg("-std=c11");
    command.arg(if cfg!(debug_assertions) { "-O0" } else { "-O3" });
    let compilation = command
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
        "native GC fixture compilation failed: {}{}",
        String::from_utf8_lossy(&compilation.stdout),
        String::from_utf8_lossy(&compilation.stderr)
    );

    let execution = Command::new(&executable)
        .output()
        .expect("run native GC fixture");
    assert!(
        execution.status.success(),
        "native GC fixture failed: {}{}",
        String::from_utf8_lossy(&execution.stdout),
        String::from_utf8_lossy(&execution.stderr)
    );

    std::fs::remove_dir_all(directory).expect("remove native GC fixture directory");
}

fn temporary_directory() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "el-runtime-native-gc-{}-{}",
        std::process::id(),
        if cfg!(debug_assertions) {
            "development"
        } else {
            "release"
        }
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir(&path).expect("create native GC fixture directory");
    path
}
