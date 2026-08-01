use el_ast::Value;
use el_parser::parse;
use el_span::{Location, SourceMap};

fn parsed(source: &str) -> (SourceMap, el_ast::Program) {
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.ell", source);
    let program = parse(file, source).expect("source parses");
    (sources, program)
}

#[test]
fn preserves_literal_spellings_and_decoded_values_without_integer_overflow() {
    let source = "defmodule Main do\n  def values() do\n    0xFF_FF_FF_FF_FF_FF_FF_FF_FF\n    1_2.5_0e+3\n    \"A\\x42\\n\\u{1f642}\"\n    \"\\\\\\\"\\'\\r\\t\\0\"\n    '\\u{03bb}'\n    :not_found\n    true\n    unit\n  end\nend\n";
    let (_, program) = parsed(source);
    let values = program
        .root
        .descendants()
        .filter_map(|node| node.value.as_ref())
        .collect::<Vec<_>>();

    assert!(values.contains(&&Value::Integer {
        spelling: "0xFF_FF_FF_FF_FF_FF_FF_FF_FF".to_owned(),
        radix: 16,
        digits: "FFFFFFFFFFFFFFFFFF".to_owned(),
    }));
    assert!(values.contains(&&Value::Float {
        spelling: "1_2.5_0e+3".to_owned(),
        normalized: "12.50e+3".to_owned(),
    }));
    assert!(values.contains(&&Value::String {
        spelling: "\"A\\x42\\n\\u{1f642}\"".to_owned(),
        decoded: "AB\n🙂".to_owned(),
    }));
    assert!(values.contains(&&Value::Rune {
        spelling: "'\\u{03bb}'".to_owned(),
        decoded: 'λ',
    }));
    assert!(values.contains(&&Value::Atom {
        spelling: ":not_found".to_owned(),
        name: "not_found".to_owned(),
    }));
    assert!(values.contains(&&Value::Boolean(true)));
    assert!(values.contains(&&Value::Unit));
}

#[test]
fn rejects_invalid_unicode_scalars_and_unrepresentable_array_lengths() {
    for fragment in [
        "\"\\u{d800}\"",
        "'\\u{110000}'",
        "value: [u8; 999999999999999999999999999999999999] = #[]",
    ] {
        let source = format!("defmodule Main do\n  def bad() do\n    {fragment}\n  end\nend\n");
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/main.ell", &source);
        assert!(parse(file, &source).is_err(), "accepted {fragment}");
    }
}

#[test]
fn every_ast_span_is_a_utf8_boundary_with_crlf_and_multibyte_text() {
    let source = "defmodule Main do\r\n  def text() do\r\n    \"λ🙂\"\r\n  end\r\nend\r\n";
    let (sources, program) = parsed(source);

    for node in program.root.descendants() {
        sources.verify_span(node.span).expect("AST span is valid");
    }
    let literal = program
        .root
        .descendants()
        .find(|node| node.kind.as_str() == "string")
        .expect("string node");
    assert_eq!(
        program.debug_tree(),
        include_str!("snapshots/multibyte_crlf.ast")
    );
    assert_eq!(
        sources.location(literal.span.file(), literal.span.start()),
        Ok(Location { line: 3, column: 5 })
    );
}

#[test]
fn precedence_and_associativity_are_explicit_and_deterministic() {
    let source = "defmodule Main do\n  def operators() do\n    a or b and c == d < e ++ f | g ^ h & i << j + k * l\n    a - b - c\n    a ++ b ++ c\n  end\nend\n";
    let (_, first) = parsed(source);
    let (_, second) = parsed(source);
    assert_eq!(first.debug_tree(), second.debug_tree());

    let precedence = first
        .root
        .descendants()
        .find(|node| node.kind.as_str() == "logical_or_expr")
        .expect("full precedence expression");
    assert_eq!(
        precedence.debug_tree(),
        include_str!("snapshots/precedence.ast")
    );

    let subtract = first
        .root
        .descendants()
        .find(|node| {
            node.kind.as_str() == "additive_expr" && node.value == Some(Value::Text("-".to_owned()))
        })
        .expect("subtraction tree");
    assert_eq!(subtract.children[0].kind.as_str(), "additive_expr");

    let concat = first
        .root
        .descendants()
        .find(|node| {
            node.kind.as_str() == "concat_expr"
                && node
                    .children
                    .get(1)
                    .is_some_and(|right| right.kind.as_str() == "concat_expr")
        })
        .expect("concatenation tree");
    assert_eq!(concat.children[1].kind.as_str(), "concat_expr");
}

#[test]
fn newlines_are_soft_only_at_normative_continuation_points() {
    let accepted = [
        "defmodule Main do\n  def x() do\n    (1\n      + 2)\n  end\nend\n",
        "defmodule Main do\n  def x() do\n    [1,\n      2]\n  end\nend\n",
        "defmodule Main do\n  # comment only\n  def x() do\n    1 +\n      2\n  end\nend\n",
    ];
    for source in accepted {
        parsed(source);
    }

    for source in [
        "defmodule Main do\n  def x() do\n    1\n    + 2\n  end\nend\n",
        "defmodule Main do\n  def x() do\n    1 2\n  end\nend\n",
    ] {
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/main.ell", source);
        assert!(parse(file, source).is_err());
    }
}

#[test]
fn rejects_private_or_malformed_protocol_implementation_members() {
    for source in [
        "defmodule Main do\n  defprotocol P do\n    value = 1\n  end\nend\n",
        "defmodule Main do\n  defimpl P, for: T do\n    defp hidden() do\n      unit\n    end\n  end\nend\n",
    ] {
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/main.ell", source);
        assert!(parse(file, source).is_err());
    }
}

#[test]
fn conforming_ast_omits_peg_wrappers_punctuation_and_recovery_nodes() {
    let source =
        "defmodule Main do\n  def value(input: i32) -> i32 do\n    input + 1\n  end\nend\n";
    let (_, program) = parsed(source);
    let parser_only = [
        "module_body",
        "module_item",
        "function_head",
        "parameters",
        "block_item",
        "expression",
        "primary_expr",
        "literal",
        "additive_operator",
        "arrow",
        "EOI",
    ];

    for node in program.root.descendants() {
        let kind = node.kind.as_str();
        assert!(!parser_only.contains(&kind), "parser noise: {kind}");
        assert!(!kind.starts_with("kw_"), "keyword punctuation: {kind}");
        assert!(!kind.starts_with("recovery_"), "recovery node: {kind}");
    }
}
