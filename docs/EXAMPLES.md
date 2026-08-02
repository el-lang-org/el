# EL Examples

This non-normative document is a practical companion to
[DESIGN.md](DESIGN.md), [GRAMMAR.md](GRAMMAR.md), and [TYPES.md](TYPES.md). It
shows the target EL version 1 syntax and semantics in small, copyable examples.

EL is still being designed and implemented. `GRAMMAR.md` controls accepted
syntax, `TYPES.md` controls static semantics, and `DESIGN.md` controls observable
runtime behavior and accepted decisions. Where those specifications do not yet
fix a standard-library API, this document says so rather than inventing one.
Unless a snippet contains `defmodule`, assume it appears in the body of an
appropriate module or function.

## 1. A minimal executable project

An EL project has an `el.toml` manifest and source files under `src/`:

```text
hello/
  el.toml
  src/
    main.ell
```

`el.toml`:

```toml
[package]
name = "hello"
namespace = "Hello"
version = "0.1.0"

[deps]

[target]
main = "Main"
```

`src/main.ell`:

```el
defmodule Main do
  def main() -> i32 do
    IO.println("Hello, world!")
    0
  end
end
```

The path `src/main.ell` requires the package-relative declaration
`defmodule Main`. With the manifest namespace, its fully qualified module name
is `Hello.Main`. `Main.main() -> i32` is the executable entry point, and its
result is forwarded as the process exit code. The host operating system may
expose fewer bits of that value to a waiting process.

From the project directory:

```console
el --help
el --version
el check
el build
el build --release
```

Every package is importable from source without a library target. `[target]` is
optional and singular; omitting it makes the package library-only. V1 has no
multiple executable targets, cross-compilation, or integrated `el test` command.
Use `--locked` with `check` or `build` when the existing `el.lock` must not
change. These commands and flags are the normative v1 CLI, not provisional
spellings. Successful commands exit 0, reported project or build failures exit
1, and malformed invocations exit 2. V1 has no `el run`; execute the built
native program directly.

## 2. Bindings and types

`=` introduces an immutable binding. A type annotation is optional when the
type can be inferred:

```el
answer = 42
next: i64 = answer + 1
message: string = "ready"
```

Use `mut` when a local binding must change, and `:=` to update it:

```el
mut count: i64 = 0
count := count + 1
```

`:=` evaluates to `unit`. It can update a mutable local directly or update one
direct struct field by reconstructing and rebinding a mutable local. Indexed and
nested-field updates are not supported in v1.

Numeric types do not coerce implicitly. Convert explicitly when types differ:

```el
wide: i64 = 100
narrow: i32 = i32(wide)
```

Unconstrained integer literals default to `i64`, and unconstrained floating
literals default to `f64`. Integer arithmetic is checked for overflow in every
build mode.

Fixed-width signed integers use two's-complement representation, and `f32` and
`f64` use IEEE binary32 and binary64. `isize` and `usize` match the target's
32-bit or 64-bit pointer width. Aggregate offsets, padding, calling conventions,
and object-file compatibility are not stable v1 interfaces.

Integers may use decimal, binary, octal, or hexadecimal notation with `_` digit
separators. Strings are double-quoted UTF-8, runes are single-quoted Unicode
scalar values, and atoms use colon-prefixed `snake_case`:

```el
mask = 0xff_00
permissions = 0b110_100
ratio = 1.5e-3
message = "line one\nline two"
letter = 'λ'
status = :not_found
```

V1 has no numeric suffixes, raw or multiline strings, interpolation format specifiers,
hexadecimal floating literals, or literal NaN and infinity.

Values implementing `Show` may be embedded with `#{...}`:

```el
message = "user #{name} has #{count} messages"
literal_marker = "write \#{value} to show interpolation syntax"
```

Evaluation is eager, exactly once, and left to right. This includes operator
operands, function arguments, collection elements, struct initializers,
pipelines, and indexing. `and` and `or` short-circuit.

