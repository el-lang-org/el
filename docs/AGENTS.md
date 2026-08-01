# EL repository guidance

EL is a small programming language with a Rust bootstrap implementation. Keep
it predictable and aligned with the checked-in specifications. Do not invent
compiler behavior when a contract is missing or ambiguous.

## Before making a change

1. Identify the affected compiler stage and current roadmap milestone.
2. Read the relevant authoritative document below; read additional documents
   when a change crosses subject boundaries.
3. Keep the change within the contract. For an intentional language change,
   update the authoritative document and record an accepted decision in
   `DESIGN.md` before implementation or examples depend on it.
4. Prefer a narrow end-to-end milestone slice over disconnected scaffolding.
5. Preserve unrelated uncommitted work and avoid drive-by rewrites.

## Document routing

| Subject | Source to read |
| --- | --- |
| Lexical rules and concrete syntax | `GRAMMAR.md` |
| Type formation, static semantics, and well-formedness | `TYPES.md` |
| Compiler representations, lowering boundaries, and verifier invariants | `IR.md` |
| Vision, runtime semantics, architecture, roadmap, open questions, and accepted decisions | `DESIGN.md` |
| Toolchain, dependency, build, and native prerequisite instructions | `BUILDING.md` |
| Illustrative programs and user-facing examples | `EXAMPLES.md` plus the authoritative source for the behavior |

When documents overlap, the source with authority for that subject controls.
`EXAMPLES.md` never overrides a normative document. Treat an accidental
compiler/specification disagreement as a compiler bug.

## Repository-wide implementation rules

- Follow `DESIGN.md` milestones in order and complete each stated exit test.
- Keep compiler stages distinct; do not use LLVM IR as the type checker or
  primary semantic representation.
- Preserve byte spans through diagnostic-producing stages and preserve EL's
  left-to-right, exactly-once evaluation order through lowering.
- Use stable typed IDs for compiler-local entities. Never use source spellings,
  addresses, pointer values, or hash-map iteration order as semantic identities
  or snapshot output.
- Verify invariants at compiler boundaries. Invalid compiler-generated IR is an
  internal error; invalid EL input receives a structured source diagnostic and
  must not panic or pass into later phases. Keep parser recovery nodes out of
  name resolution and later phases.
- Isolate backend, platform, garbage-collector, and `unsafe` details behind
  small private interfaces. Do not expose backend or GC library types across
  Core IR or source-language APIs.
- Do not add deferred features, alternate syntax, a JIT/interpreter, or extra
  CLI commands without an accepted design decision.
- Prefer small, stage-focused crates and modules, minimal dependencies, narrow
  public APIs, and ordinary Rust conventions. Document invariants and
  non-obvious safety requirements rather than restating code.
- Represent expected failures with structured errors. Reserve panics,
  unreachable paths, and unchecked indexing for documented, proven internal
  invariants.
- Make generated output deterministic; sort unordered data before emitting
  diagnostics, snapshots, lockfiles, or generated text.

## Tests and handoff

- Add a regression test for every compiler or runtime bug at the narrowest
  useful layer. Test accepted and rejected forms, including source spans and
  diagnostics for negative cases.
- Prefer semantic assertions and deterministic AST, Typed AST, and Core IR
  snapshots. Inspect LLVM text only for invariants that cannot be tested earlier.
- For backend-sensitive semantics, test development and optimized builds.
- While iterating, run the narrowest relevant tests. Before handoff, follow
  `BUILDING.md`, run the workspace checks and applicable milestone exit test,
  and run all checks independent of any unavailable external dependency.
- Update examples only after the normative specification and implementation
  agree. Summarize changed behavior, validation, and precisely which checks were
  blocked by host dependencies.
