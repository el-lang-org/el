use el_parser::parse;
use el_resolve::{resolve, resolve_package};
use el_span::SourceMap;
use el_types::{Type, TypeId, TypedExprKind, check, verify};

fn checked(source: &str) -> Result<el_types::TypedProgram, Vec<el_span::Diagnostic>> {
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.ell", source);
    let parsed = parse(file, source).expect("fixture parses");
    let resolved = resolve(&parsed).expect("fixture resolves");
    check(&resolved)
}

fn checked_package(sources: &[&str]) -> Result<el_types::TypedProgram, Vec<el_span::Diagnostic>> {
    let mut source_map = SourceMap::new();
    let parsed = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let file = source_map.add_file(format!("src/{index}.ell"), *source);
            parse(file, source).expect("fixture parses")
        })
        .collect::<Vec<_>>();
    let resolved = resolve_package(&parsed).expect("fixture resolves");
    el_types::check_package(&resolved)
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
fn checks_exact_named_function_values_and_indirect_calls() {
    let source = "defmodule Main do\n  def identity(value: a) -> a do\n    value\n  end\n  defp increment(value: i32) -> i32 do\n    value + 1\n  end\n  def private_value() -> (i32) -> i32 do\n    increment\n  end\n  def apply(function: (i32) -> i32, value: i32) -> i32 do\n    function(value)\n  end\n  def main() -> i32 do\n    chosen: (i32) -> i32 = identity\n    apply(chosen, private_value()(41)) - 41\n  end\nend\n";

    let typed = checked(source).expect("named function values type check");
    let debug = typed.debug_tree();
    assert!(
        debug.contains("function d0 [a=i32]: (i32) -> i32"),
        "{debug}"
    );
    assert!(debug.contains("function d1 []: (i32) -> i32"), "{debug}");
    assert!(debug.contains("indirect call: i32"), "{debug}");
    verify(&typed).expect("function-value Typed AST verifies");
}

#[test]
fn rejects_ambiguous_or_inexact_function_values() {
    let ambiguous = "defmodule Main do\n  def identity(value: a) -> a do\n    value\n  end\n  def value() -> unit do\n    identity\n    unit\n  end\nend\n";
    assert!(
        checked(ambiguous)
            .expect_err("generic function values require a concrete expected type")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2112")
    );

    let inexact = "defmodule Main do\n  def convert(value: i64) -> i64 do\n    value\n  end\n  def main() -> i32 do\n    function: (i32) -> i32 = convert\n    function(1)\n  end\nend\n";
    assert!(
        checked(inexact)
            .expect_err("function parameter and result types are exact")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2113")
    );
}

#[test]
fn function_value_visibility_is_checked_when_named_not_when_called() {
    let secrets = "defmodule Secrets do\n  defp hidden(value: i32) -> i32 do\n    value + 1\n  end\n  def expose() -> (i32) -> i32 do\n    hidden\n  end\nend\n";
    let main = "defmodule Main do\n  def main() -> i32 do\n    Secrets.expose()(41)\n  end\nend\n";
    checked_package(&[secrets, main]).expect("a returned private function value remains callable");

    let invalid = "defmodule Main do\n  def main() -> i32 do\n    function: (i32) -> i32 = Secrets.hidden\n    function(41)\n  end\nend\n";
    assert!(
        checked_package(&[secrets, invalid])
            .expect_err("external source cannot directly name a private function")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2138")
    );
}

#[test]
fn checks_generic_struct_construction_projection_and_mutable_root_update() {
    let source = "defmodule Main do\n  defstruct Pair(a) do\n    first: a\n    second: i32\n  end\n  def main() -> i32 do\n    mut pair: Pair(i32) = %Pair{second: 2, first: 40}\n    copy = pair\n    pair.second := pair.second + copy.second\n    pair.first + pair.second\n  end\nend\n";

    let typed = checked(source).expect("struct expressions type check");
    let debug = typed.debug_tree();
    assert!(debug.contains("struct d0: Pair(i32)"));
    assert!(debug.contains("struct project d0 .1: i32"));
    assert!(debug.contains("field assign"));
    verify(&typed).expect("struct Typed AST verifies");
}

#[test]
fn rejects_incomplete_duplicate_unknown_and_immutable_struct_updates() {
    let cases = [
        ("value = %Pair{first: 1}\n    value.first", "E2148"),
        (
            "value = %Pair{first: 1, first: 2, second: 3}\n    value.first",
            "E2147",
        ),
        (
            "value = %Pair{first: 1, missing: 2, second: 3}\n    value.first",
            "E2144",
        ),
        (
            "value = %Pair{first: 1, second: 2}\n    value.first := 3\n    value.first",
            "E2104",
        ),
    ];
    for (body, code) in cases {
        let source = format!(
            "defmodule Main do\n  defstruct Pair do\n    first: i32\n    second: i32\n  end\n  def main() -> i32 do\n    {body}\n  end\nend\n"
        );
        let diagnostics = checked(&source).expect_err("invalid struct use is rejected");
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.code == code),
            "missing {code}: {diagnostics:?}"
        );
    }
}

#[test]
fn checks_fixed_array_indexing_with_a_usize_index() {
    let source = "defmodule Main do\n  def get(values: [i32; 2], index: usize) -> i32 do\n    values[index]\n  end\n  def make() -> [i64; 2] do\n    #[1, 2]\n  end\n  def direct() -> i64 do\n    #[1, 2][1] + make()[0]\n  end\n  def main() -> i32 do\n    get(#[40, 2], 0)\n  end\nend\n";

    let typed = checked(source).expect("fixed-array indexing type checks");
    assert!(typed.debug_tree().contains("index: i32"));
    verify(&typed).expect("indexed Typed AST verifies");
}

#[test]
fn checks_managed_slice_construction_subslicing_copy_length_and_indexing() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    array: [i32; 3] = #[10, 20, 30]\n    whole: Slice(i32) = Slice.from_array(array)\n    part = Slice.subslice(whole, 1, 2)\n    copy = Slice.copy(part)\n    if Array.length(array) == 3 and Slice.length(copy) == 2 do\n      copy[1]\n    else\n      0\n    end\n  end\nend\n";

    let typed = checked(source).expect("slice operations type check");
    let debug = typed.debug_tree();
    assert!(debug.contains("slice from array: Slice(i32)"));
    assert!(debug.contains("subslice: Slice(i32)"));
    assert!(debug.contains("slice copy: Slice(i32)"));
    assert!(debug.contains("index: i32"));
    verify(&typed).expect("slice Typed AST verifies");

    checked(
        "defmodule Main do\n  def empty() -> Slice(i32) do\n    Slice.from_array(#[])\n  end\nend\n",
    )
    .expect("the expected slice item type infers an empty array item type");
}

#[test]
fn rejects_invalid_slice_intrinsic_inputs() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    index: i32 = 0\n    slice = Slice.from_array(#[1, 2])\n    Slice.subslice(slice, index, 1)[0]\n  end\nend\n";

    let diagnostics = checked(source).expect_err("non-usize slice bounds are rejected");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2113")
    );
}

#[test]
fn rejects_indexing_strings_lists_and_non_usize_indices() {
    let cases = [
        ("\"abc\"[0]", "E2150"),
        ("[1, 2][0]", "E2150"),
        ("#[1, 2][index]", "E2113"),
    ];
    for (expression, code) in cases {
        let source = format!(
            "defmodule Main do\n  def main() -> i32 do\n    index: i32 = 0\n    {expression}\n  end\nend\n"
        );
        let diagnostics = checked(&source).expect_err("invalid indexing is rejected");
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.code == code),
            "missing {code}: {diagnostics:?}"
        );
    }
}

#[test]
fn desugars_left_associative_pipelines_into_first_call_arguments() {
    let source = "defmodule Main do\n  def add(value: i32, extra: i32) -> i32 do\n    value + extra\n  end\n  def identity(value: a) -> a do\n    value\n  end\n  def main() -> i32 do\n    40 |> add(2) |> identity()\n  end\nend\n";

    let typed = checked(source).expect("pipeline type checks after call rewriting");
    let el_types::TypedItem::Expr(outer) = &typed.functions[2].body.items[0] else {
        panic!("pipeline expression")
    };
    let TypedExprKind::Call {
        function,
        arguments,
        ..
    } = &outer.kind
    else {
        panic!("outer pipeline desugars to a call")
    };
    assert_eq!(*function, typed.functions[1].id);
    assert_eq!(arguments.len(), 1);
    let TypedExprKind::Call {
        function,
        arguments,
        ..
    } = &arguments[0].kind
    else {
        panic!("left-associated input remains the first argument")
    };
    assert_eq!(*function, typed.functions[0].id);
    assert_eq!(arguments.len(), 2);
    assert!(matches!(arguments[0].kind, TypedExprKind::Integer(40)));
    assert!(matches!(arguments[1].kind, TypedExprKind::Integer(2)));
    assert_eq!(outer.ty, TypeId(0));
    verify(&typed).expect("desugared pipeline Typed AST verifies");
}

