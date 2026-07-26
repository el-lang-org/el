# EL Grammar

Status: normative version 1 specification
Language: **EL**
Last updated: 2026-07-26

## 1. Scope and authority

This document is the source of truth for the lexical and concrete syntactic
forms accepted by an EL version 1 compiler. [TYPES.md](TYPES.md) determines
whether a syntactically valid program is statically well formed.
[DESIGN.md](DESIGN.md) owns language motivation, runtime semantics, and accepted
decisions. [EXAMPLES.md](EXAMPLES.md) is illustrative.

The compiler's checked-in `pest` grammar implements this contract; it is not an
independent source of language syntax. Parser recovery may recognize incomplete
or erroneous forms only to issue diagnostics and must never make a rejected
program valid.

An intentional accepted-language change updates this file and records an
accepted decision in `DESIGN.md` before compiler behavior or examples depend on
it. A disagreement between this file and the parser is a compiler bug.

## 2. Lexical structure

### 2.1 Source files

- Source file extension: `.el`.
- Source text is UTF-8.
- UTF-8 BOMs are not accepted. A physical newline is either LF or CRLF; a bare
  carriage return is invalid. Outside literals, horizontal whitespace is ASCII
  space or tab. A line comment excludes its terminating newline.
- V1 identifiers use ASCII letters, digits, and `_` as specified in section 3.
- Value and function names use `snake_case`.
- Primitive type names are lower case.
- Type variables are lower case; named type constructors, protocols, and modules
  use `PascalCase`.
- `#[` begins an array literal; any other `#` begins a line comment.
- Nested block comments are deferred.

### 2.2 Keywords

V1 reserved words:

```text
def defer defimpl defmodule defp defprotocol defstruct do else end false for if in
match mut return true type when while
```

`@derive` and `@type` are built-in attributes and are reserved as complete
attribute names.

Future keywords are not reserved until their feature is accepted.

### 2.3 Newlines and statement separation

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

### 2.4 Literals

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
interpolation, raw strings, multiline strings, and adjacent-literal
concatenation are deferred.

An atom literal is `:` followed by an ASCII `snake_case` identifier, such as
`:ok` or `:not_found`. Quoted atoms and conversion of runtime strings to atoms
are not supported. `true`, `false`, and `unit` are the literal values of `bool`
and `unit`.

## 3. Syntactic grammar

This document is the normative EL v1 source grammar. Compiler recovery
productions may accept incomplete input only to issue diagnostics; they must
never make an otherwise rejected program valid.
The compiler's checked-in `pest` grammar must implement this contract; it is not
a second source of language syntax. A disagreement is a compiler bug or
requires an accepted decision that updates this document.

The notation below uses `/` for ordered choice and postfix `?`, `*`, and `+` for
optionality and repetition. Lowercase lexical names are defined immediately
after the syntactic productions. Horizontal
space and comments may occur between tokens. `NL` is one physical newline that
remains significant under section 2.3; newlines treated as
continuation whitespace do not produce `NL`. `body(item)` means zero or more
`item` forms separated by `NL`, with optional leading and trailing `NL`. There
is no other statement separator.

