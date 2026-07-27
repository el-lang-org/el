# EL Intermediate Representations

Status: version 1 implementation contract
Language: **EL**
Last updated: 2026-07-26

## 1. Scope and authority

This document defines the boundaries and invariants of EL's compiler
representations. It is authoritative for the bootstrap compiler, its verifier,
and representation-level tests, but it is not a source-language compatibility
promise. [GRAMMAR.md](GRAMMAR.md) owns accepted syntax, [TYPES.md](TYPES.md)
owns static semantics, and [DESIGN.md](DESIGN.md) owns architectural decisions
and observable runtime behavior.

The compiler may revise data structures or textual debug formats without a
language-version change when all invariants here and all observable EL behavior
remain intact. A change to the stage boundaries or semantic invariants requires
an accepted design decision before implementation depends on it.

This document describes logical representations rather than Rust declarations.
Rust enums, arenas, indices, interning strategies, and ownership arrangements
may differ while preserving the same information and checks.

## 2. Pipeline

The unified compiler and package tool is named `el`.

```text
.el source
    |
    v
PEG parse tree -> AST -> name resolution + type checking -> Typed AST
    -> generic Core IR -> monomorphization -> concrete Core IR
    -> LLVM IR -> LLVM optimization -> object file
    -> system linker -> native executable
```

The middle of the pipeline is deliberately split into a **typed AST** and a
small **Core IR**:

```text
PEG parse tree -> AST -> name resolution + type checking -> Typed AST
    -> desugaring -> generic Core IR -> monomorphization
    -> concrete Core IR -> LLVM IR
```

Yes, the typed AST belongs immediately after the source AST's names are resolved
and types are checked. It preserves source-level structure for good diagnostics,
while Core IR removes syntax sugar before backend code generation.

## 3. Bootstrap implementation

- Implementation language: Rust.
- Parsing formalism: PEG.
- PEG library: `pest` 2.8.7 with `pest_derive` 2.8.7, using a checked-in `.pest`
  grammar. The exact versions are pinned in the Rust lockfile.
- Expression precedence: encode explicit precedence levels in the grammar or
  use the parser library's Pratt parsing support; do not use left recursion.
- Backend: LLVM through Rust bindings.
- LLVM version: 22.1.8.
- Rust binding: Inkwell 0.9.0 with the `llvm22-1-prefer-dynamic` feature. The
  exact crate version is pinned in the Rust lockfile, and compiler distributions
  provide the matching LLVM shared library.
- Compilation mode: native AOT only.
- Initial target: the compiler host target.
- Linking: invoke the platform C linker through the installed compiler driver
  initially; direct LLD integration can come later.

## 4. Compiler representations

Keep these layers distinct:

- **Parse tree:** mirrors the grammar and retains source spans.
- **AST:** represents source constructs without parser noise; names may still be
  unresolved and expressions do not yet carry final types.
- **Typed AST:** preserves source constructs, while every expression has a type,
  every name has a unique symbol identity, and each protocol call names either a
  concrete implementation or a generic constraint. Implicit structural-union
  injections are explicit typed nodes by this stage.
- **Generic Core IR:** contains a smaller typed language after pipelines, `++`,
  `for`, pattern matching, and derived implementations are expanded or lowered;
  it may still refer to declared type parameters.
- **Concrete Core IR:** contains only reachable concrete instantiations after
  monomorphization and is the input to LLVM lowering.
- **LLVM IR:** target-oriented representation; it must not be used as EL's type
  checker or primary semantic model.

Every AST, typed AST, and diagnostic-producing Core IR node should retain a byte
span into a source file. Line and column numbers are derived for display.

## 5. Initial lowering strategy

- Core IR preserves the language's left-to-right, exactly-once evaluation order;
  LLVM optimizations may reorder operations only when the change is unobservable.
