# EL Language Design

Status: living design document  
Working language name: **EL**  
Last updated: 2026-07-26

This document is the source of truth for EL's requirements, semantics, compiler
architecture, open questions, and design decisions. When implementation and this
document disagree, either the implementation is a bug or the difference must be
recorded in the decision log.

## 1. Vision

EL is a statically typed, garbage-collected systems programming language with:

- Elixir-inspired surface syntax;
- a Go-inspired type system;
- immutable local bindings by default;
- explicit local mutability with `mut` and rebinding with `:=`;
- native ahead-of-time (AOT) compilation through LLVM; and
- a compiler bootstrapped in Rust using a PEG parser.

EL should feel small, readable, and predictable. Version 1 deliberately favors
a coherent language that we can finish over a wide language with unfinished
features.

### 1.1 Design philosophy: simplicity

EL's primary design philosophy is **simplicity**: language features should be
easy to read, explain, compose, and implement correctly.

Simplicity does not mean removing useful power. It means:

- prefer a small set of orthogonal features over many overlapping features;
- make control flow, mutation, allocation, and failure visible in source code;
- provide one unsurprising way to express common operations;
- keep syntax regular and make desugaring rules explicit;
- reject implicit conversions and hidden control flow;
- keep the compiler pipeline inspectable, with small intermediate forms; and
- add a feature only when its interaction with existing features is understood.

When two designs are otherwise comparable, EL chooses the one with fewer rules
and fewer feature interactions. The decision log must explain exceptions.

## 2. Goals and non-goals

### 2.1 Version 1 goals

- Produce a standalone native executable for the host platform.
- Detect type errors at compile time with useful source locations.
- Provide value semantics and immutable bindings by default.
- Support explicit mutable local bindings.
- Support functions, lexical blocks, conditionals, loops, and recursion.
- Support lower-case primitive types and the accepted composite types.
- Support protocols, explicit protocol implementations, and deriving.
- Support tagged tuples and pattern matching for recoverable errors.
- Support `while` loops and protocol-backed `for ... in` loops.
- Support the pipeline operator (`|>`).
- Support strings, binary data, and garbage-collected heap values.
- Build projects described by an `el.toml` manifest.
- Make source programs visually familiar to an Elixir programmer.
- Keep the runtime and compiler small enough to understand.

### 2.2 Version 1 non-goals

- Concurrency, processes, actors, channels, or an async runtime.
- A BEAM-compatible runtime or Elixir/Erlang interoperability.
- A bytecode interpreter or JIT compiler.
- Macros, metaprogramming, or compile-time code execution.
- Inheritance or classes.
- Exceptions, `throw`, or catchable panics. Recoverable errors are values.
- General user-defined generics (revisit after the built-in parametric types and
  protocol system are stable).
- User-facing foreign-function interfaces (FFI).
- Full source compatibility with either Elixir or Go.
- A self-hosted compiler.

### 2.3 Systems-programming intent

Native code alone does not make a language suitable for systems programming.
EL needs explicit-width numeric types, defined data layout, predictable runtime
behavior, and deterministic handling of operating-system resources. Garbage
collection manages memory, but it must not manage files, sockets, or other
scarce resources.

V1 does not expose an FFI or an `unsafe` language feature. The compiler runtime
and standard library may use native platform APIs internally, but that boundary
is not part of the EL language. A public FFI is out of scope unless a future
decision explicitly adds it.

## 3. Language tour

This example is illustrative. Syntax remains subject to parser prototypes.

```el
defmodule Main do
  @derive [Eq, Show, Hash]
  defstruct Point do
    x: i64
    y: i64
  end

  defp distance_squared(p: Point) -> i64 do
    p.x * p.x + p.y * p.y
  end

  def main() -> i32 do
    origin = %Point{x: 0, y: 0}
    mut n: i64 = 0

    while n < 10 do
      n := n + 1
    end

    [origin]
    |> List.map(distance_squared)
    |> IO.println()

    0
  end
end
```

In this example:

- `origin` is an immutable binding whose type is inferred as `Point`.
- `n` is a mutable local binding and may be rebound with `:=`.
- `=` introduces a binding; it is not assignment.
- `def` is public and `defp` is private to the module.
- `@derive` asks the compiler to generate protocol implementations.
- `|>` inserts its left operand as the first argument of the call on its right.
- A block evaluates to its final expression.
- Function parameter and return types are explicit.

## 4. Lexical structure

### 4.1 Source files

- Source file extension: `.el` (provisional; this conflicts visually with Emacs
  Lisp and may be reconsidered).
- Source text is UTF-8.
- Identifiers use ASCII letters, digits, and `_` in the initial implementation.
- Value and function names use `snake_case`.
- Primitive type names are lower case.
- Custom type, protocol, and module names use `PascalCase`.
- `#` begins a line comment.
- Nested block comments are deferred.

### 4.2 Keywords

Initial reserved words:

```text
def defimpl defmodule defp defprotocol defstruct do else end false for if in
match mut return true while
```

`@derive` and `@type` are built-in attributes and are reserved as complete
attribute names.

Future keywords are not reserved until their feature is accepted.

## 5. Bindings and mutability

### 5.1 Immutable binding

`=` creates a new immutable binding:

```el
x = 41
y: i64 = x + 1
```

