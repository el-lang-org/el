# EL Implementation Plan

Status: implementation-ready planning document

Language: **EL**

Last updated: 2026-07-28

> The filename `IMPLEMENTAION_PLAN.md` preserves the spelling requested when
> this plan was created.

## 1. Purpose

This document turns the accepted EL v1 specifications into an ordered execution
plan for the Rust bootstrap compiler, runtime, standard library, and unified
`el` tool. It is a work plan, not a new source of language semantics.

The repository currently contains specifications and examples only. There is no
Rust workspace or implementation yet, so work begins at Milestone 0. Each
milestone must finish with a narrow end-to-end result, its stated exit test, and
all independent workspace checks passing.

The plan optimizes for:

- early executable vertical slices;
- distinct, verifiable compiler stages;
- deterministic output and stable compiler-local identities;
- source-based diagnostics from the first parser slice;
- regression tests at the narrowest useful layer; and
- strict control of v1 scope.

## 2. Authority and change control

Implementation work must follow the repository's subject-specific authorities:

| Subject | Authority | Implementation consequence |
| --- | --- | --- |
| Lexical rules and concrete syntax | [`GRAMMAR.md`](GRAMMAR.md) | The checked-in `pest` grammar implements this contract and cannot silently extend it. |
| Types and static well-formedness | [`TYPES.md`](TYPES.md) | Resolver and type-checker behavior must follow its dependency order and rejection rules. |
| Compiler representations | [`IR.md`](IR.md) | AST, Typed AST, Generic Core IR, Concrete Core IR, and their verifiers remain distinct. |
| Runtime semantics, architecture, roadmap, CLI, and decisions | [`DESIGN.md`](DESIGN.md) | Observable behavior, milestone order, and the complete v1 command surface come from this document. |
| Illustrative programs | [`EXAMPLES.md`](EXAMPLES.md) | Examples become tests only after they agree with all normative documents. |

If implementation reveals a missing or ambiguous contract, stop work on the
affected behavior. Update the authoritative specification and add an accepted
decision to `DESIGN.md` before implementing the behavior. Treat accidental
compiler/specification disagreements as compiler bugs.

## 3. Fixed implementation constraints

The following choices are already accepted and are not reopened by this plan:

- Bootstrap language: Rust.
- User-facing compiler and package tool: `el`.
- Parser: PEG using exactly `pest` 2.8.7 and `pest_derive` 2.8.7.
- Backend: LLVM 22.1.8 through exactly Inkwell 0.9.0 with
  `llvm22-1-prefer-dynamic`.
- Compilation: native AOT for the compiler host only.
- Linking: initially invoke the installed platform C compiler driver.
- Memory: vendored, statically linked Boehm GC behind a private runtime ABI.
- Pipeline:

  ```text
  PEG parse tree -> AST -> name resolution and type checking -> Typed AST
      -> Generic Core IR -> monomorphization -> Concrete Core IR
      -> LLVM IR -> object file -> native executable
  ```

- No interpreter, JIT, concurrency, user-facing FFI, exceptions, macros,
  closures, cross-compilation, `el run`, or `el test` in v1.

## 4. Decisions and prerequisites before coding

Milestone 0 must resolve these implementation details without inventing source
language behavior:

1. **Rust toolchain pin.** Select and record an exact toolchain in
   `rust-toolchain.toml`. The current workstation has Rust 1.97.1, but that is
   an environment observation, not yet the project pin.
2. **Boehm GC pin.** Select an exact upstream release and source revision,
   record its license and checksum, and document the supported host build path.
3. **LLVM discovery.** Document how LLVM 22.1.8 is located for local builds and
   CI, including the environment expected by `llvm-sys`.
4. **Supported bootstrap host.** Start with the active host, but do not claim a
   supported target until compiler, linker, runtime, GC stress, and conformance
   tests pass there.
5. **Snapshot convention.** Prefer checked-in deterministic golden files and a
   small repository-owned comparison helper. Add a snapshot dependency only if
   it materially improves diagnostics or reviewability.
6. **Manifest parser.** Evaluate a narrowly scoped TOML dependency when full
   manifest parsing begins. Milestone 0 may implement only the minimal discovery
   needed by its exit test.

Version selection for Rust or Boehm GC is an implementation/release choice. Any
change that affects observable language behavior still requires the normal
specification decision process.

## 5. Proposed repository structure

Use stage-oriented crates to make the specified compiler boundaries enforceable
by Cargo. The split follows durable representations and native boundaries, not
every individual compiler pass:

```text
.
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── crates/
│   ├── el-span/               # FileId, Span, SourceMap, diagnostic primitives
│   ├── el-ast/                # parser-independent source AST and debug form
│   ├── el-parser/             # pest grammar, AST construction, validation
│   ├── el-resolve/            # namespaces, visibility, Resolved AST, stable IDs
│   ├── el-types/              # canonical types, checking, verified Typed AST
│   ├── el-ir/                 # Generic/Concrete Core IR, lowering, verify, mono
│   ├── el-codegen/            # LLVM lowering, objects, host-linker integration
│   ├── el-runtime/            # private runtime ABI, GC, and platform support
│   ├── el-driver/             # project loading and check/build/emit pipeline
│   └── el-cli/                # thin user-facing `el` binary
├── runtime/
│   └── vendor/                # pinned native sources and notices
├── stdlib/                    # EL standard-library modules and intrinsics
├── tests/
│   ├── fixtures/              # accepted/rejected EL projects and programs
│   ├── snapshots/             # AST, Typed AST, Core IR, and diagnostics
│   ├── conformance/           # tests derived directly from specifications
│   └── e2e/                   # compile/link/execute cases
└── examples/                  # runnable milestone examples
```

There is intentionally no `el-lexer` crate. `pest` is a scannerless PEG parser,
and the lexical contract belongs in the same checked-in grammar as the concrete
syntax. A separate lexer would risk duplicating lexical authority and making
significant-newline behavior inconsistent. Lexical helpers may be private
modules inside `el-parser`.

Use `stdlib/` rather than `std/` so repository paths and documentation do not
confuse the EL standard library with Rust's `std` crate.

### 5.1 Crate ownership

- **`el-span`** owns dependency-light source primitives shared by all compiler
  stages: `FileId`, half-open byte `Span`, immutable source storage, line/column
  mapping, and the common span-bearing diagnostic record. Rendering policy stays
  at the driver/CLI boundary.
- **`el-ast`** owns only source-oriented AST nodes, literal spellings/decoded
  values, recovery markers, spans, and deterministic AST debug output. It has no
  resolved identity, canonical type, or backend knowledge.
- **`el-parser`** owns the checked-in `.pest` grammar, `pest` integration,
  parse-tree-to-AST conversion, grammar validation, and syntax recovery. No
  `pest` type crosses its public boundary.
- **`el-resolve`** owns declaration collection, module and lexical namespaces,
  visibility, package ownership, stable declaration/symbol IDs, and a Resolved
  AST. It resolves written type paths to declarations but does not perform type
  inference or canonical type formation.
- **`el-types`** owns canonical type storage, aliases, inference, constraints,
  protocol coherence, union normalization, pattern analysis, finite-layout
  well-formedness, the Typed AST, and its verifier.
- **`el-ir`** owns the shared Generic/Concrete Core IR CFG, Typed AST lowering,
  representation verifiers, deterministic debug forms, target-independent
  layout classification, and monomorphization. If monomorphization later becomes
  independently substantial, it may move to `el-mono` without changing the
  source-language pipeline.
- **`el-codegen`** is the only compiler crate allowed to depend on Inkwell or
  LLVM. It owns target layout completion, LLVM lowering and verification, object
  emission, debug locations, and host-linker invocation.
- **`el-runtime`** owns the versioned private runtime ABI, native allocation
  wrappers, Boehm GC integration, failure reporting, process and platform I/O,
  and its native build support. Compiler stages see only its narrow ABI contract.
- **`el-driver`** owns manifest discovery/loading, package graphs, lockfiles,
  source discovery, pipeline orchestration, build profiles, output locations,
  and the implementations of `check`, `build`, and `emit`.
- **`el-cli`** owns strict argument parsing, help/version text, exit-status
  mapping, and final stdout/stderr presentation. It contains no parser, type
  checker, or backend logic.

These crates are compiler-internal components, not a stable public Rust API.
Avoid broad `pub` surfaces; expose only the data needed by the next stage.

### 5.2 Dependency direction

The dependency graph must remain acyclic and flow toward later compiler stages:

```text
el-cli     -> el-driver
el-driver  -> el-parser, el-resolve, el-types, el-ir, el-codegen
el-parser  -> el-ast, el-span
el-ast     -> el-span
el-resolve -> el-ast, el-span
el-types   -> el-resolve, el-ast, el-span
el-ir      -> el-types, el-span
el-codegen -> el-ir, el-span, el-runtime ABI
el-runtime -> native platform and vendored GC only
```

All span-bearing crates may depend directly on `el-span`; omitted arrows above
keep the diagram readable. `el-resolve` must not depend on `el-types`, which
allows `el-types` to consume the Resolved AST without a cycle. `el-codegen` is
the sole LLVM boundary, and `el-runtime` never depends on an AST, type-system,
or IR crate.

Create crates when their first vertical slice needs them rather than landing
empty scaffolding for every future milestone. The structure is a starting
layout, not a reason to freeze poor internal APIs.