Integer arithmetic, division edge cases, shifts, and conversions are checked.
Use per-type functions when modular arithmetic is intentional:

```el
next = I64.wrapping_add(counter, 1)
mask = (flags & 0xff) << shift
```

Bitwise binary operands have matching integer types, while shift counts are
`usize`. Signed division truncates toward zero, and remainder has the sign of
the dividend. Division by zero, an oversized shift, and a failed numeric
conversion are unrecoverable unless the compiler can diagnose them statically.

Floats follow IEEE 754, including NaN and signed-zero comparison behavior. They
support primitive comparisons but do not implement `Eq`, `Ord`, or `Hash` in
v1, so they cannot be map keys or participate in derivation of those protocols.

EL has no `null`, nullable references, or implicit zero values.

## 3. Functions, blocks, and visibility

Function parameter and return types are explicit. A block returns its final
expression:

```el
def add(left: i64, right: i64) -> i64 do
  left + right
end

defp non_negative(value: i64) -> bool do
  value >= 0
end
```

`def` declares a public function. `defp` declares a function visible only
inside its module. Omitting the return annotation means `-> unit`:

```el
def greet(name: string) do
  IO.println("Hello, " ++ name)
end
```

Named functions can be passed as values:

```el
def square(value: i64) -> i64 do
  value * value
end

def apply(value: i64, f: (i64) -> i64) -> i64 do
  f(value)
end

result = apply(5, square)
```

Generic named functions specialize from the expected function type and the
surrounding call. Each resulting function value is monomorphic:

```el
def identity(value: a) -> a do
  value
end

def apply_i64(value: i64, f: (i64) -> i64) -> i64 do
  f(value)
end

result = apply_i64(5, identity) # identity specializes to (i64) -> i64
```

A local binding shadows a bare function name, while a qualified public name
such as `Math.square` remains available. Code inside a module may pass or return
one of its `defp` functions; callers may invoke the received value even though
they cannot name that private function directly.

Protocol operations such as `Show.show` cannot themselves be function values in
v1; define an ordinary named wrapper when needed. Anonymous functions, closures,
partial application, and bound receiver methods are also not part of v1.

## 4. Newlines and pipelines

EL has no semicolons. A newline separates complete constructs. A line continues
when the expression is visibly incomplete:

```el
total = subtotal +
  tax

result = input
  |> normalize()
  |> validate()
```

The pipeline operator inserts its left side as the first argument of the call
on its right:

```el
value |> transform(a, b)
```

is equivalent to:

```el
transform(value, a, b)
```

Pipelines associate from left to right. In v1, the right side must be a
statically resolvable call; placeholders and arbitrary pipeline targets are not
supported.

## 5. Conditionals

`if` is an expression, and conditions must have type `bool`:

```el
label = if score >= 50 do
  "pass"
else
  "fail"
end
```

When the result is used, both branches are required and must have the same type,
unless an expected structural union type accepts both branches:

```el
value: i64 | string = if use_number do
  1
else
  "one"
end
```

An effect-only conditional may omit `else`; its type is then `unit`:

```el
if verbose do
  IO.println("starting")
end
```

There is no truthiness: integers, strings, lists, and other values cannot be
used as conditions.

## 6. Pattern matching and recoverable errors

Recoverable errors are ordinary tagged values rather than exceptions:

```el
def print_number(input: string) do
  match Parser.parse_int(input) do
    {:ok, value} -> IO.println(value)
    {:error, reason} -> IO.report(reason)
  end
end
```

`match` is an expression. Its arms must be exhaustive, and when its result is
used, every reachable arm must produce the same type (subject to expected-union
injection):

```el
def unwrap_or(result: {:ok, i64} | {:error, string}, fallback: i64) -> i64 do
  match result do
    {:ok, value} -> value
    {:error, _reason} -> fallback
  end
end
```

A wildcard covers all remaining alternatives:

```el
name = match status do
  :ready -> "ready"
  _ -> "not ready"
end
```