Rebinding `x` in the same lexical scope is a compile-time error. A nested scope
may shadow it; the compiler should warn when the shadowing appears accidental.

### 5.2 Mutable binding

`mut` creates a mutable local binding:

```el
mut x: i64 = 1
x := x + 1
```

Rules:

- `mut` is part of a binding declaration, not a type qualifier.
- `:=` updates an existing mutable local binding.
- `:=` is a statement and evaluates to `unit`.
- Assigning to an immutable binding is a compile-time error.
- The assigned expression must have exactly the binding's type.
- A mutable binding must be initialized.
- Parameters are immutable in v1.

In the initial v1 semantics, mutability applies to the local binding itself. It
does not implicitly make an aggregate, its fields, or aliased values mutable.
Field assignment (`point.x := 2`) and indexed assignment (`items[0] := 2`) are
deferred until aliasing and value/reference semantics are designed.

### 5.3 Why `=` and `:=` differ

EL intentionally does not copy Go's meaning of `:=`. The language uses:

```text
name = expression       introduce an immutable binding
mut name = expression   introduce a mutable binding
name := expression      update a mutable binding
```

This rule is simple to parse, simple to type-check, and makes mutation visible.

## 6. Types

EL's type system is Go-inspired rather than Go-identical: it is small and
static, uses explicit conversions, has no implicit numeric coercions, and favors
simple nominal types. EL protocols use explicit `defimpl` declarations rather
than Go's implicit structural interface satisfaction.

### 6.1 Primitive and built-in scalar types

All primitive type names are lower case:

```text
bool
i8 i16 i32 i64 isize
u8 u16 u32 u64 usize
f32 f64
rune
string bytes bits
unit
```

`rune` is one Unicode scalar value (a Unicode code point excluding surrogate
values). `string`, `bytes`, and `bits` have dedicated semantics in section 7.
`Buffer` is a standard-library type, not a primitive, so it uses PascalCase.

Implementation starts with `bool`, `i64`, `i32`, and `unit`; the other built-ins
are added after the pipeline works end to end.

Rules:

- Integer literals are checked against the expected type when one exists.
- An otherwise unconstrained integer literal defaults to `i64`.
- An otherwise unconstrained floating literal defaults to `f64`.
- Arithmetic requires matching operand types.
- Conversions are explicit, for example `i32(value)`.
- Integer overflow behavior is an open question.
- EL has no `null`, null literal, nullable reference, or implicit zero reference.

### 6.2 Custom types and aliases

Custom type names use PascalCase. `@type` creates a transparent alias; it does
not create a distinct nominal type:

```el
@type UserId = u64
@type ParseResult = {:ok, i64} | {:error, string}
```

The second form is a closed union of tagged tuple shapes. Closed tagged unions
allow the type checker to verify exhaustive `match` expressions. A future
distinct-type feature must use different syntax rather than changing alias
semantics.

### 6.3 Structs

`defstruct` defines a nominal product type with named fields:

```el
@derive [Eq, Show, Hash]
defstruct User do
  id: u64
  name: string
end

user = %User{id: 1, name: "Ada"}
```

All fields must be initialized in v1. Fields are immutable after construction.
There is no implicit zero-value construction. Whether structs are copied values,
GC references, or have two explicit forms must be decided before LLVM lowering.

`@derive` requests compiler-generated `defimpl` blocks. Derivation succeeds only
when every participating field supports the requested protocol. For example,
deriving `Eq` for `User` requires `Eq` implementations for `u64` and `string`.

### 6.4 Composite types

V1 supports these composite categories:

- tuples: heterogeneous fixed-size values such as `{string, i64}`;
- lists: immutable homogeneous linked lists, type `[a]`;
- maps: immutable key/value collections, type `map[k, v]`;
- arrays: fixed-size homogeneous values, type `[a; N]`;
- slices: bounded views over contiguous elements, type `slice[a]`;
- structs: nominal records declared with `defstruct`;
- functions: callable values such as `(i64, i64) -> i64`; and
- atoms: interned symbolic literals such as `:ok`, `:error`, and `:eof`.

Built-in composites may be parameterized even though user-defined generic types
and generic functions are deferred. Exact ownership and lifetime rules for
slices must be resolved before their implementation.

Atoms have singleton literal identities. They are commonly used as the first
element of a tagged tuple. Arbitrary conversion of runtime strings to atoms is
not supported because an unbounded atom table would create a memory leak.

### 6.5 Functions and visibility

```el
def add(left: i64, right: i64) -> i64 do
  left + right
end

defp validate(value: i64) -> bool do
  value >= 0
end
```

- `def` declares a public module function.
- `defp` declares a function visible only within its module.
- Parameter and return types are mandatory.
- The final expression is the normal return value.
- `return expression` is allowed for early return.
- Omitting `-> type` means `-> unit`.
- Overloading by parameter types is not supported.
- Named functions can be used as function values.
- Closures and anonymous functions are deferred.
- User-defined generic functions are deferred in v1.

### 6.6 Protocols and implementations

`defprotocol` declares behavior and `defimpl` explicitly implements that
behavior for a type:

```el
defprotocol Show do
  def show(value: Self) -> string
end

defimpl Show, for: Point do
  def show(value: Point) -> string do
    # implementation
  end
end
```

