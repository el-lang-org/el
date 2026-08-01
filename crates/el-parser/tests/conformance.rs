use el_parser::{parse, parse_bytes, parse_recovering};
use el_span::SourceMap;

fn parses(source: &str) -> bool {
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/all.ell", source);
    parse(file, source).is_ok()
}

fn assert_parses(source: &str) {
    if !parses(source) {
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/all.ell", source);
        panic!("did not parse: {:?}\n{source}", parse(file, source));
    }
}

fn assert_rejected(source: &str) {
    assert!(!parses(source), "unexpectedly parsed {source:?}");
}

#[test]
fn accepts_declarations_generics_constraints_and_all_type_families() {
    assert_parses(
        "defmodule Complete.Syntax do\n\
         @derive [Eq, Show]\n\
         defstruct Box(a) when a: Show do\n\
           value: a\n\
         end\n\
         @type Pair(a) = {a, a}\n\
         defprotocol Container do\n\
           type Item\n\
           def get(value: Self) -> Item\n\
         end\n\
         defimpl Container, for: Box(a) when a: Show do\n\
           type Item = a\n\
           def get(value: Box(a)) -> a do\n\
             value.value\n\
           end\n\
         end\n\
         defp types(callback: (i32, i64) -> bool, pair: {i32, i64},\n\
                    list: [i32], array: [u8; 16], named: Map(string, i32),\n\
                    projected: Reader.Error(r), result: :ok | {:error, string}) -> unit do\n\
           unit\n\
         end\n\
         end\n",
    );
}

#[test]
fn accepts_literals_collections_postfix_and_every_operator_level() {
    assert_parses(
        "defmodule Main do\n\
         def literals() do\n\
           0b1010\n\
           0o17\n\
           0xCA_FE\n\
           1_000\n\
           1.5e-3\n\
           \"line\\n\\u{1f642}\"\n\
           '\\u{03bb}'\n\
           :not_found\n\
           true\n\
           false\n\
           unit\n\
           {1, 2}\n\
           [1, 2 | tail]\n\
           #[1, 2]\n\
           %{\"one\" => 1, \"two\" => 2}\n\
           %Point{x: 1, y: 2}\n\
           Module.call(1, 2).field[0]\n\
           !a or b and c == d < e ++ f | g ^ h & i << j + k * l\n\
           value :: i32\n\
           input |> Module.call(1)\n\
         end\n\
         end\n",
    );
}

#[test]
fn accepts_bindings_control_flow_patterns_defer_and_bitstrings() {
    assert_parses(
        "defmodule Main do\n\
         def flow(items: [i32]) -> i32 do\n\
           value = 0\n\
           mut count: i32 = 1\n\
           count := count + 1\n\
           point.x := 2\n\
           defer IO.flush()\n\
           defer do\n\
             IO.close()\n\
           end\n\
           while count < 10 do\n\
             count := count + 1\n\
           end\n\
           for {head, tail} in items do\n\
             head\n\
           end\n\
           if count > 0 do\n\
             1\n\
           else\n\
             2\n\
           end\n\
           <<count::unsigned-big-size(16), payload::bytes>>\n\
           match payload do\n\
             <<size::unsigned-big-size(8), rest::bytes>> -> size\n\
             %Point{x: x} -> x\n\
             [head | tail] -> head\n\
             value: i32 -> value\n\
             -1 -> 0\n\
             _ -> return count\n\
           end\n\
         end\n\
         end\n",
    );
}

#[test]
fn validates_non_associative_operators_pipeline_assignments_and_bitstrings() {
    for body in [
        "a == b == c",
        "a < b < c",
        "a :: i32 :: i64",
        "a |> value",
        "a |> Module.call() + value",
        "a.b.c := value",
        "a[0] := value",
        "<<x::signed-unsigned-size(8)>>",
        "<<x::unsigned-size(7)>>",
        "match data do\n  <<head::bytes, tail::bytes>> -> head\nend",
    ] {
        let source = format!("defmodule Main do\n  def bad() do\n    {body}\n  end\nend\n");
        assert_rejected(&source);
    }
}

#[test]
fn accepts_empty_and_multiline_match_arm_bodies() {
    assert_parses(
        "defmodule Main do\n\
         def arms(value: i32) do\n\
           match value do\n\
             0 ->\n\
             1 ->\n\
               first()\n\
               second()\n\
             _ -> unit\n\
           end\n\
         end\n\
         end\n",
    );
}

#[test]
fn rejects_invalid_source_tokens_and_decodes_utf8_at_the_boundary() {
    for body in ["1_", "1__0", "0b2", "camelCase", "\"bad\\q\"", "''", "'ab'"] {
        let source = format!("defmodule Main do\n  def bad() do\n    {body}\n  end\nend\n");
        assert_rejected(&source);
    }

    let mut sources = SourceMap::new();
    let file = sources.add_file("src/bad.ell", "");
    assert!(parse_bytes(file, b"\xff").is_err());
}