## 6. Cross-cutting compiler foundations

These foundations are established early and reused by every milestone.

### 6.1 Source ownership and spans

- `FileId` indexes immutable UTF-8 source text.
- `Span` is a half-open byte range tied to one `FileId`.
- Line and column values are derived lazily from source text, never stored as
  semantic identity.
- AST, Typed AST, and diagnostic-producing Core IR nodes retain the narrowest
  useful source span.
- Generated nodes retain a generated-kind marker and origin chain.
- Tests cover multibyte UTF-8, LF, CRLF, EOF, zero-width recovery locations,
  and package-relative source paths.

### 6.2 Stable identities and determinism

- Use typed IDs for modules, declarations, symbols, types, implementations,
  functions, blocks, values, and slots.
- Source spelling is display data, not semantic identity.
- Arena addresses, pointer values, and hash-map iteration order never enter
  snapshots or generated names.
- Sort unordered data before diagnostics, debug output, lockfiles, or generated
  text are emitted.
- Each representation has a deterministic human-readable debug form with local,
  deterministic ID numbering.

### 6.3 Diagnostics

- Represent errors structurally: code, severity, primary span, message,
  secondary labels, notes, and optional help.
- Parsing, validation, resolution, and typing may recover enough to report
  independent errors, but a module containing any error never lowers.
- Invalid source receives diagnostics; invalid compiler-generated IR is an
  internal error.
- Diagnostic snapshots normalize package roots and exclude host paths.
- Exit status and stdout/stderr routing are tested independently from rendered
  prose.

### 6.4 Verification boundaries

Run a verifier after each of these transitions:

```text
parser -> conforming AST
resolver/type checker -> Typed AST
lowering -> Generic Core IR
monomorphization -> Concrete Core IR
LLVM lowering -> verified LLVM module
```

Verifier failures are compiler defects. Tests deliberately construct malformed
representations and prove each invariant is rejected.

### 6.5 Evaluation order

All lowering APIs make left-to-right, exactly-once evaluation explicit. Use
ordered operand lists and emitted temporary `ValueId`s rather than recursively
handing unordered expression trees to the backend. Give short-circuiting,
pipeline insertion, map duplicate replacement, field update, matching, and
deferred cleanup dedicated tests.

## 7. Milestone execution plan

Milestones execute in the order defined by `DESIGN.md`. Later work may be
researched early, but it must not add production dependencies or behavior to an
earlier milestone unless that milestone needs it for its exit test.

### Milestone 0 — Project skeleton

**Objective:** establish a reproducible Rust workspace and a conforming CLI
shell without implementing the language.

**Deliverables**

- [x] Pin the exact Rust toolchain.
- [x] Create the workspace plus `el-cli` (with binary name `el`), `el-driver`,
  `el-span`, and `el-runtime`; add the remaining stage crates with their first
  vertical slice instead of landing empty future scaffolding.
- [x] Pin direct Rust dependencies exactly in `Cargo.lock`; start with `pest` and
  `pest_derive` only when the parser crate is introduced.
- [x] Record LLVM 22.1.8 and Inkwell 0.9.0 requirements without forcing ordinary
  frontend-only tests to link LLVM.
- [x] Select, document, and prepare the exact vendored Boehm GC release without
  exposing it through source-language APIs.
- [x] Implement exact `el --help` and `el --version` stream/exit behavior.
- [x] Build a strict command parser whose known-but-unimplemented project commands
  fail as ordinary tool errors, not panics.
- [x] Discover the nearest ancestor `el.toml` for project commands.
- [x] Add structured error plumbing, temporary-directory test support, golden-file
  conventions, and CI jobs for format, lint, unit tests, and license checks.
- [x] Document local prerequisites and keep frontend checks runnable without LLVM
  or Boehm GC.

**Tests**

- [x] Help and version output, streams, and status 0.
- [x] Unknown commands, duplicate options, missing values, and status 2.
- [x] Manifest discovery from the project root, descendants, and missing-manifest
  paths.
- [x] Workspace builds with no network access after dependencies are fetched.

- [ ] **Exit gate:** `el --help` runs and CI builds the workspace. All standard Rust
  checks pass.

### Milestone 1 — Parser and AST

**Objective:** implement the complete normative grammar and produce a clean,
spanned AST that later stages can trust.

**Deliverables**

1. [x] Implement source decoding rules: UTF-8, BOM rejection, LF/CRLF handling,
   horizontal whitespace, comments, identifiers, keywords, and attributes.
2. [x] Implement literal tokens and decoding for integers, floats, strings, runes,
   atoms, booleans, and `unit`, preserving spelling and decoded value.
3. [x] Encode significant-newline behavior explicitly. Test delimiter depth,
   commas, trailing operators, comment-only lines, leading operators, and
   semicolon rejection.
4. [x] Add declarations and types: modules, functions, structs, aliases, protocols,
   implementations, generics, constraints, associated types, composites,
   functions, unions, and fixed literal array lengths.
5. [x] Add expressions and blocks with the exact precedence and associativity
   ladder, including postfix forms, `::`, and `|>`.
6. [x] Add every collection constructor, control-flow form, pattern family,
   `defer`, and byte-aligned bitstring form.
7. [x] Convert `pest` pairs into parser-independent AST types with byte spans.
8. [x] Run grammar validation before semantic analysis: pipeline target shape,
   assignment target shape, protocol body contents, chained non-associative
   operators, bitstring modifiers, semicolons, leading-operator continuation,
   and remaining recovery nodes.
9. [x] Add focused recovery at declaration and block boundaries. Recovered trees
   may produce diagnostics but cannot become conforming ASTs.
10. [x] Add deterministic AST debug output.

**Tests**

- [x] Accepted and rejected fixture for every grammar production and validation
  rule.
- [x] Adjacent precedence-level snapshots and associativity cases.
- [x] Longest-token conflicts: `|`/`|>`, `:`/`::`/`:=`, `#`/`#[`, shifts, arrows,
  comparisons, and concatenation.
- [x] Literal boundaries, escapes, separators, overflow-independent tokenization,
  and invalid identifier forms.
- [x] Span snapshots including multibyte text and CRLF.
- [x] Recovery tests proving malformed input never reaches name resolution.

- [x] **Exit gate:** parse a typed `Main.main() -> i32` and snapshot its complete AST.

### Milestone 2 — Names, types, Typed AST, and initial Core IR

**Objective:** accept or reject the initial semantic language without LLVM and
establish verified source-to-Core boundaries.

**Pass order**

1. [x] Validate module paths and collect declarations in source order.
2. [x] Build separate module, type, protocol, function, local-value, and associated-
   type namespaces.
3. [x] Resolve visibility, package ownership, lexical scopes, and the closed core
   prelude using stable IDs.
4. [x] Expand transparent aliases and reject direct, mutual, generic, and cross-
   module cycles.
5. [x] Form canonical types and collect implicit function type parameters and
   explicit named-type parameters.
6. [x] Validate constraints and the subset of implementation metadata needed by the
   initial slice; complete protocol dispatch remains Milestone 7.
7. [x] Bidirectionally check expressions and patterns, using expected types only at
   the locations allowed by `TYPES.md`.
8. [x] Normalize unions, remove duplicates, sort members canonically, and use
   occurs-checked first-order unification to reject overlap with witnesses.
9. [x] Reject infinite inline layouts; recognize only specified built-in managed
   indirection boundaries.
10. [x] Produce and verify a Typed AST with resolved identities, canonical `TypeId`s,
    substitutions, value categories, pattern facts, and explicit union
    injections.
11. [x] Define the shared Core IR CFG: typed operations, block parameters,
    terminators, typed slots, source origins, and stage markers.
12. [x] Lower the milestone's supported source forms into Generic Core IR and verify
    ID ownership, typing, slot initialization, exact control-flow signatures,
    and absence of forbidden sugar.

**Initial semantic slice**

- [x] Primitive `i32`, `i64`, `bool`, and `unit` types.
- [x] Function signatures and direct calls.
- [x] Immutable and mutable bindings, exact `:=`, lexical scopes, and shadowing.
- [x] Checked arithmetic representation, returns, blocks, and initial generics.
- [x] Expected-type-directed literals, empty-value diagnostics, named function
  resolution, aliases, and structural union injection.

**Tests**

- [x] Accepted/rejected pairs for each static rule in the initial slice.
- [x] Scope, shadowing, duplicate name, visibility, and stable-ID snapshots.
- [x] Generic inference from arguments and expected results, recursive generic
  calls, ambiguity, and unsatisfied constraints.
- [x] Alias-cycle, union normalization/overlap, occurs-check, witness, and finite-
  layout cases.
- [x] Typed AST snapshots and malformed Typed AST verifier tests.
- [x] Generic Core IR evaluation-order snapshots and malformed CFG tests.
- [x] Source spans on all negative cases; no invalid module reaches lowering.

- [x] **Exit gate:** accepted and rejected programs cover binding, mutation, calls,
  return types, generic inference, and ambiguous empty values without invoking
  LLVM.

**Implementation progress (2026-07-27)**

- [x] Added stage-owned `el-resolve`, `el-types`, and `el-ir` crates and wired the
  target-independent parse-to-Generic-Core path through `el-driver`.