Protocol conformance is checked statically. A protocol value or generic protocol
dispatch may require runtime dispatch; its representation is a v1 design task.
There is no implicit implementation based only on a matching method name.

`Self` refers to the implementation type. Protocol-associated types are allowed
only where required by core protocols such as `Iterable`; general user-defined
generics remain deferred.

### 6.7 Core standard-library protocols

The initial standard library defines:

- `Eq`: equality; backs `==` and `!=` for non-primitive values.
- `Ord`: ordering; backs `<`, `<=`, `>`, and `>=` where implemented.
- `Show`: readable conversion to `string`.
- `Hash`: produces hash input for map keys.
- `Iterable`: exposes an item type and iteration used by `for ... in`.
- `Writer`: writes `string` or `bytes` and returns tagged success/error values.
- `Reader`: reads data and returns tagged success/error or end-of-input values.
- `Concat`: concatenates two values of the same type.

The `Concat` operation is spelled `concat`:

```el
defprotocol Concat do
  def concat(left: Self, right: Self) -> Self
end
```

The standard library implements it for `string`, `bytes`, `bits`, and `[a]`.
The `++` operator desugars to `Concat.concat(left, right)`. Concatenating an
immutable linked list requires copying its left spine, so repeated list `++`
inside a loop may be quadratic; diagnostics or documentation should flag this.

## 7. Strings and binaries

### 7.1 `string`

`string` is distinct from binary data and is always valid UTF-8. Construction
from untrusted bytes validates encoding and returns a tagged success/error value.

Integer indexing is not supported: `text[i]` is a compile-time error. UTF-8 code
points have variable encoded width, so such an operation would hide whether `i`
means a byte offset, code-point offset, or user-perceived character offset.
Programs use these `String` functions instead:

```el
String.byte_size(text)
String.length(text)
String.bytes(text)
String.codepoints(text)
String.graphemes(text)
```

Their semantics are:

- `byte_size(s) -> usize` returns the number of bytes in the UTF-8 encoding. It
  is O(1) because `string` stores its byte length.
- `length(s) -> usize` returns the number of Unicode grapheme clusters. It is
  generally O(n), matching Elixir's human-text-oriented meaning of length.
- `bytes(s)` returns a read-only, non-allocating view whose items are `u8`.
- `codepoints(s)` returns an iterable view whose items are `rune` values.
- `graphemes(s)` returns an iterable view whose items are valid `string` slices,
  one extended grapheme cluster at a time.

The exact concrete view types remain an API-design task, but these operations do
not return mutable access to the string's storage. Code-point and grapheme
iteration are distinct operations.

### 7.2 `bytes`, `bits`, and `rune`

- `bytes` is a raw, byte-aligned sequence. It carries no text encoding promise.
- `bits` is a bit sequence of arbitrary length and generalizes `bytes`.
- `rune` is one Unicode scalar value, not a byte and not a grapheme cluster.

Conversions among these types are explicit. Converting `bytes` to `string`
validates UTF-8. Converting `string` to `bytes` exposes its UTF-8 encoding.

### 7.3 `Buffer`

`Buffer` is a growable standard-library builder for constructing `string` or
`bytes` without repeated immutable concatenation. Its initial value-style API
keeps mutation explicit:

```el
mut buffer = Buffer.new()
buffer := Buffer.append(buffer, "hello")
buffer := Buffer.append(buffer, " world")
text = Buffer.to_string(buffer)
```

The runtime may reuse storage when it can prove uniqueness, but observable
semantics remain local rebinding. `Buffer.to_string` validates UTF-8 when the
buffer was built from raw bytes.

### 7.4 Bitstring construction and matching

The target design supports Elixir-style bit-level construction and pattern
matching. V1 supports byte-aligned segments and matching; the `bits` value model
itself permits arbitrary bit lengths. Explicit library operations may construct
and inspect arbitrary-length `bits` values.

Full source-level patterns with arbitrary or dynamic bit widths are deferred to
the next language version because they require substantially more parsing,
type-checking, bounds, exhaustiveness, and lowering rules. The future syntax
should remain compatible with forms such as:

```el
<<version::size(3), flags::size(5), payload::bits>>
```

## 8. Expressions and control flow

### 8.1 Blocks

`do ... end` forms a lexical scope and evaluates to its final expression. An
empty block evaluates to `unit`.

### 8.2 Conditionals

```el
label = if score >= 50 do
  "pass"
else
  "fail"
end
```

Conditions must have type `bool`; EL has no truthiness. If an `if` is used as a
value, both branches are required and must have the same type. An `if` used only
for effects may omit `else`, in which case its type is `unit`.

### 8.3 Pattern matching and errors

Recoverable errors are tagged tuple values, normally `{:ok, value}` or
`{:error, reason}`. EL has no exceptions, throwing, or catching:

```el
match Parser.parse_int(input) do
  {:ok, value} -> value
  {:error, reason} -> IO.report(reason)
end
```

`match` is an expression. All reachable arms must return the same type when its
result is used. The type checker verifies exhaustiveness for closed tagged tuple
unions, `bool`, and other finite types it understands. A wildcard `_` arm makes
a match exhaustive.