#[test]
fn recovery_collects_boundary_errors_but_never_returns_a_conforming_ast() {
    let source = "defmodule Main do\n  nonsense ???\n  def bad() do\n    @broken\n  end\nend\n";
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.ell", source);
    let outcome = parse_recovering(file, source);

    assert!(outcome.program.is_none());
    assert!(outcome.diagnostics.len() >= 2, "{outcome:?}");
}

#[test]
fn longest_tokens_and_newline_continuations_are_unambiguous() {
    assert_parses(
        "defmodule Main do\n\
         def tokens(x: i32) -> i32 do\n\
           mut value: i32 = #[1, 2][0]\n\
           value := x\n\
           value = value |> Main.id()\n\
           value = value :: i32\n\
           value = value << 1 >= 2\n\
           value = [1 | tail]\n\
           value = 1 ++\n\
             other\n\
           value\n\
         end\n\
         def id(value: i32) -> i32 do\n\
           value\n\
         end\n\
         end\n",
    );
}

#[test]
fn accepts_every_primitive_operator_spelling_and_integer_bit_width() {
    let primitives = [
        "bool", "i8", "i16", "i32", "i64", "isize", "u8", "u16", "u32", "u64", "usize", "f32",
        "f64", "rune", "string", "bytes", "bits", "unit",
    ];
    for primitive in primitives {
        let source = format!(
            "defmodule Main do\n  def value(input: {primitive}) -> {primitive} do\n    input\n  end\nend\n"
        );
        assert_parses(&source);
    }

    for expression in [
        "a + b", "a - b", "a * b", "a / b", "a % b", "a << b", "a >> b", "a & b", "a ^ b", "a | b",
        "a ++ b", "a < b", "a <= b", "a > b", "a >= b", "a == b", "a != b", "a and b", "a or b",
        "-a", "!a", "~a",
    ] {
        let source = format!("defmodule Main do\n  def op() do\n    {expression}\n  end\nend\n");
        assert_parses(&source);
    }

    for width in [8, 16, 24, 32, 40, 48, 56, 64] {
        for modifier in ["signed-big", "unsigned-little", "integer-native"] {
            let source = format!(
                "defmodule Main do\n  def bits(value: i64) do\n    <<value::{modifier}-size({width})>>\n  end\nend\n"
            );
            assert_parses(&source);
        }
    }
}

#[test]
fn rejects_token_boundaries_reserved_names_and_invalid_bitstring_combinations() {
    for body in [
        "0b", "0b102", "0o8", "0xG", "1e", "1._0", "1e_2", ":bad_", ":Bad", "@derive", "def", "if",
        "while",
    ] {
        let source = format!("defmodule Main do\n  def bad() do\n    {body}\n  end\nend\n");
        assert_rejected(&source);
    }

    for segment in [
        "x::size(8 + 0)",
        "x::integer",
        "x::bytes-signed",
        "x::big-little-size(8)",
        "x::integer-integer-size(8)",
        "x::bytes-size(1)-size(2)",
    ] {
        let source = format!("defmodule Main do\n  def bad() do\n    <<{segment}>>\n  end\nend\n");
        assert_rejected(&source);
    }
}

#[test]
fn comma_continuation_works_outside_delimiters() {
    assert_parses(
        "defmodule Main do\n\
         defimpl Show,\n\
           for: Point do\n\
           def show(value: Point) -> string do\n\
             \"point\"\n\
           end\n\
         end\n\
         end\n",
    );
}

#[test]
fn rejects_malformed_declaration_type_expression_pattern_and_control_families() {
    let modules = [
        "defmodule Main do\nend\ndefmodule Other do\nend\n",
        "defmodule Main do\n  @derive [Eq]\n  def value() do\n    unit\n  end\nend\n",
        "defmodule Main do\n  defstruct Point do\n    x i32\n  end\nend\n",
        "defmodule Main do\n  @type Pair(a) = {a}\nend\n",
        "defmodule Main do\n  def bad(value i32) do\n    unit\n  end\nend\n",
        "defmodule Main do\n  defprotocol P do\n    def value() do\n      unit\n    end\n  end\nend\n",
        "defmodule Main do\n  defimpl P for: T do\n  end\nend\n",
        "defmodule Main do\n  @type Bad(a,) = a\nend\n",
        "defmodule Main do\n  def bad(value: i32 |) do\n    unit\n  end\nend\n",
        "defmodule Main do\n  def bad(value: [i32; length]) do\n    unit\n  end\nend\n",
    ];
    for source in modules {
        assert_rejected(source);
    }

    let bodies = [
        "mut = 1",
        "return",
        "defer value",
        "while true\n  unit\nend",
        "for value items do\n  unit\nend",
        "if true do\n  unit",
        "match value do\nend",
        "{1}",
        "[1,]",
        "#[1,]",
        "%{1: 2}",
        "%Point{x 1}",
        "call(1,)",
        "value[]",
        "a +",
        "match value do\n  {x} -> x\nend",
        "match value do\n  [x] -> x\nend",
        "match value do\n  %Point{x} -> x\nend",
        "<<value>>",
    ];
    for body in bodies {
        let source = format!("defmodule Main do\n  def bad() do\n    {body}\n  end\nend\n");
        assert_rejected(&source);
    }
}