- [x] Added deterministic module, declaration, parameter, local, function, block,
  value, and slot IDs for the initial single-module slice.
- [x] Added source-spanned duplicate declaration/parameter/local, unknown-name,
  immutability, arity, type-mismatch, literal-range, and ambiguous-generic
  diagnostics.
- [x] Added bidirectional checking for the four initial primitives, annotated and
  inferred bindings, exact local assignment, checked arithmetic, direct calls,
  declared/final returns, and unconstrained first-order generic inference from
  arguments and expected results.
- [x] Added deterministic Typed AST and Generic Core debug forms plus independent
  verifier rejection tests. Initial lowering preserves left-to-right evaluation,
  represents mutable locals as typed slots, and emits checked arithmetic.
- [x] Added a separate alias namespace with source-order declaration IDs, generic
  alias arity checks, direct/mutual/generic cycle diagnostics, transparent
  expansion, canonical union flattening/deduplication/order, overlap witnesses for
  the currently supported first-order forms, expected-type union injection, and
  corresponding Typed AST/Core IR verifier coverage.
- [x] Added canonical atom, list, tuple, and function types; recursive generic-call
  substitution through those constructors; occurs-checked first-order union
  overlap unification with deterministic witnesses; typed proper/improper lists
  and tagged tuples; expected empty-list inference and a dedicated ambiguous-empty
  diagnostic; and left-to-right composite construction in verified Generic Core
  IR. The AST now preserves the improper-list boundary instead of confusing it
  with bitwise `|`.
- [x] Added nominal generic struct type formation and Core type declarations plus
  finite-layout checks for direct, mutual, tuple/union-wrapped, alias-wrapped, and
  managed-List-guarded recursion. Nominal applications participate in recursive
  generic inference and union-overlap witnesses.
- [x] Added the initial constraint subset: a separate protocol namespace, closed
  core-prelude protocol lookup, constraint identities on Typed AST/Core functions,
  recursive generic constraint justification, and concrete standard-protocol
  satisfaction diagnostics. Same-module qualified calls, local/function shadowing,
  and explicit type ascriptions now participate in bidirectional checking.
- [x] Added nested lexical scopes for conditional branches, block-parameter Core
  CFG lowering, exact branch signatures, conservative slot-initialization flow,
  and malformed multi-block verifier coverage.
- [x] Added fixed-array and map canonical types, contextual empty forms, homogeneous
  inference, map key constraints, recursive substitution/unification, ordered Core
  construction, and Typed AST/Core verifier coverage.
- [x] Added explicit pattern reachability, irrefutability, and exhaustiveness facts
  for boolean, scalar catch-all, and typed structural-union member patterns, plus
  switch/projection Core lowering and unreachable/non-exhaustive diagnostics.
- [x] Added source-ordered user implementation identities and complete metadata for
  protocol members, implementation targets, associated type assignments, method
  membership/completeness, and preservation through Typed AST and Generic Core IR.
- [x] Added package-wide declaration identities, qualified cross-module type and
  function resolution, visibility checks, duplicate module rejection, and cross-
  module alias/layout-cycle validation.
- [x] Expanded deterministic Core snapshots and malformed verifier tests for nested
  conditional and match CFGs, edge arity/types, targets, duplicate blocks,
  conditions, collections, unions, and implementation metadata.
- [x] Completed the remaining Milestone 2 accepted/rejected matrix and span-sensitive
  snapshots for structural tuple/list/struct patterns and package orchestration
  before closing the exit gate.
- [x] Added recursive structural-pattern usefulness/exhaustiveness checking,
  duplicate-binding and field diagnostics, tuple/list/struct Core projections,
  list-constructor switches, and verified binding flow through CFG block parameters.
- [x] Added manifest-independent package frontend orchestration across parsing,
  package-wide resolution, type checking, visibility enforcement, and Generic Core
  lowering. Strict manifests, dependency graphs, and lockfiles remain Milestone 8.

### Milestone 3 — First native executable

**Objective:** compile a small verified EL program into a linked host-native
executable.

**Deliverables**

- [x] Implement deterministic reachability roots beginning at `Main.main() -> i32`.
- [x] Monomorphize reachable unconstrained generic functions and concrete generic
  layouts using `(declaration identity, normalized substitution)` worklist keys.
- [x] Reuse identical specializations and reject any residual type parameter,
  projection, constraint call, derive request, or abstract layout.
- [x] Compute target layout for `i32`, `i64`, `bool`, and `unit`.
- [x] Lower Concrete Core IR arithmetic, direct calls, slots, blocks, and returns to
  private Inkwell-backed LLVM code.
- [x] Emit mandatory integer checks with source origins and no debug/release semantic
  difference.
- [ ] Attach basic LLVM debug locations to generated functions and operations as
  required by accepted decision D-008.
- [x] Initialize the host target, verify LLVM modules, emit object files, and invoke
  the host compiler driver as linker.
- [x] Add the native process entry shim that calls EL `Main.main` and forwards its
  `i32` result.
- [x] Record target triple and pointer width in reproducibility metadata.
- [x] Implement development and release output directories from the CLI contract.

**Tests**

- [x] Monomorphization reuse, recursion, deterministic order, and malformed
  Concrete Core IR rejection.
- [x] LLVM module verification and only narrowly targeted LLVM text assertions.
- [x] Linker failure diagnostics without panics.
- [x] Debug and release executable parity for arithmetic and exit status.

- [ ] **Exit gate:** compile and run a program whose process exit status is calculated
  by EL code.

**Implementation progress (2026-07-27)**

- [x] Preserved function module ownership and visibility through Typed AST and
  Generic Core IR, and added deterministic executable-root selection that requires
  an exported, nongeneric `Main.main() -> i32` with no parameters.
- [x] Added deterministic declaration-and-normalized-substitution worklists that
  emit only reachable unconstrained function specializations and instantiate the
  concrete nominal layouts referenced by their signatures and operations.
- [x] Added specialization identity metadata and a Concrete Core verifier covering
  deduplication, recursive reachability, exact concrete calls and layouts, and all
  residual generic constructs representable by the current Core instruction set.
- [x] Added the stage-owned `el-codegen` crate and an explicit host primitive ABI
  boundary that computes checked size/alignment facts for Concrete Core `i32`,
  `i64`, `bool`, and `unit` types without assuming fixed `TypeId` positions.
- [x] Added feature-gated Inkwell 0.9.0 lowering for primitive constants and
  arithmetic, direct calls, mutable slots, block-parameter PHIs, branches, and
  returns. The private boundary verifies every generated LLVM module and reports
  unsupported later-milestone Core operations as structured backend errors. The
  no-link API check passes locally; LLVM-linked tests pass against the installed
  pinned Homebrew LLVM 22.1.8 environment.
- [x] Added profile-independent signed overflow intrinsics for addition,
  subtraction, and multiplication; guarded division/remainder zero and `MIN / -1`
  cases before LLVM can observe undefined behavior; and routed failures to stable
  `integer_overflow` or `division_by_zero` runtime categories with the operation's
  file and byte-span origin. Pure debug/release tests and the no-link LLVM API check
  pass locally, including LLVM-linked checks against pinned Homebrew LLVM 22.1.8.
- [x] Added LLVM host-target initialization, target triple/data-layout assignment,
  re-verification before object emission, and host object-file output. Added a
  shell-free C compiler-driver linker boundary with a conventional `CC` override
  and structured launch/status/stdout/stderr failures. Linker failure tests and the
  no-link LLVM API check pass locally; production object emission passes against
  the pinned Homebrew LLVM 22.1.8 environment.
- [x] Added a verified native `main` shim that selects the rooted, public,
  nongeneric `Main.main() -> i32` specialization, calls it without arguments, and
  returns the exact `i32` result to the host process. Malformed executable roots
  produce structured backend errors rather than an invalid native entry point.
- [x] Added deterministic target metadata populated from LLVM's selected host
  target machine, including its target triple and validated 32- or 64-bit pointer
  width. The backend returns these facts with emitted objects and can record them
  in a stable metadata file or display them in build diagnostics.
- [x] Added driver-owned build output preparation beneath
  `build/<target-triple>/debug/` and `build/<target-triple>/release/`, with the
  package-named host executable, deterministic object and metadata locations,
  safe package-ID validation, directory creation, and structured filesystem errors.
- [x] Added distinct O1 development and O3 release optimization pipelines, support
  for linking generated code with private runtime objects, and a native end-to-end
  test. Both profiles execute EL multiplication with status 42 and classify checked
  `i32` overflow identically. The computed process-exit portion of the exit gate now
  passes on the pinned LLVM 22.1.8 host; basic LLVM debug locations from D-008 remain
  before Milestone 3 can be closed.

### Milestone 4 — Core control flow and matching

**Objective:** complete the core expression-oriented control-flow model and
make cleanup explicit in Core IR.

**Deliverables**

- [x] Comparisons, `if`, `while`, short-circuit `and`/`or`, and early `return`.
- [x] Tuples, atoms, closed unions, explicit injection, discriminants, payloads,
  typed member patterns, tagged tuples, and exhaustive `match`.
- [x] Pattern usefulness/exhaustiveness analysis with unreachable-arm diagnostics.
- [x] Pipeline validation and desugaring that evaluates the left input before
  explicit arguments.
