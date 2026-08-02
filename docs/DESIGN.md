# EL Language Design

Status: living design document  
Language name: **EL**
Last updated: 2026-08-01

This document is the source of truth for EL's vision, observable runtime
semantics, compiler architecture, roadmap, open questions, and design decisions.
The specification set is divided by responsibility:

- [GRAMMAR.md](GRAMMAR.md) is normative for lexical and concrete syntax;
- [TYPES.md](TYPES.md) is normative for static semantics and well-formedness;
- [IR.md](IR.md) defines the bootstrap compiler's representation contracts; and
- [EXAMPLES.md](EXAMPLES.md) is an illustrative companion.

The descriptive language tour and topic sections here explain those contracts
and preserve their rationale. When overlapping prose disagrees, the document
with explicit authority for that subject controls. An intentional behavior
change must update its authoritative specification and be recorded in the
decision log before implementation or examples depend on it.

## 1. Vision

EL is a statically typed, garbage-collected systems programming language with:

- Elixir-inspired surface syntax;
- a small static type system;
- immutable local bindings by default;
- explicit local mutability with `mut` and rebinding with `:=`;
- native ahead-of-time (AOT) compilation through LLVM; and
- a compiler bootstrapped in Rust using a PEG parser.

EL should feel small, readable, and predictable. Version 1 deliberately favors
a coherent language that we can finish over a wide language with unfinished
features.

EL also values developer experience and developer happiness. Common work should
feel direct and pleasant: syntax should be readable, tools and diagnostics
should be helpful, and routine tasks should require little ceremony. This does
not override correctness, predictability, or simplicity; it guides choices
between designs that satisfy those constraints.

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
- Make common development workflows pleasant through clear diagnostics,
  readable code, and low ceremony.
- Provide value semantics and immutable bindings by default.
- Support explicit mutable local bindings.
- Support functions, lexical blocks, conditionals, loops, and recursion.
- Support lower-case primitive types and the accepted composite types.
- Support inferred, monomorphized generic functions and generic named types.
- Support closed disjoint structural unions with exhaustive matching.
- Support protocols, explicit protocol implementations, and deriving.
- Support tagged tuples and pattern matching for recoverable errors.
- Support `while` loops and protocol-backed `for ... in` loops.
- Support the pipeline operator (`|>`).
- Support strings, binary data, and garbage-collected heap values.
- Build projects described by an `el.toml` manifest.
- Expose command-line arguments, environment lookup, and portable string-based
  file paths through a defined process boundary.
- Make source programs visually familiar to an Elixir programmer.
- Keep the runtime and compiler small enough to understand.

### 2.2 Version 1 non-goals

- Concurrency, processes, actors, channels, or an async runtime.
- A BEAM-compatible runtime or Elixir/Erlang interoperability.
- A bytecode interpreter or JIT compiler.
- Macros, metaprogramming, or compile-time code execution.
- Inheritance or classes.
- Exceptions, `throw`, or catchable panics. Recoverable errors are values.
- Runtime-reified generics, higher-kinded types, specialization, or
  protocol-typed runtime values.
- User-facing foreign-function interfaces (FFI).
- Full source compatibility with either Elixir or Go.
- A self-hosted compiler.

### 2.3 Systems-programming intent

Native code alone does not make a language suitable for systems programming.
EL v1 provides native compilation, exact scalar widths, checked arithmetic,
contiguous arrays and slices, predictable runtime behavior, and deterministic
handling of operating-system resources. Garbage collection manages memory, but
it must not manage files, sockets, or other scarce resources.

V1 does not expose an FFI or an `unsafe` language feature. The compiler runtime
and standard library may use native platform APIs internally, but that boundary
is not part of the EL language. A public FFI is out of scope unless a future
decision explicitly adds it.

Aggregate memory layout, foreign calling conventions, and object-file
compatibility are not source-language guarantees in v1. EL v1 is therefore a
systems-oriented native language foundation, not yet a language for layout-
sensitive FFI, memory-mapped hardware, kernel code, or interoperable binary
libraries.

## 3. Language tour

This example is illustrative, but every syntax form it uses conforms to the
normative v1 grammar in [GRAMMAR.md](GRAMMAR.md).

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
    |> Enum.map(distance_squared)
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

The normative lexical contract is in [GRAMMAR.md](GRAMMAR.md). This section is
an explanatory summary.

### 4.1 Source files

- Source file extension: `.ell`.
- Source text is UTF-8.
- UTF-8 BOMs are not accepted. A physical newline is either LF or CRLF; a bare
  carriage return is invalid. Outside literals, horizontal whitespace is ASCII
  space or tab. A line comment excludes its terminating newline.
- V1 identifiers use ASCII letters, digits, and `_` as specified in
  [GRAMMAR.md](GRAMMAR.md).
- Value and function names use `snake_case`.
- Primitive type names are lower case.
- Type variables are lower case; named type constructors, protocols, and modules
  use `PascalCase`.
- `#[` begins an array literal; any other `#` begins a line comment.
- Nested block comments are deferred.

### 4.2 Keywords

V1 reserved words:

```text
def defer defimpl defmodule defp defprotocol defstruct do else end false for if in
match mut return true type when while with
```

`@derive` and `@type` are built-in attributes and are reserved as complete
attribute names.

Future keywords are not reserved until their feature is accepted.

### 4.3 Newlines and statement separation

EL has no semicolon token. A newline separates expressions or statements when
the preceding tokens form a complete construct at the current delimiter depth.
Multiple statements cannot be placed on one line with a separator.

A newline is treated as whitespace when continuation is unambiguous: inside an
open `(...)`, `[...]`, or `{...}` delimiter, after a comma, after a non-pipeline
operator that requires a right operand, or before a pipeline operator that
begins the next line. No backslash or other explicit line-continuation token
exists. For example:

```el
total = left +
  right

result = input
  |> normalize()
  |> validate()
```

The pipeline operator is the only operator that may continue a complete
expression from the preceding line. In a multiline pipeline, each `|>` must be
at the beginning of its continued line (after optional indentation) and its
right operand must begin on that same line. Other operators at the beginning of
a line do not retroactively continue a complete expression. Blank and
comment-only lines do not produce empty statements. A semicolon receives a
syntax diagnostic rather than being treated as optional punctuation.

### 4.4 Literals

Integer literals use decimal notation or the `0b`, `0o`, and `0x` prefixes for
binary, octal, and hexadecimal. An underscore may separate digits but may not
lead, trail, or repeat. Integer literals have no suffix; their type comes from
an expected type or defaults to `i64`. A leading `-` is the unary operator and
is not part of the literal token.

Floating-point literals use decimal notation and contain a decimal point, an
exponent, or both, as in `1.0`, `1e10`, and `1.5e-3`. Digits may use the same
underscore separators. An otherwise unconstrained floating literal defaults to
`f64`. Hexadecimal floating literals and literal spellings for NaN and infinity
are not supported in v1.

A double-quoted string literal is valid UTF-8. A single-quoted rune literal must
contain exactly one Unicode scalar value. The supported escapes, where
applicable, are `\\`, `\"`, `\'`, `\n`, `\r`, `\t`, `\0`, `\xNN`, and
`\u{...}`. The decoded result of a string literal must remain valid UTF-8, and a
Unicode escape must denote a scalar value rather than a surrogate. String
format specifiers, raw strings, multiline strings, and adjacent-literal
concatenation are deferred. Strings may interpolate a `Show` value with
`#{expression}` as specified by `GRAMMAR.md` and `TYPES.md`.

An atom literal is `:` followed by an ASCII `snake_case` identifier, such as
`:ok` or `:not_found`. Quoted atoms and conversion of runtime strings to atoms
are not supported. `true`, `false`, and `unit` are the literal values of `bool`
and `unit`.

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
- `name.field := expression` performs a direct struct-field update by replacing
  the value held by the mutable local binding `name`.
- `:=` is a statement and evaluates to `unit`.
- Assigning to an immutable binding is a compile-time error.
- The assigned expression must have exactly the target's type: the binding type
  for a local update or the declared field type for a field update.
- A mutable binding must be initialized.
- Parameters are immutable in v1.

Mutability applies to the local binding itself. It does not make an aggregate,
its fields, or aliased values mutable. V1 nevertheless permits a direct field
update rooted at a mutable local:

```el
mut point = %Point{x: 1, y: 2}
point.x := point.x + 1
```

This is field-update syntax that reconstructs the struct and rebinds the mutable
root `point`. It is semantically equivalent to evaluating the right-hand side
against the old value and then replacing `point` with a shallow fieldwise copy
whose `x` field contains the result. Other values copied from `point` are not
changed. The compiler may lower the operation to an in-place store only when
that optimization is unobservable.

The root must be a mutable local binding, the named direct field must exist, and
the right-hand side must have exactly the field's declared type. An immutable
local, parameter, temporary expression, or arbitrary call result cannot be an
update root. Nested field updates (`user.address.city := value`) and indexed
updates (`items[0] := value`) are deferred. Keeping the accepted target narrow
avoids introducing general reference or shared-mutation semantics.

### 5.3 Why `=` and `:=` differ

EL intentionally does not copy Go's meaning of `:=`. The language uses:

```text
name = expression       introduce an immutable binding
mut name = expression   introduce a mutable binding
name := expression      update a mutable binding
name.field := expression
                        reconstruct a struct and update its mutable root binding
```

This rule is simple to parse, simple to type-check, and makes mutation visible.

## 6. Types

The normative static semantics are defined in [TYPES.md](TYPES.md). This section
is a design-oriented tour of the same type system and its standard-library
contracts; if the descriptions diverge, `TYPES.md` controls type formation,
equality, inference, checking, conformance, and static well-formedness.

EL's type system is small and static: it uses explicit conversions, has no
implicit numeric coercions, and favors simple nominal types. EL protocols use
explicit `defimpl` declarations rather than implicit structural interface
satisfaction.

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

The fixed-width signed integers use two's-complement representation, and the
fixed-width unsigned integers use ordinary binary representation. `isize` and
`usize` have the compilation target's pointer width; v1 targets have either
32-bit or 64-bit pointers. `f32` and `f64` use IEEE 754 binary32 and binary64.
A `rune` converts losslessly to `u32`, though its in-memory representation is not
otherwise public.

Implementation starts with `bool`, `i64`, `i32`, and `unit`; the other built-ins
are added after the pipeline works end to end.

Rules:

- Integer literals are checked against the expected type when one exists.
- An otherwise unconstrained integer literal defaults to `i64`.
- An otherwise unconstrained floating literal defaults to `f64`.
- Arithmetic requires matching operand types, except that a shift count has
  type `usize`.
- Conversions are explicit, for example `i32(value)`.
- Integer addition, subtraction, multiplication, and unary negation are checked
  for overflow in every build mode. Unary negation is invalid for unsigned
  integers. Ordinary overflow is an unrecoverable runtime failure;
  compile-time-known overflow is a compile-time error.
- Signed integer division truncates toward zero, and its remainder has the sign
  of the dividend. Division or remainder by zero and the signed `MIN / -1` or
  `MIN % -1` cases are unrecoverable failures unless known at compile time, when
  they are errors.
- Bitwise `&`, `|`, and `^` require matching integer types. Unary `~` preserves
  its integer type. Shift counts are `usize`; a count at least as large as the
  left operand's width fails. Left shift is checked for discarded significant
  bits, signed right shift is arithmetic, and unsigned right shift is logical.
- PascalCase modules corresponding to every integer type, such as `I64` and
  `Usize`, provide explicit `wrapping_add`, `wrapping_sub`, `wrapping_mul`,
  `wrapping_neg`, `wrapping_shl`, and `wrapping_shr` operations. Wrapping shifts
  reduce the count modulo the width; a wrapping left shift discards high bits.
- Integer-to-integer conversion checks the destination range. Float-to-integer
  conversion truncates toward zero and rejects NaN, infinity, and out-of-range
  results. Integer-to-float conversion permits explicit precision loss, while
  `f64` to `f32` uses IEEE rounding and may produce infinity. Integer-to-`rune`
  conversion rejects values that are not Unicode scalar values. A statically
  known invalid conversion is a compile-time error; otherwise it is an
  unrecoverable runtime failure.
- `f32` and `f64` follow IEEE 754 round-to-nearest, ties-to-even semantics.
  Floating division by zero produces IEEE infinity or NaN, and optimized builds
  may not use transformations that violate these semantics. NaN payload and sign
  are unspecified. NaN is unequal to every value, ordered comparisons involving
  NaN are false, and positive and negative zero compare equal.
- Floats support primitive comparison operators but do not implement `Eq`,
  `Ord`, or `Hash` in v1. They therefore cannot be map keys and prevent those
  protocols from being derived for a containing type. A future explicit
  total-order wrapper may provide those capabilities.
- EL has no `null`, null literal, nullable reference, or implicit zero reference.

### 6.2 Generics, custom types, and aliases

Named type constructors use PascalCase. Lowercase type identifiers in a
declaration signature introduce inferred type parameters; conventional names
include `a`, `b`, `k`, and `v`. Primitive names such as `i64`, `bool`, and
`string` remain reserved lowercase types rather than parameters.

Type parameters are implicit in function signatures. `when a: Protocol`
constrains a parameter to types implementing that protocol:

```el
def map(values: [a], f: (a) -> b) -> [b] do
  match values do
    [] -> []
    [value | rest] -> [f(value) | map(rest, f)]
  end
end

def max(left: a, right: a) -> a when a: Ord do
  if left >= right do
    left
  else
    right
  end
end
```

The signature implicitly quantifies `a` and `b`; no type-parameter list appears
after the function name. A generic body is checked once using its declared
protocol constraints, and each reachable concrete use is monomorphized before
LLVM lowering. Recursive calls must preserve the current type arguments;
polymorphic recursion is not supported in v1.

Generic named declarations put parameters in parentheses after the name and may
carry the same `when` constraints:

```el
defstruct Box(a) do
  value: a
end

defstruct Pair(a, b) do
  first: a
  second: b
end

defstruct Stack(a) when a: Ord do
  items: [a]
end
```

Named type application also uses parentheses, for example `Box(i64)` and
`Map(string, User)`. The list spelling `[a]` is the canonical shorthand for
`List(a)`. There is no per-call type-argument syntax. Argument types normally
determine an instantiation; when they do not, the expected type flows into the
call from a binding annotation, return context, or inline `expression :: Type`
ascription:

```el
value: i64 = Parser.parse("42")
empty: [i64] = []
sum(Parser.parse_all(lines) :: [i64])
```

If neither arguments nor an expected type determine every parameter, inference
fails with a diagnostic rather than choosing an arbitrary type.

`@type` creates a transparent alias; it does not create a distinct nominal type:

```el
@type UserId = u64
@type ParseResult = {:ok, i64} | {:error, string}
@type Result(a, e) = {:ok, a} | {:error, e}
@type Scalar = i64 | string
```

`ParseResult`, `Result(a, e)`, and `Scalar` are closed structural unions. A
future distinct-type feature must use different syntax rather than changing
transparent alias semantics.

Transparent aliases must be acyclic. Direct, mutual, generic, and cross-module
alias cycles are rejected even when a reference occurs beneath a managed
container. This keeps alias expansion, type equality, union normalization, and
diagnostics finite. Recursive data uses a nominal struct with recursion guarded
by a managed container instead.

#### 6.2.1 Structural union types

`A | B` forms a closed structural union in any type position. It does not create
a nominal type. Union equality is order-independent; the compiler expands
transparent aliases, flattens nested unions, removes duplicate members, and
uses a canonical member order. For example, `A | (B | A)` and `B | A` are the
same type.

Every normalized alternative must be provably disjoint from every other
alternative for all permitted finite generic substitutions. Formally, two
alternatives are disjoint when no such substitution can make their normalized
types equal. This is a static type rule rather than a comparison of physical
representations; the union's hidden discriminant records which alternative was
injected.

Concrete unequal types are disjoint. This includes different primitive types,
atoms, nominal struct constructors, invariant applications of the same generic
constructor, tuple shapes or element types, array lengths or element types, and
function signatures. Consequently, `List(i64) | List(string)` and
`{:ok, i64} | {:ok, string}` are valid even if some values have similar runtime
representations. These generic tagged unions are also valid; the second is the
prelude's canonical optional-value type:

```el
@type Result(a, e) = {:ok, a} | {:error, e}
@type Option(a) = {:some, a} | :none
```

Unconstrained alternatives that may overlap are rejected:

```el
@type Either(a, b) = a | b       # invalid: a and b may be the same type
@type Optional(a) = a | :none    # invalid: a may include :none
```

The compiler decides this by first-order unification after normalization.
Generic constructors are invariant, nominal constructor identities must match,
and tuple fields, array lengths and elements, and function parameters and return
types unify recursively. The unifier performs an occurs check and never accepts
an infinite substitution. Positive protocol constraints do not establish
disjointness, and an unresolved associated-type projection is treated
conservatively as capable of overlap.

Before checking, the compiler expands acyclic transparent aliases, flattens
nested unions, normalizes members recursively, removes members that are already
exactly equal, and sorts them into a stable canonical order. It then tests every
remaining pair. When overlap is found, the diagnostic should show a witness
substitution where possible; for example, `Box(a) | Box(i64)` overlaps when
`a = i64`.

A generic union is checked at its declaration and normalized again after each
concrete substitution. The concrete check is a defensive compiler invariant;
a declaration accepted by the generic check must not acquire overlapping
members during monomorphization.

A value injects implicitly into a union only when an expected union type is
available from a parameter, declared return type, binding annotation, or `::`
ascription. EL does not infer a new union merely because unrelated branches or
expressions have different types. This keeps accidental type widening out of
local inference:

```el
def choose(flag: bool) -> i64 | string do
  if flag do
    1
  else
    "one"
  end
end

value: i64 | string = choose(condition)
```

A general union is eliminated with exhaustive `match`. A typed binding pattern
`name: Type` selects one normalized alternative:

```el
def describe(value: i64 | string) -> string do
  match value do
    number: i64 -> Show.show(number)
    text: string -> text
  end
end
```

Tagged alternatives continue to use ordinary structural patterns such as
`{:ok, value}`. Typed patterns must name exactly one normalized member; a
wildcard may cover all remaining members.

After monomorphization, a union value uses a hidden discriminant and a payload
large and aligned enough for its concrete members. The discriminant and member
order are not observable language values. Structural unions do not
automatically implement protocols and cannot be a `defimpl` target in v1; code
must match the union before invoking member-specific protocol behavior.

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