#[test]
fn desugars_with_and_scopes_successful_pattern_bindings() {
    let source = "defmodule Main do\n  @type Parsed = {:ok, i32} | :error\n  @type Result = i32 | :error\n  def parse(value: i32) -> Parsed do\n    if value >= 0 do\n      {:ok, value}\n    else\n      :error\n    end\n  end\n  def add(left: i32, right: i32) -> Result do\n    with {:ok, first} <- parse(left),\n         {:ok, second} <- parse(right) do\n      first + second\n    end\n  end\nend\n";

    let typed = checked(source).expect("with expression type checks");
    let debug = typed.debug_tree();
    assert!(
        debug.matches("match exhaustive=true").count() >= 2,
        "{debug}"
    );
    assert!(debug.contains("local s"), "{debug}");
    verify(&typed).expect("desugared with Typed AST verifies");
}

#[test]
fn rejects_a_with_failure_outside_the_result_type() {
    let source = "defmodule Main do\n  def step() -> :ok | :error do\n    :error\n  end\n  def run() -> i32 do\n    with :ok <- step() do\n      42\n    end\n  end\nend\n";

    let diagnostics = checked(source).expect_err("unrepresentable propagation is rejected");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2164"),
        "{diagnostics:?}"
    );
}

#[test]
fn records_immediate_deferred_call_inputs_and_immutable_block_captures() {
    let source = "defmodule Main do\n  def cleanup(value: i32) -> unit do\n    unit\n  end\n  def immediate() -> i32 do\n    1\n  end\n  def main(unused: i32) -> unit do\n    mut value: i32 = 10\n    defer cleanup(value + immediate())\n    defer do\n      cleanup(value)\n    end\n    value := 20\n    unit\n  end\nend\n";

    let typed = checked(source).expect("deferred actions type check at registration");
    let main = &typed.functions[2];
    let el_types::TypedItem::DeferCall {
        function,
        arguments,
        ..
    } = &main.body.items[1]
    else {
        panic!("first action is a deferred call")
    };
    assert_eq!(*function, typed.functions[0].id);
    assert!(matches!(arguments[0].kind, TypedExprKind::Binary { .. }));

    let el_types::TypedItem::Let { symbol: source, .. } = &main.body.items[0] else {
        panic!("mutable source binding")
    };
    let el_types::TypedItem::DeferBlock { captures, body, .. } = &main.body.items[2] else {
        panic!("second action is a deferred block")
    };
    assert_eq!(captures.len(), 1, "unused outer bindings are not captured");
    assert_eq!(captures[0].source, *source);
    assert_ne!(captures[0].symbol, captures[0].source);
    let el_types::TypedItem::Expr(call) = &body.items[0] else {
        panic!("deferred body call")
    };
    let TypedExprKind::Call { arguments, .. } = &call.kind else {
        panic!("deferred body contains a typed call")
    };
    assert!(matches!(
        arguments[0].kind,
        TypedExprKind::Local(symbol) if symbol == captures[0].symbol
    ));
    verify(&typed).expect("deferred registration facts verify");
}

#[test]
fn rejects_invalid_deferred_actions_and_capture_mutation() {
    let cases = [
        ("defer immediate()\n    unit", "E2113"),
        ("defer do\n      return unit\n    end\n    unit", "E2142"),
        (
            "defer do\n      defer cleanup(1)\n      unit\n    end\n    unit",
            "E2141",
        ),
        (
            "mut value: i32 = 1\n    defer do\n      value := 2\n      unit\n    end\n    unit",
            "E2104",
        ),
    ];
    for (body, code) in cases {
        let source = format!(
            "defmodule Main do\n  def cleanup(value: i32) -> unit do\n    unit\n  end\n  def immediate() -> i32 do\n    1\n  end\n  def main() -> unit do\n    {body}\n  end\nend\n"
        );
        let diagnostics = checked(&source).expect_err("invalid deferred action is rejected");
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.code == code),
            "missing {code}: {diagnostics:?}"
        );
    }
}

#[test]
fn checks_comparisons_short_circuit_logic_while_and_nested_returns() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    mut n: i32 = 5\n    mut result: i32 = 1\n    while n > 1 do\n      result := result * n\n      n := n - 1\n    end\n    if true do\n      result := result\n    end\n    if n == 1 and result >= 120 do\n      return result\n    else\n      0\n    end\n  end\nend\n";

    let typed = checked(source).expect("core control flow type checks");
    let debug = typed.debug_tree();

    assert!(debug.contains("while"));
    assert!(debug.contains("comparison Greater"));
    assert!(debug.contains("logical And"));
    assert!(debug.contains("return"));
    verify(&typed).expect("control-flow Typed AST verifies");
}