- [x] `defer` registration: immediate target/argument evaluation for calls and
  by-value capture environments for blocks.
- [x] Cleanup CFGs that preserve block results and run actions once in LIFO order on
  fallthrough and every normal `return` path.
- [x] Unrecoverable failure terminators that deliberately bypass cleanup.
- [x] LLVM lowering for block parameters, conditional branches, switches, aggregate
  values, and cleanup paths.

**Tests**

- [x] Branch values, loops, nested returns, short-circuit effects, and pipelines.
- [x] Exhaustive and non-exhaustive finite/structural matches, top-to-bottom arm
  order, typed union patterns, and unreachable arms.
- [x] Deferred call timing versus block timing, capture snapshots, per-iteration
  cleanup, nested scopes, saved results, LIFO order, and failure skipping.
- [x] Core IR cleanup verifier tests and LLVM verification for every generated
  module.

- [x] **Exit gate:** compile and run iterative factorial, a tagged-result parser, and
  an exhaustive `i64 | string` match.

### Milestone 5 — Boehm GC integration

**Objective:** make managed native programs correct under conservative
collection before adding the bulk of heap-backed types.

**Deliverables**

- [x] Define and version a small private runtime ABI.
- [x] Vendor, build, and statically link the pinned Boehm GC release.
- [x] Initialize the collector before any managed runtime state.
- [x] Provide separate scanned and pointer-free allocation wrappers whose scan class
  never changes.
- [x] Classify private runtime calls as allocating or non-allocating while treating
  every EL call as a possible collection point.
- [x] Preserve live, aligned, unmodified base pointers across every collection point
  in locals, arguments, returns, globals, aggregate payloads, and hidden cleanup
  state.
- [x] Add managed-global registration and allocation-exhaustion termination.
- [x] Add a test-only stress mode that attempts collection at every managed
  allocation.
- [x] Keep all Boehm types and controls out of compiler stage APIs and EL source.

**Tests**

- [x] Graph retention and reclamation pressure in debug and optimized builds.
- [x] Base-pointer survival in registers, stack slots, calls, recursion, unions,
  saved block results, and deferred captures.
- [x] Scanned object graphs and pointer-free buffers.
- [x] Allocation exhaustion category, source location, nonzero exit, and no cleanup
  unwinding.

- [x] **Exit gate:** an optimized native EL program retains a reachable heap graph and
  survives collection-at-every-allocation stress while temporary allocations are
  reclaimable.

### Milestone 6 — Data types and text

**Objective:** implement v1 value categories and text/binary semantics on the
verified managed runtime.

**Deliverable groups**

1. [x] **Structs:** nominal identity, all-fields construction, field projection,
   generic specialization, immutable value semantics, and direct mutable-root
   field update by reconstruction.
2. [x] **Sequential data:** lists, fixed arrays, slices, indexing, managed backing
   retention, O(1) subslicing, and explicit copying.
3. [x] **Maps:** immutable operations, `Eq`/`Hash` key requirements, seeded hashing,
   deterministic insertion order, duplicate replacement, and order-independent
   equality.
4. [ ] **Text and binary:** valid UTF-8 `string`, `rune`, `bytes`, arbitrary-length
   `bits`, byte-aligned source bitstrings, conversions, bounds checks, and
   inspectable UTF-8 errors.
5. [x] **Unicode:** bundle Unicode 17.0.0 data and implement untailored UAX #29
   revision 47 grapheme segmentation independent of host locale.
6. [x] **Views:** eager codepoint/grapheme collections and lazy views that retain
   source backing storage.
7. [x] **Buffer:** explicit value-style byte/string append operations and immutable
   conversion snapshots.
8. [x] **Function values:** exact monomorphic direct code targets, indirect calls,
   visibility behavior, and generic specialization from expected types.
9. [ ] **Numbers:** remaining integer widths, pointer-sized integers, floats,
   explicit checked conversions, shifts, bitwise operations, and wrapping APIs.
10. [x] **Collection helpers:** `Enum` traversal machinery needed by the data layer
    plus the fixed List/Array/Slice/Bytes operations. Protocol surface integration
    is completed in Milestone 7.

**Tests**

- [ ] Construction, inference, layout, access, and immutable-copy semantics for
  every data category.
- [x] Fixed-array length inference and rejection of symbolic/derived lengths.
- [x] Map insertion order across seeds and all update/remove/reinsert cases.
- [ ] Bounds and numeric failure categories in debug and release.
- [x] Valid/invalid UTF-8 offsets and parity between string and buffer validation.
- [ ] Full Unicode 17.0.0 `GraphemeBreakTest.txt` conformance for eager, lazy, and
  length APIs.
- [ ] GC stress for nested composite graphs, views, base retention, immutable
  sharing, and function values.
- [x] Compile-time rejection of `string[index]`.

- [ ] **Exit gate:** process valid UTF-8, reject invalid UTF-8, retain composite heap
  graphs under GC stress, and diagnose integer indexing on `string`.

**Implementation progress (2026-07-29)**

- [x] Completed the struct value slice end to end: nominal and generic all-fields
  construction, direct field projection, source-order initializer evaluation,
  deterministic Generic/Concrete Core struct operations, specialized LLVM layouts,
  and direct mutable-root field update lowered to shallow reconstruction and slot
  rebinding.
- [x] Added accepted and rejected type-checker coverage for inferred and expected
  generic applications, missing/duplicate/unknown fields, immutable update roots,
  and Typed AST verification; added Core verifier and specialization coverage plus
  LLVM and development/release native regressions proving immutable-copy semantics.
- [x] Completed the fixed-array construction and read-indexing slice: literal lengths
  remain part of the canonical type, concrete arrays have specialized LLVM layouts,
  indices require target-width `usize`, and checked reads lower through a verified
  `index_out_of_bounds` failure edge in development and release builds. Calls and
  literals can feed an index expression without changing left-to-right evaluation.
- [x] Added compile-time diagnostics for indexing `string`, lists, and other
  unsupported categories, including the Milestone 6 exit-gate regression for
  `string[index]`; added Typed AST, Generic/Concrete Core, LLVM, and native tests for
  successful reads, invalid index types, and out-of-bounds category 5 behavior.
- [x] Added the first managed-slice implementation end to end: canonical `Slice(a)` types,
  `Slice.from_array`, O(1) bounds-checked `Slice.subslice`, explicit `Slice.copy`,
  `Array.length`, `Slice.length`, and checked slice indexing. Concrete slices retain
  the collector-visible allocation base separately from their derived data pointer
  and length, while copies allocate independent compact backing storage.
- [x] Added Typed AST and Core verifier coverage for slice source/item/bound types,
  specialized managed-value classification, LLVM lowering tests for allocation,
  derived views and copy, and development/release native GC-stress regressions that
  retain nested list elements through shared views and independent copies. Subslice
  and slice-index failures use the stable `index_out_of_bounds` failure edge.
- [x] Completed the sequential-data deliverable with generic `List.reverse`,
  including expected-type inference for empty lists, a dedicated verified Core
  operation, deterministic O(n) LLVM loop lowering, and fresh immutable list-node
  allocation without mutating or reusing the source spine.
- [x] Rooted both the source list and partially constructed reversed prefix across
  every allocation. Development/release collection-at-every-allocation tests reverse
  lists containing managed list elements, apply subsequent allocation pressure, and
  verify source order reversal and retained nested payloads.
- [x] Added the first managed-map runtime slice without prematurely closing the map
  deliverable: nonempty literals lower to collector-scanned key/value nodes,
  partially built maps remain rooted across every allocation, empty maps use the
  canonical null representation, primitive-key duplicates replace values in place
  without changing their first position, and `Map.size` traverses the immutable
  structure without allocating. Typed AST and Core verifiers accept only map inputs
  and a `usize` result.
- [x] Added frontend rejection for non-map `Map.size` inputs, Core allocation-effect
  coverage, LLVM lowering validation, and development/release
  collection-at-every-allocation native tests with managed list values, establishing
  the representation and rooting foundation used by the immutable API slice below.
- [x] Completed the primitive-key immutable map API slice: context-typed `Map.new`,
  option-returning `Map.fetch`, reconstruction-based `Map.put`, `Map.remove`, and
  `Map.size` now have explicit Typed AST and verified Core operations. Put replaces
  in place or appends an absent key, remove preserves survivor order, and all
  operations evaluate their inputs once from left to right without mutating the
  source map.
- [x] Added LLVM lowering that roots both the source and partially reconstructed map
  across every allocation, plus development/release GC-stress tests covering empty
  insertion, replacement, append, removal, absent removal, remove-and-reinsert,
  source immutability, option lookup, cardinality, and nested managed values. Seeded
  hashing, composite-key equality, observable iteration-order tests, and
  order-independent map equality remained before the final map slice below.
- [x] Closed the Milestone 6 map deliverable: map nodes now retain opaque hashes
  derived from a process-local runtime seed while their separate linked spine remains
  deterministic insertion order. Hash filtering never affects traversal order, and
  primitive, tuple, list, fixed-array, slice, string, and bytes keys use structural
  `Eq`/`Hash` behavior rather than pointer identity.
