# EL Language Design

Status: living design document  
Language name: **EL**
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
- Support inferred, monomorphized generic functions and generic named types.
- Support closed disjoint structural unions with exhaustive matching.
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
- Runtime-reified generics, higher-kinded types, specialization, or
  protocol-typed runtime values.
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

- Source file extension: `.el`.
- Source text is UTF-8.
- Identifiers use ASCII letters, digits, and `_` in the initial implementation.
- Value and function names use `snake_case`.
- Primitive type names are lower case.
- Type variables are lower case; named type constructors, protocols, and modules
  use `PascalCase`.
- `#` begins a line comment.
- Nested block comments are deferred.

### 4.2 Keywords

Initial reserved words:

```text
def defimpl defmodule defp defprotocol defstruct do else end false for if in
match mut return true when while
```

`@derive` and `@type` are built-in attributes and are reserved as complete
attribute names.

Future keywords are not reserved until their feature is accepted.

### 4.3 Newlines and statement separation

EL has no semicolon token. A newline separates expressions or statements when
the preceding tokens form a complete construct at the current delimiter depth.
Multiple statements cannot be placed on one line with a separator.

A newline is treated as whitespace when continuation is unambiguous: inside an
open `(...)`, `[...]`, or `{...}` delimiter, after a comma, or after an operator
that still requires a right operand. No backslash or other explicit line-
continuation token exists. For example:

```el
total = left +
  right

result = input |>
  normalize() |>
  validate()
```

An operator at the beginning of a line does not retroactively continue a
complete expression on the previous line. Blank and comment-only lines do not
produce empty statements. A semicolon receives a syntax diagnostic rather than
being treated as optional punctuation.

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
- Integer arithmetic is checked for overflow in every build mode. Overflow in
  ordinary arithmetic is an unrecoverable runtime failure; compile-time-known
  overflow is a compile-time error. Wrapping is available only through explicit
  opt-in wrapping operations.
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
empty: [i64] = List.new()
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

#### 6.2.1 Structural union types

`A | B` forms a closed structural union in any type position. It does not create
a nominal type. Union equality is order-independent; the compiler expands
transparent aliases, flattens nested unions, removes duplicate members, and
uses a canonical member order. For example, `A | (B | A)` and `B | A` are the
same type.

Every normalized alternative must be provably disjoint from every other
alternative for all permitted generic substitutions. Distinct primitive types,
nominal struct types, atoms, and differently tagged tuple shapes are disjoint.
These generic tagged unions are therefore valid:

```el
@type Result(a, e) = {:ok, a} | {:error, e}
@type Option(a) = {:some, a} | :none
```

Unconstrained alternatives that may overlap are rejected:

```el
@type Either(a, b) = a | b       # invalid: a and b may be the same type
@type Optional(a) = a | :none    # invalid: a may include :none
```

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

All fields must be initialized in v1. Fields are immutable after construction.
There is no implicit zero-value construction. Structs have value semantics: a
binding, argument, return, or aggregate field contains a struct value rather
than an observable reference with identity. A struct copy is shallow and
fieldwise, so immutable reference-backed fields may share storage. The compiler
may keep a struct in registers, place it inline, pass it indirectly, share
immutable storage, or allocate it on the managed heap when those choices cannot
be observed by EL code. Struct values are never null and have no identity
operation. Directly recursive structs are rejected in v1.

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
- arrays: fixed-size homogeneous values, type `[a; N]`;
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
- `return expression` is allowed for early return and must match the function's
  declared return type.
- Bare `return` is not supported; a unit-returning function uses `return unit`.
- Omitting `-> type` means `-> unit`.
- Overloading by parameter types is not supported.
- Named functions can be used as function values.
- Closures and anonymous functions are deferred.
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

`Reader.read` returns at most `max_bytes`. Except when `max_bytes` is zero, an
`{:ok, data}` result contains at least one byte, so callers cannot confuse an
empty successful read with end-of-input. `read_exact`, `read_all`, and similar
operations are standard-library helpers rather than required protocol methods.

`Writer.write` accepts the entire byte sequence or returns an error; callers do
not handle partial successful writes. `flush` may be a no-op for an unbuffered
implementation. A string-writing helper exposes the string's UTF-8 bytes and
calls `write`. Resource release is deliberately absent from both `Reader` and
`Writer` and remains explicit through `defer` and type-specific cleanup APIs.

