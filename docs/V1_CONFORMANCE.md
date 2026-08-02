# EL v1 conformance and release audit

Status: stabilization contract

This index makes the v1 proof traceable without moving narrow tests away from
the compiler stage that owns them. `GRAMMAR.md`, `TYPES.md`, `IR.md`, and
`DESIGN.md` remain authoritative; this file records where their contracts are
tested.

## Traceability

| Contract | Normative sections | Primary proof |
| --- | --- | --- |
| UTF-8, tokens, literals, newlines, productions, validation, recovery | `GRAMMAR.md` §§2–5 | `el-parser/tests/conformance.rs`, `details.rs`, and AST snapshots |
| Namespaces, visibility, aliases, layout cycles, protocol coherence | `TYPES.md` §§2–6, §11 | `el-resolve/tests/resolution.rs` |
| Inference, composites, unions, functions, patterns, failures, standard protocols | `TYPES.md` §§3–10 | `el-types/tests/checking.rs` |
| Typed AST, Generic/Concrete Core, lowering, evaluation order, cleanup, roots, monomorphization | `IR.md` §§4–14 | `el-ir/tests/lowering.rs` and deterministic Core snapshots |
| LLVM layout, verification, optimization profiles, runtime origins | `IR.md` §§12–15; `DESIGN.md` §§7–9 | `el-codegen` unit tests and `el-driver/tests/native.rs` |
| Private allocation ABI, failures, Unicode 17, I/O, process/path behavior | `DESIGN.md` §§6.8, 7, 9, 13 | `el-runtime` unit/native tests, grapheme corpus, and managed native driver tests |
| Strict manifest, packages, lockfiles, path mapping | `DESIGN.md` §§10–11 | `el-driver::package` tests and driver project tests |
| Commands, options, streams, and statuses | `DESIGN.md` §14 | `el-cli` parser tests, process tests, and golden output |
| Reference accepted and rejected programs | `EXAMPLES.md` §§1–13, 21 | `tests/conformance/v1` via `el-driver/tests/v1_conformance.rs` |

Every rejected repository-level fixture records `phase`, `code`, and `spec` in
its first-line metadata. Every accepted fixture has a concrete `Main.main` and
must reach verified Generic Core IR. Native semantics are run in development
and release profiles by `el-driver/tests/native.rs`; managed-runtime and GC
stress tests add the runtime archive and collection-at-every-allocation mode.

## Frozen v1 distribution values

- Manifest syntax revision: 1 (`el_driver::MANIFEST_FORMAT_VERSION`).
- Lockfile syntax revision: 1 (`el_driver::LOCKFILE_FORMAT_VERSION`).
- Private compiler/runtime ABI revision: 5 (`el_runtime::PRIVATE_ABI_VERSION`
  and `EL_PRIVATE_ABI_VERSION`).
- LLVM: 22.1.8.
- Boehm GC: 8.2.12, source revision
  `4fab5386df64466b2b61fc7209bef033cad1e6cc`.
- Unicode data: 17.0.0; untailored UAX #29 revision 47.

The runtime ABI is private to one matching compiler distribution. EL v1 makes
no object-file, symbol-name, native aggregate-layout, or calling-convention
compatibility promise between distributions.

## Scope audit

The CLI rejects `run`, `test`, `fmt`, `repl`, target/output selection, and global
verbosity options. Grammar fixtures reject closures and nested field updates.
The source language has no foreign declaration form, concurrency construct,
exception surface, null value, JIT, interpreter, or cross-compilation option.
Backend and collector types remain behind private Rust and C interfaces.

## Release gate

Run the commands in `BUILDING.md` on every target listed as supported in
`V1_DISTRIBUTION.md`. A target is removed from that list if any required command
cannot run or any accepted/rejected fixture differs. Distributions must include
the notices required by `LICENSE_POLICY.md`.