- Immutable scalar locals become LLVM SSA values where practical.
- Mutable locals may initially lower to entry-block `alloca` instructions plus
  loads/stores; LLVM's `mem2reg` pass can promote safe cases to SSA.
- A direct struct-field update evaluates its right-hand side using the old
  struct value, constructs the updated shallow value, and stores that value into
  the mutable root. Lowering may use an in-place field store only when no EL
  program can observe a difference.
- `if` expressions lower to control-flow blocks and a `phi` value.
- `while` lowers to condition, body, and exit basic blocks.
- `for pattern in value` lowers through the statically selected `Iterable`
  implementation.
- `match` lowers to tests and branches after exhaustiveness checking.
- A concrete structural union lowers to a hidden discriminant plus an aligned
  payload; typed injections construct a member and exhaustive matches switch on
  the discriminant before lowering the selected member pattern.
- Every EL call is conservatively a possible collection point. Live managed
  values remain raw base pointers in collector-visible locations across it;
  optimization must not leave the only live representation as an integer,
  tagged value, or interior pointer. Runtime calls carry an internal
  allocating/non-allocating classification.
- A deferred call evaluates into hidden target/argument slots at registration;
  a deferred block captures referenced values into a compiler-generated
  immutable environment. Cleanup blocks preserve the scope result and thread
  registered actions through normal exits in reverse registration order. Saved
  results, call values, and capture environments remain visible to the GC.
- `return` lowers to the function exit only after routing control through the
  cleanup blocks for every exited lexical scope.
- `left ++ right` lowers to the selected `Concat.concat(left, right)` call.
- `left |> call(args)` is rewritten to `call(left, args)` before Core IR.
- Each reachable named function value lowers to a concrete monomorphized code
  target with no environment. Indirect calls use its exact function type;
  representation and calling convention remain private runtime ABI details.
- Integer arithmetic, shifts, and conversions emit the required overflow, range,
  zero-divisor, and shift-count checks. Float lowering does not enable LLVM
  fast-math flags that weaken EL's IEEE semantics.
- Monomorphization starts from concrete entry points, specializes reachable
  generic functions and named types, resolves their constrained protocol calls,
  and reuses an existing specialization for an identical type substitution.
- Runtime operations are called through a small, versioned internal ABI.

The runtime ABI is private to a matching compiler distribution. V1 packages are
compiled from EL source as part of the resolved build and do not exchange stable
object files. Compiler versions need not preserve object-file compatibility,
symbol names, aggregate layouts, or calling conventions.

Build output records the LLVM target triple, pointer width, and pinned Unicode
data version for diagnostics and reproducibility. Intentional target-dependent
source behavior is limited to
`isize`/`usize` width, `native` bitstring byte order, operating-system APIs and
error values, process-exit-code observation, and practical allocation or
collection limits.
## 6. Common identity and provenance

Every representation uses stable compiler-local identities rather than source
spellings as semantic keys:

- `FileId` identifies one source file and indexes its UTF-8 source text.
- `Span` is a half-open byte range within one `FileId`.
- `ModuleId`, `DeclId`, `SymbolId`, `TypeId`, `ImplId`, and `FunctionId` identify
  resolved entities in their owning compilation graph.
- `ValueId`, `BlockId`, and `SlotId` are local to one Core IR function.

User-facing names are retained for diagnostics and debug output. Generated
nodes carry the narrowest useful originating span and a generated-kind marker.
A lowering must never manufacture a source location that implicates unrelated
syntax. Inlined or synthesized operations retain an origin chain sufficient to
report the source operation responsible for a mandated runtime check.

Interned identities are not stable across compiler invocations and must not
appear in package compatibility interfaces or reproducible output without
deterministic renumbering.

## 7. AST contract

The AST contains one node for every source construct that matters to diagnostics
or tooling and omits punctuation and parser-only wrapper rules. It preserves:

- declaration and source order;
- the distinction between explicit and omitted annotations;
- original identifier spellings and spans;
- parentheses only when needed for exact formatting or diagnostics;
- literal spelling plus its decoded or validated value;
- source-level pipelines, protocol calls, patterns, `defer`, and field updates;
- invalid or missing nodes used solely for diagnostic recovery.

The AST does not contain resolved symbols, inferred types, selected protocol
implementations, implicit union injections, or target layouts. No backend phase
may consume the untyped AST.

A valid AST passed to name resolution contains no parser-recovery node. The
parser may build such nodes while diagnosing malformed input, but compilation
of that module stops before semantic analysis.

## 8. Typed AST contract

The typed AST preserves the AST's source structure and adds:

- a canonical `TypeId` to every expression and pattern;
- a unique resolved identity to every declaration, reference, and binding;
- the selected declaration for every direct function reference or call;
- explicit type substitutions for generic uses once known in the current
  generic context;
- selected `ImplId` values for concrete protocol calls and a constraint identity
  for calls that remain abstract in a generic body;
- resolved associated-type projections where the current context permits it;
- explicit nodes for every implicit structural-union injection;
- reachability, exhaustiveness, and irrefutability results for patterns; and
- value-category facts needed to validate mutable roots and direct field update.

The typed AST contains no unresolved source name and no unconstrained inference
variable. A compiler-private error type may occur only after a diagnostic has
already made the module ineligible for lowering. A Typed AST accepted by its
verifier is therefore fully typed under [TYPES.md](TYPES.md).

Source constructs remain recognizable here even when they will disappear from
Core IR. This is the final representation used for source-oriented type
diagnostics.

## 9. Core IR shape

Generic and Concrete Core IR share one logical instruction set. A Core module
contains type declarations, implementation metadata needed by the current
stage, and functions. Each function contains parameters, local storage slots,
basic blocks, and a declared result type.

A basic block is a sequence of typed operations followed by exactly one
terminator. Operations produce zero or one `ValueId`; terminators produce no
value. Block parameters represent values selected by predecessor control flow,
including the result of a lowered `if` or `match`. This gives Core IR SSA value
flow without requiring mutable source bindings themselves to be in SSA form.

A `SlotId` represents compiler-local addressable storage such as a mutable local
or a hidden saved result/capture slot. Slot allocation is not observable EL heap
allocation. The verifier requires one declared type per slot, initialization
before load, and an exactly matching type on every store. Immutable source
bindings normally become `ValueId` aliases and do not require slots.

The logical operation families are:

- scalar, atom, string, bytes, and aggregate constants;
- tuple, struct, array, list, map, and bitstring construction;
- field projection, checked indexing, and slice/view operations;
- slot load and store;
- checked numeric, bitwise, comparison, boolean, and conversion operations;
- structural-union injection, discriminant inspection, and member projection;
- direct calls, exact-typed indirect calls, generic constrained calls, and
  private runtime calls;
- explicit allocation and runtime operations with allocating/non-allocating
  classification; and
- diagnostic-only source assertions needed for mandated failures.

The logical terminators are:

- unconditional branch with block arguments;
- conditional branch;
- finite discriminant or scalar switch;
- function return;
- unrecoverable failure with its stable category and source origin; and
- unreachable, permitted only after a terminating predecessor or a diagnosed
  internal impossibility.

This list defines capabilities, not mandatory Rust variant names. A
representation may combine or split operations if its verifier can enforce the
same invariants.

## 10. Generic Core IR

Generic Core IR may contain declared type parameters, normalized associated-type
projections, and constrained protocol calls. It must not contain:

- pipelines;
- `++` syntax;
- source `for` loops;
- source patterns or match-arm fallthrough;
- derive requests;
- direct struct-field update syntax;
- `defer` registration syntax; or
- unresolved names or inference variables.

Lowering from Typed AST expands those forms exactly once while preserving source
evaluation order. Pattern matching becomes explicit tests, projections, and
control-flow edges. A field update becomes evaluation of the right-hand side
against the old aggregate followed by construction and storage of the new root
value. `for` becomes calls to one selected `Iterable` implementation plus
cursor-threading control flow.