Unrecoverable runtime failures such as an internal invariant violation may abort
the process. They are not catchable and must not be used for ordinary errors.

### 8.4 Loops

```el
mut i = 0
while i < 10 do
  i := i + 1
end

for item in items do
  IO.println(item)
end
```

V1 has exactly two loop forms: `while` and `for pattern in iterable`. Both return
`unit`. A `for` loop obtains values through the `Iterable` protocol and may use a
pattern as its loop binding. C-style loops, `loop`, comprehensions, and implicit
recursion syntax are not supported. `break` and `continue` are deferred.

### 8.5 Pipeline operator

The pipeline operator is part of v1. It inserts its left operand as the first
argument of the call on its right:

```el
value |> transform(a, b)
# desugars to:
transform(value, a, b)
```

Pipelines associate left-to-right. The right operand must be a statically
resolvable call expression in v1; placeholder arguments and arbitrary pipeline
targets are deferred.

### 8.6 Operators

The first compiler slice supports:

```text
unary:          - !
multiplicative: * / %
additive:       + -
concatenation:  ++
comparison:     < <= > >=
equality:       == !=
logical:        and or
pipeline:       |>
```

Operators do not implicitly coerce values. Short-circuit semantics are required
for `and` and `or`. `++` resolves through `Concat.concat`, while `|>` is compile-
time syntax sugar and performs no protocol dispatch.

## 9. Modules, packages, and physical layout

### 9.1 Modules

- A module is declared with `defmodule Name do ... end`.
- One source file contains exactly one module declaration.
- Nested `defmodule` declarations are not supported in v1.
- `def` exports a function from its module; `defp` does not.
- The executable entry point is `Main.main() -> i32` for a target whose root
  module is `Main`.
- The return value of `main` becomes the process exit code.
- Top-level executable statements are not allowed.
- One module may span only one file in v1.

### 9.2 Project manifest

An EL project is a directory containing an `el.toml` manifest. The manifest
declares the package ID, root namespace, package version, dependencies, and build
targets. An illustrative manifest is:

```toml
[package]
name = "example"
namespace = "Example"
version = "0.1.0"

[deps]

[targets.app]
kind = "executable"
main = "Main"
```

`package.name` is the package ID used by dependency and tooling metadata;
`package.namespace` is the root namespace used by source modules. The exact
dependency source and version-constraint syntax under `[deps]` remains to be
designed. V1 dependencies are EL packages; the manifest cannot declare native
FFI libraries.

### 9.3 Physical layout

The conventional layout is:

```text
project/
  el.toml
  src/
    main.el
    parser.el
  test/
```

Every source file under `src/` contains one `defmodule`. The manifest's root
namespace scopes project modules. The exact mapping between file paths and module
names is an open question; the compiler must diagnose duplicate module names.

## 10. Memory and resource management

### 10.1 Garbage collection

V1 uses the Boehm-Demers-Weiser conservative garbage collector (Boehm GC), a
non-moving collector based on a modified mark-sweep algorithm. It is linked into
the native executable as an internal runtime dependency. The compiler distribution
pins and vendors its source, then statically links it into EL executables so users
do not need a separately installed collector.

All managed allocation goes through a small EL runtime API. The API distinguishes
objects that may contain managed references from pointer-free data so the
collector can avoid scanning raw bytes and numeric storage unnecessarily. No
Boehm-specific function, type, finalizer, or configuration is exposed to EL
programs.

Boehm GC discovers roots conservatively from machine registers, stacks, globals,
and reachable heap memory. V1 therefore does not emit precise stack maps, a
shadow stack, or per-type tracing functions. Generated code must keep live
managed references visible as valid machine pointers across any operation that
may allocate; GC-safety tests must cover optimized builds as well as debug builds.

Conservative scanning may retain an otherwise unreachable object when non-pointer
data happens to resemble its address. The collector cannot move or compact live
objects, but stable addresses simplify the initial runtime. EL's private
allocation API preserves the option to replace Boehm with a precise collector in
a later version without changing source-language semantics.

V1 has no user-visible finalizers or weak references.