#[test]
fn rejects_non_bool_control_conditions_and_unsupported_comparisons() {
    let cases = [
        ("while 1 do\n      unit\n    end\n    0", "E2113"),
        ("if 1 do\n      0\n    else\n      1\n    end", "E2113"),
        (
            "left: Map(i32, i32) = %{1 => 1}\n    right: Map(i32, i32) = %{1 => 1}\n    left < right\n    0",
            "E2139",
        ),
        ("1 and true\n    0", "E2113"),
    ];
    for (body, code) in cases {
        let source = format!("defmodule Main do\n  def main() -> i32 do\n    {body}\n  end\nend\n");
        let diagnostics = checked(&source).expect_err("invalid control flow is rejected");
        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.code == code),
            "missing {code}: {diagnostics:?}"
        );
    }
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
fn typed_ast_verifier_recomputes_pattern_usefulness_and_exhaustiveness() {
    let source = "defmodule Main do\n  def choose(flag: bool) -> i64 do\n    match flag do\n      true -> 1\n      false -> 2\n    end\n  end\nend\n";
    let typed = checked(source).expect("baseline exhaustive match");

    let mut missing_arm = typed.clone();
    let el_types::TypedItem::Expr(expression) = &mut missing_arm.functions[0].body.items[0] else {
        panic!("match expression")
    };
    let TypedExprKind::Match { arms, .. } = &mut expression.kind else {
        panic!("typed match")
    };
    arms.pop();
    assert!(
        verify(&missing_arm)
            .expect_err("stale exhaustive fact is rejected")
            .iter()
            .any(|error| error.contains("exhaustiveness fact"))
    );

    let mut duplicate_arm = typed.clone();
    let el_types::TypedItem::Expr(expression) = &mut duplicate_arm.functions[0].body.items[0]
    else {
        panic!("match expression")
    };
    let TypedExprKind::Match { arms, .. } = &mut expression.kind else {
        panic!("typed match")
    };
    arms.insert(1, arms[0].clone());
    assert!(
        verify(&duplicate_arm)
            .expect_err("stale reachability fact is rejected")
            .iter()
            .any(|error| error.contains("match pattern facts"))
    );

    let mut wrong_irrefutability = typed;
    let el_types::TypedItem::Expr(expression) =
        &mut wrong_irrefutability.functions[0].body.items[0]
    else {
        panic!("match expression")
    };
    let TypedExprKind::Match { arms, .. } = &mut expression.kind else {
        panic!("typed match")
    };
    arms[0].pattern.facts.irrefutable = true;
    assert!(
        verify(&wrong_irrefutability)
            .expect_err("stale irrefutability fact is rejected")
            .iter()
            .any(|error| error.contains("pattern facts"))
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
fn checks_list_reverse_and_expected_empty_list_inference() {
    let source = "defmodule Main do\n  def reverse(values: [a]) -> [a] do\n    List.reverse(values)\n  end\n  def empty() -> [i32] do\n    List.reverse([])\n  end\n  def main() -> i32 do\n    reversed: [i32] = reverse([42, 1])\n    match reversed do\n      [1 | _] -> 0\n      [_ | tail] -> match tail do\n        [value | _] -> value\n        [] -> 0\n      end\n      [] -> 0\n    end\n  end\nend\n";

    let typed = checked(source).expect("list reverse type checks and specializes");
    assert!(typed.debug_tree().contains("list reverse: [a]"));
    assert!(typed.debug_tree().contains("list reverse: [i32]"));
    verify(&typed).expect("list reverse Typed AST verifies");
}

#[test]
fn checks_enum_count_at_and_to_list_for_standard_iterables() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    list: [i32] = [10, 20]\n    array: [i32; 2] = #[30, 40]\n    slice = Slice.from_array(array)\n    data = String.bytes(\"AB\")\n    map: Map(i32, string) = %{1 => \"one\", 2 => \"two\"}\n    Enum.count(list)\n    Enum.count(array)\n    Enum.count(slice)\n    Enum.count(data)\n    Enum.count(map)\n    Enum.at(list, 1)\n    Enum.at(array, 2)\n    Enum.at(slice, 0)\n    Enum.at(data, 1)\n    Enum.at(map, 0)\n    Enum.to_list(list)\n    Enum.to_list(array)\n    Enum.to_list(slice)\n    Enum.to_list(data)\n    Enum.to_list(map)\n    0\n  end\nend\n";

    let typed = checked(source).expect("non-higher-order Enum traversal type checks");
    let debug = typed.debug_tree();
    assert_eq!(
        debug.matches("collection length: usize").count(),
        5,
        "{debug}"
    );
    assert_eq!(debug.matches("enum at:").count(), 5, "{debug}");
    assert!(debug.contains("enum to list: [i32]"), "{debug}");
    assert!(debug.contains("bytes to list: [u8]"), "{debug}");
    assert!(debug.contains("map to list: [{i32, string}]"), "{debug}");
    verify(&typed).expect("Enum traversal Typed AST verifies");
}

#[test]
fn rejects_enum_traversal_for_non_iterables_and_bad_indices() {
    for (expression, code) in [
        ("Enum.count(true)", "E2151"),
        ("Enum.to_list(1)", "E2151"),
        ("Enum.at(#[1], false)", "E2113"),
    ] {
        let source = format!(
            "defmodule Main do\n  def main() -> i32 do\n    {expression}\n    0\n  end\nend\n"
        );
        assert!(
            checked(&source)
                .expect_err("invalid Enum traversal is rejected")
                .iter()
                .any(|diagnostic| diagnostic.code == code)
        );
    }
}

#[test]
fn checks_enum_each_any_and_all_callbacks_for_standard_iterables() {
    let source = "defmodule Main do\n  def consume(value: i32) -> unit do\n    unit\n  end\n  def positive(value: i32) -> bool do\n    value > 0\n  end\n  def byte(value: u8) -> bool do\n    value == 65\n  end\n  def pair(value: {i32, string}) -> bool do\n    true\n  end\n  def main() -> bool do\n    list: [i32] = [1, 2]\n    array: [i32; 2] = #[1, 2]\n    slice = Slice.from_array(array)\n    data = String.bytes(\"AB\")\n    map: Map(i32, string) = %{1 => \"one\"}\n    Enum.each(list, consume)\n    Enum.any(array, positive)\n    Enum.all(slice, positive)\n    Enum.any(data, byte)\n    Enum.all(map, pair)\n  end\nend\n";

    let typed = checked(source).expect("Enum visit callbacks type check");
    let debug = typed.debug_tree();
    assert!(debug.contains("enum Each: unit"), "{debug}");
    assert_eq!(debug.matches("enum Any: bool").count(), 2, "{debug}");
    assert_eq!(debug.matches("enum All: bool").count(), 2, "{debug}");
    verify(&typed).expect("Enum visit Typed AST verifies");
}

#[test]
fn rejects_enum_visit_callback_signature_mismatches() {
    for source in [
        "defmodule Main do\n  def wrong(value: i64) -> unit do\n    unit\n  end\n  def main() -> unit do\n    values: [i32; 1] = #[1]\n    Enum.each(values, wrong)\n  end\nend\n",
        "defmodule Main do\n  def wrong(value: i32) -> i32 do\n    value\n  end\n  def main() -> bool do\n    values: [i32; 1] = #[1]\n    Enum.any(values, wrong)\n  end\nend\n",
        "defmodule Main do\n  def predicate(value: i32) -> bool do\n    true\n  end\n  def main() -> bool do\n    Enum.all(42, predicate)\n  end\nend\n",
        "defmodule Main do\n  def wrong(value: i64) -> bool do\n    true\n  end\n  def main() -> [bool] do\n    values: [i32; 1] = #[1]\n    Enum.map(values, wrong)\n  end\nend\n",
    ] {
        assert!(
            checked(source).is_err(),
            "invalid callback must be rejected"
        );
    }
}

#[test]
fn checks_enum_reduce_with_explicit_accumulator_types() {
    let source = "defmodule Main do\n  def add(total: i32, value: i32) -> i32 do\n    total + value\n  end\n  def keep(total: usize, value: u8) -> usize do\n    total\n  end\n  def main() -> i32 do\n    values: [i32] = [1, 2]\n    data = String.bytes(\"AB\")\n    Enum.reduce(data, 0 :: usize, keep)\n    Enum.reduce(values, 0, add)\n  end\nend\n";

    let typed = checked(source).expect("Enum.reduce fixes its accumulator from the initial value");
    let debug = typed.debug_tree();
    assert!(debug.contains("enum Reduce: usize"), "{debug}");
    assert!(debug.contains("enum Reduce: i32"), "{debug}");
    verify(&typed).expect("Enum.reduce Typed AST verifies");
}

#[test]
fn checks_enum_filter_returns_the_source_item_list_type() {
    let source = "defmodule Main do\n  def positive(value: i32) -> bool do\n    value > 0\n  end\n  def main() -> [i32] do\n    values: [i32; 3] = #[1, 0, 2]\n    Enum.filter(values, positive)\n  end\nend\n";
    let typed = checked(source).expect("Enum.filter type checks");
    assert!(typed.debug_tree().contains("enum Filter: [i32]"));
    verify(&typed).expect("Enum.filter Typed AST verifies");
}

#[test]
fn checks_enum_map_returns_the_callback_result_list_type() {
    let source = "defmodule Main do\n  def positive(value: i32) -> bool do\n    value > 0\n  end\n  def identity(value: a) -> a do\n    value\n  end\n  def main() -> [i32] do\n    values: [i32; 3] = #[1, 0, 2]\n    Enum.map(values, positive)\n    Enum.map(values, identity)\n    Enum.map(values, identity)\n  end\nend\n";
    let typed = checked(source).expect("Enum.map type checks and specializes from its result");
    let debug = typed.debug_tree();
    assert!(debug.contains("enum Map: [bool]"), "{debug}");
    assert_eq!(debug.matches("enum Map: [i32]").count(), 2, "{debug}");
    verify(&typed).expect("Enum.map Typed AST verifies");
}

#[test]
fn rejects_list_reverse_for_non_list_values() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    List.reverse(42)\n  end\nend\n";
    let diagnostics = checked(source).expect_err("List.reverse requires a list");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2151")
    );
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
fn selects_a_tagged_tuple_member_from_multiple_tuple_alternatives() {
    let source = "defmodule Main do\n  @type Result = {:ok, i64} | {:error, string}\n  def main() -> Result do\n    {:ok, 1}\n  end\nend\n";

    let typed = checked(source).expect("the tuple tag selects the union member");

    assert!(typed.debug_tree().contains("inject {:ok, i64}"));
}

#[test]
fn injects_each_if_branch_into_an_expected_union() {
    let source = "defmodule Main do\n  @type Parsed = {:ok, i32} | :error\n  def parse(valid: bool) -> Parsed do\n    if valid do\n      {:ok, 40}\n    else\n      :error\n    end\n  end\nend\n";

    let typed = checked(source).expect("union expectation flows into both branches");
    assert_eq!(typed.debug_tree().matches("inject").count(), 2);
    verify(&typed).expect("branch injections verify");
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
fn derives_generic_struct_protocols_and_checks_field_constraints() {
    let source = "defmodule Main do\n  @derive [Eq, Ord, Show, Hash]\n  defstruct Box(a) do\n    value: a\n  end\n  def same(left: a, right: a) -> bool when a: Eq do\n    left == right\n  end\n  def main() -> bool do\n    left = %Box{value: 1}\n    right = %Box{value: 1}\n    same(left, right)\n  end\nend\n";
    let typed = checked(source).expect("derived generic Eq is selected for Box(i32)");
    assert_eq!(typed.structs[0].derives, ["Eq", "Ord", "Show", "Hash"]);
    verify(&typed).expect("derived struct Typed AST verifies");

    let invalid = source.replace("value: 1", "value: 1.0");
    let diagnostics = checked(&invalid).expect_err("f64 prevents derived Eq");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2120")
    );
}

#[test]
fn explicit_local_protocol_implementations_satisfy_generic_constraints() {
    let source = "defmodule Main do\n  defprotocol Marker do\n  end\n  defimpl Marker, for: i32 do\n  end\n  def marked(value: a) -> a when a: Marker do\n    value\n  end\n  def main() -> i32 do\n    marked(1)\n  end\nend\n";
    checked(source).expect("explicit local implementation satisfies the constraint");
}

#[test]
fn checks_implementation_method_bodies_as_hidden_typed_functions() {
    let source = "defmodule Main do\n  defprotocol Render do\n    def render(value: Self) -> i32\n  end\n  defimpl Render, for: i32 do\n    def render(value: i32) -> i32 do\n      value + 1\n    end\n  end\n  def main() -> i32 do\n    0\n  end\nend\n";
    let typed = checked(source).expect("implementation method body checks");
    let implementation = &typed.implementations[0];
    assert_eq!(implementation.method_declarations.len(), 1);
    let method = implementation.method_declarations[0].1;
    assert!(typed.functions.iter().any(|function| function.id == method));
    verify(&typed).expect("hidden implementation method Typed AST verifies");

    let invalid = source.replace("value + 1", "true");
    let diagnostics = checked(&invalid).expect_err("invalid implementation body is rejected");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2113")
    );
}

