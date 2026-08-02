use el_parser::parse;
use el_span::SourceMap;

const TYPED_MAIN: &str = "defmodule Main do\n  def main() -> i32 do\n    40 + 2\n  end\nend\n";

#[test]
fn typed_main_ast_matches_snapshot() {
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.ell", TYPED_MAIN);
    let program = parse(file, TYPED_MAIN).expect("typed main parses");
    let actual = program.debug_tree();
    let expected = include_str!("snapshots/typed_main.ast");

    assert_eq!(actual, expected);
    sources
        .verify_span(program.span())
        .expect("program span is valid");
}

#[test]
fn accepts_crlf_and_operator_continuation() {
    let source =
        "defmodule Main do\r\n  def main() -> i32 do\r\n    40 +\r\n      2\r\n  end\r\nend\r\n";
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.ell", source);

    assert!(parse(file, source).is_ok());
}

#[test]
fn rejects_bom_bare_carriage_return_semicolon_and_leading_operator() {
    let cases = [
        "\u{feff}defmodule Main do\nend\n",
        "defmodule Main do\rend\n",
        "defmodule Main do\n  def main() -> i32 do\n    1; 2\n  end\nend\n",
        "defmodule Main do\n  def main() -> i32 do\n    1\n    + 2\n  end\nend\n",
    ];

    for source in cases {
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/main.ell", source);
        assert!(parse(file, source).is_err(), "accepted {source:?}");
    }
}

#[test]
fn rejects_invalid_integer_separators_and_identifiers() {
    for expression in ["1_", "1__0", "_value", "camelCase"] {
        let source =
            format!("defmodule Main do\n  def main() -> i32 do\n    {expression}\n  end\nend\n");
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/main.ell", &source);
        assert!(parse(file, &source).is_err(), "accepted {expression}");
    }
}

#[test]
fn parses_interpolated_strings() {
    let source = "defmodule Main do\n  def message(value: i64) -> string do\n    \"value: #{value}\"\n  end\nend\n";
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.ell", source);
    let program = parse(file, source).expect("interpolation parses");
    assert!(program.debug_tree().contains("interpolation"));

    let escaped =
        "defmodule Main do\n  def message() -> string do\n    \"literal \\#{value}\"\n  end\nend\n";
    let file = sources.add_file("src/escaped.ell", escaped);
    let program = parse(file, escaped).expect("escaped interpolation marker parses");
    assert!(program.debug_tree().contains("escaped_interpolation"));
}

#[test]
fn parses_with_clauses_and_body() {
    let source = "defmodule Main do\n  def validate() -> :ok | :error do\n    with :ok <- first(),\n         {:ok, value} <- second() do\n      value\n    end\n  end\nend\n";
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.ell", source);
    let program = parse(file, source).expect("with expression parses");
    let tree = program.debug_tree();

    assert!(tree.contains("with_expr"), "{tree}");
    assert_eq!(tree.matches("with_clause").count(), 2, "{tree}");
}
