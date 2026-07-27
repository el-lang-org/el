use el_parser::parse;
use el_resolve::resolve;
use el_span::SourceMap;
use el_types::{Type, TypeId, TypedExprKind, check, verify};

fn checked(source: &str) -> Result<el_types::TypedProgram, Vec<el_span::Diagnostic>> {
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.el", source);
    let parsed = parse(file, source).expect("fixture parses");
    let resolved = resolve(&parsed).expect("fixture resolves");
    check(&resolved)
}

#[test]
fn checks_bindings_mutation_calls_returns_and_generic_inference() {
    let source = "defmodule Main do\n  def identity(value: a) -> a do\n    value\n  end\n  def main() -> i32 do\n    mut answer: i32 = identity(40)\n    answer := answer + 2\n    answer\n  end\nend\n";

    let typed = checked(source).expect("program type checks");

    assert_eq!(typed.functions[1].result, TypeId(0));
    assert!(typed.debug_tree().contains("call d0 [a=i32]: i32"));
    assert!(typed.debug_tree().contains("assign s1"));
}

#[test]
fn expected_result_type_infers_a_zero_argument_generic_call() {
    let source = "defmodule Main do\n  def loop() -> a do\n    loop()\n  end\n  def main() -> i32 do\n    value: i32 = loop()\n    value\n  end\nend\n";

    let typed = checked(source).expect("expected result determines generic parameter");
    let el_types::TypedItem::Let { initializer, .. } = &typed.functions[1].body.items[0] else {
        panic!("binding")
    };
    assert!(matches!(initializer.kind, TypedExprKind::Call { .. }));
    assert_eq!(initializer.ty, TypeId(0));
}

#[test]
fn rejects_immutable_assignment_type_mismatch_and_unknown_names() {
    let cases = [
        ("x = 1\n    x := 2\n    x", "E2104"),
        ("mut x: i32 = 1\n    x := true\n    x", "E2113"),
        ("missing", "E2107"),
    ];
    for (body, code) in cases {
        let source = format!("defmodule Main do\n  def main() -> i32 do\n    {body}\n  end\nend\n");
        let diagnostics = checked(&source).expect_err("program is rejected");
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.code == code),
            "missing {code}: {diagnostics:?}"
        );
    }
}

#[test]
fn rejects_an_ambiguous_generic_call_without_expected_type() {
    let source = "defmodule Main do\n  def loop() -> a do\n    loop()\n  end\n  def main() -> i64 do\n    loop()\n    0\n  end\nend\n";

    let diagnostics = checked(source).expect_err("generic call is ambiguous");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2112")
    );
}

#[test]
fn typed_ast_verifier_rejects_unknown_type_ids() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    0\n  end\nend\n";
    let mut typed = checked(source).expect("program type checks");
    typed.functions[0].result = TypeId(999);

    assert!(
        verify(&typed)
            .expect_err("malformed Typed AST is rejected")
            .iter()
            .any(|error| error.contains("unknown type"))
    );
}

#[test]
fn expands_aliases_normalizes_unions_and_inserts_expected_injections() {
    let source = "defmodule Main do\n  @type First = i64 | bool\n  @type Scalar = bool | First | bool\n  def main() -> Scalar do\n    true\n  end\nend\n";

    let typed = checked(source).expect("disjoint alias union type checks");
    let debug = typed.debug_tree();

    assert!(debug.contains("function d2 main -> bool | i64"));
    assert!(debug.contains("inject bool: bool | i64"));
}

#[test]
fn rejects_generic_union_overlap_with_a_witness() {
    let source = "defmodule Main do\n  @type Either(a, b) = a | b\n  def main() -> i32 do\n    0\n  end\nend\n";

    let diagnostics = checked(source).expect_err("potential overlap is rejected");
    let overlap = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "E2115")
        .expect("overlap diagnostic");

    assert!(
        overlap
            .notes
            .iter()
            .any(|note| note.contains("overlap witness"))
    );
}

