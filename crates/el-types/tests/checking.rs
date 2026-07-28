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
        ("true < false\n    0", "E2139"),
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