V1 patterns include wildcards, new immutable bindings, literals, tuples, tagged
tuples, empty and head/tail list patterns, partial struct patterns, byte-aligned
bitstring patterns, and typed structural-union member bindings. Pattern bindings
are local to their arm, and one pattern cannot bind the same name twice.

Every match must be exhaustive, including a match over an infinite type. Such a
match normally ends with a wildcard or binding catch-all:

```el
label = match status_code do
  200 -> "ok"
  404 -> "not found"
  _ -> "other"
end
```

Arms are tried from top to bottom. An arm after a wildcard or general binding is
unreachable and is rejected. Match guards, alternative patterns, pinning, and
map patterns are not part of v1.

Use `with` when several tagged-result operations should return the first failure
unchanged:

```el
def validate(registration: Registration) -> ValidationResult do
  with :ok <- validate_username(registration.username),
       :ok <- validate_email(registration.email),
       :ok <- validate_age(registration.age) do
    {:ok, make_user(registration)}
  end
end
```

Each clause is evaluated once from left to right. A matching clause may bind
values for later clauses and the body. The first non-matching value becomes the
result; the body runs only when every clause matches. Each possible propagated
value must fit the declared or otherwise expected result type.

## 7. Early return

The normal result of a function is its final expression. Use `return expression`
only to exit early:

```el
def require_positive(value: i64) -> {:ok, i64} | {:error, :not_positive} do
  if value <= 0 do
    return {:error, :not_positive}
  end

  {:ok, value}
end
```

Bare `return` is invalid. A unit-returning function uses `return unit`.

## 8. Loops

V1 has `while` loops and protocol-backed `for ... in` loops. Both evaluate to
`unit`:

```el
mut i: i64 = 0
while i < 10 do
  IO.println(i)
  i := i + 1
end
```

```el
for item in items do
  IO.println(item)
end
```

The loop binding may be a pattern:

```el
for {name, score} in results do
  IO.println(name ++ ": " ++ Show.show(score))
end
```

The pattern must be irrefutable for the iterable's item type. For example, the
tuple pattern above is valid when each item has type `{string, i64}`. A pattern
that selects only one member of a union is rejected:

```el
# Invalid when results contains both :ok and :error alternatives.
for {:ok, value} in results do
  process(value)
end
```

Use an exhaustive `match` inside the loop when item processing is refutable.

`for` uses the statically selected `Iterable` implementation. C-style loops,
`break`, `continue`, comprehensions, and a separate infinite `loop` form are not
part of v1.

## 9. Collections and generic functions

Lists are immutable and homogeneous. Their type is `[a]`:

```el
numbers: [i64] = [1, 2, 3]
words: [string] = ["one", "two", "three"]
```

List patterns support the empty list and head/tail decomposition. Lowercase
type names in a signature introduce inferred type parameters:

```el
def map(values: [a], f: (a) -> b) -> [b] do
  match values do
    [] -> []
    [value | rest] -> [f(value) | map(rest, f)]
  end
end
```

There is no explicit type-argument syntax at call sites. EL infers concrete
types from arguments and the expected result:

```el
squares = map([1, 2, 3], square)
empty: [i64] = []
```

When needed, an inline type ascription supplies the expected type:

```el
sum(Parser.parse_all(lines) :: [i64])
```

Generic collection traversal uses `Enum` and follows each iterable's documented
order. Call sites do not write protocol constraints:

```el
squares: [i64] = Enum.map([1, 2, 3], square)

def add(total: i64, value: i64) -> i64 do
  total + value
end

total = Enum.reduce([1, 2, 3], 0, add)
second: Option(i64) = Enum.at([10, 20, 30], 1) # {:some, 20}
missing: Option(i64) = Enum.at([10, 20, 30], 3) # :none
```

The real `Enum` declarations explicitly include `when i: Iterable`; the API
reference states that shared constraint once rather than repeating it on every
signature. `at` uses zero-based `usize` positions, returns an `Option`, and
stops once it reaches the requested item. `map`, `filter`, and `to_list` always
return lists. `any` and `all` short-circuit, maps enumerate `{key, value}` in
insertion order, and function arguments are named function values because v1
has no closures.

