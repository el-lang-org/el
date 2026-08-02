# EL v1 distribution and host support

Status: technical distribution contract

## Supported target

The initial and only claimed compilation target is `aarch64-apple-darwin`
(Darwin arm64). EL performs native host compilation only: the compiler host,
LLVM target, system linker, runtime, and executable target must all match.
Cross-compilation is not part of v1.

This target claim requires the complete workspace, native development/release,
managed runtime, Unicode corpus, and GC-stress commands in `BUILDING.md` to pass
on the distribution builder. Other targets are unclaimed even if frontend tests
or an incidental build happen to work.

## Distribution contents and prerequisites

A matching technical distribution contains:

- the `el` and `elc` executables built from Rust 1.97.1;
- LLVM 22.1.8 shared libraries required by Inkwell 0.9.0;
- the statically linked EL runtime and Boehm GC 8.2.12;
- Unicode 17.0.0 generated tables;
- `GRAMMAR.md`, `TYPES.md`, `IR.md`, `DESIGN.md`, and `EXAMPLES.md`;
- Boehm's license notice and the distribution's EL project license; and
- reproducibility metadata emitted beside native build output.

Building EL requires Xcode Command Line Tools providing Apple Clang, the system
linker, `ar`, `make`, `tar`, and a pinned Homebrew-compatible LLVM 22.1.8 prefix.
End users of a self-contained binary distribution must still have the Apple
system linker/compiler driver because v1 links generated objects through it.

## Installation

Until signed packages exist, build from the exact source revision:

```sh
export LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm
export PATH="$LLVM_SYS_221_PREFIX/bin:$PATH"
cargo build --release -p el-cli --features managed-runtime
install -m 0755 target/release/el /usr/local/bin/el
install -m 0755 target/release/elc /usr/local/bin/elc
el --version
elc --version
```

Distributions must include the project and third-party notices described in
`LICENSE_POLICY.md`.

## Reproducibility metadata

Each native project build writes `el-build-metadata.toml` containing the
compiler version, LLVM version and target triple, pointer width, private runtime
ABI revision, exact Boehm release/revision, and Unicode version. `el.lock`
records exact package source identities and Git revisions. Reproduction requires
the same compiler source/distribution, lockfile, target, LLVM, and linker inputs.

## Troubleshooting

- `No suitable version of LLVM`: confirm
  `$LLVM_SYS_221_PREFIX/bin/llvm-config --version` prints exactly `22.1.8` and
  `--shared-mode` prints `shared`.
- Missing `cc`, SDK, or linker: install/select Xcode Command Line Tools and check
  `xcrun --show-sdk-path` and `cc --version`.
- Boehm configure/build failure: verify the vendored archive checksum from
  `runtime/vendor/boehm-gc/README.md`; ensure `make`, `ar`, and a C11 compiler
  are available; remove only the affected Cargo target build directory before
  retrying.
- `el.lock` missing or stale under `--locked`: run the same command once without
  `--locked`, review the deterministic lockfile, then retry.
- A diagnostic path is absolute: report a compiler defect. V1 source
  diagnostics use package-relative paths and stable codes.
- A debug/release or GC-stress result differs: the target is not conforming; do
  not distribute it as supported.