```text
program          <- SOI NL* module NL* EOI
module           <- "defmodule" module_name "do" body(module_item) "end"
module_item      <- derive_attr NL+ struct_decl
                  / struct_decl / type_alias / function_decl
                  / protocol_decl / protocol_impl

derive_attr      <- "@derive" "[" type_path ("," type_path)* "]"
struct_decl      <- "defstruct" type_name type_params? when_clause?
                    "do" body(field_decl) "end"
field_decl       <- ident ":" type
type_alias       <- "@type" type_name type_params? "=" type
function_decl    <- ("def" / "defp") function_head "do"
                    body(block_item) "end"
function_head    <- ident "(" params? ")" return_type? when_clause?
protocol_decl    <- "defprotocol" type_name "do"
                    body(protocol_item) "end"
protocol_item    <- assoc_type_decl / protocol_signature
protocol_signature <- "def" function_head
protocol_impl    <- "defimpl" type_path "," "for" ":" type when_clause?
                    "do" body(implementation_item) "end"
implementation_item <- assoc_type_def / function_decl
assoc_type_decl  <- "type" type_name
assoc_type_def   <- "type" type_name "=" type

type_params      <- "(" type_var ("," type_var)* ")"
params           <- param ("," param)*
param            <- ident ":" type
return_type      <- "->" type
when_clause      <- "when" constraint ("," constraint)*
constraint       <- type_var ":" type_path

type             <- union_type
union_type       <- primary_type ("|" primary_type)*
primary_type     <- function_type / tuple_type / list_or_array_type
                  / atom / primitive_type / named_type / type_var
function_type    <- "(" (type ("," type)*)? ")" "->" type
tuple_type       <- "{" type "," type ("," type)* "}"
list_or_array_type <- "[" type (";" array_length)? "]"
named_type       <- type_path ("(" type ("," type)* ")")?
type_path        <- type_name ("." type_name)*

block_item       <- binding / assignment / return_expr / defer_expr
                  / while_expr / for_expr / expression
binding          <- "mut"? ident (":" type)? "=" expression
assignment       <- ident ("." ident)? ":=" expression
return_expr      <- "return" expression
defer_expr       <- "defer" (call_expression / ("do" body(block_item) "end"))
while_expr       <- "while" expression "do" body(block_item) "end"
for_expr         <- "for" pattern "in" expression
                    "do" body(block_item) "end"

expression       <- pipeline_expr
pipeline_expr    <- ascription_expr ("|>" ascription_expr)*
ascription_expr  <- logical_or_expr ("::" type)?
logical_or_expr  <- logical_and_expr ("or" logical_and_expr)*
logical_and_expr <- equality_expr ("and" equality_expr)*
equality_expr    <- comparison_expr (("==" / "!=") comparison_expr)?
comparison_expr  <- concat_expr (("<=" / ">=" / "<" / ">") concat_expr)?
concat_expr      <- bit_or_expr ("++" concat_expr)?
bit_or_expr      <- bit_xor_expr ("|" bit_xor_expr)*
bit_xor_expr     <- bit_and_expr ("^" bit_and_expr)*
bit_and_expr     <- shift_expr ("&" shift_expr)*
shift_expr       <- additive_expr (("<<" / ">>") additive_expr)*
additive_expr    <- multiplicative_expr (("+" / "-") multiplicative_expr)*
multiplicative_expr <- unary_expr (("*" / "/" / "%") unary_expr)*
unary_expr       <- ("-" / "!" / "~") unary_expr / postfix_expr
postfix_expr     <- primary_expr postfix_part*
postfix_part     <- call_arguments / ("." ident) / ("[" expression "]")
call_expression  <- primary_expr non_call_postfix* call_arguments postfix_part*
non_call_postfix <- "." ident / "[" expression "]"
call_arguments   <- "(" (expression ("," expression)*)? ")"

primary_expr     <- if_expr / match_expr / bitstring_expr / struct_literal
                  / map_literal / array_literal / list_literal / tuple_literal
                  / literal / qualified_value / "(" expression ")"
qualified_value  <- ident / (type_name ".")+ ident
if_expr          <- "if" expression "do" body(block_item)
                    ("else" body(block_item))? "end"
match_expr       <- "match" expression "do" NL* match_arm
                    (NL+ match_arm)* NL* "end"
match_arm        <- pattern "->" arm_body
tuple_literal    <- "{" expression "," expression
                    ("," expression)* "}"
list_literal     <- "[" (expression ("," expression)*
                    ("|" expression)?)? "]"
array_literal    <- "#[" (expression ("," expression)*)? "]"
map_literal      <- "%{" (map_entry ("," map_entry)*)? "}"
map_entry        <- expression "=>" expression
struct_literal   <- "%" type_path "{" (field_value ("," field_value)*)? "}"
field_value      <- ident ":" expression

pattern          <- typed_pattern / bitstring_pattern / struct_pattern
                  / tuple_pattern / list_pattern / pattern_literal / "_" / ident
typed_pattern    <- ident ":" type
pattern_literal  <- "-"? (float / integer) / string / rune / atom
                  / "true" / "false" / "unit"
tuple_pattern    <- "{" pattern "," pattern ("," pattern)* "}"
list_pattern     <- "[]" / "[" pattern "|" pattern "]"
struct_pattern   <- "%" type_path "{" (field_pattern
                    ("," field_pattern)*)? "}"
field_pattern    <- ident ":" pattern

bitstring_expr   <- "<<" (bit_expr_segment ("," bit_expr_segment)*)? ">>"
bit_expr_segment <- segment_expression "::" bit_modifiers
segment_expression <- logical_or_expr ("|>" logical_or_expr)*
bitstring_pattern <- "<<" (bit_pattern_segment
                    ("," bit_pattern_segment)*)? ">>"
bit_pattern_segment <- pattern "::" bit_modifiers
bit_modifiers    <- bit_modifier ("-" bit_modifier)*
bit_modifier     <- "integer" / "signed" / "unsigned" / "big" / "little"
                  / "native" / "bytes" / ("size" "(" expression ")")

literal          <- float / integer / string / rune / atom
                  / "true" / "false" / "unit"
primitive_type   <- "bool" / "i8" / "i16" / "i32" / "i64" / "isize"
                  / "u8" / "u16" / "u32" / "u64" / "usize"
                  / "f32" / "f64" / "rune" / "string" / "bytes"
                  / "bits" / "unit"
```