`Hasher` is an opaque standard-library state initialized with a runtime-selected
seed. `Hash.hash` returns updated state rather than a stable public integer.
Values equal under `Eq` must feed equivalent data into `Hasher`; derived `Hash`
implementations process struct fields in declaration order.

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
- `bytes(s)` returns a read-only, non-allocating view whose items are `u8`; the
  view retains the immutable string storage.
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
type-checking, bounds, exhaustiveness, and lowering rules. The post-v1 syntax
follows Elixir's segment-modifier model, adapted to EL's `bytes` and `bits`
names:

```el
<<version::size(3),
  flags::size(5),
  length::unsigned-big-size(16),
  payload::bits-size(length),
  rest::bits>>
```

A segment is `value_or_pattern::modifier-modifier...`. Modifier order does not
affect semantics, though the formatter emits a canonical order. The supported
modifier categories are:

- kind: `integer`, `float`, `bytes`, `bits`, `utf8`, `utf16`, or `utf32`;
- integer sign: `signed` or `unsigned`, defaulting to `unsigned`;
- byte order where relevant: `big`, `little`, or `native`, defaulting to `big`;
- `size(expression)`; and
- `unit(positive_integer)`.

Effective width is `size * unit` bits. The default unit is one bit for integer,
float, and `bits` segments and eight bits for `bytes`. Integer segments default
to a size of eight bits. The shortcut `value::n` means `value::size(n)`, and
`value::n*u` means `value::size(n)-unit(u)`.

In a pattern, a dynamic size may use an in-scope value or a value bound by an
earlier segment, but not one bound later in the same pattern. An unsized `bits`
or `bytes` pattern captures the remainder and must be the final segment;
unsized `bytes` additionally requires byte alignment. Insufficient input or a
segment mismatch makes the enclosing pattern fail normally. Duplicate or
conflicting modifiers are compile-time errors.

Construction uses checked integer range semantics and never silently truncates
high bits. V1 parses the same segment form but accepts only cases whose segment
boundaries and total size are byte-aligned; the later version removes that
restriction.

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
value, both branches are required and must have the same type unless an expected
structural union type accepts both through unambiguous injection. EL does not
infer a new union from mismatched branches. An `if` used only for effects may
omit `else`, in which case its type is `unit`.

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
result is used, subject to the same expected-union injection rule as `if`. The
type checker verifies exhaustiveness for every closed structural union, `bool`,
and other finite types it understands. Tagged alternatives use structural
patterns, while general union alternatives may use `name: Type` typed binding
patterns. A wildcard `_` arm makes a match exhaustive.

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

- A module is declared with a package-relative name such as
  `defmodule Http.Client do ... end`.
- One source file contains exactly one module declaration.
- Nested `defmodule` declarations are not supported in v1.
- `def` exports a function from its module; `defp` does not.
- The executable entry point is `Main.main() -> i32` for a target whose root
  module is the package-relative `Main`.
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
`package.namespace` is the root namespace used by source modules. V1
dependencies are EL packages; the manifest cannot declare native FFI libraries.

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

`el.lock` records the complete resolved graph, package versions, Git commits,
and source metadata. Executable projects commit it. A path dependency remains a
live development input, so the lockfile records its identity and declared
version but does not make its contents reproducible. Registry sources, version
ranges, and a compatibility solver are deferred beyond v1.

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
namespace prefixes every project module. Module names are derived strictly from
paths relative to `src/`:

```text
namespace = "Example"

src/main.el          -> defmodule Main        -> Example.Main
src/http/client.el   -> defmodule Http.Client -> Example.Http.Client
src/json_api.el      -> defmodule JsonApi     -> Example.JsonApi
src/foo/index.el     -> defmodule Foo.Index   -> Example.Foo.Index
```

The compiler strips the source extension, requires every path component to be
lowercase `snake_case`, converts each component mechanically to `PascalCase`,
and joins components with dots. Acronyms receive no special casing, and
`index.el` has no special meaning. The declared `defmodule` must exactly match
the derived package-relative name. Manifest target module names are also
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
V1 uses an explicit, lexically scoped `defer` statement for deterministic
cleanup:

