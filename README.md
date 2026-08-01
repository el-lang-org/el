# EL

An Elixir-syntax-inspired, statically typed programming language.

EL is a small, statically typed, garbage-collected systems programming language.
It compiles ahead of time to native executables through LLVM. The bootstrap
compiler is written in Rust.

```el
defmodule Main do
  def main() -> i32 do
    IO.println("Hello, world!")
    0
  end
end
```

## Why EL

EL is designed for programmers who want readable, high-level code without
giving up native executables or precise semantics. Its v1 highlights are:

- **Immutability without ceremony.** `=` introduces an immutable binding;
  mutation must be requested with `mut` and is easy to spot at each `:=`.
  Structs have immutable value semantics, including explicit field updates.
- **A compact but expressive type system.** EL includes type inference,
  inferred monomorphized generics, structs, tuples, lists, fixed arrays,
  slices, maps, function values, transparent aliases, and tagged tuples.
- **Structural unions (sum types).** Types such as
  `{:ok, value} | {:error, reason}` model recoverable failure as an ordinary
  value.
- **Protocols instead of inheritance.** Explicit, coherent implementations,
  associated types, constraints, and `@derive` provide reusable behavior with
  compile-time dispatch and no runtime protocol boxes.
- **Pattern matching and pipelines.** Destructure values with `match` and
  express data transformations with the Elixir-style pipeline operator `|>`.
- **Explicit text and binary types.** EL distinguishes UTF-8 `string`, Unicode
  scalar `rune`, byte-oriented `bytes`, arbitrary-length `bits`, and mutable
  construction with `Buffer`. String and grapheme behavior follows pinned
  Unicode 17.0.0 rules, while byte-aligned bitstrings support construction and
  pattern matching.
- **Native speed with managed memory.** LLVM ahead-of-time compilation produces
  standalone native executables, while garbage collection manages ordinary
  heap values. Contiguous arrays and slices provide bounds-checked O(1)
  indexing.
- **Deterministic resource cleanup.** Lexically scoped `defer` runs registered
  cleanup actions in LIFO order, so files and other scarce resources do not
  depend on garbage-collector timing.

EL deliberately leaves out exceptions, hidden numeric coercions, macros, async
runtimes, a JIT, and user-facing `unsafe` or FFI features in v1.
That smaller surface is intentional: the goal is a language whose behavior can
be understood and trusted from source to native executable. See
[docs/EXAMPLES.md](docs/EXAMPLES.md) for a guided tour of these features.

## Project status

The v1 implementation and conformance milestones are complete. The only
supported native target is currently Darwin arm64 (`aarch64-apple-darwin`), and
native builds require the pinned LLVM and runtime prerequisites described in
[docs/BUILDING.md](docs/BUILDING.md).

EL is available under the [MIT License](LICENSE). Distribution requirements for
the project and its vendored components are documented in
[docs/LICENSE_POLICY.md](docs/LICENSE_POLICY.md).

## Build the compiler

Rust 1.97.1 is pinned in `rust-toolchain.toml`. Frontend development and the
default workspace checks do not require LLVM:

```sh
cargo build --workspace
cargo test --workspace
```

To build the native compiler on the supported host, install LLVM 22.1.8 and set
its prefix before enabling the managed runtime:

```sh
export LLVM_SYS_221_PREFIX=/opt/homebrew/opt/llvm
export PATH="$LLVM_SYS_221_PREFIX/bin:$PATH"

cargo build --release -p el-cli --features el-driver/managed-runtime
./target/release/el --version
```

LLVM, Boehm GC, validation, and release-gate details are maintained in
[docs/BUILDING.md](docs/BUILDING.md). The technical distribution contract is in
[docs/V1_DISTRIBUTION.md](docs/V1_DISTRIBUTION.md).

## Try the example project

After building the compiler, use the checked-in Unicode report to exercise the
manifest-driven native toolchain:

```sh
export PATH="$PWD/target/release:$PATH"
cd examples/unicode_report

el check --locked
el build --locked
executable="$(find build -type f -path '*/debug/unicode_report' -perm -111 -print -quit)"
"$executable" "Café 🇸🇬 🇸🇬 🙂"
```

The complete v1 command surface is:

```text
el --help
el --version
el check [--locked]
el build [--release] [--locked]
el emit llvm-ir --module <Module>
```

EL v1 intentionally has no `run`, `test`, REPL, JIT, or cross-compilation
command. Native executables are written beneath
`build/<target-triple>/<debug-or-release>/` in the project directory.

## Language and compiler documentation

- [docs/DESIGN.md](docs/DESIGN.md) — vision, runtime semantics, architecture,
  CLI contract, and accepted decisions
- [docs/GRAMMAR.md](docs/GRAMMAR.md) — normative lexical and concrete syntax
- [docs/TYPES.md](docs/TYPES.md) — normative type formation and static semantics
- [docs/IR.md](docs/IR.md) — compiler representations, lowering boundaries, and verifier
  invariants
- [docs/EXAMPLES.md](docs/EXAMPLES.md) — illustrative EL programs and language tour
- [docs/V1_CONFORMANCE.md](docs/V1_CONFORMANCE.md) — specification-to-test traceability

When documents overlap, the normative document for the subject takes
precedence; examples do not override the language specification.

## Repository layout

```text
crates/                    Rust compiler and runtime crates
docs/                      Language and compiler specification documents
examples/unicode_report/  Multi-module EL v1 example project
runtime/                   Vendored GC and pinned Unicode inputs
tests/conformance/v1/      Accepted and rejected v1 fixtures
tools/                     Deterministic source-data generators
```

Changes should follow the milestone and compiler-stage boundaries in
[docs/DESIGN.md](docs/DESIGN.md) and the repository guidance in [docs/AGENTS.md](docs/AGENTS.md).
Run the checks required by [docs/BUILDING.md](docs/BUILDING.md) before handing off a
change.
