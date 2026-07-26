# EL Implementation Plan

Status: implementation-ready planning document

Language: **EL**

Last updated: 2026-07-26

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
- Backend: LLVM 22.1.0 through exactly Inkwell 0.9.0 with
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
3. **LLVM discovery.** Document how LLVM 22.1.0 is located for local builds and
   CI, including the environment expected by `llvm-sys`. The current workstation
   does not expose `llvm-config`, so backend work is locally blocked until the
   matching LLVM installation is made discoverable.
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

- Pin the exact Rust toolchain.
- Create the workspace plus `el-cli` (with binary name `el`), `el-driver`,
  `el-span`, and `el-runtime`; add the remaining stage crates with their first
  vertical slice instead of landing empty future scaffolding.
- Pin direct Rust dependencies exactly in `Cargo.lock`; start with `pest` and
  `pest_derive` only when the parser crate is introduced.
- Record LLVM 22.1.0 and Inkwell 0.9.0 requirements without forcing ordinary
  frontend-only tests to link LLVM.
- Select, document, and prepare the exact vendored Boehm GC release without
  exposing it through source-language APIs.
- Implement exact `el --help` and `el --version` stream/exit behavior.
- Build a strict command parser whose known-but-unimplemented project commands
  fail as ordinary tool errors, not panics.
- Discover the nearest ancestor `el.toml` for project commands.
- Add structured error plumbing, temporary-directory test support, golden-file
  conventions, and CI jobs for format, lint, unit tests, and license checks.
- Document local prerequisites and keep frontend checks runnable without LLVM
  or Boehm GC.

**Tests**

- Help and version output, streams, and status 0.
- Unknown commands, duplicate options, missing values, and status 2.
- Manifest discovery from the project root, descendants, and missing-manifest
  paths.
- Workspace builds with no network access after dependencies are fetched.

**Exit gate:** `el --help` runs and CI builds the workspace. All standard Rust
checks pass.

### Milestone 1 — Parser and AST

**Objective:** implement the complete normative grammar and produce a clean,
spanned AST that later stages can trust.

**Deliverables**

1. Implement source decoding rules: UTF-8, BOM rejection, LF/CRLF handling,
   horizontal whitespace, comments, identifiers, keywords, and attributes.
2. Implement literal tokens and decoding for integers, floats, strings, runes,
   atoms, booleans, and `unit`, preserving spelling and decoded value.
3. Encode significant-newline behavior explicitly. Test delimiter depth,
   commas, trailing operators, comment-only lines, leading operators, and
   semicolon rejection.
4. Add declarations and types: modules, functions, structs, aliases, protocols,
   implementations, generics, constraints, associated types, composites,
   functions, unions, and fixed literal array lengths.
5. Add expressions and blocks with the exact precedence and associativity
   ladder, including postfix forms, `::`, and `|>`.
6. Add every collection constructor, control-flow form, pattern family,
   `defer`, and byte-aligned bitstring form.
7. Convert `pest` pairs into parser-independent AST types with byte spans.
8. Run grammar validation before semantic analysis: pipeline target shape,
   assignment target shape, protocol body contents, chained non-associative
   operators, bitstring modifiers, semicolons, leading-operator continuation,
   and remaining recovery nodes.
9. Add focused recovery at declaration and block boundaries. Recovered trees
   may produce diagnostics but cannot become conforming ASTs.
10. Add deterministic AST debug output.

**Tests**

- Accepted and rejected fixture for every grammar production and validation
  rule.
- Adjacent precedence-level snapshots and associativity cases.
- Longest-token conflicts: `|`/`|>`, `:`/`::`/`:=`, `#`/`#[`, shifts, arrows,
  comparisons, and concatenation.
- Literal boundaries, escapes, separators, overflow-independent tokenization,
  and invalid identifier forms.
- Span snapshots including multibyte text and CRLF.
- Recovery tests proving malformed input never reaches name resolution.

**Exit gate:** parse a typed `Main.main() -> i32` and snapshot its complete AST.

### Milestone 2 — Names, types, Typed AST, and initial Core IR

**Objective:** accept or reject the initial semantic language without LLVM and
establish verified source-to-Core boundaries.

**Pass order**

1. Validate module paths and collect declarations in source order.
2. Build separate module, type, protocol, function, local-value, and associated-
   type namespaces.