```el
match File.open(path) do
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

A `defer` registers its call or block when execution reaches the statement.
Referenced binding values are captured at registration, and deferred actions run
once in last-in, first-out order on every normal control-flow exit from the
innermost enclosing lexical block. The deferred action must evaluate to `unit`,
so a fallible cleanup operation must explicitly handle its tagged result.
`return` is not permitted within a deferred call or block.

Deferred actions are not guaranteed to run after an unrecoverable runtime
failure, process abort, or external termination. Garbage collection remains
responsible only for memory and is never the semantic mechanism for releasing a
file, stream, socket, or other non-memory resource.

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

The compiler executable is named `elc`.

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

### 11.1 Bootstrap implementation

- Implementation language: Rust.
- Parsing formalism: PEG.
- PEG library: `pest` 2.8.7 with `pest_derive` 2.8.7, using a checked-in `.pest`
  grammar. The exact versions are pinned in the Rust lockfile.
- Expression precedence: encode explicit precedence levels in the grammar or
  use the parser library's Pratt parsing support; do not use left recursion.
- Backend: LLVM through Rust bindings.
- LLVM version: 22.1.0.
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

- Immutable scalar locals become LLVM SSA values where practical.
- Mutable locals may initially lower to entry-block `alloca` instructions plus
  loads/stores; LLVM's `mem2reg` pass can promote safe cases to SSA.
- `if` expressions lower to control-flow blocks and a `phi` value.
- `while` lowers to condition, body, and exit basic blocks.
- `for pattern in value` lowers through the statically selected `Iterable`
  implementation.
- `match` lowers to tests and branches after exhaustiveness checking.
- A concrete structural union lowers to a hidden discriminant plus an aligned
  payload; typed injections construct a member and exhaustive matches switch on
  the discriminant before lowering the selected member pattern.
- `defer` lowers by threading a scope's registered cleanup actions through its
  normal exit blocks in reverse registration order.
- `return` lowers to the function exit only after routing control through the
  cleanup blocks for every exited lexical scope.
- `left ++ right` lowers to the selected `Concat.concat(left, right)` call.
- `left |> call(args)` is rewritten to `call(left, args)` before Core IR.
- Monomorphization starts from concrete entry points, specializes reachable
  generic functions and named types, resolves their constrained protocol calls,
  and reuses an existing specialization for an identical type substitution.
- Runtime operations are called through a small, versioned internal ABI.

## 12. PEG grammar sketch

This is explanatory pseudogrammar, not the final parser grammar:

```text
program       <- SOI module EOI
module        <- "defmodule" module_name "do" module_item* "end"
module_name   <- type_name ("." type_name)*
module_item   <- derive_attr? struct_decl / type_alias / function
               / protocol_decl / protocol_impl
struct_decl   <- "defstruct" type_name type_params? when_clause?
                 "do" field* "end"
type_alias    <- "@type" type_name type_params? "=" type
function      <- ("def" / "defp") ident "(" params? ")"
                 return_type? when_clause? "do" block "end"
protocol_decl <- "defprotocol" type_name "do" protocol_sig* "end"
protocol_impl <- "defimpl" type_name "," "for" ":" type when_clause?
                 "do" function* "end"