- [x] Added structural map `==`/`!=` independent of insertion order, including nested
  map values, and exposed the already-specified observable order through the map
  specialization of `Enum.to_list`. Development/release native GC-stress regressions
  cover literal duplicate replacement, stable replacement position, removal and
  reinsertion at the tail, three forced hash seeds, every standard composite-key
  category available in this milestone, and unequal values. The private runtime ABI
  advanced to revision 2 for the seeded-hasher entry point and map-node layout.
- [x] Added the first Milestone 6 text API slice with O(1) `String.byte_size`.
  The type checker recognizes only a `string` input and a `usize` result, Typed AST
  and Core verifiers enforce that contract, and LLVM reads the byte length already
  carried by the immutable UTF-8 string representation without allocation.
  Frontend/Core tests cover invalid input types and multibyte UTF-8, while native
  development/release tests distinguish encoded byte length from scalar or
  grapheme counts. The broader text/binary and Unicode groups remain open.
- [x] Introduced the first immutable `bytes` representation and retained-view API:
  `String.bytes` exposes a string's exact UTF-8 storage, `Bytes.byte_size` reads
  its structural length in O(1), and bounds-checked `Bytes.slice` produces an
  O(1) view with a collector-visible backing base, derived data pointer, and byte
  length. Typed AST and Core verifiers enforce exact string/bytes/`usize` types;
  Concrete Core classifies byte views as containing base references; and LLVM
  preserves the base across nested views without allocation. Frontend/Core and
  development/release native tests cover multibyte storage, nested slice lengths,
  invalid argument types, and category 5 out-of-bounds behavior. Byte indexing,
  `bits`, source bitstrings, and UTF-8 validation remain open.
- [x] Added checked `bytes[index: usize] -> u8` access and the required narrow `u8`
  foundation across canonical types, literal range checking, Typed AST/Core
  verification, monomorphization, target layouts, and LLVM. Byte reads address the
  visible view rather than its backing start, preserve category 5 bounds failures,
  and treat `u8`/`usize` ordered comparisons as unsigned. Frontend tests reject
  out-of-range `u8` literals; Core tests verify the result and failure edge; and
  development/release native tests read exact ASCII, combining-mark, and
  supplementary-code-point UTF-8 bytes through source and sliced views. The
  remaining integer widths, arithmetic, shifts, bitwise operations, and explicit
  conversions remain in the numbers group.
- [x] Completed the required fresh `bytes`/list conversions: `Bytes.from_list`
  context-types literals as `[u8]`, counts the source once, allocates independent
  pointer-free byte storage, and copies in source order; `Bytes.to_list` rebuilds a
  fresh scanned `[u8]` spine without sharing mutable representation details.
  Typed AST and Core verifiers enforce the exact signatures and classify both as
  collection points. LLVM roots the source and partially built output across every
  allocation, and development/release collection-at-every-allocation native tests
  cover empty-capable loops, boundary byte values, order, source immutability, and
  retained byte storage under subsequent allocation pressure.
- [x] Added the scalar `rune` value category and `Rune.to_string` end to end.
  Rune literals retain their parser-validated Unicode scalar identity, use a
  target-independent 32-bit Core/LLVM value, support scalar ordering and structural
  `Eq`/`Hash`, and encode to immutable valid UTF-8 through a verified allocating
  operation. The backend uses pointer-free backing storage and preserves exact
  one-, two-, three-, and four-byte encodings. Frontend/Core tests cover the exact
  types and allocation effect; development/release collection-at-every-allocation
  native tests cover every UTF-8 width, retained strings under allocation pressure,
  and duplicate rune map keys. Rune/integer conversions, rune patterns, and eager
  or lazy string codepoint APIs remained open before the slice below.
- [x] Added eager `String.codepoints(text) -> [rune]` with direct, locale-independent
  decoding of the string representation's guaranteed-valid UTF-8. The type checker,
  Typed AST verifier, Generic/Concrete Core verifiers, and monomorphizer enforce the
  exact string-to-rune-list contract and classify the operation as allocating.
  LLVM selects the one-, two-, three-, or four-byte decode path without reading past
  a scalar, appends freshly allocated scanned nodes in source order, and roots both
  the source and partial list at every collection point. Development/release
  collection-at-every-allocation tests cover empty text, ASCII, combining marks,
  three-byte scalars, supplementary scalars, deterministic order, and retention
  under subsequent allocation pressure. The lazy codepoint view and rune patterns
  remain open alongside grapheme APIs.
- [x] Added strict `String.from_bytes` validation and inspectable
  `String.Utf8Error` offsets, completing the first valid/rejected UTF-8 exit-gate
  path. The opaque error is nameable only through its specified standard type and
  exposes only `String.utf8_error_offset`; successful conversion returns
  `{:ok, string}`, while malformed or incomplete input returns
  `{:error, String.Utf8Error}` with the invalid sequence's starting byte offset.
  A non-allocating private runtime validator rejects stray continuations, invalid
  leads, overlong encodings, surrogate encodings, values above U+10FFFF, malformed
  continuations, and incomplete suffixes. LLVM copies successful visible byte views
  into independent pointer-free string backing, roots the input across allocation,
  and constructs verified tagged-union results for both arms. Development/release
  collection-at-every-allocation tests cover valid mixed-width text, prefixed
  failures, incomplete suffixes, each restricted UTF-8 boundary, stable offsets,
  and retained successful snapshots. The private runtime ABI is revision 3 for the
  validator entry point.
- [x] Completed the minimal value-style `Buffer` API end to end: `Buffer.new`,
  `byte_size`, distinct byte/bytes/string append operations, immutable `to_bytes`
  snapshots, and strict `to_string` results. `Buffer` is a distinct standard type
  that implements none of `Eq`, `Ord`, or `Hash`; Generic/Concrete Core classify
  its backing as managed and mark every copying operation as a collection point.
  LLVM currently realizes the permitted simple implementation by copying into
  fresh pointer-free backing for each append and conversion, preserving old buffer
  values and returned snapshots without exposing mutation. `Buffer.to_string`
  lowers through `Buffer.to_bytes` and the same runtime validator used by
  `String.from_bytes`, so the first-invalid and incomplete-suffix offsets are
  identical by construction. Type, Core, LLVM, and development/release native
  GC-stress tests cover all append forms, empty values, retained snapshots after
  later appends, valid text, and invalid-offset parity. The Buffer deliverable and
  UTF-8 parity test are closed; runtime `bits` and source bitstrings remained at
  this point.
- [x] Added the minimal arbitrary-length `bits` runtime surface: lossless
  `Bytes.to_bits`, O(1) bounds-checked `Bits.slice`, O(1) `Bits.bit_size`, direct
  MSB-first boolean indexing, and alignment-sensitive `Bits.to_bytes`. Bits retain
  the immutable byte base plus an arbitrary bit offset and length, allowing nested
  non-byte-aligned views without copying. Aligned-length conversion returns
  `{:some, bytes}` and packs even non-byte-aligned views into fresh pointer-free
  storage; other lengths return `:none`. Structural equality and hashing traverse
  visible bits rather than backing identity or padding. Typed AST and both Core
  verifiers enforce exact source/result shapes and explicit bounds failures, while
  LLVM preserves bases across the allocating packing path. Development/release
  collection-at-every-allocation tests cover empty, aligned, and non-aligned views,
  MSB-first indexing, exact repacking, retained source storage, and index failures.
  Byte-aligned `<<...>>` construction/pattern lowering and `Concat` remain before
  the text/binary deliverable closes.
- [x] Added byte-aligned source `<<...>>` construction end to end. Empty
  construction, integer segments at every v1 width, signed/unsigned fit checks,
  target-independent big/little order, target-native order, complete unsized
  `bytes` segments, and exact runtime-sized `bytes` segments now lower through a
  dedicated verified Core operation. Segment operands and size expressions retain
  left-to-right, exactly-once evaluation; statically known literal/value size
  violations are rejected during checking, while dynamic violations use the stable
  `bitstring_size_mismatch` failure category without truncation or padding. LLVM
  allocates pointer-free output, keeps managed inputs visible across collection,
  and copies bytes in source order. Typed AST, Generic/Concrete Core, LLVM, and
  development/release collection-at-every-allocation regressions cover exact byte
  layout, empty output, integer-fit failures, and byte-size failures. Source
  `<<...>>` patterns and `Concat` remain open, so the broader text/binary deliverable
  is not yet closed.
- [x] Added byte-aligned source `<<...>>` patterns end to end. Empty patterns,
  signed and unsigned integer literals/bindings at every v1 width, big/little/native
  decoding, runtime-sized retained `bytes` views, final unsized remainder capture,
  and size expressions using outer or earlier-segment bindings now flow through the
  Typed AST, verified Generic/Concrete Core, and LLVM. Unsigned captures use the
  specified `u64` type; short input, literal mismatch, and leftover input take the
  next match arm without an unrecoverable failure. Development/release GC-stress
  regressions cover exact decoding, bounds-safe normal failure, view retention,
  signed extension, maximum `u64`, and all source widths and byte orders. `Concat`
  remains open in Milestone 7, so the broader text/binary deliverable remains open.
- [x] Completed named monomorphic function values end to end. Bare and qualified
  ordinary function references now form exact structural function values, generic
  references specialize from their expected function type, local bindings shadow
  bare function names, and visibility is enforced when a function is named while
  already-returned private values remain callable. Calls through function-valued
  locals and returned values lower to verified indirect calls with exact parameter
  and result types; function values implement none of `Eq`, `Ord`, `Hash`, or
  `Show`.