`ident` matches `[a-z][a-z0-9]*(?:_[a-z0-9]+)*`, and `type_name` matches
`[A-Z][A-Za-z0-9]*`; both are ASCII. `type_var` is an `ident` that is not a
keyword or primitive type. `module_name` is a `type_path`. `array_length` is a
decimal integer token satisfying the literal-length restriction in
[TYPES.md §5](TYPES.md#5-composite-types).
`integer`, `float`, `string`, `rune`, and `atom` are exactly the tokens defined
in section 2.4; keyword tokens require an identifier boundary. The lexer uses
longest-token matching for `::`, `:=`, `->`, `=>`, `==`, `!=`, `<=`, `>=`,
`<<`, `>>`, `++`, `|>`, and `#[` before their one-character prefixes or the
line-comment rule.

`arm_body` is a `body(block_item)` terminated by `end` or by the next
`pattern ->` header at the current match nesting depth. This boundary is
syntactic, not indentation-sensitive. The restrictions on bitstring modifier
combinations and widths are defined by
[DESIGN.md §7.4](DESIGN.md#74-bitstring-construction-and-matching). Restrictions
on pipeline right operands, assignment-target shape, and protocol-body contents
are grammar-validation rules and must be diagnosed before type checking. A
top-level bitstring segment expression excludes `::` so the
following `::` unambiguously begins its modifiers; an ascribed segment operand
can be parenthesized. Operator associativity and non-associativity are encoded
above and match [DESIGN.md §8.7](DESIGN.md#87-operators).
## 4. Grammar-validation rules

The grammar recognizes the outer shape of several constructs whose local
restrictions are clearer to diagnose in a validation pass. That pass runs after
parsing and before name resolution or type checking. It must reject:

- invalid bitstring modifier combinations and widths;
- a pipeline whose right operand is not a statically resolvable call;
- an assignment target whose syntax is not an identifier or one direct field;
- declarations that are not permitted in protocol or implementation bodies;
- chained comparison, equality, or ascription operators;
- semicolons and a leading operator that attempts to continue a complete prior
  line; and
- any recovery node remaining after parsing.

These are syntax diagnostics even when validation consults the parsed form.
Static facts such as whether an assignment root is actually mutable, whether a
typed pattern selects exactly one union member, or whether a `for` pattern is
irrefutable for its inferred item type are checked under
[TYPES.md](TYPES.md).

## 5. Conformance obligations

Parser conformance tests are derived from this document and include:

- one accepted and rejected fixture for every production and validation rule;
- precedence and associativity snapshots for every adjacent operator level;
- LF and CRLF cases plus significant-newline and continuation boundaries;
- longest-token conflicts such as `|`/`|>`, `:`/`::`/`:=`, and `#`/`#[`;
- valid and invalid literal boundaries, escapes, separators, and identifier
  forms;
- deterministic AST shape and source-span snapshots for accepted programs; and
- recovery tests proving that malformed input cannot produce a conforming AST.