All fields must be initialized in v1. Fields are immutable components of the
struct value; `mutable_local.field := value` reconstructs the struct and rebinds
the mutable root rather than mutating a field through object identity. There is
no implicit zero-value construction. Structs have value semantics: a binding,
argument, return, or aggregate field contains a struct value rather than an
observable reference with identity. A struct copy is shallow and fieldwise, so
immutable reference-backed fields may share storage. The compiler may keep a
struct in registers, place it inline, pass it indirectly, share immutable
storage, or allocate it on the managed heap when those choices cannot be
observed by EL code. Struct values are never null and have no identity operation.

A recursive struct is well formed only when every recursive containment cycle
crosses a built-in managed-indirection boundary. Structs, tuples, fixed arrays,
structural unions, transparent aliases, and user-defined generic structs are
inline constructors and do not break a cycle. `List(a)`, `Map(k, v)`,
`Slice(a)`, `string`, `bytes`, `bits`, and designated opaque managed standard-
library types do break a cycle. Zero-length arrays receive no exception.

```el
defstruct Invalid do
  next: Invalid             # invalid: inline infinite layout
end

defstruct Node do
  children: [Node]          # valid: List is managed indirection
end
```

The same rule detects mutual, cross-module, and generic constructor cycles. For
example, wrapping a recursive field in an inline tuple, array, union, or
`Box(a)`-style value struct does not make it finite. If an associated-type
projection affects layout, the compiler checks the concrete projection again
after protocol resolution and reports both the declaration and instantiation
that create an invalid cycle.

`@derive` requests compiler-generated `defimpl` blocks. Derivation succeeds only
when every participating field supports the requested protocol. For example,
deriving `Eq` for `User` requires `Eq` implementations for `u64` and `string`.

Generic struct values use the same immutable value semantics as concrete
structs. Their layout is specialized for each concrete type application used by
the program.

### 6.4 Composite types

V1 supports these composite categories:

- tuples: heterogeneous fixed-size values such as `{string, i64}`;
- lists: immutable homogeneous linked lists, type `[a]`;
- maps: immutable key/value collections, type `Map(k, v)`;
- arrays: fixed-size homogeneous values such as `[i64; 3]`, where the length is
  part of the type;
- slices: bounded views over contiguous elements, type `Slice(a)`;
- structs: nominal records declared with `defstruct`;
- functions: callable values such as `(i64, i64) -> i64`; and
- atoms: interned symbolic literals such as `:ok`, `:error`, and `:eof`.

Built-in and user-defined type constructors use the same type inference and
application rules. A `Slice(a)` is an immutable, read-only value describing a
contiguous range of backing storage. It retains that storage through a managed
reference, so it may safely outlive the binding or scope from which it was
created. Subslicing shares the backing storage and is O(1); all bounds are
checked.

Conceptually, a slice contains a backing-storage reference, an offset, and a
length. The backing reference is not observable identity. When slicing a value
stored inline, the compiler may promote or copy its storage as needed, provided
the choice is not observable. Retaining a small slice may retain a much larger
backing allocation, so the standard library provides an explicit copying
operation for programs that need independent compact storage.

Atoms have singleton literal identities. They are commonly used as the first
element of a tagged tuple. Arbitrary conversion of runtime strings to atoms is
not supported because an unbounded atom table would create a memory leak.

Composite construction has one canonical spelling per category:

```el
tuple = {name, score}
list = [1, 2, 3]
list_with_tail = [head | tail]
array = #[1, 2, 3]
map = %{"one" => 1, "two" => 2}
point = %Point{x: 1, y: 2}
```

A tuple contains at least two elements. V1 has no singleton tuple, and `unit`
serves instead of an empty tuple. List literals are immutable and homogeneous;
`[head | tail]` requires `tail` to have the same list type. `#[...]` constructs a
fixed-size homogeneous array, and its element count is inferred into the array
type. For example, `coordinates = #[10, 20, 30]` infers `[i64; 3]`; a local
binding does not repeat the length unless an explicit contract is useful. Array
repetition syntax is deferred. Empty list and map literals require an expected
type when their element types cannot otherwise be inferred. An empty array
literal always has length zero but likewise needs an expected item type, as in
`empty: [u8; 0] = #[]`.

In user-written types, an array length is a nonnegative integer literal
representable as `usize`, such as `[a; 2]` or `[Point; 16]`. V1 has no symbolic
length variables, const generics, length arithmetic, or inferred `_` placeholder
inside an array type. A function may remain generic over the item type while
fixing a literal length, but it cannot abstract over the length:

```el
def first_of_pair(values: [a; 2]) -> a do
  values[0]
end
```

Algorithms accepting arbitrary contiguous lengths use `Slice(a)`; generic
traversal uses `Iterable` or `Enum`. Arrays of different lengths are distinct
types and have no implicit conversion.

`%{key => value}` constructs an immutable `Map(k, v)`. Map construction and key
operations require `k` to implement both `Eq` and `Hash`. The core option type is
the transparent tagged union `Option(a) = {:some, a} | :none`, and the minimal
immutable map API is:

```el
Map.new() -> Map(k, v)
Map.fetch(map: Map(k, v), key: k) -> Option(v)
Map.put(map: Map(k, v), key: k, value: v) -> Map(k, v)
Map.remove(map: Map(k, v), key: k) -> Map(k, v)
Map.size(map: Map(k, v)) -> usize
```

Each operation that examines or changes keys requires `k: Eq` and `k: Hash`;
signatures above omit repeated `when` clauses for readability.

Maps iterate in deterministic insertion order. Replacing the value for an
existing equal key preserves that key's position. Removing a key deletes its
position, and inserting it again appends it at the end. Map literals evaluate
entries from left to right; a later duplicate replaces the earlier value
without moving the key. Map equality compares key/value membership and ignores
insertion order, while `Show` and `Iterable` observe insertion order. The
runtime's seeded hash strategy never changes this order.

Read indexing is supported for arrays, slices, `bytes`, and `bits`:

```el
item = array[index]
item = slice[index]
byte = data[index]
bit = bit_data[index]
```

The index has type `usize`. An out-of-bounds index is an unrecoverable runtime
failure. Array and slice indexing returns their item type, byte indexing returns
`u8`, and bit indexing returns `bool`. Bit index zero denotes the most-significant
bit of the first source byte. Strings, lists, and maps do not support index
syntax; maps use `Map.fetch` so absence remains explicit. Indexed update remains
outside v1.

Slices are constructed and subdivided through functions rather than range
syntax:

```el
whole = Slice.from_array(array)
part = Slice.subslice(whole, start, length)
copy = Slice.copy(part)
```

`from_array` creates a managed view over the array's elements. `subslice` is
bounds-checked and shares backing storage; `copy` creates compact independent
managed storage. V1 has no slice literal or range expression.

#### 6.4.1 Core collection modules

Generic traversal belongs to `Enum`, not to a particular collection module.
Every `Enum` signature below requires `i: Iterable`; the repeated `when i:
Iterable` clause is omitted from the listing for readability but remains
mandatory in the actual EL declaration:

```el
Enum.count(values: i) -> usize
Enum.at(values: i, index: usize) -> Option(Iterable.Item(i))
Enum.to_list(values: i) -> [Iterable.Item(i)]
Enum.map(values: i, function: (Iterable.Item(i)) -> b) -> [b]
Enum.filter(values: i, predicate: (Iterable.Item(i)) -> bool) ->
  [Iterable.Item(i)]
Enum.reduce(values: i, initial: a,
  reducer: (a, Iterable.Item(i)) -> a) -> a
Enum.each(values: i, function: (Iterable.Item(i)) -> unit) -> unit
Enum.any(values: i, predicate: (Iterable.Item(i)) -> bool) -> bool
Enum.all(values: i, predicate: (Iterable.Item(i)) -> bool) -> bool
```

These functions follow the selected iterable's deterministic order. `at` uses a
zero-based `usize` position, returns `{:some, item}` when that position exists,
returns `:none` otherwise, and stops after finding the requested item.
`to_list`, `map`, and `filter` return lists because v1 has no higher-kinded
abstraction for reconstructing an arbitrary input container. `reduce` is strict
and left-to-right. `each` visits every item, while `any` and `all` stop as soon
as their result is known. On maps the item type is `{k, v}` and order is
insertion order. Function arguments are monomorphic named function values in
v1 because anonymous functions and closures are deferred.

`Enum.count` traverses the iterable and is O(n). Collection-specific structural
sizes and minimal conversion operations are:

```el
List.reverse(values: [a]) -> [a]
Array.length(values: [a; N]) -> usize
Slice.length(values: Slice(a)) -> usize
Bytes.byte_size(values: bytes) -> usize
Bytes.slice(values: bytes, start: usize, length: usize) -> bytes
Bytes.from_list(values: [u8]) -> bytes
Bytes.to_list(values: bytes) -> [u8]
```

Here `N` schematically denotes the array type's compiler-known length; it is not
valid user generic syntax. `Array.length` and `Slice.from_array` are compiler-
provided standard intrinsics instantiated for every concrete literal length.
Standard array implementations of `Eq`, `Ord`, `Hash`, `Show`, and `Iterable`
are generated on the same concrete-length basis when their item constraints
hold. Array, slice, and byte sizes are O(1). `Enum.at` traverses at most
`min(index + 1, length)` items. `List.reverse`, byte/list conversion, and `Enum`
list-producing operations are O(n) and allocate fresh logical values.
`Bytes.slice` is bounds-checked with `index_out_of_bounds` and may share
immutable backing storage. `List.new` is omitted: an empty list is written `[]`
with an expected type when necessary. Sorting, searching, zipping, chunking,
and similar conveniences are ordinary future library growth rather than v1
language surface.

#### 6.4.2 Representation boundary

Arrays contain exactly `N` elements in source order. Arrays and slices are
semantically contiguous and provide O(1) indexing; slice offsets and lengths are
representable as `usize`. A string exposes one stable UTF-8 byte sequence through
its APIs regardless of how the compiler stores it.

V1 does not stabilize struct field offsets, padding, or alignment; tuple, union,
list, map, string, or slice physical layouts; union discriminant sizes or
values; the storage size of `bool`, atoms, or `unit`; managed-object headers;
function-value representation; symbol mangling; or calling conventions. The
compiler may scalarize, copy, share, inline, or heap-allocate values whenever EL
code cannot observe the choice.

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
- Parameter types are mandatory. A written return type is exact; if omitted,
  it defaults to `unit`.
- The final expression is the normal return value.
- `return expression` is allowed for early return and must match the function's
  declared return type.
- Bare `return` is not supported; a unit-returning function uses `return unit`.
- Overloading by parameter types is not supported.
- A bare named function such as `square` produces a function value; a qualified
  reference such as `Math.square` does the same after ordinary visibility and
  module resolution. Adding parentheses calls the function.
- Every function value has one exact structural type such as `(i64) -> i64` and
  contains no captured environment. Parameter and result types must match
  exactly; v1 has no function-type variance or implicit coercion.
- A local binding shadows a bare function name. Qualification can still name a
  visible public function. Outside code cannot directly name a `defp` function,
  although owning-module code may pass or return its value.
- A generic named function is specialized to one concrete function value using
  its expected function type and surrounding inference. V1 has no polymorphic
  function values; an undetermined specialization is a compile-time error.
- Protocol operations cannot be taken as function values in v1. Code that needs
  one defines an ordinary named wrapper with the required protocol constraint.
- Function values can be called and passed but implement none of `Eq`, `Ord`,
  `Hash`, or `Show`.
- Closures and anonymous functions are deferred.
- Partial application and bound receiver methods are deferred.
- Lowercase type identifiers in the signature are inferred generic parameters.
- `when parameter: Protocol` clauses constrain generic parameters.
- Call sites never supply an explicit type-argument list.

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

An implementation for a generic type may introduce and constrain the type
parameters appearing in its target:

```el
defimpl Show, for: Box(a) when a: Show do
  def show(value: Box(a)) -> string do
    Show.show(value.value)
  end
end
```

Protocol conformance and dispatch are checked statically. A call using a generic
constraint is resolved separately for each monomorphized concrete
instantiation. Protocol names are not runtime value types in v1, and there are
no witness tables or existential protocol boxes. There is no implicit
implementation based only on a matching method name.

`Self` refers to the implementation type. Protocol-associated types may appear
through a constrained generic parameter, including those required by core
protocols such as `Iterable`.

An associated type is a named type selected by an implementation. The protocol
must declare each associated type explicitly even when its uses in method
signatures might permit inference:

```el
defprotocol Iterable do
  type Item
  type Cursor

  def iter(value: Self) -> Cursor
  def next(cursor: Cursor) -> {:item, Item, Cursor} | :done
end
```

Every implementation assigns each declared associated type exactly once:

```el
defimpl Iterable, for: List(a) do
  type Item = a
  type Cursor = List.Cursor(a)

  def iter(value: List(a)) -> List.Cursor(a) do
    # implementation
  end

  def next(cursor: List.Cursor(a)) ->
      {:item, a, List.Cursor(a)} | :done do
    # implementation
  end
end
```

Missing, duplicate, or undeclared assignments are compile-time errors. Associated
types have no defaults, parameters, or separate constraints in v1. After
substituting `Self` and all associated types, every implementation method must
exactly match its protocol signature. Only `def` methods are allowed inside a
`defimpl`; they are protocol entries rather than separately exported module
functions.

Outside a protocol declaration, an associated type is referenced with a
qualified projection such as `Iterable.Item(a)`, `Iterable.Cursor(a)`, or
`Reader.Error(r)`. Inside the declaring protocol its bare name is used. A
projection remains abstract while a constrained generic body is checked and is
resolved statically for each concrete monomorphized implementation.

An implementation may be declared only by the package that owns the protocol or
the package that owns the target type. Across the resolved dependency graph,
exactly one implementation may exist for a protocol/type pair. Generic
implementation heads must not overlap, and positive protocol constraints do not
make otherwise overlapping heads distinct. V1 has no specialization, negative
implementations, default protocol method bodies, or protocol inheritance.
Transparent aliases and structural unions are not implementation targets.
Requesting `@derive` when an explicit implementation already exists is an error.

### 6.7 Core standard-library protocols

The initial standard library defines:

- `Eq`: equality; backs `==` and `!=` for non-primitive values.
- `Ord`: ordering; backs `<`, `<=`, `>`, and `>=` where implemented.
- `Show`: readable conversion to `string`.
- `Hash`: feeds values into the opaque, seeded `Hasher` used by maps.
- `Iterable`: exposes an item type and iteration used by `for ... in`.
- `Writer`: writes complete `bytes` values and returns tagged success/error
  values; string writing is a UTF-8 helper layered over it.
- `Reader`: reads data and returns tagged success/error or end-of-input values.
- `Concat`: concatenates two values of the same type.

The core method sets are:

```el
defprotocol Eq do
  def eq(left: Self, right: Self) -> bool
end

defprotocol Ord do
  def compare(left: Self, right: Self) -> :less | :equal | :greater
end

defprotocol Show do
  def show(value: Self) -> string
end

defprotocol Iterable do
  type Item
  type Cursor

  def iter(value: Self) -> Cursor
  def next(cursor: Cursor) -> {:item, Item, Cursor} | :done
end

defprotocol Reader do
  type Error

  def read(reader: Self, max_bytes: usize) ->
    {:ok, bytes} | :eof | {:error, Error}
end

defprotocol Writer do
  type Error

  def write(writer: Self, data: bytes) -> {:ok, unit} | {:error, Error}
  def flush(writer: Self) -> {:ok, unit} | {:error, Error}
end

defprotocol Hash do
  def hash(value: Self, state: Hasher) -> Hasher
end
```

`Iterable.iter` statically selects one implementation, and that same
implementation supplies `next`; a cursor does not trigger independent protocol
dispatch. Cursors are immutable state values, and `for` threads each returned
cursor into the next call.

Standard iteration order is part of the API: lists traverse head to tail;
arrays and slices use increasing indices; `bytes` uses increasing byte offsets;
string codepoint and grapheme views follow source order; and maps yield `{key,
value}` tuples in insertion order. An implementation may choose any immutable
cursor representation that preserves its documented order.

`Reader.read` returns at most `max_bytes`. Except when `max_bytes` is zero, an
`{:ok, data}` result contains at least one byte, so callers cannot confuse an
empty successful read with end-of-input. `read_exact`, `read_all`, and similar
operations are standard-library helpers rather than required protocol methods.

`Writer.write` accepts the entire byte sequence or returns an error; callers do
not handle partial successful writes. `flush` may be a no-op for an unbuffered
implementation. A string-writing helper exposes the string's UTF-8 bytes and
calls `write`. Resource release is deliberately absent from both `Reader` and
`Writer` and remains explicit through `defer` and type-specific cleanup APIs.

The common console functions accept any value that implements `Show` and return
`unit`:

```el
IO.print(value: a) -> unit when a: Show
IO.println(value: a) -> unit when a: Show
IO.report(value: a) -> unit when a: Show
```

`print` writes to standard output without a newline; `println` writes to standard
output with one newline; and `report` writes to standard error with one newline.
Each function statically invokes the selected `Show.show` implementation; this
is behavior declared by the function's constraint, not a general implicit
conversion to `string`. A `string` therefore writes as itself, while errors,
numbers, and collections can be passed directly. Failure in these convenience
functions is unrecoverable. Programs that must recover use
`IO.stdin() -> IO.Stdin`, `IO.stdout() -> IO.Stdout`, and
`IO.stderr() -> IO.Stderr`. `IO.Stdin` implements `Reader`; the output types
implement `Writer`; their associated error type is `IO.Error`. These
process-owned handles are not closed by EL programs.

Files expose statically separated byte reader and writer handles:

```el
File.open_read(path: string) ->
  {:ok, File.Reader} | {:error, File.Error}
File.create(path: string) ->
  {:ok, File.Writer} | {:error, File.Error}
File.append(path: string) ->
  {:ok, File.Writer} | {:error, File.Error}
@type File.Stream = File.Reader | File.Writer
File.close(stream: File.Stream) ->
  {:ok, unit} | {:error, File.Error}
```

`File.Reader` implements `Reader`, and `File.Writer` implements `Writer`.
`create` creates or truncates; `append` creates if absent and otherwise writes at
the end. V1 has no combined read/write handle, seeking, permission API, or text
mode. Text encoding remains explicit through `String.from_bytes` and
`String.bytes`. `IO.Error` and `File.Error` implement `Show`, so diagnostic
branches can pass them directly to `IO.report`.

`IO.Error` and `File.Error` are opaque immutable values with stable inspection
APIs. The shared closed kind and operation types are:

```el
@type IO.ErrorKind =
  :not_found | :permission_denied | :already_exists | :invalid_input |
  :is_directory | :not_directory | :closed | :broken_pipe |
  :out_of_space | :other

@type IO.Operation =
  :open_read | :create | :append | :read | :write | :flush | :close

IO.error_kind(error: IO.Error) -> IO.ErrorKind
IO.error_operation(error: IO.Error) -> IO.Operation
IO.error_code(error: IO.Error) -> Option(i64)

File.error_kind(error: File.Error) -> IO.ErrorKind
File.error_operation(error: File.Error) -> IO.Operation
File.error_code(error: File.Error) -> Option(i64)
```