Collection-specific operations remain where they expose structure or conversion:

```el
reversed = List.reverse([1, 2, 3])
array_count = Array.length(#[1, 2, 3])
byte_count = Bytes.byte_size(data)
copied_bytes = Bytes.from_list(Bytes.to_list(data))
```

`++` concatenates values through the `Concat` protocol:

```el
all = prefix ++ suffix
message = "Hello, " ++ name
```

For linked lists, concatenation copies the left spine. Repeated list `++` in a
loop can therefore be quadratic.

Tuples, fixed-size arrays, and immutable maps have distinct construction syntax:

```el
pair = {"Ada", 42}
array = #[1, 2, 3]
scores = %{"Ada" => 42, "Grace" => 50}
```

Tuples have at least two elements. The number of elements in `#[...]` becomes
the concrete array length. Local code normally lets the compiler infer it:

```el
coordinates = #[10, 20, 30] # inferred [i64; 3]
empty: [u8; 0] = #[]        # empty arrays need an expected item type
```

User-written array types contain literal lengths. They may be generic over the
item type, but not over the length:

```el
def first_of_pair(values: [a; 2]) -> a do
  values[0]
end
```

Symbolic `[a; N]`, length arithmetic, and `[a; _]` are not valid v1 source.
Algorithms accepting any contiguous length use `Slice(a)`:

```el
def sum(values: Slice(i64)) -> i64 do
  mut total: i64 = 0

  for value in values do
    total := total + value
  end

  total
end

total = sum(Slice.from_array(#[1, 2, 3, 4]))
```

`Array.length` and `Slice.from_array` work for every concrete array length as
compiler-provided standard operations. Arrays of different lengths are distinct
types and do not convert implicitly.

Map keys must implement both `Eq` and `Hash`; lookup returns the standard
`Option` tagged union rather than a nullable value:

```el
match Map.fetch(scores, "Ada") do
  {:some, score} -> IO.println(score)
  :none -> IO.println("missing")
end
```

Map iteration is deterministic insertion order. Updating an existing key keeps
its position; removing and reinserting it moves it to the end. Duplicate keys in
a literal keep their first position but take their last value. Equality still
compares membership rather than insertion order, and the runtime hash seed does
not affect traversal:

```el
scores = Map.put(scores, "Ada", 43)   # "Ada" keeps its position
scores = Map.remove(scores, "Ada")
scores = Map.put(scores, "Ada", 44)   # "Ada" is now last
```

Lists iterate head to tail, arrays and slices by increasing index, bytes by
increasing byte offset, and string views in source order. All iteration cursors
are immutable values.

Arrays, slices, bytes, and bits support bounds-checked read indexing by `usize`:

```el
first = array[0]
byte = data[byte_index]
bit: bool = bit_data[bit_index]
```

For `bits`, index zero is the most-significant bit of the first source byte.
Strings, lists, and maps do not support indexing. Slices use explicit
construction so sharing and copying remain visible:

```el
whole = Slice.from_array(array)
part = Slice.subslice(whole, 1, 2)
independent = Slice.copy(part)
```

## 10. Structs and deriving

`defstruct` declares a nominal type with immutable fields. Every field must be
initialized:

```el
@derive [Eq, Show, Hash]
defstruct User do
  id: u64
  name: string
end

user = %User{id: 1, name: "Ada"}
```

Structs have value semantics and no observable identity. Copying a struct is a
shallow fieldwise copy; immutable reference-backed fields may share storage.
Fields are not independently mutable, but a direct field update may reconstruct
a struct and rebind a mutable local:

```el
original = %User{id: 1, name: "Ada"}
mut renamed = original
renamed.name := "Grace"

# original.name is still "Ada"
# renamed.name is "Grace"
```

The root must be a mutable local, the assigned value must have exactly the
field's type, and the update evaluates to `unit`. V1 does not accept nested
field paths, indexed targets, parameters, temporaries, or call results as update
targets.