3. Resolve visibility, package ownership, lexical scopes, and the closed core
   prelude using stable IDs.
4. Expand transparent aliases and reject direct, mutual, generic, and cross-
   module cycles.
5. Form canonical types and collect implicit function type parameters and
   explicit named-type parameters.
6. Validate constraints and the subset of implementation metadata needed by the
   initial slice; complete protocol dispatch remains Milestone 7.
7. Bidirectionally check expressions and patterns, using expected types only at
   the locations allowed by `TYPES.md`.
8. Normalize unions, remove duplicates, sort members canonically, and use
   occurs-checked first-order unification to reject overlap with witnesses.
9. Reject infinite inline layouts; recognize only specified built-in managed
   indirection boundaries.
10. Produce and verify a Typed AST with resolved identities, canonical `TypeId`s,
    substitutions, value categories, pattern facts, and explicit union
    injections.
11. Define the shared Core IR CFG: typed operations, block parameters,
    terminators, typed slots, source origins, and stage markers.
12. Lower the milestone's supported source forms into Generic Core IR and verify
    ID ownership, typing, slot initialization, exact control-flow signatures,
    and absence of forbidden sugar.

**Initial semantic slice**

- Primitive `i32`, `i64`, `bool`, and `unit` types.
- Function signatures and direct calls.
- Immutable and mutable bindings, exact `:=`, lexical scopes, and shadowing.
- Checked arithmetic representation, returns, blocks, and initial generics.
- Expected-type-directed literals, empty-value diagnostics, named function
  resolution, aliases, and structural union injection.

**Tests**

- Accepted/rejected pairs for each static rule in the initial slice.
- Scope, shadowing, duplicate name, visibility, and stable-ID snapshots.
- Generic inference from arguments and expected results, recursive generic
  calls, ambiguity, and unsatisfied constraints.
- Alias-cycle, union normalization/overlap, occurs-check, witness, and finite-
  layout cases.
- Typed AST snapshots and malformed Typed AST verifier tests.
- Generic Core IR evaluation-order snapshots and malformed CFG tests.
- Source spans on all negative cases; no invalid module reaches lowering.

**Exit gate:** accepted and rejected programs cover binding, mutation, calls,
return types, generic inference, and ambiguous empty values without invoking
LLVM.

### Milestone 3 — First native executable

**Objective:** compile a small verified EL program into a linked host-native
executable.

**Deliverables**

- Implement deterministic reachability roots beginning at `Main.main() -> i32`.
- Monomorphize reachable unconstrained generic functions and concrete generic
  layouts using `(declaration identity, normalized substitution)` worklist keys.
- Reuse identical specializations and reject any residual type parameter,
  projection, constraint call, derive request, or abstract layout.
- Compute target layout for `i32`, `i64`, `bool`, and `unit`.
- Lower Concrete Core IR arithmetic, direct calls, slots, blocks, and returns to
  private Inkwell-backed LLVM code.
- Emit mandatory integer checks with source origins and no debug/release semantic
  difference.
- Initialize the host target, verify LLVM modules, emit object files, and invoke
  the host compiler driver as linker.
- Add the native process entry shim that calls EL `Main.main` and forwards its
  `i32` result.
- Record target triple and pointer width in reproducibility metadata.
- Implement development and release output directories from the CLI contract.

**Tests**

- Monomorphization reuse, recursion, deterministic order, and malformed
  Concrete Core IR rejection.
- LLVM module verification and only narrowly targeted LLVM text assertions.
- Linker failure diagnostics without panics.
- Debug and release executable parity for arithmetic and exit status.

**Exit gate:** compile and run a program whose process exit status is calculated
by EL code.

### Milestone 4 — Core control flow and matching

**Objective:** complete the core expression-oriented control-flow model and
make cleanup explicit in Core IR.

**Deliverables**

- Comparisons, `if`, `while`, short-circuit `and`/`or`, and early `return`.
- Tuples, atoms, closed unions, explicit injection, discriminants, payloads,
  typed member patterns, tagged tuples, and exhaustive `match`.
- Pattern usefulness/exhaustiveness analysis with unreachable-arm diagnostics.
- Pipeline validation and desugaring that evaluates the left input before
  explicit arguments.
- `defer` registration: immediate target/argument evaluation for calls and
  by-value capture environments for blocks.
