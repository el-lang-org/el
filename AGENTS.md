# AGENTS.md

This repository contains the specification and bootstrap implementation of the
EL programming language. Keep the language small, predictable, and aligned with
the checked-in specifications. Do not invent behavior in the compiler when a
contract is missing or ambiguous.

## Sources of truth

Specification authority is split by subject:

- `GRAMMAR.md` is normative for lexical rules and concrete syntax.
- `TYPES.md` is normative for type formation, static semantics, and
  well-formedness.
- `IR.md` is normative for compiler representations, lowering boundaries, and
  verifier invariants.
- `DESIGN.md` owns the language vision, observable runtime semantics, compiler
  architecture, roadmap, open questions, and accepted decisions.
- `EXAMPLES.md` is illustrative. It must agree with the normative documents but
  does not override them.

When documents overlap, follow the authority for the relevant subject. If an
intentional language change is needed, update its authoritative document and
record an accepted decision in `DESIGN.md` before making implementation or
example changes that depend on it. Treat an accidental disagreement between a
specification and the compiler as a compiler bug, not permission to alter the
specification.

## Implementation direction

- Follow the milestones in `DESIGN.md` in order. Each milestone must finish
  with its stated exit test; prefer a narrow end-to-end slice over broad,
  disconnected scaffolding.
- Bootstrap in Rust. The user-facing binary and unified package tool is `el`.
- Use the pinned toolchain and dependency versions required by the design:
  `pest`/`pest_derive` 2.8.7, LLVM 22.1.0, and Inkwell 0.9.0 with
  `llvm22-1-prefer-dynamic`. Record exact Rust dependencies in the lockfile.
- Keep the compiler stages distinct:

  ```text
  PEG parse tree -> AST -> name resolution and type checking -> Typed AST
      -> Generic Core IR -> monomorphization -> Concrete Core IR
      -> LLVM IR -> object file -> native executable
  ```

- Do not use LLVM IR as the type checker or primary semantic representation.
- Preserve byte spans through the AST, Typed AST, and diagnostic-producing Core
  IR nodes. Derive display line and column positions from source text.
- Preserve EL's left-to-right, exactly-once evaluation order through lowering.
- Give compiler-local entities stable typed IDs. Do not use source spellings,
  arena addresses, pointer values, or hash-map iteration order as semantic
  identities or snapshot output.
- Verify representation invariants at every compiler boundary. Invalid
  compiler-generated IR is an internal error; invalid EL input receives a
  source-based diagnostic and must not panic the compiler.
- Keep parser recovery nodes out of name resolution and later phases.
- Isolate backend, platform, garbage-collector, and `unsafe` details behind
  small private interfaces. Do not expose Inkwell, LLVM, or Boehm GC types
  across Core IR or source-language APIs.
- Do not add deferred v2 features, alternate syntax, a JIT/interpreter, or
  extra CLI commands unless an accepted design decision brings them into scope.

## Code organization and style

- Prefer small crates and modules with one clear compiler-stage responsibility.
- Keep dependencies minimal. Reuse the standard library before adding a crate,
  and explain additions that affect the compiler architecture or distribution.
- Use ordinary Rust naming and formatting conventions. Keep public APIs narrow;
  document invariants and non-obvious safety requirements rather than restating
  the code.
- Represent expected failures with structured errors and diagnostics. Reserve
  `panic!`, `unreachable!`, and unchecked indexing for proven internal
  invariants, with the invariant made clear nearby.
- Make output deterministic. Sort data derived from unordered collections before
  diagnostics, snapshots, lockfiles, or generated text are emitted.
- Avoid drive-by rewrites and preserve unrelated uncommitted work.

## Tests

- Add a regression test for every parsing, typing, lowering, code-generation,
  runtime, or GC bug at the narrowest useful layer.
- Test accepted and rejected forms. A rejected program must fail with a useful
  diagnostic and never reach a later compiler phase as if it were valid.
- Prefer semantic assertions and deterministic AST/Typed AST/Core IR snapshots.
  Inspect LLVM text only when the invariant cannot be tested at the Core IR or
  execution level.
- Cover source spans and diagnostics for negative cases. Avoid assertions that
  depend on host paths, allocation addresses, hash order, or unstable LLVM
  spelling.
- For backend changes, test both development and optimized builds when
  semantics, overflow checks, evaluation order, or GC visibility may differ.
- Run the narrowest relevant test while iterating, then run the workspace checks
  before handing off a completed change. Once the Rust workspace exists, the
  standard checks are:

  ```sh
  cargo fmt --all -- --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  ```

  Run any additional milestone-specific conformance or end-to-end commands
  documented in the repository. If an external dependency such as LLVM, a
  linker, or Boehm GC is unavailable, report the exact check that could not run
  and still run all independent checks.

## Change checklist

Before completing a change:

1. Identify the authoritative specification and relevant roadmap milestone.
2. Keep the change within that contract, or update the contract and decision log
   first when the behavior change is intentional.
3. Add or update tests, including negative and span-sensitive coverage where
   applicable.
4. Run formatting, linting, tests, and the milestone exit test that are available.
5. Update examples only after the normative specification and implementation
   agree.
6. Summarize changed behavior, validation performed, and any checks that remain
   blocked by host dependencies.
