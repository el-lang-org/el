# EL Type System

Status: normative version 1 specification
Language: **EL**
Last updated: 2026-07-26

## 1. Scope and authority

This document is the source of truth for EL version 1 type formation, equality,
normalization, inference, checking, protocol conformance, and static
well-formedness. [GRAMMAR.md](GRAMMAR.md) defines how types and typed constructs
are written. [DESIGN.md](DESIGN.md) explains their motivation and owns runtime
semantics, library behavior, architecture, and accepted decisions.
[EXAMPLES.md](EXAMPLES.md) is illustrative.

When this document and another descriptive passage disagree about static
semantics, this document controls. An intentional type-system change updates
this document and records an accepted decision in `DESIGN.md` before compiler
behavior or examples depend on it.

EL programs are checked statically. There is no implicit numeric conversion,
truthiness, nullable reference, runtime-reified generic, or runtime protocol
value. The compiler may carry an error type internally for diagnostic recovery;
that type is not an EL type and cannot make an otherwise invalid program valid.

EL's type system is small and static: it uses explicit conversions, has no
implicit numeric coercions, and favors simple nominal types. EL protocols use
explicit `defimpl` declarations rather than implicit structural interface
satisfaction.

## 2. Primitive and built-in scalar types

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
values). `string`, `bytes`, and `bits` have dedicated runtime semantics in
[DESIGN.md §7](DESIGN.md#7-strings-and-binaries).
`Buffer` is a standard-library type, not a primitive, so it uses PascalCase.

The fixed-width signed integers use two's-complement representation, and the
fixed-width unsigned integers use ordinary binary representation. `isize` and
`usize` have the compilation target's pointer width; v1 targets have either
32-bit or 64-bit pointers. `f32` and `f64` use IEEE 754 binary32 and binary64.
A `rune` converts losslessly to `u32`, though its in-memory representation is not
otherwise public.

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

## 3. Generics, custom types, and aliases

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

### 3.1 Structural union types

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

## 4. Structs

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

## 5. Composite types

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

### 5.1 Core collection modules

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

### 5.2 Representation boundary

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

## 6. Functions and visibility

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

## 7. Protocols and implementations

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

## 8. Core standard-library protocols

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

An interpolated string expression has type `string`. Every expression inside a
`#{...}` segment must satisfy `Show`, and the compiler statically selects the
same implementation as an explicit `Show.show` call. Segment expressions are
evaluated eagerly, exactly once, and from left to right. Interpolation performs
human-readable conversion only; it is not serialization and provides no
format-specifier syntax in v1.

The basic text predicates and separator operation have these fixed signatures:

```el
String.empty(text: string) -> bool
String.contains(text: string, pattern: string) -> bool
String.split(text: string, separator: string) -> [string]
```

Both arguments to `contains` and `split` must be `string`; these operations do
not accept raw `bytes` or implicit conversions.

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
- `Show` covers standard scalar and collection types and derived structs. Its
  output is human-readable diagnostics, not a stable serialization format, and
  formatting may evolve between language releases. Standard I/O and file error
  types also implement `Show`.

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

## 9. Process inputs and native paths

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

## 10. Typing contexts and expression checking

The type checker maintains separate namespaces for modules, types, protocols,
functions, local values, and associated types. A resolved identity, not its
source spelling, determines whether two references name the same declaration.
Visibility and package ownership are checked during resolution before expression
typing depends on a declaration.

Checking is bidirectional:

- **synthesis** computes a type from an expression when its operands and
  declaration determine one; and
- **checking** validates an expression against an expected type and permits
  expected-type-directed literal selection, generic inference, empty collection
  inference, and structural-union injection.

Expected types flow from parameter positions, annotated bindings, declared
returns, enclosing branch results, exact function-value positions, and explicit
`expression :: Type` ascriptions. They do not flow backward across unrelated
statements or cause the compiler to invent a union. After checking, every
expression has one canonical type and no unresolved inference variable.

An ascription checks its operand against the written type and synthesizes that
type. It performs no conversion. Explicit numeric conversion uses a primitive
conversion call such as `i32(value)` and is governed by section 2.

### 10.1 Bindings, assignment, and scopes

`name = expression` and `name: Type = expression` introduce a new immutable
local. `mut name = expression` and its annotated form introduce a mutable local.
The initializer is checked before the new identity enters its scope, every
binding is initialized, and the same lexical scope cannot redeclare that
identity. A nested scope may introduce a new identity with the same spelling.

`name := expression` is valid only when `name` resolves to a mutable local. The
right-hand side is checked against the binding's exact type and the expression's
result type is `unit`.

`name.field := expression` additionally requires `name` to be a mutable local of
a struct type and `field` to be one of that struct's direct fields. The right
side is checked against the declared field type. Parameters, immutable locals,
temporaries, nested fields, and indexed expressions are not update roots. This
operation also has type `unit` and does not change the binding's type.

Function parameters and pattern bindings are immutable. A pattern binding is
visible only in the successful arm or loop body associated with that pattern.
One pattern cannot introduce the same spelling twice.

### 10.2 Blocks, conditionals, and termination

A nonempty block has the type of its final expression; an empty block has type
`unit`. Statements such as assignment, `defer`, `while`, and `for` have type
`unit`. Earlier nonterminating expressions in a block must be valid even though
their values are discarded.

An `if` condition checks against `bool`. When an `if` is checked in a value
context, it requires both branches and checks both against one expected type. If
no expected type is available, both branches must synthesize the same canonical
type. Mismatched types do not infer a union. An `if` without `else` is accepted
only in a discarded/effect context and has type `unit`.

`return expression` checks its expression against the enclosing function's
declared result and terminates its path. A terminating path satisfies any local
expected result because it produces no value. A function body checks against
its declared result on every normal path. An omitted return annotation denotes
`unit`; bare `return` is never valid.

A deferred call is type-checked at its registration site, including its target
and arguments, and its selected function result must be `unit`. A deferred block
is checked against `unit`; it may contain neither `return` nor another `defer`.
Each referenced outer binding becomes an immutable by-value capture, so the
deferred block cannot assign to it even when the original binding is mutable.
Locals declared inside the deferred block follow the ordinary rules.

`while` checks its condition against `bool` and its body in an effect context.
It has type `unit`. A `for` expression selects one `Iterable` implementation for
the iterable's type, checks its pattern as irrefutable for that implementation's
normalized `Item`, and checks its body in an effect context. It also has type
`unit`.

### 10.3 Calls, operators, and function values

A direct call checks arguments exactly once from left to right against the
selected function parameters. Arity must match. Generic inference collects
constraints from argument and expected-result positions, solves one consistent
substitution, validates every `when` constraint, and rejects ambiguity. V1 has
no overload resolution by parameter type.

A named function reference is checked or synthesized as one exact structural
function type. A generic function reference requires enough surrounding
information to select one monomorphic specialization. Function parameter and
result types match exactly; there is no variance or implicit adaptation.

Primitive operator typing follows sections 2 and 8. `and` and `or` require and
return `bool`. `++` selects `Concat.concat` for two operands of the same type and
returns that type. Nonprimitive equality and ordering select lawful `Eq` and
`Ord` implementations; both operands have the same type and the result is
`bool`. The pipeline is typed after rewriting its left operand into the first
argument position of its statically resolved right-hand call.

Field access requires a struct value and synthesizes the declared field type.
Indexing requires `usize` and is available only for arrays, slices, `bytes`, and
`bits`, with the result types specified in section 5.

### 10.4 Literals and collection construction

Integer and floating literals first use a compatible expected primitive type;
otherwise they default as specified in section 2. A statically out-of-range
literal is rejected. Boolean, rune, string, bytes, bits, atom, and `unit`
literals have their corresponding exact types.

Tuple elements synthesize or check positionally. List and array elements are
homogeneous. A list tail checks against the same list type, and an array's source
element count becomes its literal type length. Map entries share one key type
and one value type, and the key type must implement `Eq` and `Hash`. Empty list,
map, and array literals require an expected element type when no element can
provide it; `#[]` always contributes length zero.

A struct literal names one nominal struct application, supplies every field
exactly once, and checks each initializer against its declared substituted type.
Unknown, missing, and duplicate fields are errors.

### 10.5 Pattern checking and exhaustiveness

A pattern is checked against one subject type and does not synthesize an
independent type. `_` and a new binding are irrefutable. A literal pattern is
valid only for its exact compatible subject type. Tuple, list, struct, and
bitstring patterns recursively check their components against the corresponding
subject parts.

A typed binding pattern `name: Member` is valid only when the subject is a
normalized structural union and `Member` is exactly one member. It both selects
that member and binds `name` with the member type. It is not a general runtime
type test.

Every `match` is exhaustive. Arms are analyzed top to bottom, and a provably
subsumed arm is rejected. When a match is checked against an expected result,
every reachable nonterminating arm checks against it. Without an expected type,
all such arms must synthesize the same canonical type. Infinite scalar domains
normally require a wildcard or binding catch-all; finite and structural domains
are decomposed recursively.

### 10.6 Source bitstrings

A `<<...>>` construction has type `bytes`, and a bitstring pattern checks only
against a `bytes` subject. V1 source segments are byte-aligned.

An integer construction segment accepts any integer operand. Its required
literal size is one of 8, 16, 24, 32, 40, 48, 56, or 64 bits, and the operand
must fit the selected signedness and width. In a pattern, an unsigned integer
segment binds `u64` and a signed segment binds `i64`; literal subpatterns are
range-checked accordingly.

A `bytes` construction segment checks its operand against `bytes`. A size
expression checks against `usize`. In a pattern it may refer to an outer binding
or one introduced by an earlier segment, never a later segment. Only the final
pattern segment may be an unsized `bytes` capture.

Duplicate, conflicting, unknown, or out-of-scope modifiers are static errors.
Empty `<<>>` has type `bytes`. Runtime sizing, matching, and byte-order behavior
are defined in [DESIGN.md §7.4](DESIGN.md#74-bitstring-construction-and-matching).

## 11. Static well-formedness order

The compiler observes this dependency order, though implementations may combine
passes when diagnostics remain equivalent:

1. parse and grammar validation under [GRAMMAR.md](GRAMMAR.md);
2. module/path validation and declaration collection;
3. namespace, visibility, and package-owner resolution;
4. transparent-alias expansion and cycle rejection;
5. type formation, generic parameter collection, and constraint validation;
6. protocol implementation completeness, ownership, coherence, and overlap;
7. expression and pattern checking with inference;
8. union normalization/disjointness and exhaustiveness analysis;
9. inline-containment and finite-layout validation; and
10. Typed AST verification before [IR.md](IR.md) lowering.

Diagnostics should report the earliest actionable source cause and may continue
with compiler-private error types. Recovery never establishes conformance and a
module containing a static error is not lowered.

## 12. Conformance obligations

Type-system tests are derived from this document and include accepted/rejected
pairs for:

- every primitive operation, conversion edge, and literal default;
- annotated and inferred bindings, exact assignment, and shadowing;
- expected-type propagation and ambiguous empty values;
- generic calls, recursion, constraints, and monomorphic function values;
- alias normalization and direct, mutual, generic, and cross-module cycles;
- union canonicalization, injection, overlap witnesses, and concrete rechecks;
- protocol completeness, associated types, coherence, and implementation
  overlap across packages;
- every pattern family, unreachable arms, exhaustiveness, and `for`
  irrefutability;
- finite and infinite recursive layouts through every inline and managed
  constructor; and
- deterministic Typed AST snapshots containing resolved identities, canonical
  types, selected implementations, and explicit union injections.