The kind is the portable basis for control flow. `:other` represents a host
failure without a more specific v1 mapping. The optional code preserves a
target-dependent operating-system error number for diagnostics; portable
programs do not branch on it. Runtime I/O retries interrupted host operations
internally rather than exposing interruption as a stable kind. Adding a new
kind or operation changes an exhaustive union and therefore requires a recorded
language-version decision.

Both error types implement `Show`, `Eq`, and `Hash`, but not `Ord`. Equality and
hashing use exactly the documented operation, kind, and optional system code;
unexposed diagnostic text does not participate. Copies have ordinary value
semantics and no resource identity. Values are created only by the standard
library rather than by user construction.

`Hasher` is an opaque standard-library state initialized with a runtime-selected
seed. `Hash.hash` returns updated state rather than a stable public integer.
Values equal under `Eq` must feed equivalent data into `Hasher`; derived `Hash`
implementations process struct fields in declaration order.

Protocol implementations must obey these semantic laws:

- `Eq` is reflexive, symmetric, and transitive.
- `Ord` defines a total order and returns `:equal` exactly when `Eq.eq` is true.
- Values equal under `Eq` feed equivalent data into `Hash`.

A user implementation that violates these laws has erroneous behavior, such as
failed lookups or inconsistent comparisons, but cannot by itself cause memory
unsafety. Equality and ordering require matching operand types; v1 has no
cross-numeric equality or ordering. Floats implement none of `Eq`, `Ord`, or
`Hash` because IEEE NaN behavior conflicts with these laws.

The standard implementations are fixed as follows:

- `Eq` covers `bool`, integers, `rune`, `string`, `bytes`, `bits`, atoms, and
  `unit`; tuples when every element implements `Eq`; lists, arrays, and slices
  when their item type does; maps when their values do; and derived structs when
  every field does. Equality is structural. Slices compare visible contents,
  and map equality ignores insertion order.
- `Ord` covers the same scalar types, except maps, and extends structurally to
  tuples, lists, arrays, slices, and derived structs whose components implement
  `Ord`. Sequential values compare lexicographically with a proper prefix
  first; derived structs compare fields in declaration order. Strings compare
  Unicode scalar values without normalization or case folding.
- `Hash` covers the lawful `Eq` scalar and sequential types and extends to
  tuples and derived structs when every component implements `Hash`. Strings
  hash their exact UTF-8 bytes. Maps do not implement `Hash` in v1.
