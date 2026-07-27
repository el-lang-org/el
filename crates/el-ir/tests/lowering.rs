use el_ir::{Terminator, ValueId, lower, verify};
use el_parser::parse;
use el_resolve::resolve;
use el_span::SourceMap;
use el_types::{Type, TypeId, check};

fn lowered(source: &str) -> el_ir::GenericModule {
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.el", source);
    let parsed = parse(file, source).expect("fixture parses");
    let resolved = resolve(&parsed).expect("fixture resolves");
    let typed = check(&resolved).expect("fixture type checks");
    lower(&typed)
}

#[test]
fn lowers_calls_and_mutation_in_left_to_right_order() {
    let module = lowered(
        "defmodule Main do\n  def identity(value: a) -> a do\n    value\n  end\n  def main() -> i32 do\n    mut answer: i32 = identity(40)\n    answer := answer + 2\n    answer\n  end\nend\n",
    );

    assert_eq!(
        module.debug_text(),
        "fn f0 identity -> t4 {\n  b0:\n    return v0\n}\nfn f1 main -> t0 {\n  slot q0: t0\n  b0:\n    v0 = const Integer(40): t0\n    v1 = call f0(v0): t0\n    store q0, v1\n    v2 = load q0: t0\n    v3 = const Integer(2): t0\n    v4 = checked.Add v2, v3: t0\n    store q0, v4\n    v5 = load q0: t0\n    return v5\n}\n"
    );
}

#[test]
fn core_verifier_rejects_an_undefined_return_value() {
    let mut module = lowered("defmodule Main do\n  def main() -> i32 do\n    0\n  end\nend\n");
    module.functions[0].blocks[0].terminator = Terminator::Return {
        value: ValueId(999),
        origin: module.functions[0].span,
    };

    assert!(
        verify(&module)
            .expect_err("malformed Core IR is rejected")
            .iter()
            .any(|error| error.contains("undefined value"))
    );
}

#[test]
fn core_verifier_rejects_an_unjustified_unreachable_entry() {
    let mut module = lowered("defmodule Main do\n  def main() -> i32 do\n    0\n  end\nend\n");
    module.functions[0].blocks[0].terminator = Terminator::Unreachable {
        origin: module.functions[0].span,
    };

    assert!(
        verify(&module)
            .expect_err("entry cannot be an internal-impossibility sink")
            .iter()
            .any(|error| error.contains("unjustified unreachable"))
    );
}

#[test]
fn lowers_explicit_union_injection() {
    let module = lowered(
        "defmodule Main do\n  @type Scalar = bool | i64\n  def choose() -> Scalar do\n    true\n  end\nend\n",
    );

    assert!(module.debug_text().contains("inject t2"));
    verify(&module).expect("union Core IR verifies");
}

#[test]
fn core_verifier_rejects_noncanonical_union_types() {
    let mut module = lowered("defmodule Main do\n  def main() -> i32 do\n    0\n  end\nend\n");
    module.types.push(Type::Union(vec![TypeId(0), TypeId(0)]));

    assert!(
        verify(&module)
            .expect_err("duplicate union members are rejected")
            .iter()
            .any(|error| error.contains("duplicate member"))
    );
}