#[test]
fn dispatches_concrete_core_protocol_operators_to_explicit_methods() {
    let source = "defmodule Main do\n  defstruct Score do\n    value: i32\n  end\n  defimpl Eq, for: Score do\n    def eq(left: Score, right: Score) -> bool do\n      left.value == right.value\n    end\n  end\n  defimpl Ord, for: Score do\n    def compare(left: Score, right: Score) -> :less | :equal | :greater do\n      if left.value < right.value do\n        :less\n      else\n        if left.value > right.value do\n          :greater\n        else\n          :equal\n        end\n      end\n    end\n  end\n  defimpl Concat, for: Score do\n    def concat(left: Score, right: Score) -> Score do\n      %Score{value: left.value + right.value}\n    end\n  end\n  def main() -> bool do\n    one = %Score{value: 1}\n    two = %Score{value: 2}\n    sum = one ++ two\n    one == one and one != two and one < two and one <= two and two > one and two >= one and sum.value == 3\n  end\nend\n";
    let typed = checked(source).expect("explicit core protocol methods dispatch");
    let tree = typed.debug_tree();
    let method = |implementation: usize, name: &str| {
        typed.implementations[implementation]
            .method_declarations
            .iter()
            .find(|(candidate, _)| candidate == name)
            .expect("method declaration")
            .1
    };
    assert!(
        tree.matches(&format!("call d{}", method(0, "eq").0))
            .count()
            >= 2
    );
    assert!(
        tree.matches(&format!("call d{}", method(1, "compare").0))
            .count()
            >= 4
    );
    assert_eq!(
        tree.matches(&format!("call d{}", method(2, "concat").0))
            .count(),
        1
    );
    verify(&typed).expect("dispatched Typed AST verifies");
}

#[test]
fn checks_concat_for_every_standard_protocol_type() {
    let source = "defmodule Main do\n  def main() -> bool do\n    left: [i32] = [1]\n    right: [i32] = [2]\n    data = Bytes.from_list([65])\n    bits = Bytes.to_bits(data)\n    \"a\" ++ \"b\" == \"ab\" and Bytes.byte_size(data ++ data) == 2 and Bits.bit_size(bits ++ bits) == 16 and left ++ right == [1, 2]\n  end\nend\n";
    let typed = checked(source).expect("all standard Concat types check");
    assert_eq!(typed.debug_tree().matches("concat:").count(), 3);
    assert_eq!(typed.debug_tree().matches("buffer append").count(), 2);
    verify(&typed).expect("concat Typed AST verifies");

    let invalid =
        "defmodule Main do\n  def main() -> [i32; 2] do\n    #[1, 2] ++ #[3, 4]\n  end\nend\n";
    let diagnostics = checked(invalid).expect_err("arrays do not implement Concat");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2160")
    );
}

#[test]
fn checks_for_patterns_against_static_iterable_items() {
    let source = "defmodule Main do\n  def main() -> unit do\n    list: [i32] = [1, 2]\n    array: [i32; 2] = #[1, 2]\n    slice = Slice.from_array(array)\n    bytes = String.bytes(\"AB\")\n    map: Map(i32, string) = %{1 => \"one\"}\n    for value in list do\n      unit\n    end\n    for value in array do\n      unit\n    end\n    for value in slice do\n      unit\n    end\n    for value in bytes do\n      unit\n    end\n    for {key, value} in map do\n      unit\n    end\n    for value in String.codepoint_view(\"A🙂\") do\n      unit\n    end\n    for value in String.grapheme_view(\"é\") do\n      unit\n    end\n  end\nend\n";
    let typed = checked(source).expect("standard Iterable implementations select Item types");
    assert_eq!(typed.debug_tree().matches("for Binding").count(), 6);
    assert!(typed.debug_tree().contains("for Tuple"));
    verify(&typed).expect("for Typed AST verifies");

    let invalid = "defmodule Main do\n  def main() -> unit do\n    values: [[i32]] = [[1]]\n    for [head | tail] in values do\n      unit\n    end\n  end\nend\n";
    let diagnostics = checked(invalid).expect_err("refutable for patterns are rejected");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2162")
    );
}

#[test]
fn normalizes_qualified_iterable_item_projections_at_instantiation() {
    let source = "defmodule Main do\n  def keep(values: i, value: Iterable.Item(i)) -> Iterable.Item(i) when i: Iterable do\n    value\n  end\n  def main() -> i32 do\n    values: [i32] = [1]\n    keep(values, 42)\n  end\nend\n";
    let typed = checked(source).expect("Iterable.Item remains abstract once and normalizes at use");
    assert!(typed.debug_tree().contains("Iterable.Item(i)"));
    verify(&typed).expect("projected Typed AST verifies");
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
fn decomposes_singleton_atoms_lists_and_nested_finite_domains() {
    let source = "defmodule Main do\n  def atom(value: :ready) -> i64 do\n    match value do\n      :ready -> 1\n    end\n  end\n  def list(values: [bool]) -> i64 do\n    match values do\n      [] -> 0\n      [true | _] -> 1\n      [false | _] -> 2\n    end\n  end\n  def nested(value: {bool, bool}) -> i64 do\n    match value do\n      {true, true} -> 0\n      {true, false} -> 1\n      {false, _} -> 2\n    end\n  end\nend\n";

    let typed = checked(source).expect("finite and structural domains are exhaustive");
    assert_eq!(
        typed.debug_tree().matches("match exhaustive=true").count(),
        3
    );
}

#[test]
fn reports_structurally_subsumed_arms_at_the_pattern_with_the_covering_arm() {
    let source = "defmodule Main do\n  def choose(value: {bool, i64}) -> i64 do\n    match value do\n      {true, _} -> 1\n      {true, 0} -> 2\n      {false, _} -> 3\n    end\n  end\nend\n";

    let diagnostics = checked(source).expect_err("narrower tuple arm is unreachable");
    let unreachable = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "E2125")
        .expect("unreachable-arm diagnostic");
    assert_eq!(
        unreachable.primary.start(),
        source.find("{true, 0}").unwrap()
    );
    assert_eq!(unreachable.labels.len(), 1);
    assert_eq!(
        unreachable.labels[0].span.start(),
        source.find("{true, _}").unwrap()
    );
}

#[test]
fn reports_collective_coverage_and_actionable_non_exhaustiveness() {
    let covered = "defmodule Main do\n  def choose(flag: bool) -> i64 do\n    match flag do\n      true -> 1\n      false -> 2\n      _ -> 3\n    end\n  end\nend\n";
    let diagnostics = checked(covered).expect_err("finite cases collectively cover wildcard");
    let unreachable = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "E2125")
        .expect("collective unreachable-arm diagnostic");
    assert!(unreachable.labels.is_empty());
    assert!(
        unreachable
            .notes
            .iter()
            .any(|note| note.contains("collectively cover"))
    );

    let missing = covered.replace("      false -> 2\n      _ -> 3\n", "");
    let diagnostics = checked(&missing).expect_err("bool match is not exhaustive");
    let non_exhaustive = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "E2126")
        .expect("non-exhaustive diagnostic");
    assert!(
        non_exhaustive
            .help
            .as_deref()
            .is_some_and(|help| help.contains("wildcard/binding"))
    );
}