Deferred calls have their target and arguments evaluated into hidden values or
slots at the registration point. Deferred blocks have explicit immutable
capture values. All exits from a scope are routed through shared LIFO cleanup
blocks; the original result is carried through those blocks.

A generic function is verified once using its declared constraints. Recursive
generic calls must carry the current substitution; Core IR cannot express
polymorphic recursion.

## 11. Monomorphization

Monomorphization starts from concrete entry points and other compiler-defined
roots. A worklist key consists of the generic declaration identity plus its
fully normalized concrete type substitution. Equal keys reuse one
specialization.

For each specialization the pass:

1. substitutes all type parameters;
2. normalizes aliases, unions, and associated-type projections;
3. resolves every constrained protocol call to one coherent implementation;
4. specializes all reachable named types, functions, and function values;
5. rechecks union disjointness and finite layout as defensive invariants; and
6. adds newly referenced specializations to the worklist.

Failure to resolve an associated type or protocol call at this stage is a
compiler defect if the Generic Core IR verifier accepted the function.
Specialization order must be deterministic for reproducible diagnostics and
debug snapshots.

## 12. Concrete Core IR

Concrete Core IR contains no type parameter, inference variable, unresolved
associated-type projection, generic constrained call, derive request, or
abstract layout. Every function, call target, function value, aggregate, union,
and runtime operation has a concrete type.

Before LLVM lowering, target layout computes size, alignment, field offsets,
union payload layout, and the target-width forms of `isize` and `usize`.
Layout facts are private compiler data and do not become EL source guarantees.

Managed values carry enough type classification for the backend to distinguish:

- a collector-visible base reference;
- a view that must retain a base reference;
- scanned aggregate storage that may contain managed references; and
- pointer-free payload storage.

No optimization may erase the last collector-visible base reference live across
a possible collection point. Interior or derived pointers never substitute for
that base reference.

## 13. Verification

Every boundary verifies its input in debug builds and in dedicated IR tests.
Malformed compiler-generated IR is an internal error, never an EL diagnostic.
At minimum, the verifier checks:

- every ID belongs to the current module or function and is defined once;
- every block has one terminator and every referenced block exists;
- predecessor arguments exactly match target block-parameter types;
- operation operand and result types satisfy [TYPES.md](TYPES.md);
- slots are initialized before use and stores match their declarations;
- control-flow targets are structurally reachable or explicitly unreachable;
- source-level sugar forbidden at the current stage is absent;
- Generic Core IR has only declared parameters and justified constraints;
- Concrete Core IR is fully concrete and all calls have exact signatures;
- union injections and projections use normalized members and valid tags;
- runtime-failure operations carry a valid category and source origin;
- allocating operations preserve all live managed base references; and
- cleanup control flow runs registered actions once, in reverse order, on
  fallthrough and `return`, but not on unrecoverable failure.

## 14. Debug form and tests

AST, Typed AST, Generic Core IR, and Concrete Core IR each provide a deterministic
human-readable debug form for snapshots. The debug form prints stable source
names and deterministically numbered IDs; it does not expose arena addresses,
hash-map order, or nondeterministic symbol names.

Required representation tests include:

- AST snapshots for every grammar production and span boundary;
- Typed AST snapshots for name resolution, inferred types, protocol selection,
  union injection, and pattern analysis;
- Core IR snapshots for evaluation order and every desugaring;
- verifier rejection tests built from deliberately malformed IR;
- monomorphization tests for deduplication, recursion, associated types, and
  deterministic ordering;
- cleanup CFG tests for fallthrough, nested return, loops, and failure;
- collection-at-every-allocation tests in debug and optimized builds; and
- small targeted LLVM IR checks only where a backend invariant cannot be tested
  at the Core IR level.
