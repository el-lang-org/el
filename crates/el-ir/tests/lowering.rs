use el_ir::{
    EntryPointError, FunctionId, Operation, Terminator, ValueId, executable_reachability_roots,
    lower, monomorphize, verify, verify_concrete,
};
use el_parser::parse;
use el_resolve::{resolve, resolve_package};
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

fn lowered_package(sources: &[&str]) -> el_ir::GenericModule {
    let mut source_map = SourceMap::new();
    let parsed = sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let file = source_map.add_file(format!("src/{index}.el"), *source);
            parse(file, source).expect("fixture parses")
        })
        .collect::<Vec<_>>();
    let resolved = resolve_package(&parsed).expect("fixture resolves");
    let typed = el_types::check_package(&resolved).expect("fixture type checks");
    lower(&typed)
}

#[test]
fn selects_main_main_as_the_deterministic_executable_root() {
    let module = lowered_package(&[
        "defmodule Library do\n  def main() -> i32 do\n    1\n  end\n  def answer() -> i32 do\n    42\n  end\nend\n",
        "defmodule Main do\n  def helper() -> i32 do\n    0\n  end\n  def main() -> i32 do\n    Library.answer()\n  end\nend\n",
    ]);

    let roots = executable_reachability_roots(&module).expect("valid executable entry");

    assert_eq!(roots.functions, vec![FunctionId(3)]);
    assert_eq!(module.functions[3].module_name, "Main");
}

#[test]
fn rejects_missing_private_and_invalid_executable_entries() {
    let missing = lowered("defmodule Library do\n  def main() -> i32 do\n    0\n  end\nend\n");
    assert_eq!(
        executable_reachability_roots(&missing),
        Err(EntryPointError::Missing)
    );

    let private = lowered("defmodule Main do\n  defp main() -> i32 do\n    0\n  end\nend\n");
    assert!(matches!(
        executable_reachability_roots(&private),
        Err(EntryPointError::Private { .. })
    ));

    let parameters = lowered(
        "defmodule Main do\n  def main(argument: i32) -> i32 do\n    argument\n  end\nend\n",
    );
    assert!(matches!(
        executable_reachability_roots(&parameters),
        Err(EntryPointError::HasParameters { count: 1, .. })
    ));

    let result = lowered("defmodule Main do\n  def main() -> i64 do\n    0\n  end\nend\n");
    assert!(matches!(
        executable_reachability_roots(&result),
        Err(EntryPointError::WrongResult { .. })
    ));
}

#[test]
fn monomorphizes_only_reachable_generic_functions_in_key_order() {
    let module = lowered(
        "defmodule Main do\n  def identity(value: a) -> a do\n    value\n  end\n  def dead() -> i64 do\n    99\n  end\n  def main() -> i32 do\n    identity(42)\n  end\nend\n",
    );
    let roots = executable_reachability_roots(&module).expect("valid entry");

    let concrete = monomorphize(&module, &roots).expect("reachable graph specializes");

    assert_eq!(concrete.functions.len(), 2);
    assert_eq!(concrete.functions[0].name, "identity");
    assert_eq!(concrete.functions[0].parameters[0].ty, TypeId(0));
    assert_eq!(concrete.functions[0].result, TypeId(0));
    assert_eq!(concrete.functions[1].name, "main");
    assert_eq!(concrete.roots.functions, vec![FunctionId(1)]);
    let Operation::Call {
        function,
        substitutions,
        ..
    } = &concrete.functions[1].blocks[0].operations[1]
    else {
        panic!("main calls identity")
    };
    assert_eq!(*function, FunctionId(0));
    assert!(substitutions.is_empty());
}

#[test]
fn specializes_reachable_generic_nominal_layouts() {
    let module = lowered(
        "defmodule Main do\n  defstruct Box(a) do\n    value: a\n  end\n  def loop() -> a do\n    loop()\n  end\n  def make() -> Box(i32) do\n    loop()\n  end\n  def main() -> i32 do\n    make()\n    0\n  end\nend\n",
    );
    let roots = executable_reachability_roots(&module).expect("valid entry");

    let concrete = monomorphize(&module, &roots).expect("generic layout specializes");

    assert_eq!(concrete.structs.len(), 1);
    assert_eq!(concrete.structs[0].name, "Box");
    assert_eq!(concrete.structs[0].arguments, vec![TypeId(0)]);
    assert_eq!(concrete.structs[0].fields[0].1, TypeId(0));
    assert!(
        concrete
            .types
            .iter()
            .all(|ty| !matches!(ty, Type::Parameter { .. }))
    );
}

#[test]
fn reuses_identical_specializations_and_recursive_worklist_entries() {
    let module = lowered(
        "defmodule Main do\n  def recur(value: a) -> a do\n    if true do\n      value\n    else\n      recur(value)\n    end\n  end\n  def main() -> i32 do\n    recur(1 :: i32)\n    recur(2 :: i32)\n  end\nend\n",
    );
    let roots = executable_reachability_roots(&module).expect("valid entry");

    let concrete = monomorphize(&module, &roots).expect("recursive generic specializes");

    assert_eq!(
        concrete
            .functions
            .iter()
            .filter(|function| function.name == "recur")
            .count(),
        1
    );
    let recursive = concrete
        .functions
        .iter()
        .find(|function| function.name == "recur")
        .expect("recur specialization");
    assert!(recursive.blocks.iter().any(|block| block.operations.iter().any(
        |operation| matches!(operation, Operation::Call { function, substitutions, .. } if *function == recursive.id && substitutions.is_empty())
    )));
    verify_concrete(&concrete).expect("reused recursive specialization verifies");
}