#[test]
fn infinite_scalar_domains_require_a_catch_all_and_reject_duplicate_literals() {
    let incomplete = "defmodule Main do\n  def choose(value: i64) -> i64 do\n    match value do\n      0 -> 0\n      1 -> 1\n    end\n  end\nend\n";
    assert!(
        checked(incomplete)
            .expect_err("a finite set of integers cannot cover i64")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2126")
    );

    let complete = incomplete.replace("      1 -> 1\n", "      _ -> 1\n");
    checked(&complete).expect("a wildcard covers the remaining integer domain");

    let duplicate = complete.replace("      _ -> 1", "      0 -> 2\n      _ -> 1");
    let diagnostics = checked(&duplicate).expect_err("a repeated literal is unreachable");
    let unreachable = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "E2125")
        .expect("duplicate-literal diagnostic");
    assert_eq!(
        unreachable.primary.start(),
        duplicate.rfind("0 -> 2").unwrap()
    );
    assert_eq!(unreachable.labels.len(), 1);
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
fn tagged_tuple_patterns_select_and_destructure_union_members() {
    let source = "defmodule Main do\n  @type Option(a) = {:some, a} | :none\n  def value(option: Option(i32)) -> i32 do\n    match option do\n      {:some, frequency} -> frequency + 1\n      :none -> 1\n    end\n  end\n  def nested(value: {Option(i32), bool}) -> i32 do\n    match value do\n      {{:some, frequency}, _} -> frequency\n      {:none, _} -> 0\n    end\n  end\nend\n";
    let typed = checked(source).expect("tagged tuple pattern selects its union member");
    let debug = typed.debug_tree();
    assert!(debug.contains("StructuralUnionMember"), "{debug}");
    verify(&typed).expect("structural union-member Typed AST verifies");
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

#[test]
fn checks_string_literals_and_exhaustive_integer_string_unions() {
    let source = "defmodule Main do\n  @type Scalar = i64 | string\n  def choose(text: bool) -> Scalar do\n    if text do\n      \"forty-two\"\n    else\n      42\n    end\n  end\n  def classify(value: Scalar) -> i32 do\n    match value do\n      number: i64 -> 40\n      text: string -> 2\n    end\n  end\n  def main() -> i32 do\n    classify(choose(true))\n  end\nend\n";

    let typed = checked(source).expect("string union is well typed and exhaustive");
    let debug = typed.debug_tree();
    assert!(debug.contains("string \"forty-two\": string"), "{debug}");
    assert!(debug.contains("inject string"), "{debug}");
    assert!(debug.contains("match exhaustive=true"), "{debug}");
}

#[test]
fn checks_utf8_string_byte_size_and_rejects_non_string_inputs() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    if String.byte_size(\"é🙂\") == 7 do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("String.byte_size accepts valid UTF-8 text");
    assert!(typed.debug_tree().contains("collection length: usize"));

    let invalid = source.replace("String.byte_size(\"é🙂\")", "String.byte_size(1)");
    assert!(
        checked(&invalid)
            .expect_err("String.byte_size rejects non-string inputs")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2151")
    );
}

#[test]
fn checks_rune_literals_ordering_and_string_conversion() {
    let source = "defmodule Main do\n  def render(value: rune) -> string do\n    Rune.to_string(value)\n  end\n  def main() -> i32 do\n    values: Map(rune, string) = %{'a' => render('a'), '🙂' => render('🙂')}\n    if 'a' < '🙂' and Map.size(values) == 2 do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("runes are scalar values with conversion and ordering");
    let debug = typed.debug_tree();
    assert!(debug.contains("rune 'a': rune"), "{debug}");
    assert!(debug.contains("rune to string: string"), "{debug}");
    verify(&typed).expect("rune Typed AST verifies");

    let invalid = source.replace("Rune.to_string(value)", "Rune.to_string(1)");
    assert!(
        checked(&invalid)
            .expect_err("Rune.to_string rejects non-rune inputs")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2151")
    );
}

#[test]
fn checks_eager_string_codepoints() {
    let source = "defmodule Main do\n  def decode(text: string) -> [rune] do\n    String.codepoints(text)\n  end\n  def main() -> i32 do\n    if decode(\"Aé🙂\") == ['A', 'e', '́', '🙂'] do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("String.codepoints returns eager rune values");
    let debug = typed.debug_tree();
    assert!(debug.contains("string codepoints: [rune]"), "{debug}");
    verify(&typed).expect("string codepoints Typed AST verifies");

    let invalid = source.replace("String.codepoints(text)", "String.codepoints(1)");
    assert!(
        checked(&invalid)
            .expect_err("String.codepoints rejects non-string inputs")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2151")
    );
}

#[test]
fn checks_unicode_grapheme_length() {
    let source = "defmodule Main do\n  def count(text: string) -> usize do\n    String.length(text)\n  end\n  def main() -> i32 do\n    if count(\"Aé🇸🇬👩‍👩‍👧‍👦\") == 4 do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("String.length returns a grapheme count");
    let debug = typed.debug_tree();
    assert!(debug.contains("string grapheme length: usize"), "{debug}");
    verify(&typed).expect("String.length Typed AST verifies");

    let invalid = source.replace("String.length(text)", "String.length(1)");
    assert!(
        checked(&invalid)
            .expect_err("String.length rejects non-string inputs")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2151")
    );
}

#[test]
fn checks_basic_string_operations() {
    let source = "defmodule Main do\n  def fields(text: string) -> [string] do\n    String.split(text, \",\")\n  end\n  def main() -> i32 do\n    if String.empty(\"\") and String.contains(\"café\", \"fé\") and fields(\"a,,b,\") == [\"a\", \"\", \"b\", \"\"] do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("basic String operations type-check");
    let debug = typed.debug_tree();
    assert!(debug.contains("string empty: bool"), "{debug}");
    assert!(debug.contains("string contains: bool"), "{debug}");
    assert!(debug.contains("string split: [string]"), "{debug}");
    verify(&typed).expect("basic String operations Typed AST verifies");

    for invalid in [
        "String.empty(1)",
        "String.contains(\"abc\", 1)",
        "String.split(\"abc\", 1)",
    ] {
        let invalid_source = format!(
            "defmodule Main do\n  def main() -> i32 do\n    {invalid}\n    0\n  end\nend\n"
        );
        assert!(
            checked(&invalid_source).is_err(),
            "{invalid} must be rejected"
        );
    }
}

#[test]
fn checks_string_downcase_replace_and_enum_frequencies() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    lowered: string = String.downcase(\"ÉL İ\")\n    replaced: string = String.replace(lowered, \"é\", \"e\")\n    counts: Map(string, usize) = Enum.frequencies([replaced, replaced, \"x\"])\n    if Map.size(counts) == 2 do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("new String and Enum operations type-check");
    let debug = typed.debug_tree();
    assert!(debug.contains("string downcase: string"), "{debug}");
    assert!(debug.contains("string replace: string"), "{debug}");
    assert!(
        debug.contains("enum frequencies: Map(string, usize)"),
        "{debug}"
    );
    verify(&typed).expect("new operations produce valid Typed AST");

    for invalid in [
        "String.downcase(1)",
        "String.replace(\"a\", 1, \"b\")",
        "Enum.frequencies(42)",
    ] {
        let invalid_source = format!(
            "defmodule Main do\n  def main() -> i32 do\n    {invalid}\n    0\n  end\nend\n"
        );
        assert!(
            checked(&invalid_source).is_err(),
            "{invalid} must be rejected"
        );
    }
}

#[test]
fn checks_eager_graphemes_and_lazy_string_views() {
    let source = "defmodule Main do\n  def graphemes(text: string) -> [string] do\n    String.graphemes(text)\n  end\n  def codepoint_view(text: string) -> String.CodepointView do\n    String.codepoint_view(text)\n  end\n  def grapheme_view(text: string) -> String.GraphemeView do\n    String.grapheme_view(text)\n  end\n  def main() -> i32 do\n    if graphemes(\"é🇸🇬\") == [\"é\", \"🇸🇬\"] and Enum.to_list(codepoint_view(\"A🙂\")) == ['A', '🙂'] and Enum.to_list(grapheme_view(\"é🇸🇬\")) == [\"é\", \"🇸🇬\"] do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("grapheme APIs and lazy views type-check");
    verify(&typed).expect("grapheme APIs and lazy views verify");

    for call in [
        "String.graphemes(1)",
        "String.codepoint_view(1)",
        "String.grapheme_view(1)",
    ] {
        let invalid = source.replace("String.graphemes(text)", call);
        assert!(
            checked(&invalid).is_err(),
            "{call} rejects a non-string input"
        );
    }
}

#[test]
fn checks_byte_aligned_bitstring_construction() {
    let source = "defmodule Main do\n  def packet(prefix: bytes, value: i32, size: usize) -> bytes do\n    <<prefix::bytes-size(size), value::signed-little-size(16), 255::unsigned-big-size(8)>>\n  end\n  def empty() -> bytes do\n    <<>>\n  end\nend\n";
    let typed = checked(source).expect("byte-aligned bitstring construction type-checks");
    assert!(typed.debug_tree().contains("bitstring: bytes"));
    verify(&typed).expect("bitstring Typed AST verifies");

    for invalid in [
        source.replace("prefix::bytes-size(size)", "1::bytes-size(size)"),
        source.replace(
            "value::signed-little-size(16)",
            "true::signed-little-size(16)",
        ),
        source.replace("255::unsigned-big-size(8)", "256::unsigned-big-size(8)"),
        source.replace(
            "prefix::bytes-size(size)",
            "String.bytes(\"x\")::bytes-size(2)",
        ),
    ] {
        assert!(
            checked(&invalid).is_err(),
            "accepted invalid bitstring: {invalid}"
        );
    }
}

#[test]
fn checks_byte_aligned_bitstring_patterns_and_binding_order() {
    let source = "defmodule Main do\n  def parse(packet: bytes, prefix_size: usize) -> usize do\n    match packet do\n      <<7::unsigned-big-size(8), prefix::bytes-size(prefix_size), rest::bytes-size(Bytes.byte_size(prefix))>> -> Bytes.byte_size(rest)\n      _ -> 0\n    end\n  end\n  def signed(packet: bytes) -> i32 do\n    match packet do\n      <<-2::signed-big-size(16)>> -> 1\n      _ -> 0\n    end\n  end\nend\n";
    let typed = checked(source).expect("byte-aligned bitstring pattern type-checks");
    assert!(typed.debug_tree().contains("Bitstring"));
    verify(&typed).expect("bitstring-pattern Typed AST verifies");

    for invalid in [
        source.replace("packet: bytes", "packet: string"),
        source.replace("prefix_size: usize", "prefix_size: i32"),
        source.replace("7::unsigned-big-size(8)", "256::unsigned-big-size(8)"),
        source.replace("-2::signed-big-size(16)", "-32769::signed-big-size(16)"),
        source.replace("-2::signed-big-size(16)", "-1::unsigned-big-size(16)"),
        source.replace(
            "prefix::bytes-size(prefix_size), rest::bytes-size(Bytes.byte_size(prefix))",
            "prefix::bytes-size(Bytes.byte_size(rest)), rest::bytes",
        ),
    ] {
        assert!(
            checked(&invalid).is_err(),
            "accepted invalid bitstring pattern: {invalid}"
        );
    }

    let duplicate = "defmodule Main do\n  def parse(packet: bytes) -> i32 do\n    match packet do\n      <<1::unsigned-big-size(8)>> -> 1\n      <<1::unsigned-big-size(8)>> -> 2\n      _ -> 0\n    end\n  end\nend\n";
    assert!(
        checked(duplicate).is_err(),
        "accepted duplicate bitstring arm"
    );
}

#[test]
fn checks_utf8_validation_results_and_error_offsets() {
    let source = "defmodule Main do\n  def valid(value: {:ok, string}) -> usize do\n    match value do\n      {:ok, text} -> String.byte_size(text)\n    end\n  end\n  def invalid(value: {:error, String.Utf8Error}) -> usize do\n    match value do\n      {:error, reason} -> String.utf8_error_offset(reason)\n    end\n  end\n  def inspect(data: bytes) -> usize do\n    match String.from_bytes(data) do\n      value: {:ok, string} -> valid(value)\n      value: {:error, String.Utf8Error} -> invalid(value)\n    end\n  end\n  def main() -> i32 do\n    if inspect(String.bytes(\"é\")) == 2 do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("UTF-8 validation result is inspectable");
    let debug = typed.debug_tree();
    assert!(debug.contains("string from bytes:"), "{debug}");
    assert!(debug.contains("UTF-8 error offset: usize"), "{debug}");
    verify(&typed).expect("UTF-8 validation Typed AST verifies");

    for invalid in [
        source.replace("String.from_bytes(data)", "String.from_bytes(1)"),
        source.replace(
            "String.utf8_error_offset(reason)",
            "String.utf8_error_offset(1)",
        ),
    ] {
        assert!(
            checked(&invalid)
                .expect_err("invalid UTF-8 API input is rejected")
                .iter()
                .any(|diagnostic| diagnostic.code == "E2151")
        );
    }
}

#[test]
fn checks_value_style_buffer_operations_and_utf8_results() {
    let source = "defmodule Main do\n  def build() -> Buffer do\n    first = Buffer.append_string(Buffer.new(), \"hello\")\n    second = Buffer.append_byte(first, 32)\n    Buffer.append_bytes(second, String.bytes(\"world\"))\n  end\n  def inspect(buffer: Buffer) -> {:ok, string} | {:error, String.Utf8Error} do\n    Buffer.to_string(buffer)\n  end\n  def main() -> i32 do\n    if Buffer.byte_size(build()) == Bytes.byte_size(Buffer.to_bytes(build())) do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("Buffer's minimal API type checks");
    let debug = typed.debug_tree();
    for operation in [
        "buffer new: Buffer",
        "buffer append String: Buffer",
        "buffer append Byte: Buffer",
        "buffer append Bytes: Buffer",
        "buffer to bytes: bytes",
        "buffer to string:",
    ] {
        assert!(
            debug.contains(operation),
            "missing {operation:?} in {debug}"
        );
    }
    verify(&typed).expect("Buffer Typed AST verifies");

    for invalid in [
        source.replace(
            "Buffer.append_byte(first, 32)",
            "Buffer.append_byte(first, true)",
        ),
        source.replace(
            "Buffer.append_bytes(second, String.bytes(\"world\"))",
            "Buffer.append_bytes(second, \"world\")",
        ),
        source.replace("Buffer.to_string(buffer)", "Buffer.to_string(1)"),
    ] {
        assert!(
            checked(&invalid)
                .expect_err("invalid Buffer API input is rejected")
                .iter()
                .any(|diagnostic| matches!(diagnostic.code.as_str(), "E2113" | "E2151"))
        );
    }
}

#[test]
fn checks_show_backed_string_interpolation() {
    let source = "defmodule Main do\n  def message(count: usize, ready: bool) -> string do\n    \"count=#{count} ready=#{ready} atom=#{:ok} unit=#{unit} rune=#{'λ'}\"\n  end\nend\n";
    let typed = checked(source).expect("Show-backed interpolation type checks");
    let debug = typed.debug_tree();
    assert!(debug.contains("integer to string"), "{debug}");
    assert!(debug.contains("boolean to string"), "{debug}");
    assert!(debug.contains("show constant \":ok\""), "{debug}");
    verify(&typed).expect("interpolated-string Typed AST verifies");

    let invalid = "defmodule Main do\n  def message() -> string do\n    \"buffer=#{Buffer.new()}\"\n  end\nend\n";
    assert!(
        checked(invalid)
            .expect_err("non-Show interpolation is rejected")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2120")
    );
}

#[test]
fn checks_arbitrary_bit_views_indexing_and_alignment_conversion() {
    let source = "defmodule Main do\n  def convert(data: bytes) -> {:some, bytes} | :none do\n    bits = Bytes.to_bits(data)\n    Bits.bit_size(bits)\n    view = Bits.slice(bits, 1, 7)\n    view[0]\n    Bits.to_bytes(view)\n  end\nend\n";
    let typed = checked(source).expect("bits APIs and direct indexing type check");
    let debug = typed.debug_tree();
    for operation in [
        "bytes to bits: bits",
        "bits slice: bits",
        "collection length: usize",
        "index: bool",
        "bits to bytes:",
    ] {
        assert!(
            debug.contains(operation),
            "missing {operation:?} in {debug}"
        );
    }
    verify(&typed).expect("bits Typed AST verifies");

    for invalid in [
        source.replace("Bytes.to_bits(data)", "Bytes.to_bits(1)"),
        source.replace("Bits.slice(bits, 1, 7)", "Bits.slice(bits, true, 1)"),
        source.replace("Bits.to_bytes(view)", "Bits.to_bytes(data)"),
    ] {
        assert!(
            checked(&invalid)
                .expect_err("invalid bits API input is rejected")
                .iter()
                .any(|diagnostic| matches!(diagnostic.code.as_str(), "E2113" | "E2151"))
        );
    }
}

#[test]
fn checks_string_bytes_and_bounds_checked_byte_views() {
    let source = "defmodule Main do\n  def view(text: string) -> bytes do\n    Bytes.slice(String.bytes(text), 1, 2)\n  end\n  def main() -> i32 do\n    if Bytes.byte_size(view(\"é🙂\")) == 2 do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("string bytes and byte slicing type check");
    let debug = typed.debug_tree();
    assert!(debug.contains("string bytes: bytes"), "{debug}");
    assert!(debug.contains("bytes slice: bytes"), "{debug}");
    verify(&typed).expect("byte-view Typed AST verifies");

    for invalid in [
        source.replace("String.bytes(text)", "String.bytes(1)"),
        source.replace(
            "Bytes.slice(String.bytes(text), 1, 2)",
            "Bytes.slice(text, 1, 2)",
        ),
        source.replace(
            "Bytes.slice(String.bytes(text), 1, 2)",
            "Bytes.slice(String.bytes(text), true, 2)",
        ),
    ] {
        assert!(
            checked(&invalid)
                .expect_err("invalid byte-view input is rejected")
                .iter()
                .any(|diagnostic| matches!(diagnostic.code.as_str(), "E2113" | "E2151"))
        );
    }
}

#[test]
fn checks_fresh_bytes_list_conversions() {
    let source = "defmodule Main do\n  def round_trip(values: [u8]) -> [u8] do\n    Bytes.to_list(Bytes.from_list(values))\n  end\nend\n";
    let typed = checked(source).expect("byte/list conversions type check");
    let debug = typed.debug_tree();
    assert!(debug.contains("bytes from list: bytes"), "{debug}");
    assert!(debug.contains("bytes to list: [u8]"), "{debug}");
    verify(&typed).expect("byte/list conversion Typed AST verifies");

    for invalid in [
        source.replace("Bytes.from_list(values)", "Bytes.from_list([256])"),
        source.replace("Bytes.from_list(values)", "Bytes.from_list(\"bad\")"),
        source.replace(
            "Bytes.to_list(Bytes.from_list(values))",
            "Bytes.to_list(values)",
        ),
    ] {
        assert!(
            checked(&invalid)
                .expect_err("invalid byte/list conversion input is rejected")
                .iter()
                .any(|diagnostic| matches!(diagnostic.code.as_str(), "E2113" | "E2151"))
        );
    }
}

#[test]
fn checks_byte_indexing_as_u8_and_literal_range() {
    let source = "defmodule Main do\n  def first(data: bytes) -> u8 do\n    data[0]\n  end\n  def main() -> i32 do\n    if first(String.bytes(\"abc\")) == 97 do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("bytes indexing returns u8");
    let debug = typed.debug_tree();
    assert!(debug.contains("index: u8"), "{debug}");
    verify(&typed).expect("byte-index Typed AST verifies");

    let out_of_range = "defmodule Main do\n  def invalid() -> u8 do\n    256\n  end\nend\n";
    assert!(
        checked(out_of_range)
            .expect_err("u8 literals are range checked")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2106")
    );
}

#[test]
fn checks_every_integer_width_and_rejects_out_of_range_literals() {
    let source = "defmodule Main do\n  def i8_value() -> i8 do\n    127\n  end\n  def i16_value() -> i16 do\n    32767\n  end\n  def i32_value() -> i32 do\n    2147483647\n  end\n  def i64_value() -> i64 do\n    9223372036854775807\n  end\n  def isize_value() -> isize do\n    42\n  end\n  def u8_value() -> u8 do\n    255\n  end\n  def u16_value() -> u16 do\n    65535\n  end\n  def u32_value() -> u32 do\n    4294967295\n  end\n  def u64_value() -> u64 do\n    18446744073709551615\n  end\n  def usize_value() -> usize do\n    42\n  end\n  def main() -> i32 do\n    i8_value() + 1\n    i16_value() / 1\n    isize_value() % 2\n    u8_value() * 1\n    u16_value() - 1\n    u32_value() + 1\n    u64_value() / 1\n    usize_value() % 2\n    0\n  end\nend\n";
    let typed = checked(source).expect("all integer widths type check");
    for expected in [
        Type::I8,
        Type::I16,
        Type::I32,
        Type::I64,
        Type::Isize,
        Type::U8,
        Type::U16,
        Type::U32,
        Type::U64,
        Type::Usize,
    ] {
        assert!(typed.types.contains(&expected), "missing {expected:?}");
    }
    verify(&typed).expect("integer-width Typed AST verifies");

    let cases = [
        ("i8", "128".to_owned()),
        ("i16", "32768".to_owned()),
        ("i32", "2147483648".to_owned()),
        ("i64", "9223372036854775808".to_owned()),
        ("isize", (isize::MAX as u128 + 1).to_string()),
        ("u8", "256".to_owned()),
        ("u16", "65536".to_owned()),
        ("u32", "4294967296".to_owned()),
        ("u64", "18446744073709551616".to_owned()),
        ("usize", (usize::MAX as u128 + 1).to_string()),
    ];
    for (ty, literal) in cases {
        let source =
            format!("defmodule Main do\n  def invalid() -> {ty} do\n    {literal}\n  end\nend\n");
        assert!(
            checked(&source)
                .expect_err("out-of-range literal is rejected")
                .iter()
                .any(|diagnostic| diagnostic.code == "E2106"),
            "missing range diagnostic for {ty}"
        );
    }
}

#[test]
fn checks_integer_unary_bitwise_and_shift_operators() {
    let source = "defmodule Main do\n  def signed(value: i16, count: usize) -> i16 do\n    ~ -value & 255 | value << count ^ value >> count\n  end\n  def unsigned(value: u32, count: usize) -> u32 do\n    ~value & 255 | value << count ^ value >> count\n  end\n  def main() -> i32 do\n    signed(4, 1)\n    unsigned(4, 1)\n    0\n  end\nend\n";
    let typed = checked(source).expect("integer unary, bitwise, and shift operators type check");
    let debug = typed.debug_tree();
    assert!(debug.contains("integer unary Negate"), "{debug}");
    assert!(debug.contains("integer unary BitwiseNot"), "{debug}");
    assert!(debug.contains("integer binary ShiftLeft"), "{debug}");
    assert!(debug.contains("integer binary ShiftRight"), "{debug}");
    verify(&typed).expect("integer operator Typed AST verifies");

    checked("defmodule Main do\n  def minimum() -> i8 do\n    -128\n  end\nend\n")
        .expect("the signed minimum literal is representable through unary minus");

    for invalid in [
        "defmodule Main do\n  def bad(value: u8) -> u8 do\n    -value\n  end\nend\n",
        "defmodule Main do\n  def bad(value: u8, count: i32) -> u8 do\n    value << count\n  end\nend\n",
        "defmodule Main do\n  def bad(left: u8, right: u16) -> u8 do\n    left & right\n  end\nend\n",
        "defmodule Main do\n  def bad() -> i8 do\n    -129\n  end\nend\n",
    ] {
        checked(invalid).expect_err("invalid integer operator types are rejected");
    }
}

#[test]
fn rejects_compile_time_known_integer_failures_but_keeps_dynamic_checks() {
    for expression in [
        "127 + 1",
        "0 - 1",
        "64 * 2",
        "1 / 0",
        "-128 / -1",
        "-128 % -1",
        "- -128",
        "1 << 8",
        "64 << 1",
    ] {
        let ty = if expression == "0 - 1" { "u8" } else { "i8" };
        let source = format!(
            "defmodule Main do\n  def invalid() -> {ty} do\n    {expression}\n  end\nend\n"
        );
        assert!(
            checked(&source)
                .expect_err("compile-time-known integer failure is rejected")
                .iter()
                .any(|diagnostic| diagnostic.code == "E2106"),
            "missing compile-time diagnostic for {expression}"
        );
    }

    let dynamic = "defmodule Main do\n  def add(left: i8, right: i8) -> i8 do\n    left + right\n  end\n  def divide(left: i8, right: i8) -> i8 do\n    left / right\n  end\n  def negate(value: i8) -> i8 do\n    -value\n  end\n  def shift(value: i8, count: usize) -> i8 do\n    value << count\n  end\nend\n";
    checked(dynamic).expect("dynamic integer failures remain runtime checks");
}

#[test]
fn checks_explicit_integer_conversions_and_static_ranges() {
    let source = "defmodule Main do\n  def narrow(value: i64) -> i8 do\n    i8(value)\n  end\n  def pipeline(value: i64) -> u16 do\n    value |> u16()\n  end\n  def main() -> i32 do\n    i8(1)\n    i16(2)\n    i32(3)\n    i64(4)\n    isize(5)\n    u8(6)\n    u16(7)\n    u32(8)\n    u64(9)\n    usize(10)\n    narrow(11)\n    pipeline(12)\n    0\n  end\nend\n";
    let typed = checked(source).expect("all integer conversion targets type check");
    assert!(
        typed.debug_tree().matches("integer convert").count() >= 12,
        "{}",
        typed.debug_tree()
    );
    verify(&typed).expect("integer conversion Typed AST verifies");

    for invalid in [
        "defmodule Main do\n  def bad() -> u8 do\n    u8(-1)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> u8 do\n    u8(256)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> i8 do\n    i8(true)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> i8 do\n    i8(1, 2)\n  end\nend\n",
    ] {
        checked(invalid).expect_err("invalid integer conversion is rejected");
    }
}

#[test]
fn checks_integer_to_rune_conversions_and_unicode_scalar_ranges() {
    let source = "defmodule Main do\n  def convert(value: i64) -> rune do\n    rune(value)\n  end\n  def pipeline(value: u32) -> rune do\n    value |> rune()\n  end\n  def main() -> i32 do\n    rune(65)\n    rune(1114111)\n    convert(128578)\n    pipeline(66)\n    0\n  end\nend\n";
    let typed = checked(source).expect("valid Unicode scalar conversions type check");
    assert!(
        typed.debug_tree().matches("integer convert").count() >= 4,
        "{}",
        typed.debug_tree()
    );
    verify(&typed).expect("integer-to-rune conversion Typed AST verifies");

    for invalid in [
        "defmodule Main do\n  def bad() -> rune do\n    rune(-1)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> rune do\n    rune(55296)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> rune do\n    rune(1114112)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> rune do\n    rune(true)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> rune do\n    rune(1, 2)\n  end\nend\n",
    ] {
        checked(invalid).expect_err("invalid integer-to-rune conversion is rejected");
    }
}

#[test]
fn checks_all_integer_wrapping_intrinsics() {
    let source = "defmodule Main do\n  def signed(value: i8, count: usize) -> i8 do\n    I8.wrapping_add(I8.wrapping_neg(value), I8.wrapping_shl(value, count))\n  end\n  def unsigned(value: u64, count: usize) -> u64 do\n    U64.wrapping_sub(U64.wrapping_mul(value, value), U64.wrapping_shr(value, count))\n  end\n  def main() -> i32 do\n    I16.wrapping_add(1, 2)\n    I32.wrapping_sub(1, 2)\n    I64.wrapping_mul(1, 2)\n    Isize.wrapping_neg(1)\n    U8.wrapping_shl(1, 9)\n    U16.wrapping_shr(1, 17)\n    U32.wrapping_neg(1)\n    Usize.wrapping_add(1, 2)\n    signed(1, 2)\n    unsigned(2, 1)\n    0\n  end\nend\n";
    let typed = checked(source).expect("all per-width wrapping intrinsics type check");
    assert!(
        typed.debug_tree().matches("wrapping integer").count() >= 14,
        "{}",
        typed.debug_tree()
    );
    verify(&typed).expect("wrapping integer Typed AST verifies");

    for invalid in [
        source.replace("I16.wrapping_add(1, 2)", "I16.wrapping_add(1, true)"),
        source.replace("U8.wrapping_shl(1, 9)", "U8.wrapping_shl(1, 9 :: u8)"),
        source.replace("U32.wrapping_neg(1)", "U32.wrapping_neg(1, 2)"),
        source.replace("Usize.wrapping_add(1, 2)", "Usize.wrapping_add(1)"),
    ] {
        checked(&invalid).expect_err("invalid wrapping intrinsic input is rejected");
    }
}

#[test]
fn checks_float_literals_arithmetic_negation_and_comparisons() {
    let source = "defmodule Main do\n  def single(value: f32) -> f32 do\n    positive = value * 2.0 + 0.5\n    -positive\n  end\n  def double(value: f64) -> f64 do\n    value / 2.0 - value % 1.5\n  end\n  def main() -> i32 do\n    if single(1.25) < 0.0 and double(4.0) >= 1.0 and 0.0 == -0.0 do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("f32 and f64 expressions type check");
    let debug = typed.debug_tree();
    assert!(debug.contains("float negate: f32"), "{debug}");
    assert!(debug.contains("binary Divide: f64"), "{debug}");
    assert!(debug.contains("comparison Less: bool"), "{debug}");
    verify(&typed).expect("float Typed AST verifies");

    for invalid in [
        source.replace("single(1.25)", "single(true)"),
        source.replace("value * 2.0", "value * 2"),
        source.replace("single(1.25)", "single(1e100)"),
    ] {
        checked(&invalid).expect_err("invalid float expression is rejected");
    }
}

#[test]
fn checks_float_literal_patterns_and_signed_zero_usefulness() {
    let source = "defmodule Main do\n  def classify(value: f64) -> i32 do\n    match value do\n      0.0 -> 0\n      -1.5 -> 1\n      _ -> 2\n    end\n  end\n  def single(value: f32) -> i32 do\n    match value do\n      1.25 -> 1\n      _ -> 0\n    end\n  end\nend\n";
    let typed = checked(source).expect("positive and negative float patterns type check");
    verify(&typed).expect("float pattern Typed AST verifies");

    let duplicate_zero = source.replace("      -1.5 -> 1", "      -0.0 -> 1");
    assert!(
        checked(&duplicate_zero)
            .expect_err("positive and negative zero are duplicate float patterns")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2125")
    );
    let non_exhaustive = source.replace("      _ -> 2\n", "");
    assert!(
        checked(&non_exhaustive)
            .expect_err("finite float literal patterns do not exhaust the float domain")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2126")
    );
    let out_of_range = source.replace("      1.25 -> 1", "      1e100 -> 1");
    assert!(
        checked(&out_of_range)
            .expect_err("out-of-range f32 pattern is rejected")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2106")
    );
}

#[test]
fn checks_explicit_float_numeric_conversions_and_static_failures() {
    let source = "defmodule Main do\n  def to_single(value: i64) -> f32 do\n    f32(value)\n  end\n  def to_double(value: u64) -> f64 do\n    value |> f64()\n  end\n  def truncate(value: f64) -> i32 do\n    i32(value)\n  end\n  def narrow(value: f64) -> f32 do\n    f32(value)\n  end\n  def widen(value: f32) -> f64 do\n    f64(value)\n  end\n  def main() -> i32 do\n    to_single(-42)\n    to_double(42)\n    truncate(42.9)\n    narrow(1.5)\n    widen(1.5)\n    u8(-0.9)\n    0\n  end\nend\n";
    let typed = checked(source).expect("all explicit float conversion directions type check");
    assert!(
        typed.debug_tree().matches("numeric convert").count() >= 6,
        "{}",
        typed.debug_tree()
    );
    verify(&typed).expect("numeric conversion Typed AST verifies");

    for invalid in [
        "defmodule Main do\n  def bad() -> i8 do\n    i8(0.0 / 0.0)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> i8 do\n    i8(1.0 / 0.0)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> u8 do\n    u8(-1.0)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> i8 do\n    i8(128.0)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> f32 do\n    f32(true)\n  end\nend\n",
        "defmodule Main do\n  def bad() -> f64 do\n    f64(1, 2)\n  end\nend\n",
    ] {
        checked(invalid).expect_err("invalid explicit numeric conversion is rejected");
    }
}

#[test]
fn checks_map_size_and_rejects_non_map_inputs() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    values: Map(i32, string) = %{1 => \"one\", 2 => \"two\"}\n    if Map.size(values) == 2 do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("Map.size accepts an immutable map");
    assert!(typed.debug_tree().contains("collection length: usize"));

    let invalid = source.replace("Map.size(values)", "Map.size(1)");
    assert!(
        checked(&invalid)
            .expect_err("Map.size rejects non-map inputs")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2151")
    );
}

#[test]
fn checks_immutable_map_put_and_remove_inputs() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    empty: Map(i32, string) = Map.new()\n    original: Map(i32, string) = %{1 => \"one\"}\n    updated = Map.put(original, 1, \"uno\")\n    removed = Map.remove(updated, 1)\n    fetched = Map.fetch(updated, 1)\n    if Map.size(empty) == 0 and Map.size(original) == 1 and Map.size(removed) == 0 do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("map updates preserve the map type");
    let debug = typed.debug_tree();
    assert!(debug.contains("map put: Map(i32, string)"), "{debug}");
    assert!(debug.contains("map remove: Map(i32, string)"), "{debug}");
    assert!(debug.contains("map fetch:"), "{debug}");

    let ambiguous = source.replace("empty: Map(i32, string) = Map.new()", "empty = Map.new()");
    assert!(
        checked(&ambiguous)
            .expect_err("Map.new requires an expected map type")
            .iter()
            .any(|diagnostic| diagnostic.code == "E2107")
    );

    for invalid in [
        source.replace(
            "Map.put(original, 1, \"uno\")",
            "Map.put(original, true, \"uno\")",
        ),
        source.replace("Map.put(original, 1, \"uno\")", "Map.put(original, 1, 2)"),
        source.replace("Map.remove(updated, 1)", "Map.remove(updated, true)"),
        source.replace("Map.fetch(updated, 1)", "Map.fetch(updated, true)"),
    ] {
        assert!(
            checked(&invalid)
                .expect_err("map key and value types are exact")
                .iter()
                .any(|diagnostic| diagnostic.code == "E2113")
        );
    }
}

#[test]
fn checks_composite_map_keys_structural_equality_and_insertion_order_view() {
    let source = "defmodule Main do\n  def main() -> i32 do\n    values: Map({i32, i32}, string) = %{{1, 2} => \"first\", {3, 4} => \"second\"}\n    ordered = Enum.to_list(Map.put(values, {1, 2}, \"updated\"))\n    same: Map({i32, i32}, string) = %{{3, 4} => \"second\", {1, 2} => \"updated\"}\n    if values != same and Map.put(values, {1, 2}, \"updated\") == same do\n      0\n    else\n      1\n    end\n  end\nend\n";
    let typed = checked(source).expect("composite map keys and structural map equality type check");
    let debug = typed.debug_tree();
    assert!(
        debug.contains("map to list: [{{i32, i32}, string}]"),
        "{debug}"
    );
    assert!(debug.contains("comparison Equal: bool"), "{debug}");
    verify(&typed).expect("map equality and insertion-order view verify");
}

#[test]
fn checks_typed_file_reader_writer_and_exhaustive_results() {
    let source = "defmodule Main do\n  def consume_ok(value: {:ok, bytes}) -> i32 do\n    match value do\n      {:ok, data} -> if Bytes.byte_size(data) > 0 do 1 else 0 end\n    end\n  end\n  def consume(reader: File.Reader) -> i32 do\n    match Reader.read(reader, 1024) do\n      value: {:ok, bytes} -> consume_ok(value)\n      _ -> 0\n    end\n  end\n  def produce(writer: File.Writer, data: bytes) -> i32 do\n    match Writer.write(writer, data) do\n      value: {:ok, unit} -> 0\n      value: {:error, File.Error} -> 1\n    end\n  end\n  def cleanup(reader: File.Reader) -> unit do\n    match File.close(reader) do\n      value: {:ok, unit} -> unit\n      value: {:error, File.Error} -> unit\n    end\n  end\n  def opened(value: {:ok, File.Reader}) -> i32 do\n    match value do\n      {:ok, reader} ->\n        defer cleanup(reader)\n        consume(reader)\n    end\n  end\n  def main() -> i32 do\n    match File.open_read(\"input.txt\") do\n      value: {:ok, File.Reader} -> opened(value)\n      value: {:error, File.Error} -> 0\n    end\n  end\nend\n";
    let typed = checked(source).expect("standard file results type check exhaustively");
    let debug = typed.debug_tree();
    assert!(debug.contains("standard ReaderRead"), "{debug}");
    assert!(debug.contains("standard WriterWrite"), "{debug}");
    assert!(debug.contains("standard FileOpenRead"), "{debug}");
    verify(&typed).expect("standard-call Typed AST verifies");
}