Structs may be generic:

```el
defstruct Box(a) do
  value: a
end

defstruct Pair(a, b) do
  first: a
  second: b
end
```

Type application uses parentheses:

```el
boxed: Box(i64) = %Box{value: 42}
pair: Pair(string, i64) = %Pair{first: "age", second: 37}
```

Recursive nominal data must cross managed indirection. A list-backed tree node
is valid:

```el
defstruct Node do
  value: i64
  children: [Node]
end
```

A direct field, tuple, fixed array, structural union, or user-defined value
struct does not break a recursive layout cycle. This is invalid:

```el
defstruct InvalidNode do
  next: InvalidNode
end
```

Deriving succeeds only when all participating fields implement the requested
protocol.

## 11. Type aliases, tagged unions, and general unions

`@type` creates a transparent alias, not a distinct nominal type:

```el
@type UserId = u64
@type ParseResult = {:ok, i64} | {:error, string}
@type Result(a, e) = {:ok, a} | {:error, e}
@type Maybe(a) = {:just, a} | :nothing
```

Transparent aliases must be acyclic, including cycles beneath a managed
container. Recursive data uses a nominal struct such as `Node` above so alias
expansion and union normalization always terminate.

`Option(a) = {:some, a} | :none` is supplied by the reserved core prelude and is
not redeclared by user modules.

`A | B` is a closed structural union. Alternatives are disjoint when no finite
substitution of their type variables can make their normalized types equal.
Concrete invariant applications with different arguments are therefore valid:

```el
@type NumberList = List(i64) | List(f64)
@type TaggedValue = {:ok, i64} | {:ok, string}
```

Physical representation does not determine disjointness; injection records a
hidden member discriminant. A value is injected into a union only when an
expected union type is available:

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

Use a typed binding pattern to select a member of a general union:

```el
def describe(value: i64 | string) -> string do
  match value do
    number: i64 -> Show.show(number)
    text: string -> text
  end
end
```

Tagged alternatives use structural patterns such as `{:ok, value}`. General
unions do not automatically implement protocols and cannot be a `defimpl`
target in v1; match the union before calling member-specific behavior.

## 12. Protocols and implementations

Protocols declare behavior, and implementations are explicit:

```el
defprotocol Show do
  def show(value: Self) -> string
end

defstruct Point do
  x: i64
  y: i64
end

defimpl Show, for: Point do
  def show(value: Point) -> string do
    "Point(" ++ Show.show(value.x) ++ ", " ++ Show.show(value.y) ++ ")"
  end
end
```

A generic function constrains a type parameter with `when`:

```el
def max(left: a, right: a) -> a when a: Ord do
  if left >= right do
    left
  else
    right
  end
end
```

Implementations for generic types can constrain their parameters too:

```el
defimpl Show, for: Box(a) when a: Show do
  def show(value: Box(a)) -> string do
    Show.show(value.value)
  end
end
```

Protocol dispatch is static. Protocol names are not runtime value types in v1,
and a matching method name does not implicitly satisfy a protocol.

Protocols declare associated types explicitly, and implementations assign them
explicitly:

```el
defprotocol Iterable do
  type Item
  type Cursor

  def iter(value: Self) -> Cursor
  def next(cursor: Cursor) -> {:item, Item, Cursor} | :done
end

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

Generic code uses a qualified projection to name the type selected by an
implementation:

```el
def collect(values: a) -> [Iterable.Item(a)] when a: Iterable do
  # implementation
end
```

An implementation must be declared by the package owning either the protocol or
the target type. Overlapping implementations, specialization, implicit
associated-type inference, and a conflict between `@derive` and an explicit
implementation are compile-time errors.

## 13. Strings, bytes, runes, and buffers

`string` always contains valid UTF-8. It is distinct from raw `bytes`, arbitrary
length `bits`, and a single Unicode scalar value (`rune`). These string
operations make the unit of inspection explicit:

```el
byte_count = String.byte_size(text)
grapheme_count = String.length(text)

