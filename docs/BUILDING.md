# Building and validating EL

EL v1 claims only the Darwin arm64 native host target described in
`V1_DISTRIBUTION.md`. A builder must pass every command in this document before
shipping that target. Other hosts remain useful for frontend development but
are not supported compilation targets.

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

EL is pinned to LLVM 22.1.8 and Inkwell 0.9.0 with
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

The first command must report `22.1.8`. The active Darwin arm64 workstation has
the pinned Homebrew LLVM at `/opt/homebrew/opt/llvm`. Set `LLVM_SYS_221_PREFIX`
to that prefix for local backend checks.

Enable the backend with `el-codegen`'s `llvm` feature. Managed executable builds
also select `managed-runtime`; this makes the generated process entry initialize
the collector and exposes the linker path that adds the matching runtime and GC
archives:

```sh
cargo test -p el-codegen --features llvm,managed-runtime
```

A host without LLVM can still type-check the private Inkwell API boundary
without linking or executing it:

```sh
cargo check -p el-codegen --features llvm-api-check --tests
```

The `llvm-api-check` feature is a contributor check only; it does not produce a
usable compiler backend.

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

Milestone 5 now encapsulates this native build behind `el-runtime`. Build the
pinned collector and private runtime archive with:

```sh
cargo test -p el-runtime --features boehm
cargo test -p el-runtime --features gc-stress-test
cargo test --release -p el-runtime --features gc-stress-test
LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm \
  cargo test -p el-driver --features gc-stress-test --test native
LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm \
  cargo test -p el-driver --features allocation-failure-test --test native
```

The default workspace build remains frontend-friendly and does not require a C
toolchain. `gc-stress-test` compiles the runtime so each managed allocation first
requests a full collection; it is a compiler/runtime conformance mode, not an EL
source option. `allocation-failure-test` is a separate conformance build that
forces managed allocation failure; do not combine it with `gc-stress-test`.
Passing these commands alone does not make Darwin arm64 a supported EL target.

## V1 release gate

Set the pinned LLVM prefix, then run all independent, backend, runtime, profile,
and stress checks:

```sh
export LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm
export PATH="$LLVM_SYS_221_PREFIX/bin:$PATH"

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --release --workspace
cargo test -p el-codegen --features llvm,managed-runtime
cargo test -p el-runtime --features gc-stress-test
cargo test --release -p el-runtime --features gc-stress-test
cargo test -p el-driver --features gc-stress-test --test native
cargo test --release -p el-driver --features gc-stress-test --test native
cargo build --workspace --locked --offline
```

Verify `/opt/homebrew/opt/llvm/bin/llvm-config --version` is `22.1.8` before the
backend commands. Distribution license requirements are documented in
`LICENSE_POLICY.md`.

## Unicode data regeneration

Unicode 17.0.0 source data and the official grapheme conformance corpus are
checked in under `runtime/unicode/17.0.0`. Regenerate the deterministic private C
range tables and conformance fixture after verifying those pinned inputs with:

```sh
python3 tools/generate_unicode_grapheme_tables.py
```

Normal builds consume the checked-in generated files and do not require Python,
network access, a host Unicode library, or locale data.