#[test]
fn lowers_lists_tagged_tuples_and_union_injection_in_order() {
    let module = lowered(
        "defmodule Main do\n  @type Option(a) = {:some, a} | :none\n  def values(tail: [i64]) -> [i64] do\n    [1, 2 | tail]\n  end\n  def main() -> Option(i64) do\n    {:some, 1}\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("list [v1, v2] | v0"));
    assert!(debug.contains("tuple {v0, v1}"));
    assert!(debug.contains("inject"));
    verify(&module).expect("composite Core IR verifies");
}

#[test]
fn lowers_arrays_and_maps_in_source_order() {
    let module = lowered(
        "defmodule Main do\n  def array() -> [i64; 2] do\n    #[1, 2]\n  end\n  def map() -> Map(i64, bool) do\n    %{1 => true, 2 => false}\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("array #[v0, v1]"));
    assert!(debug.contains("map %{v0 => v1, v2 => v3}"));
    verify(&module).expect("collection Core IR verifies");
}

#[test]
fn lowers_nested_scopes_to_typed_cfg_edges() {
    let module = lowered(
        "defmodule Main do\n  def choose(flag: bool) -> i64 do\n    if flag do\n      1\n    else\n      2\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert_eq!(debug, include_str!("snapshots/nested_if.core"));
    assert!(debug.contains("branch_if v0, b1, b2"));
    assert!(debug.contains("branch b3(v1)"));
    assert!(debug.contains("b3(v3: t1):"));
    verify(&module).expect("multi-block CFG verifies");
}

#[test]
fn verifier_rejects_wrong_cfg_argument_signature() {
    let mut module = lowered(
        "defmodule Main do\n  def choose(flag: bool) -> i64 do\n    if flag do\n      1\n    else\n      2\n    end\n  end\nend\n",
    );
    let Terminator::Branch { arguments, .. } = &mut module.functions[0].blocks[1].terminator else {
        panic!("then edge")
    };
    arguments.clear();
    assert!(
        verify(&module)
            .expect_err("edge mismatch is rejected")
            .iter()
            .any(|error| error.contains("supplies 0 arguments"))
    );
}

#[test]
fn lowers_exhaustive_pattern_facts_to_a_switch_cfg() {
    let module = lowered(
        "defmodule Main do\n  def choose(flag: bool) -> i64 do\n    match flag do\n      true -> 1\n      false -> 2\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert_eq!(debug, include_str!("snapshots/exhaustive_bool.core"));
    assert!(debug.contains("switch v0 [Boolean(true) => b2] default b3"));
    assert!(debug.contains("b1(v3: t1):"));
    verify(&module).expect("pattern CFG verifies");
}

#[test]
fn verifier_rejects_unknown_duplicate_and_mistyped_cfg_structure() {
    let source = "defmodule Main do\n  def choose(flag: bool) -> i64 do\n    if flag do\n      1\n    else\n      2\n    end\n  end\nend\n";

    let mut unknown = lowered(source);
    let Terminator::Branch { target, .. } = &mut unknown.functions[0].blocks[1].terminator else {
        panic!("branch")
    };
    *target = el_ir::BlockId(999);
    assert!(
        verify(&unknown)
            .expect_err("unknown target")
            .iter()
            .any(|error| error.contains("unknown block"))
    );

    let mut duplicate = lowered(source);
    let repeated = duplicate.functions[0].blocks[1].clone();
    duplicate.functions[0].blocks.push(repeated);
    assert!(
        verify(&duplicate)
            .expect_err("duplicate block")
            .iter()
            .any(|error| error.contains("block twice"))
    );

    let mut mistyped = lowered(source);
    let Terminator::CondBranch { condition, .. } = &mut mistyped.functions[0].blocks[0].terminator
    else {
        panic!("conditional")
    };
    *condition = ValueId(1);
    assert!(
        verify(&mistyped)
            .expect_err("non-bool condition")
            .iter()
            .any(|error| error.contains("non-bool"))
    );
}

#[test]
fn preserves_complete_implementation_metadata_at_the_core_boundary() {
    let module = lowered(
        "defmodule Main do\n  defstruct Box do\n    value: i64\n  end\n  defprotocol Render do\n    type Output\n    def render(value: Box) -> i64\n  end\n  defimpl Render, for: Box do\n    type Output = i64\n    def render(value: Box) -> i64 do\n      0\n    end\n  end\nend\n",
    );
    assert_eq!(module.implementations.len(), 1);
    assert!(
        module
            .debug_text()
            .starts_with("impl i0 Render for t4 [Output=t1]\n")
    );
    verify(&module).expect("implementation metadata verifies");
}

#[test]
fn lowers_typed_union_patterns_to_member_switches_and_projections() {
    let module = lowered(
        "defmodule Main do\n  @type Scalar = bool | i64\n  def choose(value: Scalar) -> i64 do\n    match value do\n      number: i64 -> number\n      flag: bool -> 0\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("UnionMember(TypeId(1))"));
    assert!(debug.contains("project t1 v0: t1"));
    verify(&module).expect("typed union match Core verifies");
}

#[test]
fn lowers_structural_pattern_tests_projections_and_bindings() {
    let module = lowered(
        "defmodule Main do\n  defstruct Point do\n    flag: bool\n    value: i64\n  end\n  def list(value: [i64]) -> i64 do\n    match value do\n      [] -> 0\n      [head | _] -> head\n    end\n  end\n  def tuple(value: {bool, i64}) -> i64 do\n    match value do\n      {true, number} -> number\n      {false, _} -> 0\n    end\n  end\n  def structure(value: Point) -> i64 do\n    match value do\n      %Point{flag: true, value: number} -> number\n      %Point{flag: false} -> 0\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("ListEmpty"));
    assert!(debug.contains("ListCons"));
    assert!(debug.contains("list_head"));
    assert!(debug.contains("tuple_project"));
    assert!(debug.contains("struct_project"));
    verify(&module).expect("structural pattern Core verifies");
}