Reference: [Boehm GC overview](https://hboehm.info/gc/) and
[algorithm description](https://hboehm.info/gc/gcdescr.html).

### 10.2 Resource management

GC finalizers are nondeterministic and are not sufficient for scarce resources.
Before EL claims practical systems-programming capability, it needs an explicit
resource pattern. Candidates include lexical `defer`, scoped cleanup, or an
ownership-like restricted resource type. This is an open design question.

## 11. Compiler architecture

The compiler executable is named `elc`.

```text
.el source
    |
    v
PEG parse tree -> AST -> name resolution + type checking -> Typed AST
    -> Core IR -> LLVM IR -> LLVM optimization -> object file
    -> system linker -> native executable
```

The middle of the pipeline is deliberately split into a **typed AST** and a
small **Core IR**:

```text
PEG parse tree -> AST -> name resolution + type checking -> Typed AST
    -> desugaring -> Core IR -> LLVM IR
```

Yes, the typed AST belongs immediately after the source AST's names are resolved
and types are checked. It preserves source-level structure for good diagnostics,
while Core IR removes syntax sugar before backend code generation.

### 11.1 Bootstrap implementation

- Implementation language: Rust.
- Parsing formalism: PEG.
- Proposed PEG library: `pest`, with a checked-in `.pest` grammar. This remains
  replaceable until the first parser prototype is evaluated.
- Expression precedence: encode explicit precedence levels in the grammar or
  use the parser library's Pratt parsing support; do not use left recursion.
- Backend: LLVM through Rust bindings.
- Proposed binding: Inkwell for a safer learning-oriented API over `llvm-sys`.
  The exact LLVM and Inkwell versions will be pinned together.
- Compilation mode: native AOT only.
- Initial target: the compiler host target.
- Linking: invoke the platform C linker through the installed compiler driver
  initially; direct LLD integration can come later.

### 11.2 Compiler representations

Keep these layers distinct:

- **Parse tree:** mirrors the grammar and retains source spans.
- **AST:** represents source constructs without parser noise; names may still be
  unresolved and expressions do not yet carry final types.
- **Typed AST:** preserves source constructs, while every expression has a type,
  every name has a unique symbol identity, and protocol calls are selected.
- **Core IR:** contains a smaller typed language after pipelines, `++`, `for`,
  pattern matching, and derived implementations are expanded or lowered.
- **LLVM IR:** target-oriented representation; it must not be used as EL's type
  checker or primary semantic model.

Every AST, typed AST, and diagnostic-producing Core IR node should retain a byte
span into a source file. Line and column numbers are derived for display.

### 11.3 Initial lowering strategy

- Immutable scalar locals become LLVM SSA values where practical.
- Mutable locals may initially lower to entry-block `alloca` instructions plus
  loads/stores; LLVM's `mem2reg` pass can promote safe cases to SSA.
- `if` expressions lower to control-flow blocks and a `phi` value.
- `while` lowers to condition, body, and exit basic blocks.
- `for pattern in value` lowers through the statically selected `Iterable`
  implementation.
- `match` lowers to tests and branches after exhaustiveness checking.
- `left ++ right` lowers to the selected `Concat.concat(left, right)` call.
- `left |> call(args)` is rewritten to `call(left, args)` before Core IR.
- Runtime operations are called through a small, versioned internal ABI.

## 12. PEG grammar sketch

This is explanatory pseudogrammar, not the final parser grammar:

```text
program       <- SOI module EOI
module        <- "defmodule" type_name "do" module_item* "end"
module_item   <- derive_attr? struct_decl / type_alias / function
               / protocol_decl / protocol_impl
struct_decl   <- "defstruct" type_name "do" field* "end"
type_alias    <- "@type" type_name "=" type
function      <- ("def" / "defp") ident "(" params? ")"
                 return_type? "do" block "end"
protocol_decl <- "defprotocol" type_name "do" protocol_sig* "end"
protocol_impl <- "defimpl" type_name "," "for" ":" type "do"
                 function* "end"
params        <- param ("," param)*
param         <- ident ":" type
return_type   <- "->" type
binding       <- "mut"? ident (":" type)? "=" expression
assignment    <- ident ":=" expression
if_expr       <- "if" expression "do" block ("else" block)? "end"
match_expr    <- "match" expression "do" match_arm+ "end"
while_stmt    <- "while" expression "do" block "end"
for_stmt      <- "for" pattern "in" expression "do" block "end"
```

The real grammar must resolve newline/whitespace handling, statement boundaries,
operator precedence, attributes, tagged and bitstring patterns, struct literals,
recovery behavior, and the ambiguity between a final block expression and an
expression statement.

## 13. Diagnostics

Diagnostics are a language feature, not polish to add at the end.

Minimum diagnostic structure:

```text
error[E0301]: cannot update immutable binding `x`
  --> example.el:4:3
   |
 2 |   x = 1
   |   ----- `x` is immutable because it was declared here
 3 |
 4 |   x := 2
   |   ^^^^^^ mutable update attempted here
   |
help: declare it with `mut x = 1`
```

Parser, resolver, and type-checker errors must use source spans. The compiler
should recover sufficiently to report several independent errors in one run,
but correctness is more important than aggressive recovery in the first slice.

## 14. CLI contract

Proposed initial commands:

```text
elc check
elc build
elc build --target app
elc emit llvm-ir --module Main
```

Commands locate `el.toml` in the current directory or an ancestor. `check` stops
after semantic analysis. `build` builds the default or named manifest target.
`emit llvm-ir` is a development and learning aid, not a stable language API. A
single-file developer mode may exist during bootstrapping but is not the v1
project interface.

## 15. Implementation roadmap

Each milestone ends with working tests and a runnable example. Avoid building
all syntax before any program can run.

### Milestone 0: project skeleton

- Rust workspace with `elc` compiler crate and runtime crate.
- One command-line entry point.
- Minimal `el.toml` discovery and parsing.
- Unit-test and snapshot-test conventions.
- Pin the Rust toolchain, LLVM version, LLVM binding, and Boehm GC version.

Exit test: `elc --help` runs and CI can build the workspace.

### Milestone 1: parser and AST

- PEG grammar for `defmodule`, `def`/`defp`, bindings, literals, types, and
  arithmetic.
- Source spans on all AST nodes.
- AST pretty/debug output for tests.
- Useful syntax errors for common mistakes.

Exit test: parse a typed `main` function and snapshot its AST.

### Milestone 2: names and types

- Lexical scopes and unique symbol IDs.
- Primitive types, function signatures, immutable bindings, and mutable locals.
- Type-check arithmetic, calls, returns, and `:=`.
- Produce a typed AST and lower it to a minimal Core IR.

Exit test: accepted and rejected programs cover binding, mutation, calls, and
return types without invoking LLVM.

### Milestone 3: first native executable

- Lower `i32`, `i64`, arithmetic, calls, and returns to LLVM IR.
- Emit an object file and invoke the host linker.
- Implement `Main.main() -> i32` as the entry point.

Exit test: compile and run a program whose exit status is computed by EL code.

### Milestone 4: core control and matching

- `bool`, comparisons, `if`, `while`, early `return`, and short-circuit logic.
- Tuples, atoms, closed tagged tuple aliases, and exhaustive `match`.
- Pipeline desugaring.
- LLVM verifier runs on generated modules in tests.

Exit test: compile and run iterative factorial plus a tagged-result parser.

### Milestone 5: Boehm GC integration

- Define the internal runtime ABI.
- Pin and vendor Boehm GC, then build and statically link it for each supported
  host target.
- Route traceable and pointer-free allocations through private runtime wrappers.
- Verify live references held in locals, arguments, returns, globals, nested
  calls, recursion, and interior object graphs in debug and optimized builds.
- Provide a GC stress mode that collects as frequently as practical.

Exit test: a native optimized EL program retains a reachable heap graph while
temporary allocations are reclaimed under GC stress mode.

### Milestone 6: data types and text

- `defstruct`, construction, field access, and layout.
- `string`, `rune`, `bytes`, byte-aligned `bits`, and `Buffer`.
- Lists, maps, arrays, slices, and function values.
- Decide value versus reference representation.
- Expand numeric primitives and explicit conversions.

Exit test: process valid UTF-8, reject invalid UTF-8, retain composite heap
graphs through GC, and diagnose integer indexing on `string`.

### Milestone 7: protocols and iteration

- `defprotocol`, `defimpl`, `Self`, and required associated types.
- `Eq`, `Ord`, `Show`, `Hash`, `Iterable`, and `Concat`.
- `@derive` for `Eq`, `Ord`, `Show`, and `Hash` where field constraints hold.
- `for pattern in iterable` lowering through `Iterable`.
- `==`, ordering operators, and `++` protocol lowering.

Exit test: derive protocols for a struct, iterate several concrete container
types, and concatenate all standard `Concat` types.

### Milestone 8: packages, I/O, and standard library

- Package ID, root namespace, module discovery, build targets, and EL dependency
  entries from `el.toml`'s `[deps]` table.
- `Reader` and `Writer` with tagged result values.
- Standard modules for strings, collections, buffers, bits, and I/O.
- Choose deterministic resource cleanup semantics for standard I/O resources.

Exit test: build a multi-module manifest target that reads, transforms, and
writes data while handling every recoverable error through `match`.

### Milestone 9: v1 stabilization

- Conformance suite, reference examples, and language reference.
- Stabilize diagnostics and CLI behavior.
- Freeze the v1 grammar, manifest format, and internal runtime ABI version.
- Document supported targets and binary distribution requirements.

Exit test: all v1 examples and negative conformance programs behave identically
on every supported target, with GC stress mode enabled.

## 16. Testing strategy

- **Grammar tests:** accepted/rejected syntax and precedence.
- **AST snapshots:** stable structure and source spans.
- **Typed AST/Core IR snapshots:** resolved names, types, and desugaring.
- **Semantic tests:** name and type errors with diagnostic snapshots.
- **IR tests:** verify LLVM modules; inspect small targeted IR fragments only.
- **End-to-end tests:** compile, link, execute, and check output/exit status.
- **GC stress tests:** frequent collection and heap graph survival.
- **Protocol tests:** resolution, coherence, derive constraints, and dispatch.
- **Manifest tests:** package ID, namespace, dependency, and target validation.
- **Negative tests:** invalid programs must fail without compiler crashes.
- **Differential tests:** where semantics are simple, compare interpreted test
  evaluation in the compiler with compiled execution (optional later aid).

Every bug in parsing, typing, code generation, or GC should gain a regression
test at the narrowest useful level.

## 17. Definition of v1

Version 1 is ready when:

- the accepted grammar and semantics are documented;
- `el.toml` projects reliably compile to native host executables;
- all supported language constructs are statically type-checked;
- immutable and mutable bindings behave exactly as specified;
- lower-case primitives, composites, tagged tuples, and `match` work;
- `defmodule`, `def`/`defp`, `defstruct`, `@type`, and one-module-per-file rules
  are enforced;
- protocols, explicit implementations, deriving, `for ... in`, `++`, and `|>`
  work as specified;
- `string`, `rune`, `bytes`, byte-aligned bit patterns, `bits`, and `Buffer` pass
  their validity and bounds tests;
- GC-managed programs survive stress testing;
- compiler failures produce source-based diagnostics rather than panics;
- `Reader` and `Writer` use tagged values and the standard resource pattern is
  deterministic;
- ordinary failures use tagged values rather than exceptions;
- no `null`, concurrency feature, or user-facing FFI leaks into the language;
  and
- the examples and conformance suite run on every supported platform.

## 18. Open questions

These require explicit decisions before the affected implementation begins:

1. What permanent name and source extension should the language use?
2. Which Rust PEG library should be pinned (`pest`, `peg`, or another)?
3. Which LLVM major version and Rust binding should be pinned?
4. Are structs copied values, GC references, or two explicit forms?
5. What is integer overflow behavior in debug and optimized builds?
6. What are slice ownership, lifetime, and backing-storage rules?
7. How are non-memory resources released deterministically?
8. What are the exact `Iterable`, `Reader`, `Writer`, and `Hash` method sets?
9. Does protocol dispatch support protocol-typed values in v1, or only static
    selection at concrete call sites?
10. Do semicolons exist as optional separators, or are newlines always enough?
11. Should `return` be allowed or should all functions be expression-oriented?
12. How do source paths map to module names under the manifest root namespace?
13. What dependency source and version-resolution model does `el.toml`'s
    `[deps]` table use?
14. What syntax will post-v1 arbitrary-width bitstring patterns use?

## 19. Decision process

- Decision IDs are stable and never reused.
- Status is `proposed`, `accepted`, `superseded`, or `rejected`.
- A superseded decision points to its replacement.
- Significant semantic or architectural changes require a log entry.
- Accepted decisions may still change, but the reason must be recorded.

## 20. Decision log

### D-001 — Rust bootstrap compiler

- Date: 2026-07-26
- Status: accepted
- Decision: Implement the initial EL compiler and runtime support in Rust.
- Reason: Rust provides strong implementation safety, good compiler-building
  libraries, straightforward native interoperability, and an appropriate path
  for bootstrapping a systems language.

### D-002 — LLVM native AOT backend

- Date: 2026-07-26
- Status: accepted
- Decision: Lower typed EL programs to LLVM and produce native binaries ahead of
  time. V1 will not include an interpreter or JIT.
- Reason: LLVM supplies mature optimization, machine-code generation, object
  emission, debug-info infrastructure, and broad target support.
- Consequence: LLVM versioning and distribution become part of the compiler's
  build and release engineering.

### D-003 — PEG parsing

- Date: 2026-07-26
- Status: accepted
- Decision: Express EL's grammar using a PEG parser in the Rust compiler.
- Reason: PEG provides readable ordered-choice grammars and is approachable for
  a small language implementation.
- Consequence: Ordered choice, whitespace, expression precedence, and error
  recovery must be handled intentionally; the grammar must avoid left recursion.

### D-004 — Immutable-by-default local bindings

- Date: 2026-07-26
- Status: accepted
- Decision: `name = value` introduces an immutable binding. `mut name = value`
  introduces a mutable binding, and `name := value` updates it.
- Reason: Mutation is visually explicit while ordinary code retains simple
  immutable value semantics.
- Consequence: EL's `:=` intentionally differs from Go's declaration operator.

### D-005 — No concurrency in v1

- Date: 2026-07-26
- Status: accepted
- Decision: Exclude language and runtime concurrency features from v1.
- Reason: Concurrency would expand runtime, memory-model, scheduler, and type-
  system work before the sequential language is proven.

### D-006 — Boehm GC behind a runtime allocation API

- Date: 2026-07-26
- Status: superseded by D-019
- Decision: Use conservative Boehm GC for v1, accessed only through an internal
  EL runtime ABI.
- Reason: It is the shortest credible route to a working native GC language and
  avoids requiring precise LLVM stack maps in the first compiler.
- Risks: Conservative retention, external native dependency, platform support,
  and constraints on future moving collection.
- History: D-019 temporarily selected a custom precise collector. D-023 restores
  the Boehm direction after comparing implementation risk.

### D-007 — `pest` as the Rust PEG implementation

- Date: 2026-07-26
- Status: proposed
- Decision: Begin with `pest` and a separate checked-in grammar file.
- Reason: Keeping the grammar visible and separate from compiler logic makes the
  language easier to study and review.
- Validation: Prototype operator precedence and diagnostics before acceptance.

### D-008 — Inkwell LLVM bindings

- Date: 2026-07-26
- Status: proposed
- Decision: Use Inkwell rather than calling `llvm-sys` directly.
- Reason: Its safer, higher-level API reduces incidental unsafe Rust while we
  learn LLVM construction and verification.
- Validation: Confirm support for the selected LLVM version, target setup,
  object emission, debug information, and required GC integration.

### D-009 — Mutability is initially local rebinding only

- Date: 2026-07-26
- Status: accepted
- Decision: In the initial v1 core, `mut` permits rebinding a local name but does
  not grant transitive, field, or indexed mutation.
- Reason: This gives the requested explicit mutation without prematurely
  committing to aliasing, reference, and aggregate mutation rules.

### D-010 — Simplicity as the primary philosophy

- Date: 2026-07-26
- Status: accepted
- Decision: Judge language features by how easily they can be read, explained,
  composed, and implemented correctly; prefer fewer orthogonal rules.
- Consequence: Features with unclear interactions remain staged or deferred even
  when they are individually attractive.

### D-011 — Type naming and composite categories

- Date: 2026-07-26
- Status: accepted
- Decision: Primitive types use lower-case names; custom types use PascalCase.
  V1 includes structs, tuples, lists, maps, arrays, slices, atoms, functions,
  transparent `@type` aliases, and the specified string/binary types.
- Consequence: Built-in composites have compiler-supported type parameters even
  though general user-defined generics are deferred.

### D-012 — Module and declaration syntax

- Date: 2026-07-26
- Status: accepted
- Decision: Modules use `defmodule`; structs use `defstruct`; public and private
  functions use `def` and `defp`. V1 allows one non-nested module per file.

### D-013 — Explicit protocols and deriving

- Date: 2026-07-26
- Status: accepted
- Decision: Declare protocols with `defprotocol`, implementations with
  `defimpl`, and generated implementations with `@derive` on `defstruct`.
- Consequence: This replaces the earlier Go-style implicit structural interface
  direction. Conformance is explicit and coherent.

### D-014 — Core protocol set and concatenation

- Date: 2026-07-26
- Status: accepted
- Decision: The initial protocols are `Eq`, `Ord`, `Show`, `Hash`, `Iterable`,
  `Writer`, `Reader`, and `Concat`. `left ++ right` lowers to
  `Concat.concat(left, right)` with standard implementations for `string`,
  `bytes`, `bits`, and lists.
- Note: The operation name is normalized to `concat`; earlier spellings
  `conact` and `contact` were treated as typographical errors.

### D-015 — Tagged errors and pattern matching

- Date: 2026-07-26
- Status: accepted
- Decision: Recoverable failures use tagged tuples plus exhaustive `match`. EL
  has no exceptions, throwing, or catching, and it has no `null`.

### D-016 — V1 control-flow forms

- Date: 2026-07-26
- Status: accepted
- Decision: V1 loop syntax consists of `while` and `for pattern in iterable`.
  The pipeline operator `|>` is supported and inserts its left value as the
  first argument of the call on its right.

### D-017 — Strings, binary data, and bitstring scope

- Date: 2026-07-26
- Status: accepted
- Decision: `string` is valid UTF-8 without integer indexing; `bytes` is raw
  byte-aligned data; `bits` is arbitrary-length bit data; `rune` is one Unicode
  scalar value; and `Buffer` is the growable builder.
- Scope: V1 source patterns support byte-aligned bit segments. Full arbitrary-
  width source construction and matching is deferred to the next version.

### D-018 — Manifest-based projects

- Date: 2026-07-26
- Status: accepted
- Decision: An EL project is a directory containing `el.toml`, which declares
  its package ID as `package.name`, root namespace, version, EL dependencies in
  `[deps]`, and build targets.

### D-019 — Precise stop-the-world mark-sweep GC

- Date: 2026-07-26
- Status: superseded by D-023
- Decision: V1 uses a non-moving, stop-the-world tri-color mark-sweep collector
  with compiler-maintained roots and compiler-emitted tracing descriptors.
- Reason: The sequential v1 runtime makes the collector algorithm small and
  understandable, aligning it with EL's simplicity and learning goals.
- Risk: Precise LLVM root tracking is more compiler work than using Boehm GC.
  The shadow-stack design must pass an early collection-at-every-allocation
  prototype before higher-level runtime work depends on it.

### D-020 — No user-facing FFI in v1

- Date: 2026-07-26
- Status: accepted
- Decision: EL source cannot declare or call foreign functions in v1. Native
  platform calls are private implementation details of the runtime and stdlib.

### D-021 — Typed AST and Core IR

- Date: 2026-07-26
- Status: accepted
- Decision: Name resolution and type checking produce a typed AST. Desugaring
  then produces a smaller typed Core IR before LLVM lowering.
- Reason: The typed AST preserves source structure for diagnostics; Core IR keeps
  protocols, matching, pipelines, and loops out of the LLVM backend's surface.

### D-022 — String inspection API

- Date: 2026-07-26
- Status: accepted
- Decision: The core string inspection functions are `byte_size`, `length`,
  `bytes`, `codepoints`, and `graphemes`. `length` counts Unicode grapheme
  clusters, while `byte_size` counts UTF-8 bytes.
- Consequence: String algorithms must choose their unit explicitly; integer
  indexing remains unsupported.

### D-023 — Boehm GC for v1

- Date: 2026-07-26
- Status: accepted
- Decision: Use Boehm GC behind EL's private runtime allocation API for v1.
- Reason: Integrating a mature conservative collector lets work focus on the
  parser, type system, protocols, standard library, and LLVM backend. A custom
  precise collector would require correct root tracking before ordinary heap
  programs could be trusted.
- Consequences: V1 accepts possible conservative retention, non-moving objects,
  and a vendored native build dependency that is statically linked into EL
  executables. Generated code must preserve discoverable pointer representations,
  and optimized builds require dedicated GC-safety tests. No Boehm-specific
  behavior is part of EL's public semantics.
- References: [Boehm GC](https://hboehm.info/gc/) and
  [LLVM GC integration](https://llvm.org/docs/GarbageCollection.html).

## 21. Next design checkpoint

Before writing compiler code, accept or revise these remaining choices:

1. `pest` as the PEG implementation;
2. Inkwell and a pinned LLVM major version;
3. value/reference representation for structs and slices; and
4. the exact core protocol method sets.

Once those are settled, Milestone 0 and the smallest Milestone 1 grammar can be
implemented without guessing at foundational architecture.