- `Show` covers standard scalar and collection types and derived structs. A
  tuple implements `Show` when every element does; a list, array, or slice does
  when its item type does; and `Map(k, v)` does when both `k` and `v` do (with
  the map's existing `k: Eq + Hash` requirement). Collection displays use
  `{a, b}`, `[a, b]`, `#[a, b]`, `Slice[a, b]`, and `%{key => value}` forms.
  Maps display entries in insertion order. This output is human-readable
  diagnostics, not a stable serialization format, and formatting may evolve
  between language releases. Standard I/O and file error types also implement
  `Show`.

The opaque standard errors `IO.Error`, `File.Error`, and `String.Utf8Error`
implement `Eq`, `Hash`, and `Show` according to their documented inspection
fields. They do not implement `Ord`.

Functions, buffers, floats, and resource handles implement none of `Eq`, `Ord`,
or `Hash`. Maps implement neither `Ord` nor `Hash`. Structural unions do not
automatically implement protocols; a nominal wrapper or explicit conversion is
required when protocol behavior is needed.

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

### 6.8 Process inputs and native paths

V1 programs receive process arguments and read environment variables through
the reserved `Process` module:

```el
Process.arguments() -> {:ok, [string]} | {:error, {:invalid_text, usize}}
Process.get_env(name: string) ->
  {:ok, string} | :not_found | {:error, :invalid_name | :invalid_text}
```

`arguments` returns only the arguments supplied after the executable name, in
launch order. The `usize` in `{:invalid_text, index}` is the zero-based index in
that returned argument list. Conversion is all-or-nothing: the function does
not return a partial list. `get_env` returns the launch-time value of one
variable; it returns `:not_found` when the name was absent. A name containing
U+0000 or `=` returns `{:error, :invalid_name}`. V1 provides no environment
enumeration, mutation, current-directory mutation, or executable-path API.

The runtime snapshots the arguments and environment before calling `Main.main`,
so repeated calls observe the same values. On Unix-like targets, native argument
and environment byte strings must be valid UTF-8. On Windows, native UTF-16 must
be a well-formed scalar sequence. Invalid input is reported by the tagged result
above; it never undergoes replacement-character or locale-dependent conversion.
The host's environment-name comparison rules remain target-dependent, including
Windows case insensitivity.

All v1 file functions continue to accept `string` paths. Conversion to a native
path is exact and locale-independent:

- Unix-like targets pass the string's UTF-8 bytes unchanged.
- Windows targets transcode Unicode scalar values to UTF-16 without
  normalization, case folding, separator rewriting, or canonicalization.
- A path containing U+0000 fails with `File.Error` kind `:invalid_input` and the
  requested file operation.

Consequently, every accepted EL `string` path is converted without loss, but v1
cannot name a Unix path containing invalid UTF-8 bytes or a Windows path
containing unpaired UTF-16 surrogates. Native path spelling, separators,
absolute-path rules, case-sensitivity, and symbolic-link behavior otherwise
follow the host operating system. A future native-string/path type may widen
access without changing the meaning of existing `string` paths.

## 7. Strings and binaries

### 7.1 `string`

`string` is distinct from binary data and is always valid UTF-8. Construction
from untrusted bytes validates encoding and returns a tagged success/error value.

EL v1 pins text semantics to Unicode 17.0.0. Grapheme boundaries use the
untailored default extended-grapheme-cluster algorithm from Unicode Standard
Annex #29 revision 47, conformance clause UAX29-C1-1. Compiler distributions
bundle the corresponding Unicode Character Database and segmentation tables;
host locale, operating-system APIs, and installed ICU versions cannot change an
EL program's result. A future Unicode upgrade is a recorded language-semantic
change rather than an incidental dependency update.

Integer indexing is not supported: `text[i]` is a compile-time error. UTF-8 code
points have variable encoded width, so such an operation would hide whether `i`
means a byte offset, code-point offset, or user-perceived character offset.
Programs use these `String` functions instead:

```el
String.byte_size(text)
String.length(text)
String.empty(text)
String.downcase(text)
String.contains(text, pattern)
String.split(text, separator = "")
String.replace(text, pattern, replacement)
String.bytes(text)
String.codepoints(text)
String.graphemes(text)
String.codepoint_view(text)
String.grapheme_view(text)
```

Their semantics are:

- `byte_size(s) -> usize` returns the number of bytes in the UTF-8 encoding. It
  is O(1) because `string` stores its byte length.
- `length(s) -> usize` returns the number of Unicode grapheme clusters. It is
  generally O(n), matching Elixir's human-text-oriented meaning of length. Its
  boundaries are exactly those used by `graphemes` and `grapheme_view`.
- `empty(s) -> bool` is equivalent to `byte_size(s) == 0`.
- `contains(s, pattern) -> bool` performs an exact, case-sensitive substring
  search over UTF-8 bytes. The empty pattern is contained in every string.
- `split(s, separator = "") -> [string]` separates from left to right at exact,
  non-overlapping separator matches and preserves leading, trailing, and
  adjacent empty fields. An empty separator performs no split and returns
  `[s]`.
- `bytes(s) -> bytes` returns the first-class immutable UTF-8 byte sequence and
  may share the string's immutable storage.
- `codepoints(s) -> [rune]` eagerly returns Unicode scalar values in source
  order.
- `graphemes(s) -> [string]` eagerly returns one valid string per extended
  Unicode grapheme cluster in source order under the pinned untailored UAX #29
  rules. Returned strings may share immutable source storage.
- `codepoint_view(s) -> String.CodepointView` lazily iterates `rune` values.
- `grapheme_view(s) -> String.GraphemeView` lazily iterates grapheme-cluster
  string slices.

The eager list functions favor the common developer experience and make their
allocation visible in the return type. The explicitly named views retain the
source string and support allocation-sensitive or early-terminating traversal.
No operation returns mutable access to string storage.

EL performs no implicit normalization, case folding, or locale tailoring before
segmentation. The default UAX #29 rules operate directly on the source scalar
sequence while preserving canonical-equivalent boundaries. Unicode scalar-value
validity itself includes unassigned non-surrogate code points and therefore does
not depend on whether Unicode 17.0 assigns a character to a value.

The minimal conversion API is:

```el
String.from_bytes(data: bytes) ->
  {:ok, string} | {:error, String.Utf8Error}
String.utf8_error_offset(error: String.Utf8Error) -> usize
Rune.to_string(value: rune) -> string
Bytes.to_bits(data: bytes) -> bits
Bits.to_bytes(data: bits) -> Option(bytes)
```

`from_bytes` validates UTF-8. `Bytes.to_bits` is lossless, while `Bits.to_bytes`
returns `:none` unless the bit length is byte-aligned. Conversions may share
immutable storage but never expose mutation.

`String.Utf8Error` is an opaque immutable value whose offset is the zero-based
byte offset of the first invalid UTF-8 sequence. An incomplete final sequence
reports the offset at which that sequence begins. It implements `Show`, `Eq`,
and `Hash`, but not `Ord`; equality and hashing use only the offset. The same
rules apply when `Buffer.to_string` reports this error.

### 7.2 `bytes`, `bits`, and `rune`

- `bytes` is a raw, byte-aligned sequence. It carries no text encoding promise.
- `bits` is a bit sequence of arbitrary length and generalizes `bytes`.
- `rune` is one Unicode scalar value, not a byte and not a grapheme cluster.

Conversions among these types are explicit. Converting `bytes` to `string`
validates UTF-8. Converting `string` to `bytes` exposes its UTF-8 encoding.

The minimal arbitrary-length bit API is:

```el
Bits.bit_size(value: bits) -> usize
Bits.slice(value: bits, start: usize, length: usize) -> bits
Bits.to_bytes(value: bits) -> Option(bytes)
Bytes.to_bits(value: bytes) -> bits
```

`Bits.slice` is bounds-checked and fails with `index_out_of_bounds` rather than
returning an option. It may produce a non-byte-aligned value. `Bits.to_bytes`
returns `:none` when the bit length is not divisible by eight. `bits` supports
read indexing as described in section 6.4 and concatenation through `Concat`.

### 7.3 `Buffer`

`Buffer` is a growable standard-library builder for constructing `string` or
`bytes` without repeated immutable concatenation. It is a byte builder with an
explicit value-style API:

```el
mut buffer = Buffer.new()
buffer := Buffer.append_string(buffer, "hello")
buffer := Buffer.append_byte(buffer, 0x20)
buffer := Buffer.append_string(buffer, "world")
```

Its minimal API is:

```el
Buffer.new() -> Buffer
Buffer.byte_size(buffer: Buffer) -> usize
Buffer.append_byte(buffer: Buffer, value: u8) -> Buffer
Buffer.append_bytes(buffer: Buffer, value: bytes) -> Buffer
Buffer.append_string(buffer: Buffer, value: string) -> Buffer
Buffer.to_bytes(buffer: Buffer) -> bytes
Buffer.to_string(buffer: Buffer) ->
  {:ok, string} | {:error, String.Utf8Error}
```

There is no overloaded `append`. `to_bytes` always succeeds, while `to_string`
validates the complete byte sequence. Values returned by either conversion never
change after later buffer operations. The runtime may reuse uniquely held
storage or use copy-on-write, but observable semantics remain local rebinding
and immutable snapshots.

### 7.4 Bitstring construction and matching

V1 source syntax is deliberately byte-aligned. A `<<...>>` construction
expression produces `bytes`, and a bitstring pattern consumes `bytes`:

```el
packet = <<version::unsigned-big-size(8),
  length::unsigned-big-size(16),
  payload::bytes>>

match packet do
  <<version::unsigned-big-size(8),
    length::unsigned-big-size(16),
    payload::bytes-size(usize(length))>> -> consume(version, payload)
  _ -> reject_packet()
end
```

A segment is `value_or_pattern::modifier-modifier...`. Modifier order does not
affect semantics; the formatter emits kind, sign, byte order, then size. V1
accepts only these forms:

- integer segments, optionally marked `integer`, with `signed` or `unsigned`
  (default `unsigned`), `big`, `little`, or `native` byte order (default `big`),
  and a required literal `size` of 8, 16, 24, 32, 40, 48, 56, or 64 bits;
- `bytes` segments with an optional `size(expression)` measured in bytes; and
- a final unsized `bytes` pattern that captures the remaining input.

In construction, an integer operand may have any integer type and must fit the
declared signedness and width. A sized `bytes` operand must contain exactly the
declared number of bytes. A statically known violation is a compile-time error;
otherwise construction fails unrecoverably with `bitstring_size_mismatch` and
never truncates or pads a value. An unsized construction `bytes` segment appends
the operand's complete contents.

In a pattern, unsigned integer segments bind `u64` and signed integer segments
bind `i64`; literal integer patterns are checked against the same range. A sized
`bytes` segment captures exactly that many bytes. Its size expression may use an
in-scope value or a value bound by an earlier segment, but not one bound later.
Insufficient input, a literal mismatch, or unconsumed input makes the pattern
fail normally rather than causing an unrecoverable failure.

`big` and `little` have target-independent meanings. `native` uses the
compilation target's byte order and is an intentional source of target-dependent
behavior. Duplicate, conflicting, unknown, or out-of-scope modifiers are
compile-time errors. Empty `<<>>` constructs empty `bytes` and matches only empty
`bytes`.

Source segments of kind `float`, `utf8`, `utf16`, `utf32`, or `bits`; explicit
`unit`; non-byte-aligned widths; and arbitrary-width integer segments are
post-v1. Arbitrary-length `bits` values remain usable through `Bits.slice`,
indexing, conversion, and concatenation without implying those source forms.

## 8. Expressions and control flow

### 8.1 Blocks

`do ... end` forms a lexical scope and evaluates to its final expression. An
empty block evaluates to `unit`.

#### 8.1.1 Evaluation order

EL evaluates eagerly from left to right. This applies to a function target and
its arguments, operator operands, tuple/list/array/map elements, struct field
initializers in source order, and the value and index of an indexing expression.
Every expression is evaluated exactly once. `and` and `or` are the exceptions to
eager operand evaluation and short-circuit their right operand.

A pipeline evaluates its left input before the explicit arguments written on
its right. An assignment evaluates and type-checks its right-hand side before
replacing the old binding value. A `match` evaluates its subject once and then
tests arms from top to bottom; pattern tests themselves have no user-visible
side effects.

A `with` evaluates clause expressions once from left to right. Each successful
pattern makes its bindings available to subsequent clauses and the body. The
first value that does not match its clause pattern becomes the result without
evaluating later clauses or the body.

Map literal entries are evaluated and inserted from left to right. If two
evaluated keys are equal, the later entry replaces the earlier value, but every
key and value expression is still evaluated.

### 8.2 Conditionals

```el
label = if score >= 50 do
  "pass"
else
  "fail"
end
```

Conditions must have type `bool`; EL has no truthiness. If an `if` is used as a
value, both branches are required and must have the same type unless an expected
structural union type accepts both through unambiguous injection. EL does not
infer a new union from mismatched branches. An `if` used only for effects may
omit `else`, in which case its type is `unit`.

### 8.3 Pattern matching and errors

Recoverable errors are tagged tuple values, normally `{:ok, value}` or
`{:error, reason}`. EL has no exceptions, throwing, or catching:

```el
match Parser.parse_int(input) do
  {:ok, value} -> IO.println(value)
  {:error, reason} -> IO.report(reason)
end
```

`match` is an expression. All reachable arms must return the same type when its
result is used, subject to the same expected-union injection rule as `if`. Every
match must be exhaustive. Infinite types such as integers and strings normally
require a wildcard or binding catch-all; closed structural unions, `bool`,
tuples, lists, and structs receive structural exhaustiveness checking.

V1 patterns are:

- `_`, which matches without binding;
- an identifier, which always introduces a new immutable binding;
- literal patterns;
- tuple and tagged-tuple patterns;
- `[]` and `[head | tail]` list patterns;
- struct patterns such as `%Point{x: x}`, where omitted fields are ignored;
- byte-aligned bitstring patterns from section 7.4; and
- `name: Type`, which selects exactly one normalized structural-union member.

Patterns compose recursively. A binding introduced by a pattern is visible only
in that match arm. It may shadow an outer binding under the usual shadow-warning
rule, but one pattern may not bind the same name more than once; repeated names
do not express equality constraints. Typed binding patterns are available only
for structural-union elimination, and the named type must be exactly one
normalized member.

Arms are tested from top to bottom. An arm that is provably unreachable because
an earlier arm subsumes it is a compile-time error. At minimum, the compiler
detects arms after a wildcard or general binding and repeated identical literal
arms. V1 has no match guards, alternative patterns, pinning, or map patterns.

Unrecoverable runtime failures such as an internal invariant violation may abort
the process. They are not catchable and must not be used for ordinary errors.

For linear result propagation, `with` composes refutable operations without
nested `match` expressions:

```el
with {:ok, left} <- parse_left(input),
     {:ok, right} <- parse_right(input) do
  {:ok, left + right}
end
```

Clauses are comma-separated and use `pattern <- expression`. On success, clause
bindings remain in scope for later clauses and the body. On failure, the
unmatched value is returned unchanged and must fit the `with` result type under
the normal expected-union injection rule. `with` has no `else` form; callers use
an exhaustive `match` when failures need transformation.

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
`unit`. A `for` loop obtains values through the `Iterable` protocol. Its binding
pattern must be irrefutable for the selected implementation's `Item` type, so
iteration never silently skips a mismatching item and never introduces a runtime
pattern failure. Refutable processing uses an exhaustive `match` inside the loop.
C-style loops, `loop`, comprehensions, and implicit recursion syntax are not
supported. `break` and `continue` are deferred.

### 8.5 Early return

Functions normally return the value of their final expression. `return
expression` exits the nearest enclosing function early:

```el
def require_positive(value: i64) -> {:ok, i64} | {:error, :not_positive} do
  if value <= 0 do
    return {:error, :not_positive}
  end

  {:ok, value}
end
```

The returned expression must have the function's declared return type. Bare
`return` is invalid; unit-returning functions use `return unit`. A terminating
`return` path produces no local value and is therefore compatible with any
expected type for the surrounding branch or block.

`return` may appear in nested conditionals, matches, and loops, but not at module
scope or inside a deferred cleanup action. Before control reaches the caller,
all deferred actions registered for scopes being exited run in LIFO order.

### 8.6 Pipeline operator

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

### 8.7 Operators

From highest to lowest precedence, v1 operators are:

```text
postfix:         call, field access, indexing
unary:           - ! ~
multiplicative:  * / %
additive:        + -
shift:           << >>
bitwise and:     &
bitwise xor:     ^
bitwise or:      |
concatenation:   ++
comparison:      < <= > >=
equality:        == !=
logical and:     and
logical or:      or
ascription:      ::
pipeline:        |>
```

Arithmetic, shifts, and bitwise operators associate left. `++` associates
right, and pipelines associate left. Comparison and equality operators are
non-associative, so forms such as `a < b < c` and `a == b == c` are rejected.
Unary operators associate right. An ascription applies to the complete
expression on its left at all higher precedence levels and does not chain.

Operators do not implicitly coerce values. `%` is integer-only. `and` and `or`
require `bool` operands and short-circuit. `++` resolves through
`Concat.concat`, while `|>` is compile-time syntax sugar and performs no
protocol dispatch. In expression position `|` is integer bitwise OR; in type
position it remains structural-union syntax.

### 8.8 Unrecoverable runtime failures

An unrecoverable runtime failure terminates the process immediately with a
nonzero status. It is not a value, cannot be caught, and does not unwind lexical
scopes or run any pending `defer` actions. A failure raised while executing a
deferred action likewise prevents all remaining actions from running.

For a source-mandated runtime check, the runtime makes a best-effort write to
standard error containing a stable category identifier and the package-relative
source file, line, and column of the operation that failed. The v1 categories
are `integer_overflow`, `division_by_zero`, `invalid_shift`,
`invalid_conversion`, `index_out_of_bounds`, `allocation_exhausted`, and
`console_output_failed`, plus `bitstring_size_mismatch`. Exact prose and the
numeric nonzero exit status are implementation-defined. If standard error itself
cannot be written, termination still occurs.

Debug and release builds report the same category for the same operation. V1
does not provide stack traces, custom panic values, a user-callable panic
operation, or any recovery mechanism for these failures. Compiler and runtime
invariant violations are implementation defects rather than language failures;
they may terminate with additional internal diagnostics and are not assigned a
source-level category.

## 9. Modules, packages, and physical layout

### 9.1 Modules

- A module is declared with a package-relative name such as
  `defmodule Http.Client do ... end`.
- One source file contains exactly one module declaration.
- Nested `defmodule` declarations are not supported in v1.
- `def` exports a function from its module; `defp` does not.
- The executable entry point is `Main.main() -> i32` for a target whose root
  module is the package-relative `Main`.
- The return value of `main` is forwarded to the host operating system as the
  process exit code. A platform may expose fewer bits to a waiting process; for
  example, POSIX environments commonly expose only the low eight bits.
- Top-level executable statements are not allowed.
- One module may span only one file in v1.

V1 has no imports, opened modules, user-defined preludes, or module aliases.
Names resolve as follows:

- A bare value name first resolves to a lexical local or parameter, then to a
  declaration in the current module, and finally to the fixed core prelude.
- Bare type and protocol names resolve in the current module and then in the
  core prelude.
- A module outside the current module is referenced with a qualified name such
  as `Http.Client.get`.
- Modules in the current package use package-relative names. A dependency module
  name begins with that dependency's declared root namespace. Core modules such
  as `IO`, `List`, and `String` use their prelude names.
- If a source name has more than one possible module resolution, compilation
  fails rather than selecting one by priority.

The core prelude contains no unqualified functions. Its type names are the
primitive types plus `Buffer`, `Hasher`, `List`, `Map`, `Option`, and `Slice`.
Its protocol names are `Eq`, `Ord`, `Show`, `Hash`, `Iterable`, `Reader`,
`Writer`, and `Concat`. Its root modules are:

```text
Array Bits Buffer Bytes Enum File IO List Map Process Rune Slice String
I8 I16 I32 I64 Isize U8 U16 U32 U64 Usize
```

Protocol names also qualify their operations, as in `Show.show(value)`. Ordinary
functions are never imported implicitly.

Prelude declarations and root modules are reserved. A package declaration may
not redefine one, and a package or dependency root namespace may not collide
with a prelude root. The prelude defines the single canonical optional-value
alias `Option(a) = {:some, a} | :none`; user code does not redeclare it.

`defstruct`, `@type`, and `defprotocol` declarations are public in v1. Struct
fields are public for construction and reading. A `defimpl` participates
globally in protocol resolution for the complete dependency graph. Only
functions distinguish public `def` from module-private `defp`.

A module may declare at most one function with a given name, regardless of
arity. EL does not use Elixir-style `name/arity` identities in v1; this keeps a
bare named function value unambiguous. Visibility controls whether source code
may name a function, not whether an already-obtained function value may be
called; an owning module may therefore return or pass a private function value.

### 9.2 Project manifest

An EL project is a directory containing an `el.toml` manifest. The manifest
declares the package ID, root namespace, package version, dependencies, and an
optional executable target. An illustrative executable package is:

```toml
[package]
name = "example"
namespace = "Example"
version = "0.1.0"

[deps]

[target]
main = "Main"
```

`package.name` is the package ID used by dependency and tooling metadata;
`package.namespace` is the root namespace used by source modules. V1
dependencies are EL packages; the manifest cannot declare native FFI libraries.
Package IDs use lowercase `snake_case`, and a root namespace is one `PascalCase`
component. Unknown manifest keys are errors.

Every package is automatically an importable collection of EL source modules;
there is no separate library target or stable compiled library artifact. A
package may declare at most one `[target]`. Its `main` value is package-relative,
must name a module owned by that package, and must expose `main() -> i32`. The
output executable uses `package.name`. Omitting `[target]` makes the package
library-only. Multiple executable targets and additional target kinds are
deferred beyond v1.

V1 supports local path dependencies and Git dependencies pinned to a full
commit hash:

```toml
[deps.parser]
path = "../parser"
version = "0.3.0"

[deps.http]
git = "https://example.com/http.git"
rev = "0123456789abcdef0123456789abcdef01234567"
version = "1.2.1"
```

Each dependency selects exactly one of `path` or `git`. Paths are resolved
relative to the manifest containing the entry. A Git entry requires `rev`; v1
does not accept branches, floating tags, or abbreviated commit hashes. The
dependency key must equal the resolved package's `package.name`, and its
manifest version must exactly equal the requested semantic version.

V1 performs no compatible-version search. Across the complete transitive graph,
every occurrence of one package ID must resolve to the same source, exact
version, and Git revision where applicable. A disagreement is a dependency
conflict; multiple simultaneous versions of one package are not supported.
Every package root namespace in that graph must also be unique. Duplicate root
namespaces are rejected even when the package IDs differ.
Dependency cycles are rejected.

`el.lock` records the complete resolved graph, package versions, Git commits,
and source metadata. Executable projects commit it. A path dependency remains a
live development input, so the lockfile records its identity and declared
version but does not make its contents reproducible. Registry sources, version
ranges, and a compatibility solver are deferred beyond v1.

Ordinary `el check` and `el build` create or refresh `el.lock` when resolution
changes. With `--locked`, a missing or stale lockfile is an error and the tool
does not modify it.

### 9.3 Physical layout

The conventional layout is:

```text
project/
  el.toml
  src/
    main.ell
    parser.ell
```

V1 has no language-integrated test declarations, test discovery, special
semantics for a `test/` directory, or `el test` command. Repositories may keep
ordinary scripts or separate executable packages under `test/`, but the EL tool
does not discover them. EL programs can be exercised through executable targets
and external scripts; the compiler's conformance suite is an implementation
facility rather than part of the package format.

Every source file under `src/` contains one `defmodule`. The manifest's root
namespace prefixes every project module. Module names are derived strictly from
paths relative to `src/`:

```text
namespace = "Example"

src/main.ell          -> defmodule Main        -> Example.Main
src/http/client.ell   -> defmodule Http.Client -> Example.Http.Client
src/json_api.ell      -> defmodule JsonApi     -> Example.JsonApi
src/foo/index.ell     -> defmodule Foo.Index   -> Example.Foo.Index
```

The compiler strips the source extension, requires every path component to be
lowercase `snake_case`, converts each component mechanically to `PascalCase`,
and joins components with dots. Acronyms receive no special casing, and
`index.ell` has no special meaning. The declared `defmodule` must exactly match
the derived package-relative name. The manifest target module name is also
package-relative; external package modules use their declared root namespace.

Invalid path components, path/declaration mismatches, duplicate modules, and
distinct paths that produce the same canonical module name are compile-time
errors. These rules make module discovery possible from the filesystem without
parsing every source file first.

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
shadow stack, or per-type tracing functions. Every EL function call is treated
as a possible collection point because the callee may allocate; private runtime
calls are classified explicitly as allocating or non-allocating. V1 performs no
interprocedural allocation-effect inference.

At every possible collection point, each live managed reference must remain
discoverable as an unmodified, aligned native pointer in a scanned register,
stack slot, global, or reachable managed object. It must not exist solely as an
integer, tagged pointer, compressed value, or interior pointer. Slices, string
views, and other managed views retain the base pointer of their backing
allocation even if lowering also computes a derived data pointer. Union payloads,
deferred captures, saved block results, call arguments, and runtime temporaries
obey the same rule.

Objects that may contain managed references use scanned allocation; raw byte,
bit, and numeric payload storage uses pointer-free allocation. A pointer-free
allocation must never later receive a managed reference. Managed global values
reside in collector-visible scanned storage registered during runtime startup.
Allocation failure is an unrecoverable runtime failure rather than a tagged
result.

Conservative scanning may retain an otherwise unreachable object when non-pointer
data happens to resemble its address. The collector cannot move or compact live
objects, but stable addresses simplify the initial runtime. EL's private
allocation API preserves the option to replace Boehm with a precise collector in
a later version without changing source-language semantics.

V1 has no user-visible finalizers or weak references. A compiler/runtime stress
mode attempts collection at every managed allocation site and is exercised in
both debug and optimized builds.

Reference: [Boehm GC overview](https://hboehm.info/gc/) and
[algorithm description](https://hboehm.info/gc/gcdescr.html).

### 10.2 Resource management

GC finalizers are nondeterministic and are not sufficient for scarce resources.
V1 uses an explicit, lexically scoped `defer` statement for deterministic
cleanup:

```el
match File.open_read(path) do
  {:ok, file} ->
    defer do
      match File.close(file) do
        {:ok, _} -> unit
        {:error, reason} -> IO.report(reason)
      end
    end

    process(file)

  {:error, reason} ->
    IO.report(reason)
end
```

Opaque OS resource values are the deliberate exception to ordinary value-backed
data semantics. Copying a resource handle creates another alias to the same
external resource; it does not duplicate that resource. Reads advance shared
external state, closing through one alias closes the resource for all aliases,
and operations through a closed alias return tagged errors. Closing an already
closed handle also returns a tagged error. Resource handles implement neither
`Eq` nor `Hash`, and user-defined structs do not acquire resource identity.

V1 has no affine ownership system, so programs are responsible for avoiding
unintended aliases and double close. This external identity exception is limited
to opaque standard-library resource types and must be documented on every such
type.

A `defer` is a statement of type `unit` and registers an action only when
execution reaches it. It belongs to the innermost enclosing lexical `do ... end`
block, including a function body, conditional branch, match arm, or loop body.
Actions run once in last-in, first-out order on normal fallthrough and early
`return`. A loop-body action therefore runs at the end of each reached
iteration, rather than accumulating until the enclosing function returns.

A deferred call evaluates its call target and arguments immediately, exactly
once and left to right, then stores the resulting values and delays only the
invocation:

```el
defer close(open_temporary())  # open_temporary runs at registration
```

The deferred function must return `unit`. A fallible cleanup operation therefore
uses a block and handles its tagged result explicitly.

A deferred block delays its body expressions until exit. At registration it
captures by value the current values of every referenced outer binding:

```el
defer do
  release(make_handle())       # make_handle runs at scope exit
end
```

Captured bindings are immutable snapshots inside the deferred action, even when
the original binding was mutable. The action cannot assign to a captured
binding, but it may declare and update its own local mutable bindings. This
compiler-generated capture environment is not a first-class closure and does
not add anonymous functions to v1.

On fallthrough, a block first evaluates and saves its final result, then runs its
deferred actions, and finally yields the saved result. The saved result and all
captured managed values remain GC roots during cleanup. `return` routes through
the same cleanup sequence for every exited lexical scope.

A deferred action must evaluate to `unit` and may contain neither `return` nor
another `defer`. If an action encounters an unrecoverable failure, the process
terminates immediately and no remaining actions run.

Deferred actions do not run after an unrecoverable runtime failure. They are
also not guaranteed after an implementation abort or external termination.
Garbage collection remains responsible only for memory and is never the
semantic mechanism for releasing a file, stream, socket, or other non-memory
resource.

### 10.3 V2 resource ergonomics

V2 should evaluate a structured resource scope such as
`using resource <- acquire() do ... end`. The construct could handle acquisition
failure and insert cleanup automatically while lowering to the same mechanism as
`defer`.

Before adopting it, the design must resolve resource escape and aliasing,
borrowed child handles such as an HTTP response body, nested cleanup ordering,
fallible finalization, and interaction with `Reader` and `Writer`. V1's explicit
`defer` remains the baseline primitive and is not invalidated by later syntax.

## 11. Compiler architecture

The representation boundaries and verifier invariants are specified in
[IR.md](IR.md). This section records the architectural rationale and lowering
overview.

The unified compiler and package tool is named `el`.

```text
.ell source
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

### 11.1 Bootstrap implementation

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

### 11.2 Compiler representations

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

### 11.3 Initial lowering strategy

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

## 12. Grammar overview

[GRAMMAR.md](GRAMMAR.md) is the normative v1 lexical and concrete syntax
specification. It defines source encoding, identifiers, keywords, literals,
significant newlines, declarations, types, blocks, expressions, patterns,
bitstrings, precedence, associativity, and the boundary between grammar
validation and static checking.

The checked-in `pest` grammar implements that specification and is not an
independent authority. Parser recovery may accept incomplete input only to
produce diagnostics; no recovery node may reach name resolution or make an
otherwise rejected program conforming.

Syntax examples in this document are explanatory. Any intentional change to
the accepted language updates `GRAMMAR.md` and records a decision before the
parser, formatter, examples, or conformance tests depend on it.

## 13. Diagnostics

Diagnostics are a language feature, not polish to add at the end.

Minimum diagnostic structure:

```text
error[E0301]: cannot update immutable binding `x`
  --> example.ell:4:3
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

## 14. Normative v1 CLI contract

The project/package executable is named `el`. The following invocations are its
complete normative v1 command surface:

```text
el --help
el --version
el check
el check --locked
el build
el build --release
el build --locked
el build --release --locked
el emit llvm-ir --module Main
```

The standalone compiler is named `elc` and accepts:

```text
elc --help
elc --version
elc [--release] [-o executable] source.ell
```

`--output` is the long spelling of `-o`. `elc` compiles exactly one source
module without manifest discovery, dependencies, lockfiles, or project build
metadata. The module must provide `Main.main() -> i32`. Without `-o`, the
executable uses the source stem plus any host executable suffix and is written
in the current directory. An explicit output path is interpreted relative to
the current directory.

The project commands `check`, `build`, and `emit` use `el.toml` in the current
directory when present; otherwise they walk toward the filesystem root and use
the nearest ancestor containing one. Failure to find a manifest is a project
error. `--help` and `--version` do not require a project. `check` stops after
resolving dependencies and type-checking every source declaration in the
current package. `build` performs the same checks, then emits reachable code for
the package's single executable target. A library-only package uses `check`;
`build` reports that no executable target exists.

Development builds include debug information and use low optimization.
`--release` enables optimization without changing language semantics. V1 builds
only for the compiler host; cross-compilation is deferred. Executables are
written relative to the discovered manifest root beneath
`build/<target-triple>/debug/` or `build/<target-triple>/release/`. The filename
is `package.name` plus the host's required executable suffix: no suffix on
Unix-like targets and `.exe` on Windows.

`--release` and `--locked` may appear in either order after `build`; each may
appear at most once. `--locked` applies the lockfile rules in section 9.2.
`check` accepts only `--locked`; it has no release profile because it emits no
machine code. `emit llvm-ir` requires exactly one `--module` followed by a
package-relative module name, performs the same resolution and checking as
`check`, and writes that module's textual LLVM IR to standard output. The LLVM
text and symbol names are diagnostic output and are not a stable language API.

`--help` prints the respective executable's usage to standard output.
`--version` prints exactly the executable name, one space, the compiler
distribution's semantic version, and one newline to standard output. A
successful invocation exits with status 0. A
reported source, manifest, dependency, lockfile, code-generation, or linker
failure exits with status 1. An unknown command or option, a duplicate option,
a missing option value, or an otherwise malformed invocation prints usage to
standard error and exits with status 2. Diagnostics go to standard error;
`check` and `build` need not print anything on standard output when successful.
Internal compiler defects are outside this CLI exit-status contract.

Options are executable- and command-local: v1 has no global project-directory,
color, verbosity, or target flag, and `el` has no output-path flag. Command
names and options are case-sensitive. V1 has no `el run` or `el test`; users
execute the built native program directly and pass process arguments to that
executable.

## 15. Implementation roadmap

Each milestone ends with working tests and a runnable example. Avoid building
all syntax before any program can run.

### Milestone 0: project skeleton

- Rust workspace with the `el` compiler/package-tool binary and runtime crate.
- One command-line entry point.
- Minimal `el.toml` discovery and parsing.
- Unit-test and snapshot-test conventions.
- Pin the Rust toolchain, LLVM version, LLVM binding, and Boehm GC version.

Exit test: `el --help` runs and CI can build the workspace.

### Milestone 1: parser and AST

- Implement [GRAMMAR.md](GRAMMAR.md) as a checked-in PEG grammar,
  starting with `defmodule`, `def`/`defp`, bindings, literals, types, and
  arithmetic.
- Parse the accepted scalar escapes and bases plus tuple, list, fixed-array,
  map, struct, and read-index syntax.
- Parse generic named declarations, `when` constraints, type applications, and
  `expression :: Type` ascriptions.
- Parse structural union types and typed binding patterns.
- Enforce newline separation and incomplete-line continuation; diagnose
  semicolons and leading-operator continuation.
- Source spans on all AST nodes.
- AST pretty/debug output for tests.
- Useful syntax errors for common mistakes.

Exit test: parse a typed `main` function and snapshot its AST.

### Milestone 2: names and types

- Implement the static contracts and checking order in [TYPES.md](TYPES.md).
- Lexical scopes and unique symbol IDs.
- Primitive types, function signatures, immutable bindings, and mutable locals.
- Reject every transparent-alias cycle and every named-type containment cycle
  not guarded by a built-in managed-indirection constructor.
- Type-check arithmetic, calls, returns, and `:=`.
- Infer implicit function type parameters, propagate expected types into calls,
  and type-check unconstrained generic bodies once.
- Normalize structural unions, prove member disjointness for all generic
  substitutions with occurs-checked unification, emit overlap witnesses, and
  insert injections only from expected union types.
- Produce a Typed AST and lower it to the initial verified Generic Core IR
  specified by [IR.md](IR.md).

Exit test: accepted and rejected programs cover binding, mutation, calls, and
return types, including generic inference and ambiguous empty values, without
invoking LLVM.

### Milestone 3: first native executable

- Monomorphize verified Generic Core IR and lower verified Concrete Core IR for
  `i32`, `i64`, arithmetic, calls, and returns to LLVM IR.
- Monomorphize reachable unconstrained generic functions and concrete generic
  type layouts before LLVM lowering.
- Emit an object file and invoke the host linker.
- Record the LLVM target triple and pointer width in build diagnostics and
  reproducibility metadata.
- Implement `Main.main() -> i32` as the entry point.

Exit test: compile and run a program whose exit status is computed by EL code.

### Milestone 4: core control and matching

- `bool`, comparisons, `if`, `while`, early `return`, and short-circuit logic.
- Tuples, atoms, closed structural unions, typed member patterns, and exhaustive
  `match`.
- Lower monomorphized union discriminants, payloads, injections, and matches.
- Pipeline desugaring.
- Block-scoped `defer` with LIFO cleanup on normal exits.
- LLVM verifier runs on generated modules in tests.

Exit test: compile and run iterative factorial, a tagged-result parser, and an
exhaustive `i64 | string` match.

### Milestone 5: Boehm GC integration

- Define the internal runtime ABI.
- Pin and vendor Boehm GC, then build and statically link it for each supported
  host target.
- Route traceable and pointer-free allocations through private runtime wrappers.
- Verify live base references held in locals, arguments, returns, globals,
  unions, views, deferred captures, nested calls, recursion, and interior object
  graphs in debug and optimized builds.
- Provide a GC stress mode that attempts collection at every managed allocation.
- Make allocation exhaustion an unrecoverable runtime failure.

Exit test: a native optimized EL program retains a reachable heap graph while
temporary allocations are reclaimed under GC stress mode.

### Milestone 6: data types and text

- `defstruct`, construction, field access, and layout.
- `string`, `rune`, `bytes`, byte-aligned `bits`, and `Buffer`.
- Bundle Unicode 17.0.0 data and implement untailored UAX #29 revision 47
  extended-grapheme segmentation.
- Lists, maps, arrays, slices, and function values.
- Implement canonical composite construction, immutable map operations,
  array/slice/bytes read indexing, and function-based slice construction.
- Implement the `Enum` traversal API and the minimal list, array, slice, and
  byte-specific operations from section 6.4.1.
- Implement immutable slices as managed backing-storage views.
- Expand numeric primitives and explicit conversions.

Exit test: process valid UTF-8, reject invalid UTF-8, retain composite heap
graphs through GC, and diagnose integer indexing on `string`.

### Milestone 7: protocols and iteration

- `defprotocol`, `defimpl`, `Self`, and required associated types.
- `when a: Protocol` constraints with statically resolved, monomorphized calls.
- `Eq`, `Ord`, `Show`, `Hash`, `Iterable`, and `Concat`.
- `@derive` for `Eq`, `Ord`, `Show`, and `Hash` where field constraints hold.
- `for pattern in iterable` lowering through `Iterable`.
- `==`, ordering operators, and `++` protocol lowering.

Exit test: derive protocols for a generic struct, instantiate constrained
generic functions at several concrete types, iterate several concrete container
types, and concatenate all standard `Concat` types.

### Milestone 8: packages, I/O, and standard library

- Package ID, root namespace, module discovery, the optional single executable
  `[target]`, and EL dependency entries from `el.toml`'s `[deps]` table.
- Validate strict `src/` path-to-module mapping and namespace qualification.
- Resolve exact path and commit-pinned Git dependency graphs, validate conflicts,
  reject cycles, and read and write `el.lock` with `--locked` support.
- `Reader` and `Writer` with tagged result values.
- Standard modules for strings, collections, buffers, bits, ergonomic console
  output, recoverable console streams, and typed byte-oriented file handles.
- Implement strict process-argument/environment decoding and the native path
  conversion rules from section 6.8 on every supported target.
- Use explicit `defer` for deterministic cleanup of standard I/O resources.

Exit test: build a multi-module manifest target that reads, transforms, and
writes data while handling every recoverable error through `match`.

### Milestone 9: v1 stabilization

- Conformance suite, reference examples, and language reference.
- Stabilize diagnostic presentation and verify CLI conformance.
- Verify the normative grammar and CLI contract, then freeze the
  manifest format and internal runtime ABI version.
- Document supported targets and binary distribution requirements.

Exit test: all v1 examples and negative conformance programs behave identically
on every supported target, with GC stress mode enabled.

## 16. Testing strategy

- **Grammar tests:** accepted/rejected syntax and precedence.
- **Literal tests:** numeric bases and separators, escapes, Unicode validity,
  atom spelling, and rejected deferred literal forms.
- **Composite tests:** construction and inference for tuples, lists, arrays,
  maps, and slices; map constraints; arrays, slices, bytes, and bits indexing;
  bit-order semantics; and bounds failures.
- **Fixed-array tests:** inferred literal lengths, literal-length annotations,
  generic item types at fixed lengths, empty-array expected types, distinct
  length types, rejection of symbolic lengths and length arithmetic, intrinsic
  `Array.length`/`Slice.from_array`, and generated concrete protocol
  implementations.
- **Bitstring tests:** v1 modifier acceptance and rejection, every integer width
  and byte order, signed and unsigned range checks, exact sized-byte
  construction, dynamic pattern sizes, final remainder captures, empty values,
  normal pattern failure, `bitstring_size_mismatch`, arbitrary `Bits.slice`
  results, conversion alignment, and post-v1 form rejection.
- **Type-layout tests:** direct, mutual, generic, and cross-module inline cycles;
  acyclic aliases; rejected alias cycles; and accepted managed recursion.
- **Target-model tests:** exact scalar widths, pointer-sized integers, array and
  slice contiguity semantics, native bitstring byte order, private aggregate
  layout, and recorded target metadata.
- **Numeric tests:** checked and wrapping arithmetic, division and remainder edge
  cases, shifts, conversions, IEEE comparisons, NaN behavior, and parity between
  debug and optimized builds.
- **Evaluation-order tests:** calls and arguments, operators, composite literals,
  pipelines, indexing, assignment, short-circuiting, and duplicate map keys.
- **Defer tests:** call-argument versus block-body timing, immutable snapshots,
  LIFO ordering, branch and per-iteration scope, saved block results, early
  return, GC roots, prohibited nested control flow, and skipped cleanup after an
  unrecoverable failure.
- **Runtime-failure tests:** every stable failure category, originating source
  location, nonzero termination, best-effort standard-error diagnostics,
  identical debug/release classification, and absence of unwinding or stack
  traces.
- **AST snapshots:** stable structure and source spans.
- **Typed AST/Core IR snapshots:** resolved names, types, and desugaring.
- **Semantic tests:** name and type errors with diagnostic snapshots.
- **Resolution tests:** lexical/current-module/prelude lookup, package-relative
  and dependency-qualified module names, the exact reserved prelude, ambiguity,
  visibility, and package/dependency namespace collisions.
- **Pattern tests:** binding scope, duplicate bindings, arm ordering,
  unreachable arms, total exhaustiveness, and irrefutable `for` bindings.
- **Generic tests:** inference, expected-type propagation, constraints,
  recursive calls, specialization reuse, and ambiguous-instantiation errors.
- **Function-value tests:** bare and qualified references, exact function types,
  local shadowing, private-function escape from its owning module, generic
  specialization from surrounding inference, ambiguous references, indirect
  calls, and rejection of protocol-operation values, closures, and partial
  application.
- **Union tests:** canonical normalization, disjointness, expected-type
  injection, typed patterns, exhaustive matching, and concrete layouts. Cover
  invariant concrete applications, generic overlap witnesses, occurs checks,
  associated-type projections, and post-substitution invariant checks.
- **IR tests:** verify LLVM modules; inspect small targeted IR fragments only.
- **End-to-end tests:** compile, link, execute, and check output/exit status.
- **String API tests:** eager byte/codepoint/grapheme results, lazy view item
  types and retention, UTF-8 conversion errors, byte-alignment conversion, and
  permitted immutable storage sharing. Run the Unicode 17.0.0
  `GraphemeBreakTest.txt` conformance data against `length`, `graphemes`, and
  `grapheme_view`; verify identical results across hosts and locale settings.
- **Buffer/I/O tests:** value snapshots, UTF-8 validation, console newline and
  failure behavior, `Show`-constrained console dispatch, direct string output,
  complete writes, EOF, typed file modes, shared handle state,
  close-through-alias behavior, tagged closed-handle errors, portable error-kind
  mappings, operation reporting, optional system codes, interrupted-call retry,
  and error `Eq`/`Hash` laws.
- **UTF-8 error tests:** first-invalid byte offsets including incomplete suffixes,
  parity between `String.from_bytes` and `Buffer.to_string`, and documented
  `Eq`/`Hash`/`Show` behavior.
- **Process-boundary tests:** argument order and executable-name exclusion,
  environment lookup and launch-time snapshots, invalid UTF-8/UTF-16 rejection,
  invalid environment names, exact Unix UTF-8 paths, exact Windows UTF-16
  transcoding, embedded-NUL rejection, and no locale-dependent conversion.
- **GC stress tests:** collection at every managed allocation and heap graph
  survival in debug and optimized builds. Cover locals, registers, stack slots,
  arguments, returns, globals, unions, managed views retaining base pointers,
  deferred captures, saved results, nested calls, recursion, scanned object
  graphs, and pointer-free payloads.
- **Protocol tests:** resolution, coherence, derive constraints, and dispatch.
  Cover explicit associated-type declarations and assignments, qualified
  projections, orphan rejection, overlapping generic implementations, standard
  `Eq`/`Ord`/`Hash` laws, structural derivation, and deliberately unlawful user
  implementations without memory unsafety.
- **Collection-order tests:** verify every standard `Iterable` order, cursor
  immutability, map literal duplicate replacement, update-position stability,
  remove-and-reinsert behavior, insertion-order-independent map equality, and
  iteration stability across runtime hash seeds.
- **Enum/collection API tests:** explicit declaration constraints, generic calls
  without call-site constraints, deterministic traversal, zero-based optional
  positional lookup, list result types, strict left reduction, short-circuiting,
  map tuple order, structural-size complexity, byte conversions, shared
  immutable byte slices, and bounds failures.
- **Manifest tests:** package ID, namespace, strict keys, optional single target,
  library-only packages, dependency cycles, and target validation.
- **Dependency tests:** exact-version validation, lockfile stability, transitive
  conflicts, path resolution, commit-pinned Git metadata, and `--locked`.
- **CLI tests:** every accepted invocation and option order, help/version streams,
  exit statuses 0/1/2, diagnostic streams, host debug/release output paths,
  module-specific LLVM IR emission, missing executable diagnostics, duplicate
  and unknown option rejection, consistent semantics across profiles, and
  absence of run/multi-target/test modes.
- **Negative tests:** invalid programs must fail without compiler crashes.
- **Differential tests:** where semantics are simple, compare interpreted test
  evaluation in the compiler with compiled execution (optional later aid).

Every bug in parsing, typing, code generation, or GC should gain a regression
test at the narrowest useful level.

## 17. Definition of v1

Version 1 is ready when:

- [GRAMMAR.md](GRAMMAR.md), [TYPES.md](TYPES.md), the normative CLI, and
  observable semantics are documented and covered by conformance tests;
- `el.toml` projects reliably compile to native host executables;
- all supported language constructs are statically type-checked;
- generic functions and named types infer, constrain, and monomorphize exactly
  as specified;
- immutable and mutable bindings behave exactly as specified;
- lower-case primitives, composites, closed structural unions, tagged tuples,
  typed member patterns, and exhaustive `match` work;
- `defmodule`, `def`/`defp`, `defstruct`, `@type`, and one-module-per-file rules
  are enforced;
- protocols, explicit implementations, deriving, `for ... in`, `++`, and `|>`
  work as specified;
- `Enum` traversal, optional positional lookup, and the fixed collection-specific
  size, slice, reversal, and conversion APIs preserve their documented order
  and complexity;
- fixed arrays infer literal lengths locally, use literal lengths in user-written
  contracts, and route arbitrary-length algorithms through slices or iteration;
- named monomorphic function values resolve, specialize, pass, return, and call
  without closure environments;
- `string`, `rune`, `bytes`, byte-aligned bit patterns, `bits`, and `Buffer` pass
  their validity and bounds tests, including Unicode 17.0.0 UAX #29 grapheme
  conformance;
- GC-managed programs preserve every live base reference and survive
  collection-at-every-allocation stress testing in debug and optimized builds;
- compiler failures produce source-based diagnostics rather than panics;
- `Reader` and `Writer` use tagged values and the standard resource pattern is
  deterministic;
- ordinary failures use tagged values rather than exceptions;
- recoverable I/O and UTF-8 errors expose stable inspectable fields without
  requiring diagnostic-string parsing;
- process arguments, environment lookup, and native file paths obey the strict
  text-conversion and snapshot rules on every supported target;
- unrecoverable checks use stable categories, terminate nonzero, and never
  unwind pending deferred cleanup;
- no `null`, concurrency feature, or user-facing FFI leaks into the language;
  and
- the examples and conformance suite run on every supported platform.

## 18. Open questions

These require explicit decisions before the affected implementation begins:

1. Resolved by D-035 and superseded in part by D-066: the language name is EL
   and source files use `.ell`.
2. Resolved by D-007: use `pest` 2.8.7 with `pest_derive` 2.8.7.
3. Resolved by D-008: use LLVM 22.1.8 through Inkwell 0.9.0.
4. Resolved by D-024: structs are immutable value types with no observable
   identity; physical storage is a compiler choice.
5. Resolved by D-025: integer arithmetic is checked in all builds, with explicit
   opt-in wrapping operations.
6. Resolved by D-026: slices are immutable managed views that retain their
   backing storage and may safely outlive the source binding.
7. Resolved by D-027: v1 uses explicit block-scoped `defer`; structured
   automatic resource scopes are deferred to v2 design work.
8. Resolved by D-028: use immutable `Iterable` cursors, byte-oriented
   `Reader`/`Writer` operations with associated error types, and seeded
   state-threading `Hash`.
9. Resolved by D-029: generic functions are monomorphized, protocol constraints
   dispatch statically at each concrete instantiation, and protocol-typed
   runtime values are excluded from v1.
10. Resolved by D-030: EL has no semicolons; newlines separate complete
    constructs and continue only after syntactically incomplete input.
11. Resolved by D-031: functions normally return their final expression and may
    use `return expression` for an early exit after running deferred cleanup.
12. Resolved by D-032: lowercase `snake_case` paths under `src/` map
    mechanically to package-relative PascalCase module paths, which are prefixed
    by the manifest namespace.
13. Resolved by D-033: v1 supports exact-version path dependencies and
    commit-pinned Git dependencies, with one version per package and no registry
    or compatibility solver.
14. Resolved by D-034: post-v1 bitstrings use Elixir-style hyphen-separated
    segment modifiers with `size`, `unit`, sign, endianness, and EL's canonical
    `bytes` and `bits` kinds.
15. Resolved by D-038: v1 uses qualification without imports or aliases, a fixed
    core prelude, package-relative own-module names, dependency root namespaces,
    and one function declaration per name regardless of arity.
16. Resolved by D-039: v1 defines a closed compositional pattern set, requires
    every match to be exhaustive, and permits only irrefutable `for` patterns.
17. Resolved by D-040: protocols declare associated types explicitly,
    implementations assign them explicitly, qualified projections expose them
    to generic code, and orphan plus non-overlap rules ensure coherence.
18. Resolved by D-041: v1 fixes scalar literal forms, gives tuples, lists,
    arrays, maps, and structs distinct canonical construction syntax and
    constructs slices through explicit functions. D-057 later extends read
    indexing from arrays, slices, and bytes to `bits`.
19. Resolved by D-042: evaluation is left-to-right and exactly once, integer
    arithmetic and conversions have checked semantics with explicit wrapping
    APIs, v1 includes bitwise operators, and floats preserve IEEE behavior
    without implementing `Eq`, `Ord`, or `Hash`.
20. Resolved by D-043: transparent aliases are acyclic, and recursive nominal
    data is accepted only when every containment cycle crosses a built-in
    managed-indirection constructor.
21. Resolved by D-044: union alternatives are disjoint exactly when no permitted
    finite substitution can make their normalized types equal; the compiler
    decides this with occurs-checked first-order unification.
22. Resolved by D-045: deferred calls evaluate their targets and arguments at
    registration, deferred blocks capture immutable value snapshots, and both
    run in lexical LIFO cleanup after preserving the block result.
23. Resolved by D-046: v1 guarantees exact scalar representations and contiguous
    array/slice semantics but keeps aggregate layout, calling conventions, and
    object-file compatibility private to a matching compiler/runtime.
24. Resolved by D-047: the unified tool is `el`; every package is source-
    importable and may declare one optional executable `[target]`; multi-target,
    cross-compilation, and language-integrated testing are deferred.
25. Resolved by D-048: the core prelude is closed and reserved, string plural
    inspection functions return standard eager collections, and explicitly
    named codepoint/grapheme views provide lazy traversal.
26. Resolved by D-049: `Buffer` uses explicit byte/string append operations,
    console conveniences fail unrecoverably, recoverable I/O uses protocol
    handles, and opaque resources have documented external identity and alias
    behavior. D-051 later broadens the console input type.
27. Resolved by D-050: core equality, ordering, hashing, display, and iteration
    laws have fixed standard implementations; maps iterate deterministically in
    insertion order while equality ignores that order.
28. Resolved by D-051: console conveniences accept any value implementing
    `Show`, allowing direct diagnostic output without a general implicit string
    conversion.
29. Resolved by D-052: every EL call is a possible GC collection point, live
    references remain discoverable raw base pointers, allocation scan classes
    are immutable, and allocation exhaustion is unrecoverable.
30. Resolved by D-053: unrecoverable runtime failures terminate nonzero without
    unwinding, report a stable category and source location when possible, and
    expose no v1 panic or stack-trace facility.
31. Resolved by D-054: named functions form exact monomorphic function values,
    generic references specialize through inference, private values may escape
    their owning module, and closures plus protocol-operation values are
    deferred.
32. Resolved by D-055: v1 pins Unicode 17.0.0 and untailored UAX #29 revision
    47 extended-grapheme segmentation, using bundled data independent of the
    host locale and Unicode libraries.
33. Resolved by D-056: recoverable standard errors are opaque immutable values
    with stable kind, operation, code, or UTF-8 offset accessors and documented
    `Eq`, `Hash`, and `Show` behavior.
34. Resolved by D-057: v1 source bitstrings are a byte-aligned `bytes` subset,
    arbitrary `bits` use a minimal library API and direct boolean indexing, and
    non-byte-aligned source segments remain post-v1.
35. Resolved by D-058: generic traversal lives in the `Enum` module with explicit
    declaration constraints, list-producing transformations, deterministic
    order, and a small set of collection-specific size and conversion APIs.
36. Resolved by D-059: fixed-array literals infer their concrete length, user
    type annotations use literal lengths only, and arbitrary-length algorithms
    use slices or iteration rather than const generics.
37. Resolved by D-060: `Process` exposes launch-time arguments and environment
    lookup through strict tagged UTF conversion, while `string` file paths use
    exact target-specific native conversion with documented limitations.
38. Resolved by D-061: [GRAMMAR.md](GRAMMAR.md) is the normative v1 grammar; the
    checked-in PEG grammar and parser recovery must conform to it rather than
    define a competing syntax.
39. Resolved by D-062: section 14 fixes the complete v1 `el` command surface,
    option placement, output destinations, and exit-status classes.
40. Resolved by D-063: the specification is split by explicit authority among
    `DESIGN.md`, `GRAMMAR.md`, `TYPES.md`, `IR.md`, and `EXAMPLES.md`.
41. Resolved by D-064: Generic and Concrete Core IR share a typed control-flow
    graph with SSA values, block parameters, and typed local slots.

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
- Status: accepted
- Decision: Use `pest` 2.8.7 and `pest_derive` 2.8.7 with a separate checked-in
  `.pest` grammar file. Pin the exact versions in the Rust lockfile.
- Reason: Keeping the grammar visible and separate from compiler logic makes the
  language easier to study and review. `pest` also provides byte spans and Pratt
  parsing support while preserving the parse-tree-to-AST boundary.
- Consequence: The first parser slice must validate operator precedence,
  newline handling, missing `end` diagnostics, and recovery at declaration and
  block boundaries. EL diagnostics must not expose `pest` types as their public
  representation.

### D-008 — Inkwell LLVM bindings

- Date: 2026-07-26
- Updated: 2026-07-27
- Status: accepted
- Decision: Use LLVM 22.1.8 through Inkwell 0.9.0 with the
  `llvm22-1-prefer-dynamic` feature. Pin Inkwell exactly in the Rust lockfile and
  distribute the matching LLVM shared library with `el`.
- Reason: Its safer, higher-level API reduces incidental unsafe Rust while we
  learn LLVM construction and verification. Inkwell 0.9.0 supports LLVM 22.1
  target setup, object emission, module verification, and debug information.
- Consequence: Inkwell and `llvm-sys` types remain private to the LLVM backend;
  Core IR does not expose them. Because Inkwell is pre-1.0 and LLVM major
  upgrades may be disruptive, upgrades are explicit design and build changes.
- Revision: The initial 22.1.0 selection was updated to the latest LLVM 22.1
  patch release, 22.1.8, after validating the backend against the installed
  Homebrew distribution. This does not change the Inkwell feature family or
  source-language behavior.
- Validation: The first backend slice must create and verify a module, emit an
  object for the host target, attach basic debug locations, and link a runnable
  executable.

### D-009 — Mutability is initially local rebinding only

- Date: 2026-07-26
- Status: superseded by D-037
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
- Status: superseded by D-029
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
  `[deps]`, and its build configuration. The target shape and tool contract are
  refined by D-047.

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

### D-024 — Structs have immutable value semantics

- Date: 2026-07-26
- Status: accepted
- Decision: `defstruct` declares an immutable value type. Binding, passing,
  returning, and embedding a struct operates on its value, with no source-level
  reference identity or null value. Copies are shallow and fieldwise.
- Reason: One value-semantic struct form preserves predictable behavior, avoids
  mandatory allocation and aliasing rules, and permits inline aggregate and
  array layouts.
- Consequence: The compiler may elide copies, scalarize fields, pass aggregates
  indirectly, share immutable storage, or use managed allocation when none of
  those choices are observable. Directly recursive structs are rejected in v1;
  an explicit general-purpose indirection type may be considered after v1.

### D-025 — Checked integer arithmetic in all builds

- Date: 2026-07-26
- Status: accepted
- Decision: Integer arithmetic is checked for overflow in every build mode.
  Ordinary arithmetic never silently wraps; wrapping behavior requires an
  explicit opt-in wrapping operation.
- Reason: Build optimization must not change program semantics. Checked
  arithmetic makes overflow visible and predictable, while explicit wrapping
  operations preserve access to modular arithmetic when it is intentional.
- Consequence: Compile-time-known overflow is a compile-time error, and overflow
  encountered at runtime is an unrecoverable runtime failure. Debug and
  optimized builds must behave identically. The wrapping operation names and
  exact API remain to be specified with the integer standard library.

### D-026 — Slices retain managed backing storage

- Date: 2026-07-26
- Status: accepted
- Decision: A `Slice(a)` is an immutable, read-only value containing a managed
  backing-storage reference, offset, and length. It keeps its backing storage
  reachable, may outlive the source binding or scope, and supports O(1)
  subslicing by sharing that storage.
- Reason: Managed backing storage fits v1's garbage-collected memory model and
  immutable value semantics without adding lifetime parameters to the type
  system or forcing every slice operation to copy.
- Consequence: Slice access and construction are bounds-checked. The compiler
  may promote or copy inline storage when a slice escapes, provided this is not
  observable. A small slice can retain a large backing allocation, so the
  standard library must provide an explicit copying operation for callers that
  want independent compact storage.

### D-027 — Explicit lexical `defer` for resource cleanup

- Date: 2026-07-26
- Status: accepted
- Decision: V1 uses an explicit `defer` statement for deterministic non-memory
  resource cleanup. Deferred calls and blocks are scoped to the innermost
  lexical block and execute once in LIFO order on every normal exit.
- Reason: `defer` is a small general control-flow mechanism that makes cleanup
  visible without adding resource ownership, borrowing, or lifetime rules to
  v1's type system.
- Consequence: Referenced values are captured when the action is registered. A
  deferred action must return `unit`, so tagged cleanup failures must be handled
  explicitly. Cleanup is not guaranteed after an implementation abort or
  external termination, and GC never substitutes for resource release. D-053
  later specifies that language-defined unrecoverable failures skip cleanup.
- Future: V2 should evaluate a structured `using` scope that automates
  acquisition and cleanup while preserving `defer` as the underlying primitive.

### D-028 — Core iteration, I/O, and hashing method sets

- Date: 2026-07-26
- Status: accepted
- Decision: `Iterable` defines associated `Item` and `Cursor` types with `iter`
  and `next`; `Reader` defines an associated `Error` and bounded byte `read`;
  `Writer` defines an associated `Error`, complete byte `write`, and `flush`;
  and `Hash` feeds a value into and returns an opaque seeded `Hasher` state.
- Reason: These method sets are small enough to implement directly while making
  iteration state explicit, separating EOF from read errors, hiding partial
  writes, and preventing map hashing from becoming a stable or unseeded public
  function.
- Consequence: Higher-level reading and string-writing operations are library
  helpers. `Reader` and `Writer` do not own resource cleanup. `Iterable.next`
  uses the implementation selected by `iter`, and equal values must contribute
  equivalent hash data.

### D-029 — Inferred monomorphized generics in v1

- Date: 2026-07-26
- Status: accepted
- Supersedes: D-011.
- Decision: V1 supports generic functions and named type declarations.
  Primitive scalar types remain lowercase, named type constructors remain
  PascalCase, and v1 retains structs, tuples, lists, maps, arrays, slices, atoms,
  functions, transparent aliases, and the specified string/binary types.
  Lowercase type identifiers in a function signature implicitly declare type
  parameters, `when a: Protocol` adds protocol constraints, and generic named
  declarations list parameters in parentheses after their PascalCase name.
  Named type application uses parentheses, while `[a]` remains the canonical
  shorthand for `List(a)`.
- Inference: Calls do not accept explicit type-argument lists. Type arguments
  are inferred from arguments and expected types supplied by binding
  annotations, return context, or `expression :: Type` ascription. An
  underdetermined instantiation is a compile-time error.
- Implementation: Generic bodies are checked once and reachable concrete uses
  are monomorphized before LLVM lowering. Protocol-constrained calls dispatch
  statically in each concrete specialization. Identical substitutions reuse a
  specialization, and polymorphic recursion is excluded from v1.
- Consequence: V1 has no protocol-typed runtime values, witness tables,
  runtime-reified generics, higher-kinded types, or specialization. The compiler
  and conformance suite must diagnose unsatisfied constraints and inference
  ambiguity with source spans.

### D-030 — Newlines separate statements; no semicolons

- Date: 2026-07-26
- Status: accepted
- Decision: EL does not support semicolons. A newline separates expressions or
  statements after a complete construct and is whitespace only when the current
  construct is syntactically incomplete, including inside open delimiters,
  after commas, and after non-pipeline operators requiring a right operand.
- Reason: One source-level separation rule keeps formatting and PEG parsing
  predictable and prevents compressed multi-statement lines.
- Consequence: An operator at the start of a line does not continue a completed
  previous line. D-071 adds a pipeline-specific exception. There is no explicit
  continuation token, and semicolons produce a syntax diagnostic.

### D-031 — Final expressions with explicit early `return`

- Date: 2026-07-26
- Status: accepted
- Decision: A function normally returns its final expression and may use
  `return expression` to exit early from any nested control-flow construct. The
  expression must have the declared return type; bare `return` is not allowed.
- Reason: Final expressions preserve expression-oriented code, while explicit
  early returns keep guard clauses and ordinary failure paths from requiring
  deeply nested control flow.
- Consequence: A terminating return path is compatible with any local expected
  type. Returning runs all deferred actions for exited scopes in LIFO order
  before reaching the caller. `return` is invalid outside a function and inside
  a deferred cleanup action; unit functions write `return unit`.

### D-032 — Source paths determine module names

- Date: 2026-07-26
- Status: superseded in part by D-066
- Decision: Each `.el` file under `src/` maps to one package-relative module by
  stripping `src/` and the extension, converting every lowercase `snake_case`
  path component mechanically to `PascalCase`, and joining components with
  dots. The file's `defmodule` declaration must exactly match that derived name.
- Reason: Strict path mapping makes module discovery and navigation
  deterministic without parsing all project files and avoids two competing
  sources of module identity.
- Consequence: The manifest namespace prefixes the derived module to form its
  globally qualified name. Target module names remain package-relative.
  Acronyms and `index.el` receive no special treatment; invalid components,
  declaration mismatches, duplicate modules, and canonical-name collisions are
  compile-time errors.

### D-033 — Exact path and commit-pinned Git dependencies

- Date: 2026-07-26
- Status: accepted
- Decision: V1 dependency entries select either a local path or a Git repository
  pinned to a full commit hash and require an exact semantic version matching
  the dependency manifest. Dependency keys equal package IDs.
- Resolution: The complete transitive graph contains only one source, version,
  and Git revision for each package ID. Any disagreement is an error; v1 has no
  version ranges, compatible-version solver, registry source, floating Git
  reference, or simultaneous package versions.
- Locking: `el.lock` records the resolved graph, versions, commits, and source
  metadata and is committed for executable projects. Path dependencies remain
  live inputs whose contents are not frozen by the lockfile.
- Reason: Exact sources provide local development and reproducible remote builds
  without making a registry service and dependency solver prerequisites for the
  first language release.

### D-034 — Elixir-style arbitrary-width bitstring segments after v1

- Date: 2026-07-26
- Status: accepted
- Decision: Post-v1 bitstring construction and patterns use
  `segment::modifier-modifier...`, following Elixir's order-independent segment
  modifier model. EL uses the canonical kinds `integer`, `float`, `bytes`,
  `bits`, `utf8`, `utf16`, and `utf32`, together with `size`, `unit`, integer
  signedness, and byte-order modifiers.
- Semantics: Effective width is `size * unit` bits. Dynamic pattern sizes may
  refer to in-scope values and earlier segment bindings. An unsized `bits` or
  `bytes` pattern must be final, and pattern length or value mismatches fail the
  enclosing match normally.
- Difference from Elixir: EL construction applies checked range semantics and
  does not silently truncate an integer that does not fit its segment. The
  canonical vocabulary is `bytes` and `bits`, matching EL's distinct types.
- Compatibility: V1 accepts only the byte-aligned subset of this syntax; the
  post-v1 feature removes that restriction without introducing a second form.
  D-057 fixes the exact v1 subset and its separate arbitrary-runtime-`bits` API.
- Reference: [Elixir bitstring special form](https://hexdocs.pm/elixir/Kernel.SpecialForms.html#%3C%3C%3E%3E/1).

### D-035 — EL name and `.el` source extension

- Date: 2026-07-26
- Status: superseded in part by D-066
- Decision: The permanent language name is EL and source files use the `.el`
  extension.
- Reason: Retaining the established project name avoids an unrelated naming
  exercise while the language design and implementation are the priority.
- Consequence: EL accepts the existing `.el` association with Emacs Lisp;
  editors and tooling must distinguish projects through syntax and `el.toml`.

### D-036 — Closed disjoint structural unions

- Date: 2026-07-26
- Status: accepted
- Decision: `A | B` is a closed structural union in any type position. Unions
  are order-independent, flatten nested unions, remove duplicate members after
  transparent alias expansion, and require every normalized alternative to be
  provably disjoint for all permitted generic substitutions.
- Generics: Differently tagged generic alternatives such as
  `{:ok, a} | {:error, e}` are valid. An unconstrained `a | b` or `a | :none`
  is invalid because some instantiations overlap; explicit outer tags remove
  the ambiguity.
- Inference: Member injection is implicit only when a parameter, return type,
  binding annotation, or `::` ascription supplies the expected union. EL does
  not synthesize a union solely from mismatched inferred expressions or
  branches.
- Matching: `name: Type` selects one normalized general member, tagged members
  retain their structural patterns, and `match` checks exhaustive coverage.
- Representation: Each monomorphized union has an unobservable discriminant and
  a payload sized and aligned for its concrete members.
- Protocols: Structural unions neither receive automatic protocol
  implementations nor serve as `defimpl` targets in v1. Member-specific
  protocol behavior requires an explicit match.
- Reason: Finite structural unions provide concise heterogeneous values and
  generic tagged results while disjointness and expected-type injection avoid
  ambiguous runtime membership and accidental type widening.

### D-037 — Direct struct-field update rebinds the mutable root

- Date: 2026-07-26
- Status: accepted
- Supersedes: D-009
- Decision: V1 permits `name.field := expression` when `name` is a mutable local
  struct binding. The operation evaluates the right-hand side, creates the
  shallow fieldwise updated struct value, and rebinds `name`. It evaluates to
  `unit`.
- Value semantics: The operation does not create mutable fields, reference
  identity, or shared mutation. Values previously copied from `name` remain
  unchanged. An in-place backend store is permitted only as an unobservable
  optimization.
- Scope: Only one direct field rooted at a mutable local is accepted. Immutable
  locals, parameters, temporaries, call results, nested field paths, and indexed
  targets are rejected.
- Typing: The field must exist on the statically known struct type, and the
  right-hand side must have exactly the declared field type.
- Reason: Direct field-update syntax makes value-semantic structs practical in
  stateful local algorithms while keeping mutation explicit through `:=` and
  avoiding general place, aliasing, or reference-mutation rules.

### D-038 — Qualified v1 module and name resolution

- Date: 2026-07-26
- Status: accepted
- Decision: V1 has no imports, opened modules, user-defined preludes, or module
  aliases. Bare names resolve through lexical scope, the current module, and a
  fixed core prelude. Other modules use qualified names; own-package modules use
  package-relative names, while dependency modules begin with their declared
  root namespace.
- Visibility: `defstruct`, `@type`, `defprotocol`, struct fields, and protocol
  implementations are public. Functions alone distinguish public `def` from
  module-private `defp`.
- Uniqueness: Ambiguous module references and duplicate package root namespaces
  are errors. A module may declare only one function with a given name,
  regardless of arity.
- Reason: Qualification and deterministic lookup avoid an import-precedence
  system in v1 and keep named function values unambiguous. Aliases can be added
  later without changing existing source meaning.

### D-039 — Exhaustive matching and irrefutable iteration patterns

- Date: 2026-07-26
- Status: accepted
- Decision: V1 patterns consist of wildcards, immutable bindings, literals,
  tuples, lists, structs, byte-aligned bitstrings, and typed structural-union
  member bindings. Patterns compose recursively, bind only within their arm,
  and may not bind one name more than once.
- Matching: Arms are tried top to bottom, provably unreachable arms are errors,
  and every `match` must be exhaustive. V1 excludes guards, alternative
  patterns, pinning, and map patterns.
- Iteration: A `for` binding must be irrefutable for its iterable's `Item` type.
  Refutable processing uses an exhaustive `match` in the loop body rather than
  skipping items or failing at runtime.
- Reason: Total matching prevents hidden runtime failures, while irrefutable
  loop bindings give every iterated value one predictable execution path.

### D-040 — Explicit associated types and coherent protocol implementations

- Date: 2026-07-26
- Status: accepted
- Associated types: A protocol declares every associated type with `type Name`,
  and each implementation assigns every declaration exactly once with
  `type Name = ConcreteType`. Associated types are not inferred, have no
  defaults or parameters in v1, and bare `type` is a reserved keyword.
- Projection: Generic code refers to an associated type with a qualified form
  such as `Iterable.Item(a)`. The projection stays abstract while the generic
  body is checked and resolves statically during concrete monomorphization.
- Coherence: A `defimpl` is legal only in the package owning the protocol or the
  target type. Exactly one implementation may exist for each protocol/type pair,
  generic implementation heads may not overlap, and constraints do not provide
  specialization. Transparent aliases and structural unions are not targets.
- Completeness: Implementation methods must exactly match the protocol after
  substituting `Self` and associated types. Missing, duplicate, or undeclared
  associated-type assignments are errors, as is a derived implementation that
  conflicts with an explicit implementation.
- Exclusions: V1 has no default protocol methods, protocol inheritance, negative
  implementations, specialization, runtime protocol values, or implicit
  conformance.
- Reason: Explicit declarations keep protocol contracts readable and prevent an
  unresolved or misspelled type name from silently becoming a new associated
  type. The ownership and overlap rules make static dispatch deterministic
  across dependency graphs.

### D-041 — Canonical literals and composite construction

- Date: 2026-07-26
- Status: accepted
- Scalars: Integers support decimal, binary, octal, and hexadecimal digits with
  restricted underscore separators and no suffixes. Floats use decimal points
  or exponents. Strings are double-quoted UTF-8, runes are single-quoted Unicode
  scalar values, atoms are colon-prefixed ASCII `snake_case`, and `unit` is the
  sole unit value. V1 excludes raw and multiline strings, interpolation format specifiers,
  hexadecimal floats, and literal NaN or infinity.
- Composites: Tuples use `{...}` and contain at least two elements, lists use
  `[...]` and `[head | tail]`, fixed arrays use `#[...]`, maps use
  `%{key => value}`, and structs retain `%Type{field: value}`. Empty list and map
  literals require enough expected-type information for inference.
- Collections: Immutable map key operations require `Eq` and `Hash` and expose
  absence through `Option`. Arrays, slices, and bytes support read indexing by
  `usize`; bounds failure is unrecoverable and indexed update is deferred. D-057
  later extends read indexing to `bits`, returning `bool`.
- Slices: `Slice.from_array`, `Slice.subslice`, and `Slice.copy` construct,
  share, and explicitly copy managed backing storage. V1 has no slice literal or
  range expression.
- Reason: Distinct literal forms avoid context-dependent collection meaning,
  while explicit absence, bounds behavior, and slice sharing preserve the
  language's predictable value semantics.

### D-042 — Deterministic evaluation and numeric semantics

- Date: 2026-07-26
- Status: accepted
- Evaluation: Expressions evaluate eagerly, exactly once, and left to right,
  including calls, operators, composite literals, struct initializers,
  pipelines, indexing, and assignment right-hand sides. Boolean operators
  short-circuit. Map entries insert left to right, with a later equal key
  replacing the earlier value after all expressions are evaluated.
- Integers: Ordinary arithmetic, division edge cases, left shifts, shift counts,
  and conversions are checked in every build. Signed division truncates toward
  zero and remainder follows the dividend. Per-type PascalCase modules expose
  explicit wrapping arithmetic and shifts.
- Bit operations: V1 includes `~`, `&`, `|`, `^`, `<<`, and `>>`. Binary bitwise
  operands match types, shift counts are `usize`, signed right shift is
  arithmetic, and unsigned right shift is logical.
- Floats: `f32` and `f64` follow IEEE 754 round-to-nearest, ties-to-even behavior
  without unsafe fast-math transformations. Primitive comparisons follow IEEE
  NaN and signed-zero rules, but floats do not implement `Eq`, `Ord`, or `Hash`
  and cannot be map keys in v1.
- Operators: Precedence is fixed from postfix and unary through arithmetic,
  shifts, bitwise operations, concatenation, comparisons, boolean operations,
  ascription, and pipeline. Arithmetic and bitwise forms associate left, `++`
  associates right, pipelines associate left, and comparisons do not chain.
- Reason: A fixed evaluation order makes effects and failures predictable.
  Checked numeric behavior is stable across build modes, while explicit wrapping
  and bitwise operations retain the control expected from a systems language.

### D-043 — Finite layouts and managed recursive data

- Date: 2026-07-26
- Status: accepted
- Struct recursion: A nominal struct may participate in recursive data only when
  every containment cycle crosses a built-in managed-indirection constructor.
  Direct, mutual, generic, and cross-module cycles through inline storage are
  rejected.
- Classification: Structs, tuples, arrays, structural unions, aliases, and
  user-defined generic value structs are inline. Lists, maps, slices, and
  designated opaque managed standard-library types break containment cycles.
  Zero-length arrays receive no special exemption.
- Aliases: Every direct or indirect transparent-alias cycle is rejected, even
  beneath managed storage. Recursive structural data instead uses a nominal
  struct whose recursion crosses a managed constructor.
- Generics: Constructor cycles are checked without relying on changing type
  arguments to terminate. Layout-affecting associated-type projections are
  checked again after concrete protocol resolution, with diagnostics at the
  declaration and invalid instantiation.
- Reason: One inline-containment graph detects every infinite value layout,
  while acyclic aliases keep expansion and normalization finite. Existing GC-
  managed containers still support practical trees and graphs without adding a
  general-purpose reference type to v1.

### D-044 — Union disjointness by finite non-unifiability

- Date: 2026-07-26
- Status: accepted
- Definition: Two normalized alternatives are disjoint exactly when no
  permitted finite substitution of their type variables can make the types
  equal. Disjointness concerns static types, not identical or overlapping
  physical representations; injection records an unobservable discriminant.
- Concrete types: Unequal primitives, atoms, nominal constructors, invariant
  generic applications, tuples, arrays, and function signatures are disjoint
  according to their recursively compared type structure.
- Algorithm: Expand acyclic aliases, flatten unions, recursively normalize,
  remove exact duplicates, establish a stable member order, and test each pair
  with first-order unification and an occurs check. Positive protocol
  constraints do not prove separation, and unresolved associated-type
  projections are conservatively capable of overlap.
- Diagnostics: A rejected generic union reports a witness substitution where
  possible. Accepted declarations are normalized and checked again after
  concrete substitution as a compiler invariant before layout.
- Clarifies: D-036.
- Reason: Non-unifiability gives `provably disjoint` one implementable meaning,
  accepts useful concrete unions such as `List(i64) | List(string)`, and rejects
  generic alternatives exactly when a finite instantiation can collapse them.

### D-045 — Registration-time values and lexical deferred cleanup

- Date: 2026-07-26
- Status: accepted
- Syntax: `defer` is a reserved keyword and a statement of type `unit`. A
  deferred action belongs to the innermost lexical block and registers only
  when execution reaches it.
- Calls: A deferred call evaluates its target and arguments immediately, once
  and left to right, stores those values, and delays only the invocation. The
  invoked function must return `unit`.
- Blocks: A deferred block captures referenced outer values as immutable
  snapshots at registration and executes its body at scope exit. It cannot
  update captured bindings, use `return`, or register another `defer`, but may
  use ordinary control flow and its own mutable locals. It must yield `unit`.
- Exit: Actions run once in LIFO order on fallthrough and early return. The
  block's final result is evaluated and saved before cleanup and yielded after
  cleanup. Saved results and captured values stay rooted during cleanup. A loop
  body's actions run at the end of each reached iteration.
- Failure: Cleanup is not guaranteed after an unrecoverable failure, abort, or
  external termination. If one action fails unrecoverably, remaining actions are
  likewise not guaranteed to run. D-053 supersedes this uncertainty for
  language-defined unrecoverable failures: they run no pending actions.
- Reason: Separating immediate call-argument evaluation from delayed block-body
  evaluation makes timing explicit, while immutable value captures avoid adding
  general closure or shared-mutation semantics to v1.

### D-046 — Stable scalar model and private aggregate ABI

- Date: 2026-07-26
- Status: accepted
- Stable model: Fixed-width integers have their named widths, signed integers
  are two's complement, floats are IEEE binary32/binary64, and `isize`/`usize`
  match a 32-bit or 64-bit target pointer width. Arrays contain exactly `N`
  source-ordered elements; arrays and slices are semantically contiguous and
  provide O(1) indexing.
- Target dependence: `native` bitstrings use target byte order. Pointer-sized
  integers, OS APIs and errors, exit-status observation, and practical runtime
  limits may also vary by target. Build output records the LLVM target triple
  and pointer width.
- Private layout: V1 does not stabilize aggregate field offsets, padding,
  alignment, discriminants, runtime object headers, function representation,
  symbol mangling, or calling conventions. Unobservable scalarization, sharing,
  copying, inlining, and managed allocation remain compiler choices.
- ABI: Dependencies compile from EL source as one resolved build. Object files
  are not portable across compiler versions, and the versioned compiler/runtime
  ABI is private to a matching distribution.
- Scope: V1 is a systems-oriented native foundation, not yet a platform for
  layout-sensitive FFI, memory-mapped hardware, kernel code, or interoperable
  binary libraries.
- Reason: Exact scalar and sequence guarantees support predictable native code
  without freezing aggregate representations that EL programs cannot observe in
  the absence of FFI, unsafe memory access, or a public binary ABI.

### D-047 — Single executable target and unified `el` tool

- Date: 2026-07-26
- Status: accepted
- Package: Every package is importable from EL source without a separate library
  target or stable compiled artifact. Package IDs are lowercase `snake_case`,
  root namespaces are one `PascalCase` component, unknown manifest keys are
  errors, and dependency cycles are rejected.
- Target: A package may contain one optional `[target]` with a package-relative
  `main` module owned by that package and exposing `main() -> i32`. The output
  name is `package.name`; omission makes the package library-only. Multiple
  executables and other target kinds are deferred.
- Tool: The unified compiler and package command is `el`. `check` analyzes every
  source declaration, `build` emits the single executable, `--release` changes
  optimization but not semantics, `--locked` forbids lockfile changes, and
  `emit llvm-ir` remains diagnostic. V1 targets only the host.
- Output: Debug and release executables are written under target-triple-specific
  build directories. The package name plus the host-required suffix determines
  the executable filename.
- Testing: V1 has no test declaration, discovery rule, special `test/` semantics,
  or `el test`. Projects use executable targets and external scripts; the
  compiler conformance suite is not part of the package format.
- Clarifies: D-018.
- Reason: One optional target eliminates target naming, kinds, and default
  selection. `el` accurately names a tool that resolves, checks, builds, and
  inspects projects rather than only invoking code generation.

### D-048 — Closed prelude and eager string inspection

- Date: 2026-07-26
- Status: accepted
- Prelude: V1 fixes the primitive and core container types, eight core
  protocols, standard root modules, and per-integer modules listed in section
  9.1. It imports no bare functions. Prelude declarations and roots are reserved
  against package declarations and dependency namespaces.
- Option: The prelude supplies the sole `Option(a) = {:some, a} | :none` alias;
  user code does not redeclare it.
- Common string API: `String.bytes` returns `bytes`, `String.codepoints` returns
  `[rune]`, and `String.graphemes` returns `[string]`. The latter two eagerly
  traverse and allocate lists; returned storage remains immutable and may share
  the source string.
- Views: `String.codepoint_view` returns `String.CodepointView`, and
  `String.grapheme_view` returns `String.GraphemeView`. Both implement `Iterable`,
  retain the source string, and traverse lazily.
- Conversions: `String.from_bytes` validates UTF-8, `Rune.to_string` constructs
  one-rune text, `Bytes.to_bits` is lossless, and `Bits.to_bytes` returns
  `Option(bytes)` based on byte alignment. Immutable storage may be shared.
- Reason: A closed prelude makes resolution reproducible. Eager plural functions
  optimize for common developer experience, while explicitly named views retain
  predictable allocation-sensitive traversal without requiring every caller to
  know `Enum.to_list`.

### D-049 — Explicit buffers and opaque resource handles

- Date: 2026-07-26
- Status: accepted
- Buffer: `Buffer` is a value-semantic byte builder with distinct append-byte,
  append-bytes, and append-string operations. Conversion to `bytes` always
  succeeds; conversion to `string` validates UTF-8. Returned values are immutable
  snapshots despite unobservable storage reuse or copy-on-write.
- Console: `IO.print`, `IO.println`, and `IO.report` accept `string`, return
  `unit`, and treat console failure as unrecoverable. Recoverable access uses
  process-owned `IO.Stdin`, `IO.Stdout`, and `IO.Stderr` implementations of
  `Reader` and `Writer`; programs do not close them. This input restriction is
  superseded by D-051.
- Files: `File.open_read` returns `File.Reader`; `File.create` and `File.append`
  return `File.Writer`; and `File.close` accepts their `File.Stream` union. Files
  are byte-oriented, expose tagged `File.Error`, and omit combined read/write,
  seeking, permissions, and text modes in v1.
- Resources: Opaque standard-library resource values may carry external identity
  and mutable OS state. Copies alias one resource, reads advance shared state,
  close invalidates every alias, and closed or repeated-close operations return
  tagged errors. Resource handles implement neither `Eq` nor `Hash`.
- Scope: External resource identity is an explicit standard-library exception;
  it does not give user structs reference identity or shared mutation. Without
  affine ownership, v1 programs remain responsible for alias and close discipline.
- Reason: Explicit Buffer operations avoid overloading, common console output
  stays ergonomic, protocol handles preserve recoverable I/O, and documenting
  unavoidable OS identity prevents the ordinary value model from making false
  promises about resources.

### D-050 — Protocol laws and deterministic collection order

- Date: 2026-07-26
- Status: accepted
- Laws: `Eq` is an equivalence relation, `Ord` is a total order consistent with
  `Eq`, and equal values feed equivalent data into `Hash`. Violating these laws
  is erroneous user behavior but does not permit memory unsafety.
- Numeric scope: Equality and ordering require matching operand types. V1 has
  no cross-numeric comparison, and floats implement none of `Eq`, `Ord`, or
  `Hash` because IEEE NaN behavior violates their laws.
- Standard implementations: Scalars and structurally lawful tuples, sequences,
  and derived structs receive `Eq`, `Ord`, and `Hash` as specified in section
  6.7. Maps implement structural `Eq` independent of insertion order, but not
  `Ord` or `Hash`. Functions, buffers, floats, resources, and structural unions
  do not gain these protocols automatically.
- Text: String ordering compares Unicode scalar values without normalization or
  case folding; hashing uses exact UTF-8 bytes. `Show` is human-readable and is
  not a stable serialization format.
- Iteration: Standard list, array, slice, byte, and string-view traversal has a
  fixed source order. Map iteration is deterministic insertion order; replacing
  a value preserves position, removal deletes it, reinsertion appends it, and
  seeded hashing never affects iteration.
- Reason: Protocol-backed operators and hashed collections need explicit laws
  to remain predictable. Fixing traversal order removes runtime-dependent output
  and test behavior without making maps ordered by key or exposing hash-table
  internals.

### D-051 — Show-constrained console conveniences

- Date: 2026-07-26
- Status: accepted
- Decision: `IO.print`, `IO.println`, and `IO.report` are generic functions over
  values implementing `Show`. They statically call the selected `Show.show`
  implementation and return `unit`; output failure remains unrecoverable. The
  standard `Show` implementation for `string` returns its contents unchanged,
  so existing text output gains no quoting or escaping.
- Errors: Standard `IO.Error` and `File.Error` values implement `Show`, so an
  error branch may write `IO.report(reason)` directly.
- Boundary: This does not introduce general implicit conversion to `string`.
  APIs requiring text still require `string`, and recoverable byte output still
  uses `Writer.write`.
- Supersedes: The string-only console input clause of D-049.
- Reason: Directly printing values and reporting errors removes repetitive
  `Show.show` calls at the most common diagnostic boundary while retaining a
  visible protocol constraint and predictable formatting dispatch.

### D-052 — Conservative-GC lowering contract

- Date: 2026-07-26
- Status: accepted
- Collection points: Every EL function call is treated as potentially
  collecting because its transitive body may allocate. Private runtime calls
  are explicitly classified as allocating or non-allocating; v1 performs no
  allocation-effect inference.
- Roots: At every possible collection point, every live managed reference is
  represented by an unmodified aligned native base pointer in a collector-
  visible register, stack slot, global, or reachable scanned allocation. An
  integer, tagged, compressed, or interior-only representation is insufficient.
- Views and temporaries: Managed views retain their backing allocation's base
  pointer. Union payloads, deferred captures, saved block results, arguments,
  returns, and private runtime temporaries follow the same rooting contract.
- Allocation classes: Objects that may contain references use scanned storage;
  raw byte, bit, and numeric payloads use pointer-free storage and never later
  receive a managed reference. Managed globals use registered scanned storage.
- Failure and verification: Allocation exhaustion is unrecoverable. A stress
  mode attempts collection at every managed allocation, and conformance tests
  exercise the contract in both debug and optimized builds.
- Reason: Boehm can remain a private implementation choice only if lowering has
  a precise, testable discoverability rule. Base-pointer retention also avoids
  depending on optional interior-pointer recognition or optimizer accidents.

### D-053 — Unrecoverable runtime-failure contract

- Date: 2026-07-26
- Status: accepted
- Termination: An unrecoverable runtime failure immediately terminates the
  process with a nonzero status. It is not catchable and does not unwind scopes
  or execute pending `defer` actions; failure within cleanup skips the rest.
- Diagnostic: Source-mandated checks make a best-effort standard-error report
  containing a stable failure category and package-relative source location.
  Exact prose and numeric exit status are implementation-defined, and failure
  to write standard error does not prevent termination.
- Categories: V1 defines `integer_overflow`, `division_by_zero`,
  `invalid_shift`, `invalid_conversion`, `index_out_of_bounds`,
  `allocation_exhausted`, `console_output_failed`, and
  `bitstring_size_mismatch`. Debug and release builds classify the same
  operation identically.
- Scope: V1 has no stack trace, custom panic value, user-callable panic, or
  recovery mechanism. Compiler/runtime invariant failures are implementation
  defects and may use separate internal diagnostics.
- Reason: One failure boundary makes checked operations predictable without
  introducing exception unwinding. Stable categories and source locations make
  failures testable while leaving presentation and platform exit details free
  to evolve.

### D-054 — Monomorphic named function values

- Date: 2026-07-26
- Status: accepted
- Formation and call: A bare or qualified ordinary named function reference
  forms a function value; applying parentheses calls it. Its structural
  function type has exact parameter and result types with no variance or
  implicit coercion.
- Representation: Every v1 function value denotes one concrete monomorphized
  code target and carries no captured environment. Its physical representation
  and calling convention remain private.
- Inference: A generic function reference specializes using its expected
  function type and surrounding inference. Function values are never
  polymorphic, and an unresolved specialization is a compile-time error.
- Names and visibility: A local binding shadows a bare function name.
  Qualification may still select a visible public function. External source
  cannot name `defp`, but its owning module may pass or return that function
  value, which remains callable by its recipient.
- Exclusions: Protocol operations are not function values in v1; an ordinary
  named wrapper is required. Anonymous functions, closures, partial application,
  and bound receiver methods are deferred. Function values implement none of
  `Eq`, `Ord`, `Hash`, or `Show`.
- Reason: Concrete code-pointer values support useful higher-order programming
  while keeping monomorphization, visibility, and the runtime model explicit and
  avoiding closure environments or captured protocol dispatch.

### D-055 — Pinned Unicode and grapheme semantics

- Date: 2026-07-26
- Status: accepted
- Version: EL v1 pins the Unicode Standard and Character Database version
  17.0.0.
- Segmentation: `String.length`, `String.graphemes`, and
  `String.grapheme_view` use the default extended-grapheme-cluster rules from
  UAX #29 revision 47 under conformance clause UAX29-C1-1, with no tailoring.
- Reproducibility: Compiler distributions bundle generated tables from the
  versioned Unicode data. Host locale, OS services, and installed ICU or Unicode
  libraries never affect segmentation; build metadata records the data version.
- Text identity: Segmentation performs no normalization or case folding and
  preserves the source scalar sequence. Unassigned non-surrogate scalar values
  remain valid UTF-8 text.
- Verification: The standard Unicode 17.0.0 `GraphemeBreakTest.txt` data is part
  of the conformance suite for eager, lazy, and length APIs.
- Upgrade rule: Changing Unicode or UAX behavior requires a recorded language
  semantic decision and cannot arrive as an untracked library update.
- Reason: Grapheme boundaries evolve across Unicode versions and may be tailored
  by host libraries. Pinning both data and the untailored algorithm makes text
  behavior portable and reproducible.
- References: [Unicode 17.0.0](https://www.unicode.org/versions/Unicode17.0.0/)
  and [UAX #29 revision 47](https://www.unicode.org/reports/tr29/tr29-47.html).

### D-056 — Inspectable recoverable standard errors

- Date: 2026-07-26
- Status: accepted
- I/O values: `IO.Error` and `File.Error` are opaque immutable values exposing a
  stable `IO.ErrorKind`, the `IO.Operation` that failed, and an optional `i64`
  host error code through module accessors.
- Kinds: V1 fixes `:not_found`, `:permission_denied`, `:already_exists`,
  `:invalid_input`, `:is_directory`, `:not_directory`, `:closed`,
  `:broken_pipe`, `:out_of_space`, and `:other`. Interrupted host operations are
  retried internally. Extending the closed kind or operation union requires a
  recorded language-version decision.
- Portability: Programs branch on the stable kind. System codes and `Show`
  diagnostics are target-dependent and are not portable control-flow inputs.
- UTF-8: `String.Utf8Error` exposes the zero-based byte offset of the first
  invalid sequence, including the start of an incomplete suffix. Both
  `String.from_bytes` and `Buffer.to_string` use this definition.
- Protocols: All three error types implement `Eq`, `Hash`, and `Show`, but not
  `Ord`. I/O error equality and hashing use operation, kind, and optional code;
  UTF-8 error equality and hashing use its offset. Hidden diagnostic text is
  excluded.
- Identity: Error copies have ordinary value semantics and are constructed only
  by the standard library; they do not inherit resource identity.
- Reason: Stable structured inspection supports recovery and testing without
  parsing human-readable or platform-specific messages, while preserving useful
  native diagnostic information.

### D-057 — Byte-aligned source bitstrings and direct bit indexing

- Date: 2026-07-26
- Status: accepted
- Source boundary: V1 `<<...>>` construction produces `bytes`, and bitstring
  patterns consume `bytes`. Source segments support byte-aligned integers from
  8 through 64 bits, fixed or dynamically sized `bytes`, and a final unsized
  `bytes` pattern remainder.
- Integers: Segment sign defaults to unsigned and byte order defaults to big;
  `little` and target-dependent `native` are explicit alternatives. Pattern
  captures use `u64` or `i64`. Construction is range-checked and never truncates
  or pads.
- Size behavior: Sized construction operands must match exactly. A static
  mismatch is a compile-time error and a dynamic mismatch terminates with
  `bitstring_size_mismatch`. Insufficient input, a literal mismatch, or leftover
  pattern input is ordinary pattern failure.
- Runtime bits: `Bits.bit_size`, bounds-checked `Bits.slice`, `Bits.to_bytes`,
  `Bytes.to_bits`, and `Concat` form the minimal arbitrary-length API.
  `Bits.slice` may create non-byte-aligned values.
- Indexing: `bits[index]` takes `usize`, returns `bool`, and fails with
  `index_out_of_bounds`. Index zero is the most-significant bit of the first
  source byte. This supersedes D-041's exclusion of `bits` indexing.
- Deferred forms: Float, UTF, and `bits` source segments, explicit units,
  non-byte-aligned widths, and arbitrary-width integer segments remain post-v1
  under D-034's broader syntax direction.
- Reason: The byte-aligned subset supports practical packet work without
  importing the full arbitrary-width type-checking surface. Direct indexing and
  bounds-checked slicing keep common bit inspection consistent with other
  indexed collections and avoid `Option(bool)` boilerplate.

### D-058 — Enum-centered core collection API

- Date: 2026-07-26
- Status: accepted
- Generic traversal: The reserved `Enum` module defines `count`, `at`,
  `to_list`, `map`, `filter`, `reduce`, `each`, `any`, and `all` for every
  `Iterable`.
  Actual declarations explicitly state `when i: Iterable`; documentation may
  state the shared constraint once and omit its repetition from an API listing.
- Results and order: Every function follows the implementation's deterministic
  iteration order. `to_list`, `map`, and `filter` return lists because v1 has no
  higher-kinded container reconstruction. Map items are insertion-ordered
  `{key, value}` tuples.
- Evaluation: `reduce` is strict and left-to-right, `each` visits every item,
  and `any` plus `all` short-circuit. Function arguments are concrete named
  function values under D-054.
- Positional lookup: `Enum.at(values, index)` uses a zero-based `usize` index,
  follows iteration order, and returns `Option(Iterable.Item(i))`. It returns
  `:none` when the iterable ends before the position and stops immediately
  after finding it; it traverses at most `min(index + 1, length)` items.
- Specific operations: V1 additionally fixes `List.reverse`, `Array.length`,
  `Slice.length`, `Bytes.byte_size`, `Bytes.slice`, `Bytes.from_list`, and
  `Bytes.to_list`. Structural array, slice, and byte sizes are O(1);
  `Enum.count` traverses and is O(n).
- Bounds and storage: `Bytes.slice` uses `index_out_of_bounds` and may share
  immutable storage. List-producing traversal, list reversal, and byte/list
  conversions take O(n) time and produce fresh logical values.
- Empty lists: V1 omits redundant `List.new`; code writes `[]` under an expected
  list type when inference otherwise lacks an item type.
- Scope: Sorting, searching, zipping, chunking, and related conveniences may be
  added as ordinary library evolution without expanding the language semantics.
- Reason: `Enum` gives one protocol-backed traversal vocabulary across all
  containers, while a deliberately small set of collection-specific operations
  preserves O(1) size queries and necessary conversions without duplication.

### D-059 — Literal fixed-array lengths with local inference

- Date: 2026-07-26
- Status: accepted
- Inference: A nonempty `#[...]` literal infers both its homogeneous item type
  and concrete element count. Local code normally omits the annotation, as in
  `coordinates = #[10, 20, 30]`, whose type is `[i64; 3]`.
- Written types: An array type contains a nonnegative integer literal length
  representable as `usize`, such as `[a; 2]`. V1 has no symbolic length
  variables, const generics, length arithmetic, or `[a; _]` inference syntax.
- Empty values: `#[]` has length zero but needs an expected item type, for
  example `empty: [u8; 0] = #[]`.
- Generic boundary: Functions may be generic over the item type at a fixed
  literal length. Algorithms abstracting over length accept `Slice(a)` or an
  `Iterable`; arrays of different lengths remain distinct without coercion.
- Intrinsics: `Array.length` and `Slice.from_array` are compiler-provided
  standard operations instantiated for every concrete array length. The `N` in
  their documentation is schematic and cannot be written as a user const
  parameter.
- Protocols: Standard array implementations are generated per concrete length
  when their item constraints hold.
- Reason: Literal inference keeps local array construction concise, while
  literal-only contracts avoid introducing a second const-parameter system.
  Slices already provide the runtime-length abstraction required by generic
  contiguous algorithms.

### D-060 — Strict process text and native path conversion

- Date: 2026-07-26
- Status: accepted
- Process input: The reserved `Process` module exposes launch-order user
  arguments and launch-time environment lookup. It excludes the executable name,
  snapshots both inputs before `Main.main`, and has no v1 mutation or
  enumeration API.
- Text conversion: Unix argument and environment bytes require strict UTF-8;
  Windows inputs require well-formed UTF-16. Invalid data produces tagged
  results without partial lists, replacement characters, or locale conversion.
- Paths: Existing `File` functions retain `string` paths. Unix passes their
  UTF-8 bytes unchanged; Windows transcodes scalars exactly to UTF-16. Neither
  target normalizes or canonicalizes, and embedded U+0000 is `:invalid_input`.
- Limitation: V1 cannot address invalid-UTF-8 Unix names or Windows names with
  unpaired surrogates. A future native path type may add that reach without
  changing valid `string` path behavior.
- Reason: Strict conversion gives ordinary programs a small portable process
  boundary and makes every lossy or inaccessible native spelling explicit.

### D-061 — `GRAMMAR.md` is the normative v1 grammar

- Date: 2026-07-26
- Status: accepted
- Authority: [GRAMMAR.md](GRAMMAR.md) defines accepted v1 source syntax. The
  compiler's checked-in `pest` file implements that contract and cannot silently
  extend or narrow it.
- Coverage: The grammar fixes declarations, types, blocks, patterns, bitstring
  forms, literals, postfix syntax, and the complete operator precedence and
  associativity ladder, including significant-newline insertion.
- Recovery: Parser recovery may recognize incomplete or erroneous forms only to
  produce diagnostics; recovery nodes never make a program conforming.
- Change rule: Any intentional accepted-language change updates `GRAMMAR.md` and
  records a decision before implementation or examples depend on it.
- Reason: A normative grammar makes parser tests, examples, formatter behavior,
  and independent implementations answer to one reviewable syntax contract.

### D-062 — Normative unified-tool command surface

- Date: 2026-07-26
- Status: accepted
- Commands: V1 accepts `el --help`, `el --version`, `el check [--locked]`,
  `el build [--release] [--locked]`, and
  `el emit llvm-ir --module Module`. Options are command-local,
  case-sensitive, non-repeatable, and restricted to those positions.
- Results: Success is status 0, reported project/build failure is status 1, and
  malformed invocation is status 2. Diagnostics and usage errors use standard
  error; help, version, and emitted LLVM IR use standard output.
- Scope: Build products retain the target/profile paths from section 14.
  Single-file and output-selection modes remain outside the unified `el`
  interface; D-070 defines the separate `elc` interface. Run, test,
  multi-target, and cross-target modes are outside the conforming v1 interface.
  LLVM IR text remains diagnostic and unstable even though its command is
  supported.
- Reason: Fixing commands and observable outcomes lets scripts and conformance
  tests rely on the tool before v1 stabilization without treating every CLI
  implementation choice as a language feature.

### D-063 — Specification ownership is split by subject

- Date: 2026-07-26
- Status: accepted
- Decision: `DESIGN.md` owns vision, observable runtime semantics, architecture,
  roadmap, and accepted decisions. `GRAMMAR.md` is normative for lexical and
  concrete syntax. `TYPES.md` is normative for static semantics and
  well-formedness. `IR.md` defines bootstrap-compiler representation contracts.
  `EXAMPLES.md` is illustrative.
- Precedence: When overlapping prose disagrees, the document with explicit
  authority for that subject controls. An intentional source-language change
  updates its authoritative document and this decision log before implementation
  or examples depend on it.
- Reason: Narrow sources of truth are easier to implement and test than one
  monolithic design document, while keeping rationale and decisions together
  preserves the history behind each contract.

### D-064 — Core IR is a typed control-flow graph

- Date: 2026-07-26
- Status: accepted
- Shape: Generic and Concrete Core IR use the same logical instruction set. A
  function contains typed basic blocks; each block has typed operations followed
  by one terminator. SSA `ValueId`s and block parameters carry ordinary value
  flow, while typed `SlotId`s represent mutable locals and hidden addressable
  compiler storage.
- Stages: Generic Core IR may retain declared type parameters, associated-type
  projections, and constrained protocol calls. Monomorphization produces
  Concrete Core IR with only concrete types, implementations, calls, and layouts
  before LLVM lowering.
- Lowering: Pipelines, protocol-backed operators and iteration, source patterns,
  deriving, field-update syntax, and `defer` registration are absent from Core
  IR. Their behavior is explicit in operations and control-flow edges, including
  LIFO cleanup routing for normal exits.
- Verification: Every IR boundary checks identity ownership, block and value
  typing, slot initialization, stage legality, exact calls, union operations,
  cleanup routing, source origins for failures, and conservative-GC base-pointer
  visibility.
- Flexibility: Rust data structures and textual debug formats are not stable
  interfaces and may evolve while these invariants and observable EL behavior
  remain intact.
- Reason: A small typed CFG makes evaluation order, joins, early return, pattern
  decisions, cleanup, and backend verification explicit without forcing mutable
  source bindings into SSA before the compiler is ready to promote them.

### D-065 — Developer experience is a design value

- Date: 2026-07-28
- Status: accepted
- Decision: EL values developer experience and developer happiness. Among
  designs that preserve correctness, predictability, and simplicity, prefer the
  design that makes common work more readable, helpful, direct, and pleasant.
- Consequence: Syntax and tooling should minimize incidental ceremony, and
  diagnostics should help developers understand and correct problems. This
  principle does not justify adding features with unclear interactions or
  weakening explicit language guarantees.
- Reason: A language can remain small and rigorous while respecting the people
  who use it. Making routine development satisfying is part of EL's quality,
  not merely a post-v1 tooling concern.

### D-066 — `.ell` source extension

- Date: 2026-08-01
- Status: accepted
- Decision: The permanent language name remains EL, and EL source files use the
  `.ell` extension. This supersedes only the source-extension portions of D-032
  and D-035; their module-mapping and language-name decisions remain accepted.
- Reason: `.ell` reads naturally as "EL language" while avoiding `.el`'s
  established association with Emacs Lisp.
- Consequence: Project discovery, module-to-path mapping, examples, diagnostics,
  editor integrations, and compiler tooling must recognize `.ell` as the EL
  source extension. `.el` is not an alternate EL source extension.

### D-067 — `Show`-backed string interpolation

- Date: 2026-08-02
- Status: accepted
- Syntax: A double-quoted string may contain `#{expression}` segments; `\#{`
  inserts the literal marker. Format specifiers and multiline interpolation are
  outside v1.
- Types: Every embedded expression must implement `Show`. Static protocol
  selection is identical to `Show.show(expression)`, and the resulting text is
  inserted without implicit quoting.
- Evaluation: Segments evaluate eagerly, exactly once, and from left to right.
  Formatting yields valid UTF-8 and allocation failure is unrecoverable.
- Reason: Diagnostics and ordinary presentation should not require manual
  `Buffer` construction or repeated concatenation while EL retains explicit,
  statically selected conversion semantics.

### D-068 — Basic standard-library string operations

- Date: 2026-08-02
- Status: accepted
- API: `String.empty(string) -> bool`,
  `String.contains(string, string) -> bool`, and
  `String.split(string, string = "") -> [string]` are part of the v1 `String`
  surface.
- Matching: `contains` and `split` use exact, case-sensitive UTF-8 byte
  matching without normalization, case folding, or locale tailoring. The empty
  pattern is always contained.
- Splitting: Matches are consumed left to right without overlap. Empty fields
  at the beginning, end, and between adjacent separators are retained. An empty
  separator performs no split and returns a one-element list containing the
  source string.
- Transformation: `downcase` applies the pinned Unicode lowercase mapping and
  is independent of the host locale. `replace` consumes exact, non-overlapping
  UTF-8 matches from left to right; an empty pattern is a no-op.
- Frequencies: `Enum.frequencies(iterable) -> Map(item, usize)` traverses once
  in iterable order. Keys must satisfy `Eq` and `Hash`; the resulting map stores
  one entry per distinct item and counts duplicate occurrences.
- Reason: Common validation and parsing should state their intent directly
  without hand-written recursive grapheme traversal. Keeping Unicode-aware
  segmentation under the distinct `graphemes` API makes the matching unit
  explicit.

### D-069 — `with` result propagation

- Date: 2026-08-02
- Status: accepted
- Syntax: `with` contains one or more comma-separated
  `pattern <- expression` clauses followed by `do`, a body, and `end`. There is
  no `else` form in v1.
- Evaluation: Clause expressions evaluate exactly once from left to right. A
  successful pattern contributes bindings to later clauses and the body. The
  first unmatched value is the result, and remaining clauses and the body are
  skipped. If all patterns match, the body result is returned.
- Types: Clause patterns use ordinary pattern checking. Every possible
  unmatched value must equal the result type or be one of its normalized union
  members. The usual expected-union injection applies; without an expected
  type, the body establishes the result type.
- Lowering: `with` is source sugar for nested pattern branches and is absent
  from Core IR.
- Reason: Tagged-result validation and resource pipelines should preserve
  explicit failure values without requiring deeply nested exhaustive matches or
  repeated early returns.

### D-070 — Separate `elc` single-file compiler

- Date: 2026-08-02
- Status: accepted
- Interface: `elc [--release] [-o executable] source.ell` compiles one module
  containing `Main.main() -> i32`; `--output` is the long output-option spelling.
  It also provides its own `--help` and `--version` modes.
- Output: Without an output option, the source stem plus the host executable
  suffix is written in the current directory. Compilation uses a temporary
  object and leaves only the linked executable on success.
- Isolation: `elc` performs no manifest discovery, dependency or lockfile
  processing, or project metadata emission. The manifest-driven workflow stays
  under `el`, and `el build` continues to reject source and output arguments.
- Results: Success, compilation failure, and malformed invocation use statuses
  0, 1, and 2 respectively, following the stream conventions in section 14.
- Reason: Small EL programs should be directly compilable without weakening the
  predictable manifest-driven project interface or overloading `el build`.

### D-071 — Leading operators in multiline pipelines

- Date: 2026-08-02
- Status: accepted
- Decision: A pipeline may remain on one line, but every stage that continues
  on a later line begins with `|>` after optional indentation. A newline after
  `|>` and before its target is rejected, including inside an open delimiter.
- Interaction: This is the sole exception to D-030's rule that a leading
  operator cannot continue a complete expression. It does not allow other
  infix operators to begin a continued line.
- Reason: Leading pipeline operators keep each transformation visually aligned
  with its stage and make pipelines easier to extend and reorder.

## 21. Next design checkpoint

All questions currently listed in section 18 are settled. New design work must
continue through stable decision-log entries before implementation depends on
it. D-060 through D-064 close the remaining process-boundary, grammar, CLI,
specification-ownership, and Core IR gaps; implementation now has explicit
contracts to test.