#[test]
fn rejects_an_integer_literal_ambiguous_between_union_members() {
    let source = "defmodule Main do\n  @type Integer = i32 | i64\n  def main() -> Integer do\n    1\n  end\nend\n";

    let diagnostics = checked(source).expect_err("integer member selection is ambiguous");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2116")
    );
}

#[test]
fn typed_ast_verifier_rejects_noncanonical_unions() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    0\n  end\nend\n";
    let mut typed = checked(source).expect("program type checks");
    typed.types.push(Type::Union(vec![TypeId(0), TypeId(0)]));

    assert!(
        verify(&typed)
            .expect_err("duplicate union members are rejected")
            .iter()
            .any(|error| error.contains("duplicate member"))
    );
}

#[test]
fn checks_expected_and_inferred_lists_including_an_improper_tail() {
    let source = "defmodule Main do\n  def identity(values: [a]) -> [a] do\n    values\n  end\n  def prepend(values: [i32]) -> [i32] do\n    [1 | values]\n  end\n  def main() -> [i32] do\n    identity([])\n  end\nend\n";

    let typed = checked(source).expect("list item types flow through expectations and calls");
    let debug = typed.debug_tree();

    assert!(debug.contains("function d2 main -> [i32]"));
    assert!(debug.contains("call d0 [a=i32]: [i32]"));
}

#[test]
fn rejects_an_empty_list_without_an_expected_item_type() {
    let source = "defmodule Main do\n  def main() -> unit do\n    []\n    unit\n  end\nend\n";

    let diagnostics = checked(source).expect_err("empty list item type is ambiguous");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2119")
    );
}

#[test]
fn recursively_unifies_composite_union_members_with_an_occurs_check() {
    let accepted = "defmodule Main do\n  @type RecursiveChoice(a) = a | [a]\n  def main() -> i32 do\n    0\n  end\nend\n";
    checked(accepted).expect("the occurs check rejects the infinite overlap substitution");

    let rejected = "defmodule Main do\n  @type Bad(a) = [a] | [i64]\n  def main() -> i32 do\n    0\n  end\nend\n";
    let diagnostics = checked(rejected).expect_err("finite list overlap is rejected");
    let overlap = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "E2115")
        .expect("overlap diagnostic");
    assert!(overlap.notes.iter().any(|note| note.contains("a = i64")));
}

#[test]
fn checks_tagged_tuple_unions_and_inserts_the_member() {
    let source = "defmodule Main do\n  @type Option(a) = {:some, a} | :none\n  def main() -> Option(i64) do\n    {:some, 1}\n  end\nend\n";

    let typed = checked(source).expect("tagged tuple alternatives are disjoint");

    assert!(typed.debug_tree().contains("inject {:some, i64}"));
}

#[test]
fn forms_nominal_generic_struct_types_and_unifies_their_arguments() {
    let source = "defmodule Main do\n  defstruct Box(a) do\n    value: a\n  end\n  def identity(box: Box(a)) -> Box(a) do\n    box\n  end\n  def keep(box: Box(i64)) -> Box(i64) do\n    identity(box)\n  end\nend\n";

    let typed = checked(source).expect("nominal struct applications type check");

    assert_eq!(typed.structs[0].name, "Box");
    assert!(typed.debug_tree().contains("call d1 [a=i64]: Box(i64)"));
}

#[test]
fn rejects_overlapping_nominal_struct_union_applications() {
    let source = "defmodule Main do\n  defstruct Box(a) do\n    value: a\n  end\n  @type Bad(a) = Box(a) | Box(i64)\nend\n";

    let diagnostics = checked(source).expect_err("nominal applications overlap");
    let overlap = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "E2115")
        .expect("overlap diagnostic");
    assert!(overlap.notes.iter().any(|note| note.contains("a = i64")));
}

#[test]
fn validates_concrete_and_recursive_generic_constraints() {
    let source = "defmodule Main do\n  def constrained(value: a) -> a when a: Eq do\n    value\n  end\n  def forward(value: b) -> b when b: Eq do\n    constrained(value)\n  end\n  def main() -> i64 do\n    forward(1)\n  end\nend\n";

    checked(source).expect("primitive and justified generic constraints hold");
}

