# Building EL

Milestone 0 supports frontend workspace development on the compiler host. It
does not yet claim a supported compilation target; that requires the compiler,
linker, runtime, GC stress, and conformance gates in later milestones.

## Rust workspace

Rust 1.97.1 is pinned by `rust-toolchain.toml`, including `rustfmt` and Clippy.
With the toolchain installed, run:

```sh
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

After Cargo has fetched any future locked dependencies, the workspace must also
build with `cargo build --workspace --locked --offline`. Milestone 0 has no
third-party Rust dependencies.

## LLVM prerequisite for later backend work

EL is pinned to LLVM 22.1.0 and Inkwell 0.9.0 with
`llvm22-1-prefer-dynamic`. Inkwell is intentionally not a Milestone 0
dependency, so frontend development does not require LLVM.

Backend environments must provide a matching `llvm-config` and shared LLVM
library. Set `LLVM_SYS_221_PREFIX` to the installation prefix containing
`bin/llvm-config`, and put that `bin` directory first on `PATH` when multiple
LLVM installations exist. Before enabling backend work, verify:

```sh
"${LLVM_SYS_221_PREFIX}/bin/llvm-config" --version
"${LLVM_SYS_221_PREFIX}/bin/llvm-config" --shared-mode
```

The first command must report `22.1.0`. The active Darwin arm64 workstation
does not currently expose `llvm-config`, so LLVM-dependent checks remain
blocked there. This does not block any Milestone 0 check.

## Boehm GC prerequisite for later runtime work

The exact upstream distribution archive is checked in at
`runtime/vendor/boehm-gc/gc-8.2.12.tar.gz`. Its provenance, checksum, source
revision, and license are recorded beside it. Milestone 0 does not build or
link the collector.

The initial host validation path for Darwin arm64 will extract the archive and
use its Autoconf build:

```sh
cd runtime/vendor/boehm-gc
tar -xzf gc-8.2.12.tar.gz
cd gc-8.2.12
./configure --disable-shared --enable-static --disable-cplusplus
make check
```

Runtime integration will encapsulate this native build behind `el-runtime` in
Milestone 5. Passing the commands above alone does not make Darwin arm64 a
supported EL target.