for byte in String.bytes(text) do
  inspect_byte(byte)
end

codepoints: [rune] = String.codepoints(text)
graphemes: [string] = String.graphemes(text)

if String.empty(text) == false and String.contains(text, "@") do
  fields: [string] = String.split(text, "@")
end

for grapheme in graphemes do
  IO.println(grapheme)
end
```

The plural functions favor direct use and return ordinary eager collections.
Allocation-sensitive traversal uses explicitly named views:

```el
for codepoint in String.codepoint_view(text) do
  inspect_codepoint(codepoint)
end

for grapheme in String.grapheme_view(text) do
  inspect_grapheme(grapheme)
end
```

Integer indexing such as `text[i]` is invalid. `String.length` counts Unicode
grapheme clusters, not bytes or code points.

`String.split` retains empty fields: splitting `"a,,b,"` on `","` yields
`["a", "", "b", ""]`. An empty separator performs no split and returns the
source as a one-element list. Use `String.graphemes` when the desired unit is a
Unicode extended grapheme cluster.

All grapheme APIs use EL v1's bundled Unicode 17.0.0 data and the untailored
default extended-grapheme rules from UAX #29 revision 47. Their results do not
depend on the host locale or installed Unicode libraries, and they do not
normalize or case-fold the source text.

Use `Buffer` to avoid repeated immutable concatenation when building text or
binary data:

```el
mut buffer = Buffer.new()
buffer := Buffer.append_string(buffer, "hello")
buffer := Buffer.append_byte(buffer, 0x20)
buffer := Buffer.append_string(buffer, "world")

match Buffer.to_string(buffer) do
  {:ok, text} -> IO.println(text)
  {:error, reason} ->
    offset = String.utf8_error_offset(reason)
    IO.report("invalid UTF-8 at byte " ++ Show.show(offset))
