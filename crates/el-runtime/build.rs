use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn run(command: &mut Command, description: &str) {
    let status = command.status().unwrap_or_else(|error| {
        panic!("could not {description}: {error}");
    });
    assert!(status.success(), "could not {description}: {status}");
}

fn main() {
    println!("cargo:rerun-if-changed=src/native/runtime.c");
    println!("cargo:rerun-if-changed=src/native/runtime.h");
    println!("cargo:rerun-if-changed=src/native/unicode_grapheme_data.inc");
    println!("cargo:rerun-if-changed=../../runtime/vendor/boehm-gc/gc-8.2.12.tar.gz");

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("output dir"));
    generate_lowercase_table(&output);

    if env::var_os("CARGO_FEATURE_BOEHM").is_none() {
        return;
    }

    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let source_root = output.join("boehm-source");
    let source = source_root.join("gc-8.2.12");
    let install = output.join("boehm-install");
    let archive = manifest.join("../../runtime/vendor/boehm-gc/gc-8.2.12.tar.gz");

    if !install.join("lib/libgc.a").is_file() {
        let _ = fs::remove_dir_all(&source_root);
        fs::create_dir_all(&source_root).expect("create Boehm source directory");
        run(
            Command::new("tar")
                .args([OsString::from("-xzf"), archive.into_os_string()])
                .arg("-C")
                .arg(&source_root),
            "extract vendored Boehm GC",
        );
        run(
            Command::new("./configure")
                .current_dir(&source)
                .arg("--disable-shared")
                .arg("--enable-static")
                .arg("--disable-cplusplus")
                .arg("--disable-docs")
                .arg(format!("--prefix={}", install.display())),
            "configure vendored Boehm GC",
        );
        run(
            Command::new("make").current_dir(&source).arg("-j2"),
            "build vendored Boehm GC",
        );
        run(
            Command::new("make").current_dir(&source).arg("install"),
            "install vendored Boehm GC",
        );
    }

    compile_runtime(&manifest, &output, &install);
    println!("cargo:rustc-link-search=native={}", output.display());
    println!(
        "cargo:rustc-link-search=native={}",
        install.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=el_runtime");
    println!("cargo:rustc-link-lib=static=gc");
    println!("cargo:rustc-env=EL_RUNTIME_NATIVE_DIR={}", output.display());
    println!(
        "cargo:rustc-env=EL_RUNTIME_BOEHM_LIB_DIR={}",
        install.join("lib").display()
    );
}

fn generate_lowercase_table(output: &Path) {
    let mut table = String::from("/* generated from Rust's Unicode lowercase tables */\n");
    for codepoint in 0..=0x10ffff {
        let Some(character) = char::from_u32(codepoint) else {
            continue;
        };
        let lowered = character.to_lowercase().collect::<String>();
        if lowered == character.to_string() {
            continue;
        }
        let bytes = lowered.as_bytes();
        table.push_str(&format!(
            "{{0x{codepoint:x}u, {{{}",
            bytes
                .iter()
                .map(|byte| format!("0x{byte:02x}"))
                .collect::<Vec<_>>()
                .join(",")
        ));
        for _ in bytes.len()..12 {
            table.push_str(",0");
        }
        table.push_str(&format!("}}, {}}},\n", bytes.len()));
    }
    fs::write(output.join("unicode_lowercase_data.inc"), table)
        .expect("write generated Unicode lowercase table");
}

fn compile_runtime(manifest: &Path, output: &Path, install: &Path) {
    let compiler = env::var_os("CC").unwrap_or_else(|| OsString::from("cc"));
    let object = output.join("el_runtime.o");
    let library = output.join("libel_runtime.a");
    let mut compile = Command::new(compiler);
    compile
        .arg("-std=c11")
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-Werror")
        .arg("-I")
        .arg(install.join("include"))
        .arg("-I")
        .arg(output)
        .arg("-c")
        .arg(manifest.join("src/native/runtime.c"))
        .arg("-o")
        .arg(&object);
    if env::var_os("CARGO_FEATURE_GC_STRESS_TEST").is_some() {
        compile.arg("-DEL_GC_STRESS_TEST=1");
    }
    if env::var_os("CARGO_FEATURE_ALLOCATION_FAILURE_TEST").is_some() {
        compile.arg("-DEL_RUNTIME_ALLOCATION_FAILURE_TEST=1");
    }
    run(&mut compile, "compile the EL runtime wrapper");
    run(
        Command::new("ar").arg("crs").arg(library).arg(object),
        "archive the EL runtime wrapper",
    );
}