#[test]
fn rejects_an_unsatisfied_concrete_constraint() {
    let source = "defmodule Main do\n  defstruct Box do\n    value: i64\n  end\n  def constrained(value: a) -> a when a: Eq do\n    value\n  end\n  def bad(value: Box) -> Box do\n    constrained(value)\n  end\nend\n";

    let diagnostics = checked(source).expect_err("Box has no Eq implementation");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2120")
    );
}

#[test]
fn checks_ascriptions_and_same_module_qualified_calls() {
    let source = "defmodule Main do\n  def value() -> [i32] do\n    [] :: [i32]\n  end\n  def main() -> [i32] do\n    value = Main.value()\n    Main.value()\n  end\nend\n";

    let typed = checked(source).expect("qualification bypasses a shadowing local");

    assert!(typed.debug_tree().contains("ascription: [i32]"));
}

#[test]
fn a_local_binding_shadows_a_bare_function_after_its_initializer() {
    let source = "defmodule Main do\n  def value() -> i64 do\n    1\n  end\n  def main() -> i64 do\n    value = value()\n    value()\n  end\nend\n";

    let diagnostics = checked(source).expect_err("bare name resolves to the local after binding");

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2109")
    );
}

#[test]
fn checks_arrays_maps_and_contextual_empty_forms() {
    let source = "defmodule Main do\n  def arrays() -> [i64; 2] do\n    #[1, 2]\n  end\n  def empty_array() -> [i32; 0] do\n    #[]\n  end\n  def maps() -> Map(i64, bool) do\n    %{1 => true, 2 => false}\n  end\n  def empty_map() -> Map(i32, i64) do\n    %{}\n  end\nend\n";

    let typed = checked(source).expect("array and map construction type checks");
    let debug = typed.debug_tree();
    assert!(debug.contains("array: [i64; 2]"));
    assert!(debug.contains("array: [i32; 0]"));
    assert!(debug.contains("map: Map(i64, bool)"));
}

#[test]
fn rejects_ambiguous_empty_arrays_and_maps() {
    for (literal, code) in [("#[]", "E2121"), ("%{}", "E2122")] {
        let source = format!(
            "defmodule Main do\n  def main() -> unit do\n    {literal}\n    unit\n  end\nend\n"
        );
        let diagnostics = checked(&source).expect_err("empty collection needs context");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.code == code));
    }
}

#[test]
fn nested_if_scopes_shadow_without_leaking() {
    let accepted = "defmodule Main do\n  def main(flag: bool) -> i64 do\n    value = 1\n    if flag do\n      value = 2\n      value\n    else\n      value\n    end\n  end\nend\n";
    checked(accepted).expect("a nested scope may shadow an outer local");

    let rejected = "defmodule Main do\n  def main(flag: bool) -> i64 do\n    if flag do\n      inner = 1\n      inner\n    else\n      0\n    end\n    inner\n  end\nend\n";
    let diagnostics = checked(rejected).expect_err("branch bindings do not leak");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2107")
    );
}

#[test]
fn records_pattern_reachability_and_exhaustiveness() {
    let source = "defmodule Main do\n  def choose(flag: bool) -> i64 do\n    match flag do\n      true -> 1\n      false -> 2\n    end\n  end\nend\n";
    let typed = checked(source).expect("both bool cases are exhaustive");
    let debug = typed.debug_tree();
    assert!(debug.contains("match exhaustive=true"));
    assert!(debug.contains("reachable=true irrefutable=false"));

    let missing = source.replace("      false -> 2\n", "");
    assert!(
        checked(&missing)
            .expect_err("missing false case")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2126")
    );

    let unreachable = source.replace("      false -> 2", "      _ -> 2\n      false -> 3");
    assert!(
        checked(&unreachable)
            .expect_err("arm after wildcard")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2125")
    );
}