#[test]
fn specialization_order_is_independent_of_call_discovery_order() {
    let first = lowered(
        "defmodule Main do\n  def identity(value: a) -> a do\n    value\n  end\n  def main() -> i32 do\n    identity(1 :: i64)\n    identity(2 :: i32)\n  end\nend\n",
    );
    let second = lowered(
        "defmodule Main do\n  def identity(value: a) -> a do\n    value\n  end\n  def main() -> i32 do\n    identity(2 :: i32)\n    identity(1 :: i64)\n    2\n  end\nend\n",
    );

    let first = monomorphize(
        &first,
        &executable_reachability_roots(&first).expect("first entry"),
    )
    .expect("first graph");
    let second = monomorphize(
        &second,
        &executable_reachability_roots(&second).expect("second entry"),
    )
    .expect("second graph");

    let first_specializations = first
        .functions
        .iter()
        .filter(|function| function.name == "identity")
        .map(|function| function.specialization_arguments.clone())
        .collect::<Vec<_>>();
    let second_specializations = second
        .functions
        .iter()
        .filter(|function| function.name == "identity")
        .map(|function| function.specialization_arguments.clone())
        .collect::<Vec<_>>();
    assert_eq!(first_specializations, second_specializations);
}

#[test]
fn concrete_verifier_rejects_residuals_duplicates_and_abstract_layouts() {
    let module = lowered(
        "defmodule Main do\n  def identity(value: a) -> a do\n    value\n  end\n  def main() -> i32 do\n    identity(1)\n  end\nend\n",
    );
    let roots = executable_reachability_roots(&module).expect("valid entry");
    let concrete = monomorphize(&module, &roots).expect("baseline concrete module");

    let mut residual_type = concrete.clone();
    residual_type.types.push(Type::Parameter {
        owner: residual_type.functions[0].declaration,
        name: "a".to_owned(),
    });
    assert!(
        verify_concrete(&residual_type)
            .expect_err("residual type parameter is rejected")
            .iter()
            .any(|error| error.contains("residual type parameter"))
    );

    let mut residual_call = concrete.clone();
    let call = residual_call
        .functions
        .iter_mut()
        .find(|function| function.name == "main")
        .and_then(|function| {
            function.blocks[0]
                .operations
                .iter_mut()
                .find_map(|operation| match operation {
                    Operation::Call { substitutions, .. } => Some(substitutions),
                    _ => None,
                })
        })
        .expect("main call");
    call.push((TypeId(0), TypeId(0)));
    assert!(
        verify_concrete(&residual_call)
            .expect_err("residual call substitution is rejected")
            .iter()
            .any(|error| error.contains("residual type substitution"))
    );

    let mut duplicate = concrete.clone();
    let mut repeated = duplicate.functions[0].clone();
    repeated.id = FunctionId(99);
    duplicate.functions.push(repeated);
    assert!(
        verify_concrete(&duplicate)
            .expect_err("duplicate specialization is rejected")
            .iter()
            .any(|error| error.contains("duplicates an existing specialization"))
    );

    let layout_module = lowered(
        "defmodule Main do\n  defstruct Box(a) do\n    value: a\n  end\n  def loop() -> a do\n    loop()\n  end\n  def make() -> Box(i32) do\n    loop()\n  end\n  def main() -> i32 do\n    make()\n    0\n  end\nend\n",
    );
    let mut abstract_layout = monomorphize(
        &layout_module,
        &executable_reachability_roots(&layout_module).expect("layout entry"),
    )
    .expect("baseline layout module");
    abstract_layout.structs.clear();
    assert!(
        verify_concrete(&abstract_layout)
            .expect_err("abstract layout is rejected")
            .iter()
            .any(|error| error.contains("has no concrete layout"))
    );
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
fn lowers_loops_short_circuit_logic_comparisons_and_nested_returns() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    mut n: i32 = 5\n    mut result: i32 = 1\n    while n > 1 do\n      result := result * n\n      n := n - 1\n    end\n    if false and 1 / 0 == 0 do\n      return 1\n    end\n    if true or 1 / 0 == 0 do\n      return result\n    end\n    0\n  end\nend\n",
    );
    let debug = module.debug_text();

    assert!(debug.contains("compare.Greater"));
    assert!(debug.contains("compare.Equal"));
    assert!(debug.matches("branch_if").count() >= 5);
    assert!(debug.matches("return").count() >= 2);
    verify(&module).expect("control-flow CFG verifies");
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
fn verifier_rejects_mistyped_union_discriminants_and_payload_projections() {
    let source = "defmodule Main do\n  @type Scalar = bool | i64\n  def choose(value: Scalar) -> i64 do\n    match value do\n      number: i64 -> number\n      flag: bool -> 0\n    end\n  end\nend\n";

    let mut bad_switch = lowered(source);
    let switch_ty = bad_switch.functions[0]
        .blocks
        .iter_mut()
        .find_map(|block| match &mut block.terminator {
            Terminator::Switch { subject_ty, .. } => Some(subject_ty),
            _ => None,
        })
        .expect("union discriminant switch");
    *switch_ty = TypeId(0);
    assert!(
        verify(&bad_switch)
            .expect_err("mistyped switch is rejected")
            .iter()
            .any(|error| error.contains("incorrect subject type"))
    );

    let mut bad_projection = lowered(source);
    let union_ty = bad_projection.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match operation {
            Operation::UnionProject { union_ty, .. } => Some(union_ty),
            _ => None,
        })
        .expect("union payload projection");
    *union_ty = TypeId(1);
    assert!(
        verify(&bad_projection)
            .expect_err("projection from a non-union is rejected")
            .iter()
            .any(|error| error.contains("invalid source"))
    );
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