end
```

`Buffer.to_bytes` always succeeds. `Buffer.to_string` validates UTF-8 and returns
a tagged result. `String.Utf8Error` identifies the zero-based offset of the first
invalid sequence. Distinct `append_byte`, `append_bytes`, and `append_string`
functions avoid overloading, and previously returned values remain unchanged.

## 14. Deterministic resource cleanup

Garbage collection manages memory, not files, sockets, or other scarce
resources. Register deterministic cleanup with `defer`:

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

`IO.print`, `IO.println`, and `IO.report` accept any value implementing `Show`;
they format through that protocol and treat console failure as unrecoverable.
This is not a general implicit conversion to `string`. Programs needing
recoverable behavior use the `Reader` and `Writer` protocols with `IO.stdin`,
`IO.stdout`, or `IO.stderr`.

Standard collections use recursive diagnostic formatting. Map keys and values
must both implement `Show`, and map entries retain insertion order:

```el
counts: Map(string, usize) = %{"first" => 1, "second" => 2}
IO.println(counts)
IO.println(["nested", Show.show(#[3, 4])])
```

This prints:

```text
%{first => 1, second => 2}
[nested, #[3, 4]]
```

These forms are intended for human-readable diagnostics, not parsing or stable
serialization.

`File.open_read` returns `File.Reader`; `File.create` and `File.append` return
`File.Writer`. Copying one of these opaque handles aliases the same OS resource.
Closing through one alias invalidates all aliases, and subsequent operations or
a repeated close return tagged errors.

Portable recovery inspects a stable kind instead of parsing `Show` output or a
host error code:

```el
match File.open_read(path) do
  {:ok, file} -> process(file)
  {:error, reason} ->
    match File.error_kind(reason) do
      :not_found -> IO.report("file not found")
      _ -> IO.report(reason)
    end
end
```

`File.error_operation` identifies the failed operation, while
`File.error_code` returns an optional target-dependent `i64`. `IO.Error` exposes
the equivalent accessors through `IO`. Standard I/O, file, and UTF-8 errors are
immutable values implementing `Eq`, `Hash`, and `Show`, but not `Ord`.

A deferred action is registered only when execution reaches it. A deferred call
evaluates its target and arguments immediately and delays only invocation:

```el
defer release(make_handle())
# make_handle runs now; release runs at scope exit
```

A deferred block captures referenced outer bindings as immutable value snapshots
and delays its body expressions:

```el
defer do
  release(make_handle())
  # make_handle runs at scope exit
end
```

Actions run once, in last-in-first-out order, on every normal exit from the
innermost enclosing lexical block—including an early `return`. Loop-body actions
run at the end of each reached iteration. On fallthrough, the block result is
evaluated and saved before cleanup, then yielded afterward.

A deferred action must evaluate to `unit`. It cannot use `return`, register
another `defer`, or update a captured outer binding, though it may use local
mutable bindings declared inside the deferred block.

Deferred actions do not run after an unrecoverable runtime failure. They also
are not guaranteed after an implementation abort or external termination.

## 15. Modules and source paths

Each source file contains exactly one module. Its declaration is derived from
its path relative to `src/`:

```text
Source path          Required declaration       With namespace "Example"
src/main.ell          defmodule Main              Example.Main
src/http/client.ell   defmodule Http.Client       Example.Http.Client
src/json_api.ell      defmodule JsonApi            Example.JsonApi
src/foo/index.ell     defmodule Foo.Index          Example.Foo.Index
```

Path components must be lowercase `snake_case`; each is mechanically converted
to `PascalCase`. Acronyms and `index.ell` receive no special treatment. A module
cannot span multiple files, and nested module declarations are not supported in
v1.

V1 has no imports or module aliases. Functions and declarations in the current
module use bare names; other modules use qualified names:

```el
validate(value)                 # current module
Http.Client.get(url)            # current package, package-relative module
Json.Decoder.decode(input)      # dependency whose namespace is Json
IO.println(message)             # core prelude module
```

Bare names resolve through lexical scope, the current module, and then the fixed
core prelude. Ambiguous module names are compile-time errors. Every package in a
resolved dependency graph must have a distinct root namespace.

The prelude imports core types, protocols, and modules but no bare functions.
Its names—including `Option`, `Show`, `Enum`, `IO`, `List`, `Map`, and
`String`—are
reserved against package declarations and dependency root namespaces.

Structs, aliases, protocols, struct fields, and implementations are public in
v1. Functions use `def` for public visibility and `defp` for module-private
visibility. A module may not declare the same function name at multiple arities.

## 16. Dependencies

V1 supports local path dependencies and Git dependencies pinned to a complete
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

A dependency selects exactly one of `path` or `git`. Versions are exact, Git
revisions cannot be branches, tags, or abbreviated hashes, and the dependency
key must equal the dependency package's `package.name`.

Executable projects commit `el.lock`. V1 does not support registries, version
ranges, native FFI dependencies, or multiple versions of one package in the
same dependency graph.

## 17. Byte-aligned bitstrings

V1 `<<...>>` construction produces `bytes`, and its bitstring patterns consume
`bytes`. Integer segments use literal byte-aligned widths from 8 through 64
bits; byte segments may use a dynamic size measured in bytes:

```el
packet = <<version::unsigned-big-size(8),
  length::unsigned-big-size(16),
  payload::bytes>>

match packet do
  <<version::unsigned-big-size(8),
    length::unsigned-big-size(16),
    payload::bytes-size(usize(length))>> -> process(version, payload)
  _ -> IO.report("invalid packet")
end
```

Construction checks integer ranges and exact sized-byte lengths without
truncation or padding. A dynamic mismatch terminates with
`bitstring_size_mismatch`; insufficient pattern input or a literal mismatch is
ordinary pattern failure. Integer pattern captures are `u64` when unsigned and
`i64` when signed.

Arbitrary-length `bits` remain useful without arbitrary-width source segments:

```el
all: bits = Bytes.to_bits(packet)
header: bits = Bits.slice(all, 0, 12)
first: bool = header[0]

match Bits.to_bytes(header) do
  {:some, data} -> consume(data)
  :none -> IO.report("not byte-aligned")
end
```

`Bits.slice` and indexing are bounds-checked; both use
`index_out_of_bounds`. Bit index zero is the most-significant bit of the first
source byte. `Bits.bit_size` reports arbitrary bit length, and `++` concatenates
bit sequences.

Float, UTF, `bits`, explicit-unit, and non-byte-aligned source segments are
post-v1. The intended later syntax includes forms such as:

```el
<<version::size(3),
  flags::size(5),
  length::unsigned-big-size(16),
  payload::bits-size(length),
  rest::bits>>
```

Do not rely on that final form in a v1 program.

## 18. Process inputs and paths

`Process.arguments` returns the user arguments after the executable name. Host
text is decoded strictly, so callers handle the possibility that one native
argument is not representable as an EL `string`:

```el
match Process.arguments() do
  {:ok, arguments} -> Enum.each(arguments, IO.println)
  {:error, {:invalid_text, index}} ->
    IO.report("invalid text in argument " ++ Show.show(index))
end
```

Environment lookup reads the launch-time snapshot and distinguishes absence,
an invalid requested name, and an unrepresentable native value:

```el
match Process.get_env("EL_CONFIG") do
  {:ok, path} -> load_config(path)
  :not_found -> use_defaults()
  {:error, :invalid_name} -> IO.report("invalid environment name")
  {:error, :invalid_text} -> IO.report("environment value is not valid text")
end
```

File functions accept `string` paths. Unix-like targets pass their UTF-8 bytes
unchanged; Windows transcodes them exactly to UTF-16. No locale conversion or
Unicode normalization occurs, and embedded U+0000 produces a recoverable
`:invalid_input` file error. V1 deliberately cannot name invalid-UTF-8 Unix
paths or Windows paths containing unpaired surrogates.

## 19. APIs not fixed yet

The scalar and composite grammar and the initial collection, string, Buffer,
console, file, and process APIs are fixed. Further conveniences remain subject
to the same decision process rather than being implied by these examples.

## 20. Unrecoverable runtime failures

Checked operations that cannot return an ordinary tagged error terminate the
process immediately. For example, this compiles but fails at runtime when the
index is outside the array:

```el
array = #[10, 20, 30]
value = array[index]
```

The runtime reports the stable category `index_out_of_bounds` and the operation's
source location to standard error when possible, then exits nonzero. It does not
run pending `defer` actions. Overflow, zero integer division, invalid shifts or
conversions, bitstring construction-size mismatch, allocation exhaustion, and
console-output failure follow the same contract. These failures cannot be
caught, and v1 provides no user-callable panic or stack trace.

## 21. Common compile-time errors

The following snippets are intentionally invalid:

Updating an immutable binding:

```el
x = 1
x := 2
```

Rebinding with a different type:

```el
mut count: i64 = 1
count := "one"
```

Implicit numeric conversion:

```el
small: i32 = 1
large: i64 = small
```

Using a non-boolean condition:

```el
if 1 do
  IO.println("truthy")
end
```

Indexing a UTF-8 string by integer:

```el
first = text[0]
```

Updating a field through an immutable root:

```el
point = %Point{x: 1, y: 2}
point.x := 2
```

Nested field and indexed updates are also outside v1:

```el
mut user = initial_user
user.address.city := "Paris"

mut items = initial_items
items[0] := replacement
```

Using a semicolon:

```el
x = 1; y = 2
```

Defining an overlapping structural union:

```el
@type Either(a, b) = a | b
@type Optional(a) = a | :none
@type SpecialBox(a) = Box(a) | Box(i64)
```

The generic alternatives can overlap after substitution, so all three aliases
are rejected. For `SpecialBox`, the diagnostic identifies `a = i64` as a
witness that makes both members equal.

Defining a recursive transparent alias:

```el
@type Loop = Loop
@type RecursiveList = [RecursiveList]
```

Both aliases are rejected. Managed recursion is expressed through a nominal
struct field instead.