- [x] Extended reachability and monomorphization through referenced code targets,
  including deterministic reuse of generic specializations, and lowered the private
  representation to LLVM code pointers without exposing it across Core IR. Typed
  AST and Generic/Concrete Core negative tests reject ambiguous generic references,
  inexact signatures, invalid targets, and mistyped indirect calls. LLVM coverage
  verifies code-pointer phis and indirect calls, while development/release
  collection-at-every-allocation native tests retain managed string arguments across
  indirect calls. `Concat` remains a Milestone 7 protocol deliverable, and the
  broader Milestone 6 text/binary deliverable remains open.
- [x] Added the non-higher-order `Enum` traversal foundation for every standard
  iterable currently available in Milestone 6: lists, fixed arrays, slices,
  `bytes`, and insertion-ordered maps. `Enum.count` returns a target-width
  `usize`; `Enum.at` performs zero-based traversal and returns `{:some, item}` or
  `:none` without an unrecoverable bounds failure; and `Enum.to_list` produces a
  fresh logical list in deterministic iteration order, with map items represented
  as `{key, value}` tuples.
- [x] Added exact Typed AST and Generic/Concrete Core verification for iterable
  source, item, index, option, and list-result types. LLVM lowers list/map cursor
  traversal, array/slice/byte positional access, and rooted array/slice list
  materialization without exposing cursor representation. Development/release
  collection-at-every-allocation tests cover hits, misses, all five iteration
  orders, fresh outputs, and retained managed string elements. Higher-order
  `Enum.map`, `filter`, `reduce`, `each`, `any`, and `all`, Unicode views, and the
  Milestone 7 protocol surface remain open, so the collection-helper deliverable
  is not yet closed.
- [x] Added the callback-only `Enum.each`, `Enum.any`, and `Enum.all` traversal
  slice for lists, fixed arrays, slices, `bytes`, and insertion-ordered maps.
  Callback arguments use exact named monomorphic function types; traversal is
  deterministic, `each` visits the complete input, `any` stops on the first true
  result, and `all` stops on the first false result. Empty inputs use the specified
  false/true identities for `any`/`all`.
- [x] Lowered visitor traversal through a verified collecting Core operation and
  rooted both the iterable and the current callback item across every indirect
  call. LLVM coverage exercises cursor and positional loops, while development and
  release collection-at-every-allocation tests cover all five iterable categories,
  managed string callback arguments, empty inputs, and short-circuiting before a
  deliberately failing later callback. List-producing higher-order traversal and
  reduction remained open at this point.
- [x] Added strict left-to-right `Enum.reduce` for every current standard iterable.
  The explicit initial value fixes the accumulator type and context-types the exact
  named reducer signature `(accumulator, item) -> accumulator`; empty inputs return
  that initial value unchanged. Typed AST and Generic/Concrete Core verification
  reject mismatched accumulator, item, callback, and result types.
- [x] Reused the rooted visitor loops for reduction while keeping the live
  accumulator in a volatile collector-visible stack root across every indirect
  reducer call. LLVM and development/release collection-at-every-allocation tests
  cover lists, arrays, slices, bytes, insertion-ordered maps, empty inputs, strict
  order, accumulator threading, and managed string accumulators. `Enum.map` and
  `filter` remain open, so the collection-helper deliverable is not yet closed.
- [x] Added `Enum.filter` across lists, arrays, slices, bytes, and maps. Predicates
  use exact `(item) -> bool` named function values, every input is visited in its
  deterministic order, and accepted items are appended to a fresh logical list
  without exposing the private construction tail.
- [x] The shared collecting visitor operation now roots the current item and fresh
  list head across predicate calls and node allocations. Typed/Core rejection tests,
  LLVM cursor and positional-loop coverage, and development/release GC-stress tests
  cover empty-capable filtering, stable order, map tuple items, bytes, and managed
  strings whose predicate allocates. `Enum.map` remains open, so the collection-
  helper deliverable is not yet closed.
- [x] Completed `Enum.map` for lists, fixed arrays, slices, `bytes`, and maps.
  Mapper arguments use exact `(item) -> result` named function values, including
  generic specialization from the source item and an expected list result; every
  input is transformed once in deterministic order into a fresh logical `[result]`.
- [x] Extended the verified collecting visitor so a mapper's returned value remains
  in a volatile collector-visible root while its list node is allocated. Typed AST,
  Generic/Concrete Core, and LLVM tests cover type-changing results and rooted
  indirect-call loops. Development/release collection-at-every-allocation tests
  cover all five iterable categories, empty inputs, map tuple order, bytes, and
  managed callback results. This closes the Milestone 6 collection-helper
  deliverable; protocol-backed `Iterable` integration remains in Milestone 7.
- [x] Added the pinned Unicode 17.0.0 extended-grapheme boundary foundation and
  `String.length`. Official `GraphemeBreakProperty`, `Indic_Conjunct_Break`, and
  `Extended_Pictographic` inputs generate checked-in deterministic range tables;
  segmentation implements the untailored UAX #29 revision 47 rules without host
  locale, ICU, normalization, or case-folding dependencies. Build metadata now
  records `unicode_version = "17.0.0"`, and the private runtime ABI advanced to
  revision 4 for non-allocating next-boundary and cluster-count operations.
- [x] Added exact Typed AST and Generic/Concrete Core verification for
  `String.length(string) -> usize`, plus verified LLVM lowering through the private
  count operation. The bundled official Unicode 17.0.0 `GraphemeBreakTest.txt`
  corpus checks every boundary and count in all 766 cases, while native development
  and release tests cover empty text, combining sequences, regional-indicator
  flags, emoji ZWJ families, and mixed text. `String.graphemes`, both lazy views,
  and their allocation/retention tests remain before the Unicode and views
  deliverables close.
- [x] Completed `String.graphemes(text) -> [string]` and the named
  `String.CodepointView`/`String.GraphemeView` lazy traversal types. Eager
  graphemes collect through the grapheme view, so `length`, eager collection,
  and lazy traversal all use the same pinned Unicode 17.0.0 UAX #29 revision 47
  boundary routine. Codepoint traversal decodes guaranteed-valid UTF-8 directly
  and never reads beyond a scalar.
- [x] Lazy views retain the source allocation base separately from their current
  data pointer and byte length. The complete Milestone 6 `Enum` helper surface
  accepts both views with source-order `count`, `at`, `to_list`, callback visits,
  filtering, mapping, and reduction; grapheme items are immutable string slices
  and codepoint items are `rune` values. Typed AST and Generic/Concrete Core
  verification cover the opaque view types and item contracts, LLVM tests cover
  the shared next-boundary path, and development/release collection-at-every-
  allocation regressions cover empty, combining-mark, supplementary-scalar,
  regional-indicator, and emoji-ZWJ inputs plus retained eager and lazy results.
  This closes the Unicode and views deliverable groups; direct full-corpus checks
  through all three public APIs remain in the open Milestone 6 conformance test.

### Milestone 7 — Protocols and iteration

**Objective:** complete coherent static protocols, deriving, protocol-backed
operators, and generic traversal.

**Deliverables**

- [ ] Parse-to-Typed-AST support for `defprotocol`, `defimpl`, `Self`, explicit
  associated type declarations/assignments, and qualified projections.
- [ ] Protocol ownership/orphan checks across the resolved package graph.
- [ ] Implementation completeness, exact substituted signatures, uniqueness, and
  overlap detection without using positive constraints as disambiguation.
- [ ] Generic constraint checking once and concrete implementation selection during
  monomorphization.
- [ ] Core protocols: `Eq`, `Ord`, `Show`, `Hash`, `Iterable`, and `Concat`.
- [ ] Compiler-generated standard implementations and `@derive` for `Eq`, `Ord`,
  `Show`, and `Hash` when all field constraints hold.
- [ ] `for` lowering through one statically selected `Iterable` implementation,
  immutable cursors, associated `Item`, and irrefutable patterns.
- [ ] Protocol lowering for nonprimitive equality/ordering and `++`.
- [ ] Complete `Enum` APIs with deterministic traversal and short-circuit behavior.

**Tests**

- [ ] Completeness, duplicate/unknown associated types, orphan rejection, coherence,
  overlapping generic heads, and alias/union target rejection.
- [ ] Concrete dispatch at multiple instantiations, projection normalization,
  specialization reuse, and defensive post-substitution checks.
- [ ] Derivation success/failure for generic and nested structs.
- [ ] `Eq`/`Ord`/`Hash` law tests for standard implementations.
- [ ] Every standard iteration order, map tuple order, cursor threading, and
  irrefutable/refutable `for` patterns, including zero-based `Enum.at` hits and
  misses plus bounded traversal through the requested position.
- [ ] Concatenation for `string`, `bytes`, `bits`, and lists.

- [ ] **Exit gate:** derive protocols for a generic struct, instantiate constrained
  generic functions at several concrete types, iterate multiple container types,
  and concatenate every standard `Concat` type.

### Milestone 8 — Packages, I/O, and standard library

**Objective:** turn the compiler into the complete manifest-driven v1 tool and
provide recoverable process/file boundaries.

**Deliverable groups**

1. [ ] Strict `el.toml` parsing for package ID, namespace, semantic version,
   dependencies, and optional single executable target; unknown keys are errors.