- Cleanup CFGs that preserve block results and run actions once in LIFO order on
  fallthrough and every normal `return` path.
- Unrecoverable failure terminators that deliberately bypass cleanup.
- LLVM lowering for block parameters, conditional branches, switches, aggregate
  values, and cleanup paths.

**Tests**

- Branch values, loops, nested returns, short-circuit effects, and pipelines.
- Exhaustive and non-exhaustive finite/structural matches, top-to-bottom arm
  order, typed union patterns, and unreachable arms.
- Deferred call timing versus block timing, capture snapshots, per-iteration
  cleanup, nested scopes, saved results, LIFO order, and failure skipping.
- Core IR cleanup verifier tests and LLVM verification for every generated
  module.

**Exit gate:** compile and run iterative factorial, a tagged-result parser, and
an exhaustive `i64 | string` match.

### Milestone 5 — Boehm GC integration

**Objective:** make managed native programs correct under conservative
collection before adding the bulk of heap-backed types.

**Deliverables**

- Define and version a small private runtime ABI.
- Vendor, build, and statically link the pinned Boehm GC release.
- Initialize the collector before any managed runtime state.
- Provide separate scanned and pointer-free allocation wrappers whose scan class
  never changes.
- Classify private runtime calls as allocating or non-allocating while treating
  every EL call as a possible collection point.
- Preserve live, aligned, unmodified base pointers across every collection point
  in locals, arguments, returns, globals, aggregate payloads, and hidden cleanup
  state.
- Add managed-global registration and allocation-exhaustion termination.
- Add a test-only stress mode that attempts collection at every managed
  allocation.
- Keep all Boehm types and controls out of compiler stage APIs and EL source.

**Tests**

- Graph retention and reclamation pressure in debug and optimized builds.
- Base-pointer survival in registers, stack slots, calls, recursion, unions,
  saved block results, and deferred captures.
- Scanned object graphs and pointer-free buffers.
- Allocation exhaustion category, source location, nonzero exit, and no cleanup
  unwinding.

**Exit gate:** an optimized native EL program retains a reachable heap graph and
survives collection-at-every-allocation stress while temporary allocations are
reclaimable.

### Milestone 6 — Data types and text

**Objective:** implement v1 value categories and text/binary semantics on the
verified managed runtime.

**Deliverable groups**

1. **Structs:** nominal identity, all-fields construction, field projection,
   generic specialization, immutable value semantics, and direct mutable-root
   field update by reconstruction.
2. **Sequential data:** lists, fixed arrays, slices, indexing, managed backing
   retention, O(1) subslicing, and explicit copying.
3. **Maps:** immutable operations, `Eq`/`Hash` key requirements, seeded hashing,
   deterministic insertion order, duplicate replacement, and order-independent
   equality.
4. **Text and binary:** valid UTF-8 `string`, `rune`, `bytes`, arbitrary-length
   `bits`, byte-aligned source bitstrings, conversions, bounds checks, and
   inspectable UTF-8 errors.
5. **Unicode:** bundle Unicode 17.0.0 data and implement untailored UAX #29
   revision 47 grapheme segmentation independent of host locale.
6. **Views:** eager codepoint/grapheme collections and lazy views that retain
   source backing storage.
7. **Buffer:** explicit value-style byte/string append operations and immutable
   conversion snapshots.
8. **Function values:** exact monomorphic direct code targets, indirect calls,
   visibility behavior, and generic specialization from expected types.
9. **Numbers:** remaining integer widths, pointer-sized integers, floats,
   explicit checked conversions, shifts, bitwise operations, and wrapping APIs.
10. **Collection helpers:** `Enum` traversal machinery needed by the data layer
    plus the fixed List/Array/Slice/Bytes operations. Protocol surface integration
    is completed in Milestone 7.

**Tests**

- Construction, inference, layout, access, and immutable-copy semantics for
  every data category.
- Fixed-array length inference and rejection of symbolic/derived lengths.
- Map insertion order across seeds and all update/remove/reinsert cases.
- Bounds and numeric failure categories in debug and release.
- Valid/invalid UTF-8 offsets and parity between string and buffer validation.
- Full Unicode 17.0.0 `GraphemeBreakTest.txt` conformance for eager, lazy, and
  length APIs.
- GC stress for nested composite graphs, views, base retention, immutable
  sharing, and function values.
- Compile-time rejection of `string[index]`.