type_params   <- "(" type_var ("," type_var)* ")"
type_apply    <- type_name "(" type ("," type)* ")"
type          <- union_type
union_type    <- primary_type ("|" primary_type)*
when_clause   <- "when" constraint ("," constraint)*
constraint    <- type_var ":" type_name
params        <- param ("," param)*
param         <- ident ":" type
return_type   <- "->" type
binding       <- "mut"? ident (":" type)? "=" expression
assignment    <- ident ":=" expression
ascription    <- expression "::" type
return_stmt   <- "return" expression
if_expr       <- "if" expression "do" block ("else" block)? "end"
match_expr    <- "match" expression "do" match_arm+ "end"
typed_pattern <- ident ":" type
while_stmt    <- "while" expression "do" block "end"
for_stmt      <- "for" pattern "in" expression "do" block "end"
defer_stmt    <- "defer" (call_expr / ("do" block "end"))
```

The real grammar must implement the newline and statement-boundary rules from
section 4.3 and resolve operator precedence, attributes, tagged and bitstring
patterns, struct literals, recovery behavior, and the ambiguity between a final
block expression and an expression statement.

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

- Lexical scopes and unique symbol IDs.
- Primitive types, function signatures, immutable bindings, and mutable locals.
- Type-check arithmetic, calls, returns, and `:=`.
- Infer implicit function type parameters, propagate expected types into calls,
  and type-check unconstrained generic bodies once.
- Normalize structural unions, prove member disjointness for all generic
  substitutions, and insert injections only from expected union types.
- Produce a typed AST and lower it to a minimal Core IR.

Exit test: accepted and rejected programs cover binding, mutation, calls, and
return types, including generic inference and ambiguous empty values, without
invoking LLVM.

### Milestone 3: first native executable

- Lower `i32`, `i64`, arithmetic, calls, and returns to LLVM IR.
- Monomorphize reachable unconstrained generic functions and concrete generic
  type layouts before LLVM lowering.
- Emit an object file and invoke the host linker.
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
- Verify live references held in locals, arguments, returns, globals, nested
  calls, recursion, and interior object graphs in debug and optimized builds.
- Provide a GC stress mode that collects as frequently as practical.

Exit test: a native optimized EL program retains a reachable heap graph while
temporary allocations are reclaimed under GC stress mode.

### Milestone 6: data types and text

- `defstruct`, construction, field access, and layout.
- `string`, `rune`, `bytes`, byte-aligned `bits`, and `Buffer`.
- Lists, maps, arrays, slices, and function values.
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

- Package ID, root namespace, module discovery, build targets, and EL dependency
  entries from `el.toml`'s `[deps]` table.
- Validate strict `src/` path-to-module mapping and namespace qualification.
- Resolve exact path and commit-pinned Git dependency graphs, validate conflicts,
  and read and write `el.lock`.
- `Reader` and `Writer` with tagged result values.
- Standard modules for strings, collections, buffers, bits, and I/O.
- Use explicit `defer` for deterministic cleanup of standard I/O resources.

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
- **Generic tests:** inference, expected-type propagation, constraints,
  recursive calls, specialization reuse, and ambiguous-instantiation errors.
- **Union tests:** canonical normalization, disjointness, expected-type
  injection, typed patterns, exhaustive matching, and concrete layouts.
- **IR tests:** verify LLVM modules; inspect small targeted IR fragments only.
- **End-to-end tests:** compile, link, execute, and check output/exit status.
- **GC stress tests:** frequent collection and heap graph survival.
- **Protocol tests:** resolution, coherence, derive constraints, and dispatch.
- **Manifest tests:** package ID, namespace, dependency, and target validation.
- **Dependency tests:** exact-version validation, lockfile stability, transitive
  conflicts, path resolution, and commit-pinned Git metadata.
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
- generic functions and named types infer, constrain, and monomorphize exactly
  as specified;
- immutable and mutable bindings behave exactly as specified;
- lower-case primitives, composites, closed structural unions, tagged tuples,
  typed member patterns, and exhaustive `match` work;
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

1. Resolved by D-035: the language name is EL and source files use `.el`.
2. Resolved by D-007: use `pest` 2.8.7 with `pest_derive` 2.8.7.
3. Resolved by D-008: use LLVM 22.1.0 through Inkwell 0.9.0.
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
- Status: accepted
- Decision: Use LLVM 22.1.0 through Inkwell 0.9.0 with the
  `llvm22-1-prefer-dynamic` feature. Pin Inkwell exactly in the Rust lockfile and
  distribute the matching LLVM shared library with `elc`.
- Reason: Its safer, higher-level API reduces incidental unsafe Rust while we
  learn LLVM construction and verification. Inkwell 0.9.0 supports LLVM 22.1
  target setup, object emission, module verification, and debug information.
- Consequence: Inkwell and `llvm-sys` types remain private to the LLVM backend;
  Core IR does not expose them. Because Inkwell is pre-1.0 and LLVM major
  upgrades may be disruptive, upgrades are explicit design and build changes.
- Validation: The first backend slice must create and verify a module, emit an
  object for the host target, attach basic debug locations, and link a runnable
  executable.

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
  explicitly. Cleanup is not guaranteed after an unrecoverable process abort or
  external termination, and GC never substitutes for resource release.
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
  after commas, and after operators requiring a right operand.
- Reason: One source-level separation rule keeps formatting and PEG parsing
  predictable and prevents compressed multi-statement lines.
- Consequence: An operator at the start of a line does not continue a completed
  previous line, so multiline pipelines place `|>` before the newline. There is
  no explicit continuation token, and semicolons produce a syntax diagnostic.

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
- Status: accepted
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
- Reference: [Elixir bitstring special form](https://hexdocs.pm/elixir/Kernel.SpecialForms.html#%3C%3C%3E%3E/1).

### D-035 — EL name and `.el` source extension

- Date: 2026-07-26
- Status: accepted
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

## 21. Next design checkpoint

All questions originally listed in section 18 are settled. New design work must
continue through stable decision-log entries before implementation depends on
it. D-036 adds closed disjoint structural unions to the accepted v1 type model.