2. [ ] Strict `src/` discovery and mechanical path-to-module validation.
3. [ ] Package-relative, dependency-qualified, and prelude name resolution with
   collision and ambiguity diagnostics.
4. [ ] Exact path dependency resolution and full-commit Git dependency resolution.
5. [ ] Graph-wide package/source/version/revision uniqueness, namespace uniqueness,
   dependency cycle detection, and deterministic traversal.
6. [ ] Deterministic `el.lock` generation/refresh and non-mutating `--locked`
   verification.
7. [ ] Full `el check`, `el build`, and `el emit llvm-ir --module ...` behavior,
   output paths, streams, and status classes.
8. [ ] `Reader` and `Writer` with associated errors and tagged result values.
9. [ ] Console conveniences constrained by `Show`, plus recoverable standard stream
   handles.
10. [ ] Typed file reader/writer handles, shared external identity, close-through-
    alias behavior, tagged closed-handle errors, and explicit `defer` cleanup.
11. [ ] Stable error kind/operation/code inspection, internal interrupted-call
    retry, and exact protocol laws.
12. [ ] Launch-time process arguments/environment snapshots with strict native
    UTF conversion and exact platform-specific string path conversion.
13. [ ] Complete reserved prelude and standard modules with no implicit functions.

**Tests**

- [ ] Manifest keys, module mapping, visibility, package ownership, dependency
  conflicts, cycles, namespaces, exact versions, Git pins, and lock stability.
- [ ] `--locked` missing/stale behavior without filesystem mutation.
- [ ] CLI option orders, duplicate/unknown options, streams, statuses 0/1/2, output
  locations, library-only behavior, and absence of deferred commands.
- [ ] Complete writes, EOF, flush, console newline behavior, typed file modes,
  error mappings/accessors, alias/close behavior, and interrupted calls.
- [ ] Argument ordering, executable-name exclusion, environment snapshots, invalid
  native text, embedded NULs, and exact Unix/Windows path conversion.
- [ ] Multi-package and multi-module end-to-end builds using only source packages.

- [ ] **Exit gate:** build a multi-module manifest target that reads, transforms, and
  writes data while handling every recoverable error through exhaustive `match`.

### Milestone 9 — V1 stabilization

**Objective:** prove the documented language and tool behave consistently on
every claimed host and freeze the v1 delivery contract.

**Deliverables**

- [ ] Complete grammar, type-system, IR, CLI, runtime, standard-library, and package
  conformance suites traceable to specification sections.
- [ ] Promote conforming programs from `EXAMPLES.md` into executable fixtures and
  its invalid examples into negative fixtures.
- [ ] Stabilize diagnostic codes, labels, source presentation, and path handling.
- [ ] Run all semantic tests in development and optimized builds where required.
- [ ] Verify GC stress mode on every supported target.
- [ ] Freeze the v1 manifest format and private runtime ABI version for the matching
  compiler distribution.
- [ ] Document supported targets, LLVM/Boehm/runtime packaging, linker prerequisites,
  licenses, reproducible build metadata, installation, and troubleshooting.
- [ ] Audit the binary to ensure no v2 feature, extra CLI command, public FFI,
  unstable native type, or host-only behavior leaked into v1.

- [ ] **Exit gate:** all v1 examples and negative conformance programs behave
  identically on every supported target with GC stress enabled.

## 8. Test architecture

Tests are layered so failures identify the responsible compiler boundary.

| Layer | Primary assertion | Must avoid |
| --- | --- | --- |
| Lexer/grammar | accepted/rejected source and exact span | treating recovery as acceptance |
| AST | deterministic source structure and spans | parser-library types in public output |
| Resolver | stable identities, visibility, ownership | spelling or hash order as identity |
| Type checker | canonical types and diagnostics | relying on LLVM to reject invalid EL |
| Typed AST | resolved references, substitutions, injections, pattern facts | unresolved inference variables |
| Generic Core IR | explicit sugar lowering, CFG, evaluation order | source sugar and backend types |
| Monomorphization | exact specializations and deterministic reuse | unresolved constraints or layouts |
| Concrete Core IR | fully concrete calls/types and GC classifications | target ABI becoming source semantics |
| LLVM backend | verifier success and unavoidable backend invariants | broad LLVM text snapshots |
| Runtime | categories, allocation, GC, I/O, cleanup | nondeterministic prose assertions |
| End to end | build/link/execute output and exit status | skipping narrower regression coverage |

Every compiler bug adds a regression at the narrowest failing layer. An
end-to-end regression is added as well only when it protects an integration
boundary or observable behavior.

### 8.1 Fixture conventions

- Put accepted and rejected forms beside one another by feature.
- Store the expected phase and diagnostic code in fixture metadata.
- Use package-relative paths in expectations.
- Keep executable fixtures self-contained and deterministic.
- Never depend on wall-clock time, allocation addresses, hash seed order, host
  locale, or host path spelling.
- Run release-mode semantic tests for arithmetic, optimization-sensitive
  evaluation, GC roots, views, and failure categories.

### 8.2 Required checks before handoff

Once the workspace exists, every completed change runs the narrowest tests while
iterating and then:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Also run the active milestone's exit test. If LLVM, the linker, Boehm GC, or a
platform facility is unavailable, record the exact blocked command and run all
independent frontend and representation checks.

## 9. Initial change sequence

The first reviewable changes should be small enough to verify independently:

1. [ ] **Workspace and pins:** toolchain, workspace manifests, license policy,
   initial `el-span`, `el-runtime`, `el-driver`, and `el-cli` crates, plus a CI
   build.
2. [x] **CLI shell:** exact help/version and strict command parsing with CLI
   conformance fixtures.
3. [ ] **Source foundation (in progress):** source database, `FileId`, `Span`, line mapping,
   structured diagnostics, and renderer snapshots.
4. [x] **Project discovery:** ancestor manifest search and minimal project errors.
5. [x] **Lexical grammar:** source encoding, whitespace/comments, identifiers,
   keywords, literal tokens, and newline model.
6. [x] **Declaration/type grammar:** module, functions, signatures, bindings, scalar
   types, and the first typed-main parse.
7. [x] **Expression grammar:** precedence, postfix forms, blocks, and arithmetic.
8. [x] **AST adapter:** parser-independent AST, span propagation, deterministic
   snapshots, and the Milestone 1 starter exit test.
9. [x] **Remaining v1 grammar:** composites, generics, protocols, patterns,
   bitstrings, control flow, validation, and recovery conformance.
10. [ ] **Semantic foundations:** typed IDs, declaration collection, namespaces,
    canonical type arena, and the first Typed AST slice.
11. [ ] **Initial checking:** bindings, mutation, calls, returns, literals, expected
    types, aliases, generics, unions, and layout validation.
12. [ ] **Core IR:** shared CFG, verifier, source-to-Generic lowering, and snapshots.
13. [ ] **First backend:** monomorphization, Concrete verifier, LLVM lowering,
    object emission, linking, and computed process exit status.

Do not combine the parser, type checker, Core IR, and backend into one large
change. Each boundary needs reviewable data structures and rejection tests
before the next consumer is added.

## 10. Traceability matrix

| Contract area | Owning implementation | Primary milestone | Proof |
| --- | --- | --- | --- |
| UTF-8, tokens, newlines, grammar | `el-parser` | 1 | grammar fixtures and AST spans |
| Grammar validation/recovery boundary | `el-parser` | 1 | rejected recovery/validation fixtures |
| Module and lexical resolution | `el-resolve` | 2, 8 | identity and visibility snapshots |
| Static checking and inference | `el-types` | 2, 6, 7 | accepted/rejected type fixtures |
| Union normalization/disjointness | `el-types` | 2 | witness and injection snapshots |
| Pattern analysis | `el-types` | 4 | exhaustiveness/usefulness tests |
| Representation invariants | `el-types`, `el-ir` | 2 onward | malformed representation tests |
| Monomorphization | `el-ir` | 3, 7 | reuse, recursion, projection tests |
| Native code generation | `el-codegen` | 3 onward | LLVM verify and end-to-end execution |
| Cleanup semantics | `el-ir`, `el-runtime` | 4 | cleanup CFG and execution tests |
| Conservative root visibility | `el-codegen`, `el-runtime` | 5 onward | GC stress in debug/release |
| Text, binary, Unicode | `el-types`, `el-runtime`, `stdlib/` | 6 | UTF-8/UAX #29 conformance |
| Protocol coherence/dispatch | `el-resolve`, `el-types`, `el-ir` | 7 | coherence and concrete dispatch tests |
| Manifest/dependency/lockfile | `el-driver` | 8 | deterministic graph/lock fixtures |
| I/O/process/path semantics | `el-runtime`, `stdlib/` | 8 | platform boundary tests |
| Normative CLI | `el-cli`, `el-driver` | 0, 3, 8, 9 | streams, statuses, output paths |

## 11. Risk register

### 11.1 Significant newlines in PEG

**Risk:** implicit whitespace handling can erase the distinction between a
statement boundary and continuation.

**Mitigation:** do not globally skip newlines; model continuation deliberately,
test every boundary in `GRAMMAR.md`, and lock the parse shape before broad AST
work.

### 11.2 Diagnostic recovery contaminating later stages

**Risk:** recovery nodes could masquerade as valid source and cause resolver or
type-checker failures.