**Exit gate:** process valid UTF-8, reject invalid UTF-8, retain composite heap
graphs under GC stress, and diagnose integer indexing on `string`.

### Milestone 7 — Protocols and iteration

**Objective:** complete coherent static protocols, deriving, protocol-backed
operators, and generic traversal.

**Deliverables**

- Parse-to-Typed-AST support for `defprotocol`, `defimpl`, `Self`, explicit
  associated type declarations/assignments, and qualified projections.
- Protocol ownership/orphan checks across the resolved package graph.
- Implementation completeness, exact substituted signatures, uniqueness, and
  overlap detection without using positive constraints as disambiguation.
- Generic constraint checking once and concrete implementation selection during
  monomorphization.
- Core protocols: `Eq`, `Ord`, `Show`, `Hash`, `Iterable`, and `Concat`.
- Compiler-generated standard implementations and `@derive` for `Eq`, `Ord`,
  `Show`, and `Hash` when all field constraints hold.
- `for` lowering through one statically selected `Iterable` implementation,
  immutable cursors, associated `Item`, and irrefutable patterns.
- Protocol lowering for nonprimitive equality/ordering and `++`.
- Complete `Enum` APIs with deterministic traversal and short-circuit behavior.

**Tests**

- Completeness, duplicate/unknown associated types, orphan rejection, coherence,
  overlapping generic heads, and alias/union target rejection.
- Concrete dispatch at multiple instantiations, projection normalization,
  specialization reuse, and defensive post-substitution checks.
- Derivation success/failure for generic and nested structs.
- `Eq`/`Ord`/`Hash` law tests for standard implementations.
- Every standard iteration order, map tuple order, cursor threading, and
  irrefutable/refutable `for` patterns.
- Concatenation for `string`, `bytes`, `bits`, and lists.

**Exit gate:** derive protocols for a generic struct, instantiate constrained
generic functions at several concrete types, iterate multiple container types,
and concatenate every standard `Concat` type.

### Milestone 8 — Packages, I/O, and standard library

**Objective:** turn the compiler into the complete manifest-driven v1 tool and
provide recoverable process/file boundaries.

**Deliverable groups**

1. Strict `el.toml` parsing for package ID, namespace, semantic version,
   dependencies, and optional single executable target; unknown keys are errors.
2. Strict `src/` discovery and mechanical path-to-module validation.
3. Package-relative, dependency-qualified, and prelude name resolution with
   collision and ambiguity diagnostics.
4. Exact path dependency resolution and full-commit Git dependency resolution.
5. Graph-wide package/source/version/revision uniqueness, namespace uniqueness,
   dependency cycle detection, and deterministic traversal.
6. Deterministic `el.lock` generation/refresh and non-mutating `--locked`
   verification.
7. Full `el check`, `el build`, and `el emit llvm-ir --module ...` behavior,
   output paths, streams, and status classes.
8. `Reader` and `Writer` with associated errors and tagged result values.
9. Console conveniences constrained by `Show`, plus recoverable standard stream
   handles.
10. Typed file reader/writer handles, shared external identity, close-through-
    alias behavior, tagged closed-handle errors, and explicit `defer` cleanup.
11. Stable error kind/operation/code inspection, internal interrupted-call
    retry, and exact protocol laws.
12. Launch-time process arguments/environment snapshots with strict native
    UTF conversion and exact platform-specific string path conversion.
13. Complete reserved prelude and standard modules with no implicit functions.

**Tests**

- Manifest keys, module mapping, visibility, package ownership, dependency
  conflicts, cycles, namespaces, exact versions, Git pins, and lock stability.
- `--locked` missing/stale behavior without filesystem mutation.
- CLI option orders, duplicate/unknown options, streams, statuses 0/1/2, output
  locations, library-only behavior, and absence of deferred commands.
- Complete writes, EOF, flush, console newline behavior, typed file modes,
  error mappings/accessors, alias/close behavior, and interrupted calls.
- Argument ordering, executable-name exclusion, environment snapshots, invalid
  native text, embedded NULs, and exact Unix/Windows path conversion.
- Multi-package and multi-module end-to-end builds using only source packages.

**Exit gate:** build a multi-module manifest target that reads, transforms, and
writes data while handling every recoverable error through exhaustive `match`.

### Milestone 9 — V1 stabilization