#[test]
fn typed_union_patterns_bind_members_and_cover_the_union() {
    let source = "defmodule Main do\n  @type Scalar = bool | i64\n  def choose(value: Scalar) -> i64 do\n    match value do\n      number: i64 -> number\n      flag: bool -> 0\n    end\n  end\nend\n";
    let typed = checked(source).expect("typed members exhaust the normalized union");
    let debug = typed.debug_tree();
    assert!(debug.contains("UnionMember"));
    assert!(debug.contains("match exhaustive=true"));
}

#[test]
fn checks_structural_tuple_list_and_struct_pattern_matrices() {
    let source = "defmodule Main do\n  defstruct Point do\n    flag: bool\n    value: i64\n  end\n  def tuple(value: {bool, i64}) -> i64 do\n    match value do\n      {true, number} -> number\n      {false, _} -> 0\n    end\n  end\n  def list(value: [i64]) -> i64 do\n    match value do\n      [] -> 0\n      [head | _] -> head\n    end\n  end\n  def structure(value: Point) -> i64 do\n    match value do\n      %Point{flag: true, value: number} -> number\n      %Point{flag: false} -> 0\n    end\n  end\nend\n";

    let typed = checked(source).expect("structural patterns are exhaustive");
    let debug = typed.debug_tree();
    assert!(debug.contains("Tuple"));
    assert!(debug.contains("ListCons"));
    assert!(debug.contains("Struct"));
}

#[test]
fn rejects_non_exhaustive_nested_patterns_and_duplicate_bindings() {
    let missing = "defmodule Main do\n  def choose(value: {bool, i64}) -> i64 do\n    match value do\n      {true, _} -> 1\n    end\n  end\nend\n";
    assert!(
        checked(missing)
            .expect_err("false tuple case is missing")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2126")
    );

    let duplicate = "defmodule Main do\n  def choose(value: {i64, i64}) -> i64 do\n    match value do\n      {item, item} -> item\n    end\n  end\nend\n";
    let diagnostics = checked(duplicate).expect_err("one pattern cannot bind a name twice");
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "E2130")
        .expect("duplicate binding diagnostic");
    assert_eq!(
        diagnostic.primary.start(),
        duplicate.rfind("item}").unwrap()
    );
}

#[test]
fn structural_pattern_rejections_point_at_the_offending_source() {
    let cases = [
        (
            "defmodule Main do\n  def choose(value: {bool, i64}) -> i64 do\n    match value do\n      {true, _, _} -> 1\n    end\n  end\nend\n",
            "E2132",
            "{true, _, _}",
        ),
        (
            "defmodule Main do\n  def choose(value: i64) -> i64 do\n    match value do\n      [] -> 0\n    end\n  end\nend\n",
            "E2133",
            "[]",
        ),
        (
            "defmodule Main do\n  def choose(value: i64) -> i64 do\n    match value do\n      %Point{} -> 0\n    end\n  end\nend\n",
            "E2134",
            "%Point{}",
        ),
        (
            "defmodule Main do\n  defstruct Point do\n    value: i64\n  end\n  defstruct Other do\n    value: i64\n  end\n  def choose(value: Point) -> i64 do\n    match value do\n      %Other{} -> 0\n    end\n  end\nend\n",
            "E2135",
            "Other",
        ),
        (
            "defmodule Main do\n  defstruct Point do\n    value: i64\n  end\n  def choose(value: Point) -> i64 do\n    match value do\n      %Point{value: _, value: _} -> 0\n    end\n  end\nend\n",
            "E2136",
            "value: _}",
        ),
        (
            "defmodule Main do\n  defstruct Point do\n    value: i64\n  end\n  def choose(value: Point) -> i64 do\n    match value do\n      %Point{missing: _} -> 0\n    end\n  end\nend\n",
            "E2137",
            "missing",
        ),
    ];

    for (source, code, offending) in cases {
        let diagnostics = checked(source).expect_err("structural pattern is rejected");
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == code)
            .unwrap_or_else(|| panic!("missing {code}: {diagnostics:?}"));
        assert_eq!(diagnostic.primary.start(), source.rfind(offending).unwrap());
    }
}