**Mitigation:** make conforming-AST validation an explicit gate and prevent the
semantic driver from accepting an AST containing recovery nodes.

### 11.3 Type-system breadth

**Risk:** aliases, inference, unions, protocols, associated types, coherence,
and finite-layout checks can become one tangled solver.

**Mitigation:** follow the `TYPES.md` well-formedness order, use separate
canonicalization/unification/coherence components, and snapshot the Typed AST
before lowering.

### 11.4 IR/backend semantic drift

**Risk:** implementing behavior directly in LLVM can bypass source semantics or
duplicate type checking.

**Mitigation:** require verified Core IR for every backend input and test
semantics at Core IR or execution level before inspecting LLVM text.

### 11.5 Monomorphization explosion or nondeterminism

**Risk:** recursive generic reachability can duplicate work and destabilize
output.

**Mitigation:** intern normalized substitution keys, mark worklist states before
descending, process roots/references in stable order, and test recursive reuse.

### 11.6 LLVM 22.1.8 availability

**Risk:** frontend work could become coupled to a missing native toolchain.

**Mitigation:** keep the backend behind a private crate/module boundary, make
frontend tests LLVM-independent, document discovery early, and add a dedicated
backend CI environment before Milestone 3 is declared complete.

### 11.7 Conservative GC versus LLVM optimization

**Risk:** optimized code may retain only derived, tagged, or integer forms of a
live reference.

**Mitigation:** encode managed/base-reference classification in Concrete Core
IR, treat all EL calls as collection points, retain bases explicitly, and run
collection-at-every-allocation tests in both profiles.

### 11.8 Unicode reproducibility

**Risk:** host Unicode libraries or locales can produce different grapheme
boundaries.

**Mitigation:** generate and check in versioned Unicode 17.0.0 tables, record
their provenance/checksum, avoid host segmentation APIs, and run the official
conformance data.

### 11.9 Package and Git resolution

**Risk:** filesystem canonicalization, network state, or traversal order can
make lockfiles unstable.

**Mitigation:** separate resolution identities from display paths, sort graph
output, pin full revisions, test stale locks without mutation, and isolate
network fetching behind a small package-source interface.

### 11.10 Scope expansion

**Risk:** convenient v2 features can leak into syntax, CLI, runtime, or standard
library during implementation.

**Mitigation:** maintain negative conformance fixtures for deferred syntax and
commands, and audit every milestone against the v1 non-goals.

## 12. Progress tracking

Use this checklist as the high-level implementation ledger. Check a milestone
only after its exit gate and workspace checks pass. Leave incomplete work
unchecked and add `(in progress)` after the item when useful.

- [ ] **0 — Project skeleton (in progress):** local exit gate passes; CI workflow
  awaits its first remote run.
- [x] **1 — Parser and AST:** typed-main AST snapshot.
- [x] **2 — Names and types:** initial primitive/function/binding/
  generic slice plus transparent scalar aliases and structural union injection
  now includes atoms, lists, tuples, function types, occurs-checked composite union
  overlap, tagged tuples, nominal generic structs, finite-layout checks, initial
  protocol constraints, qualification/shadowing, ascriptions, arrays/maps and all
  contextual empty forms, nested conditional CFGs, implementation metadata,
  scalar/union and structural pattern facts, package-wide resolution and frontend
  orchestration, and expanded verifier snapshots in verified Generic Core IR
  without LLVM.
- [ ] **3 — First native executable (in progress):** deterministic
  `Main.main() -> i32` reachability and initial generic function/layout
  monomorphization, host primitive target layouts, LLVM lowering with mandatory
  integer failure paths, host object emission, C-driver linking, and the native
  process entry shim, target reproducibility metadata, and profile output directories
  are implemented, and debug/release native execution has parity; accepted D-008's
  basic LLVM debug-location requirement remains before closing the milestone.
- [x] **4 — Core control and matching:** primitive `i32`/`i64`
  comparisons, `bool` equality, expression-valued `if`, `while`, short-circuit
  `and`/`or`, and early `return` now pass Typed AST/Core IR verification and
  debug/release native factorial coverage. Tuples, atoms, normalized closed
  unions, branch injections, private discriminants/member payload storage, typed
  member projections, tagged tuples, exhaustive matches, and LLVM switches now
  pass a debug/release native tagged-result test. Recursive usefulness and
  exhaustiveness analysis now covers finite, infinite-scalar, tuple, list, struct,
  atom, and union domains; detects individually and collectively subsumed arms;
  emits span-based diagnostics; and is independently rechecked by the Typed AST
  verifier. Statically resolved pipelines now desugar during type checking into
  ordinary calls with the left input in argument zero; chained and generic calls
  preserve this order through Generic Core and debug/release native execution.
  Deferred calls now retain registration-time argument values, while deferred
  blocks rewrite referenced outer bindings to fresh immutable capture symbols and
  retain registration-time SSA snapshots; timing and mutation-sensitive capture
  behavior pass debug/release native tests. Dedicated cleanup blocks now carry
  saved scope results as block parameters and route fallthrough, nested returns,
  and loop iterations through exactly-once LIFO actions; these paths pass Core IR
  verification and debug/release native tests. Checked arithmetic now exposes its
  ordered exceptional edges as category-tagged Core IR failure terminators; the
  verifier checks their plans and source origins, and debug/release execution
  confirms failures bypass pending and remaining cleanup. Concrete Core block
  parameters now lower to LLVM PHIs, branches and finite matches lower to verified
  conditional branches and switches, tuples and closed unions use aggregate
  insert/extract operations, and explicit cleanup paths retain saved results through
  verified LLVM modules. Generic and Concrete Core verifier regressions reject
  cleanup edges with missing or mistyped saved results, malformed cleanup Core is
  rejected before LLVM generation, and every successfully lowered or emitted LLVM
  module passes LLVM verification. The narrow text slice required by this milestone
  represents UTF-8 literals as immutable static pointer/length values without
  pulling forward Milestone 6 string APIs or allocation; string values survive
  tuples, calls, and union payloads. Iterative factorial, a string-backed tagged
  result parser, and an exhaustive `i64 | string` match now compile and run in both
  development and release profiles, closing the Milestone 4 exit gate.
- [x] **5 — Boehm GC:** the versioned private ABI, static vendored
  collector build, startup initialization, immutable scanned/atomic allocation
  classes, managed-global registration, allocation-exhaustion termination, and
  collection-at-every-allocation build mode are implemented behind `el-runtime`.
  A native development/O3 conformance fixture retains a scanned 4,096-node graph
  through allocation pressure, exercises pointer-free buffers, and verifies a
  registered global root under stress. The managed LLVM entry shim now initializes
  the collector before `Main.main`, and the host linker consumes the exact private
  runtime and collector archives through a tested feature boundary. Concrete Core
  now classifies direct managed bases separately from aggregates/views containing
  bases across nested arrays, tuples, unions, structs, lists, maps, and strings;
  its verifier rejects types without a classification, and every EL call is marked
  as possibly collecting. A deterministic backwards dataflow pass now computes the
  managed SSA values and addressable slots live at every collection point, including
  CFG block arguments, loops, call operands, mutable locals, saved cleanup results,
  and deferred state. LLVM lowering preserves those bases in aligned volatile stack
  storage across calls, touches live managed slots so optimization cannot promote
  away their only visible representation, and clears temporary root spills after
  each call. The first managed list backend slice allocates immutable nodes in
  scanned storage, roots partially constructed spines across each allocation, and
  lowers empty/cons tests plus head/tail projections. A native EL fixture exercises
  live graphs in SSA values, mutable slots, arguments, returns, recursive calls,
  union payloads, saved block results, and deferred captures while collection runs
  at every allocation in development and O3 builds. Runtime tests bound heap growth
  under repeated temporary-allocation pressure, verify scanned graphs, atomic
  buffers, and registered globals, and run in debug and optimized profiles. A
  deterministic allocation-failure build proves category 6, the list literal's
  exact source span, nonzero termination, and skipped deferred cleanup. The
  optimized native EL graph-retention exit gate is closed.
- [ ] **6 — Data types and text:** UTF-8, composites, views, GC, string-index
  rejection.
- [ ] **7 — Protocols and iteration:** derive, constrained generics, iteration,
  concat.
- [ ] **8 — Packages, I/O, stdlib:** multi-module recoverable I/O program.
- [ ] **9 — V1 stabilization:** cross-target full conformance under GC stress.

## 13. Definition of implementation-ready

Implementation can begin when all of the following are true:

- [x] Normative syntax, type, IR, runtime, and CLI contracts exist.
- [x] All design questions currently listed in `DESIGN.md` are resolved.
- [x] Milestone order and exit tests are defined.
- [x] The initial crate and compiler-stage boundaries are proposed.
- [x] Cross-cutting identity, span, diagnostic, determinism, and verifier rules
  are explicit.
- [x] Exact Rust toolchain is selected and pinned.
- [x] Exact Boehm GC release/source is selected and pinned.
- [x] LLVM 22.1.8 is installed or provisioned for backend CI and local backend
  work.
- [x] Milestone 0 workspace and CI are created.

The first implementation action is therefore Milestone 0, change 1: create the
workspace, make the two remaining version selections, record native prerequisites,
and establish a green build before adding language behavior.