**Objective:** prove the documented language and tool behave consistently on
every claimed host and freeze the v1 delivery contract.

**Deliverables**

- Complete grammar, type-system, IR, CLI, runtime, standard-library, and package
  conformance suites traceable to specification sections.
- Promote conforming programs from `EXAMPLES.md` into executable fixtures and
  its invalid examples into negative fixtures.
- Stabilize diagnostic codes, labels, source presentation, and path handling.
- Run all semantic tests in development and optimized builds where required.
- Verify GC stress mode on every supported target.
- Freeze the v1 manifest format and private runtime ABI version for the matching
  compiler distribution.
- Document supported targets, LLVM/Boehm/runtime packaging, linker prerequisites,
  licenses, reproducible build metadata, installation, and troubleshooting.
- Audit the binary to ensure no v2 feature, extra CLI command, public FFI,
  unstable native type, or host-only behavior leaked into v1.

**Exit gate:** all v1 examples and negative conformance programs behave
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

1. **Workspace and pins:** toolchain, workspace manifests, license policy,
   initial `el-span`, `el-runtime`, `el-driver`, and `el-cli` crates, plus a CI
   build.
2. **CLI shell:** exact help/version and strict command parsing with CLI
   conformance fixtures.
3. **Source foundation:** source database, `FileId`, `Span`, line mapping,
   structured diagnostics, and renderer snapshots.
4. **Project discovery:** ancestor manifest search and minimal project errors.
5. **Lexical grammar:** source encoding, whitespace/comments, identifiers,
   keywords, literal tokens, and newline model.
6. **Declaration/type grammar:** module, functions, signatures, bindings, scalar
   types, and the first typed-main parse.
7. **Expression grammar:** precedence, postfix forms, blocks, and arithmetic.
8. **AST adapter:** parser-independent AST, span propagation, deterministic
   snapshots, and the Milestone 1 starter exit test.
9. **Remaining v1 grammar:** composites, generics, protocols, patterns,
   bitstrings, control flow, validation, and recovery conformance.
10. **Semantic foundations:** typed IDs, declaration collection, namespaces,
    canonical type arena, and the first Typed AST slice.
11. **Initial checking:** bindings, mutation, calls, returns, literals, expected
    types, aliases, generics, unions, and layout validation.
12. **Core IR:** shared CFG, verifier, source-to-Generic lowering, and snapshots.
13. **First backend:** monomorphization, Concrete verifier, LLVM lowering,
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

### 11.6 LLVM 22.1.0 availability

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

Use this table as the high-level implementation ledger. A milestone changes to
complete only after its exit gate and workspace checks pass.

| Milestone | Status | Exit evidence |
| --- | --- | --- |
| 0 — Project skeleton | Not started | `el --help`; CI workspace build |
| 1 — Parser and AST | Not started | typed-main AST snapshot |
| 2 — Names and types | Not started | accepted/rejected semantic suite without LLVM |
| 3 — First native executable | Not started | computed native process exit status |
| 4 — Core control and matching | Not started | factorial, tagged parser, union match |
| 5 — Boehm GC | Not started | optimized graph retention under GC stress |
| 6 — Data types and text | Not started | UTF-8, composites, views, GC, string-index rejection |
| 7 — Protocols and iteration | Not started | derive, constrained generics, iteration, concat |
| 8 — Packages, I/O, stdlib | Not started | multi-module recoverable I/O program |
| 9 — V1 stabilization | Not started | cross-target full conformance under GC stress |

## 13. Definition of implementation-ready

Implementation can begin when all of the following are true:

- [x] Normative syntax, type, IR, runtime, and CLI contracts exist.
- [x] All design questions currently listed in `DESIGN.md` are resolved.
- [x] Milestone order and exit tests are defined.
- [x] The initial crate and compiler-stage boundaries are proposed.
- [x] Cross-cutting identity, span, diagnostic, determinism, and verifier rules
  are explicit.
- [ ] Exact Rust toolchain is selected and pinned.
- [ ] Exact Boehm GC release/source is selected and pinned.
- [ ] LLVM 22.1.0 is installed or provisioned for backend CI and local backend
  work.
- [ ] Milestone 0 workspace and CI are created.

The first implementation action is therefore Milestone 0, change 1: create the
workspace, make the two remaining version selections, record native prerequisites,
and establish a green build before adding language behavior.
