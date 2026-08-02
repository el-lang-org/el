use el_ir::{
    CollectionEffect, ConcreteStruct, CoreFailureCategory, EntryPointError, FunctionId,
    ManagedValueClass, Operation, Terminator, ValueId, collection_point_roots,
    executable_reachability_roots, lower, managed_value_class, monomorphize,
    operation_collection_effect, verify, verify_concrete,
};
use el_parser::parse;
use el_resolve::{DeclId, resolve, resolve_package};
use el_span::SourceMap;
use el_types::{EnumVisitKind, Type, TypeId, check};

fn lowered(source: &str) -> el_ir::GenericModule {
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.ell", source);
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
            let file = source_map.add_file(format!("src/{index}.ell"), *source);
            parse(file, source).expect("fixture parses")
        })
        .collect::<Vec<_>>();
    let resolved = resolve_package(&parsed).expect("fixture resolves");
    let typed = el_types::check_package(&resolved).expect("fixture type checks");
    lower(&typed)
}

#[test]
fn lowers_and_verifies_integer_unary_bitwise_and_shift_operations() {
    let source = "defmodule Main do\n  def transform(value: i16, count: usize) -> i16 do\n    ~ -value & 255 | value << count ^ value >> count\n  end\n  def main() -> i32 do\n    transform(4, 1)\n    0\n  end\nend\n";
    let mut generic = lowered(source);
    let debug = generic.debug_text();
    assert!(debug.contains("integer_unary.Negate"), "{debug}");
    assert!(debug.contains("integer_unary.BitwiseNot"), "{debug}");
    assert!(debug.contains("integer_binary.BitwiseAnd"), "{debug}");
    assert!(debug.contains("integer_binary.ShiftLeft"), "{debug}");
    assert!(debug.contains("InvalidShift"), "{debug}");
    verify(&generic).expect("integer operator Core verifies");

    let shift = generic.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match operation {
            Operation::IntegerBinary {
                failures,
                operator: el_ir::IntegerBinaryOperator::ShiftLeft,
                ..
            } => Some(failures),
            _ => None,
        })
        .expect("left shift operation");
    shift[0].0 = CoreFailureCategory::DivisionByZero;
    assert!(
        verify(&generic)
            .expect_err("malformed shift failure plan is rejected")
            .iter()
            .any(|error| error.contains("invalid failure plan"))
    );
}

#[test]
fn lowers_and_verifies_checked_integer_conversions() {
    let source = "defmodule Main do\n  def narrow(value: i64) -> i8 do\n    i8(value)\n  end\n  def widen(value: u8) -> u64 do\n    u64(value)\n  end\n  def scalar(value: u32) -> rune do\n    rune(value)\n  end\n  def main() -> i32 do\n    narrow(42)\n    widen(42)\n    scalar(128578)\n    0\n  end\nend\n";
    let mut generic = lowered(source);
    let debug = generic.debug_text();
    assert!(debug.contains("integer_convert"), "{debug}");
    assert!(debug.contains("InvalidConversion"), "{debug}");
    verify(&generic).expect("integer conversion Core verifies");

    let failure = generic.functions[0]
        .blocks
        .iter_mut()
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match operation {
            Operation::IntegerConvert { failure, .. } => Some(*failure),
            _ => None,
        })
        .expect("integer conversion operation");
    let function_span = generic.functions[0].span;
    generic.functions[0]
        .blocks
        .iter_mut()
        .find(|block| block.id == failure)
        .expect("conversion failure block")
        .terminator = Terminator::Failure {
        category: CoreFailureCategory::DivisionByZero,
        origin: function_span,
    };
    assert!(
        verify(&generic)
            .expect_err("malformed conversion failure target is rejected")
            .iter()
            .any(|error| error.contains("invalid failure target"))
    );
}

#[test]
fn lowers_and_verifies_wrapping_integer_operations_without_failures() {
    let mut module = lowered(
        "defmodule Main do\n  def wrap(value: i8, count: usize) -> i8 do\n    I8.wrapping_add(I8.wrapping_neg(value), I8.wrapping_shl(value, count))\n  end\n  def main() -> i32 do\n    wrap(127, 9)\n    0\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("wrapping.Negate"), "{debug}");
    assert!(debug.contains("wrapping.ShiftLeft"), "{debug}");
    assert!(debug.contains("wrapping.Add"), "{debug}");
    assert!(!debug.contains("IntegerOverflow =>"), "{debug}");
    verify(&module).expect("wrapping integer Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("wrapping operations specialize");
    verify_concrete(&concrete).expect("concrete wrapping integer Core verifies");

    let shift = module
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match operation {
            Operation::WrappingInteger {
                operator: el_ir::WrappingIntegerOperator::ShiftLeft,
                left,
                right,
                ..
            } => Some((*left, right)),
            _ => None,
        })
        .expect("wrapping shift operation");
    *shift.1 = Some(shift.0);
    assert!(
        verify(&module)
            .expect_err("malformed wrapping shift operands are rejected")
            .iter()
            .any(|error| error.contains("wrapping integer result"))
    );
}

#[test]
fn lowers_and_verifies_ieee_float_operations() {
    let module = lowered(
        "defmodule Main do\n  def calculate(value: f32) -> f32 do\n    sum = value + 1.5\n    -sum / 2.0\n  end\n  def main() -> i32 do\n    if calculate(1.5) < 0.0 do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("float.Add"), "{debug}");
    assert!(debug.contains("float.Negate"), "{debug}");
    assert!(debug.contains("float.Divide"), "{debug}");
    assert!(!debug.contains("IntegerOverflow =>"), "{debug}");
    verify(&module).expect("float Core verifies");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("float operations specialize");
    verify_concrete(&concrete).expect("concrete float Core verifies");
}

#[test]
fn lowers_float_patterns_to_ieee_equality_branches() {
    let module = lowered(
        "defmodule Main do\n  def classify(value: f64) -> i32 do\n    match value do\n      0.0 -> 0\n      -1.5 -> 1\n      _ -> 2\n    end\n  end\n  def main() -> i32 do\n    classify(-1.5)\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.matches("compare.Equal").count() >= 2, "{debug}");
    verify(&module).expect("float pattern Core verifies");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("float patterns specialize");
    verify_concrete(&concrete).expect("concrete float pattern Core verifies");
}

#[test]
fn lowers_and_verifies_float_numeric_conversions() {
    let module = lowered(
        "defmodule Main do\n  def to_float(value: i64) -> f32 do\n    f32(value)\n  end\n  def to_integer(value: f64) -> i32 do\n    i32(value)\n  end\n  def resize(value: f64) -> f64 do\n    f64(f32(value))\n  end\n  def main() -> i32 do\n    to_float(1)\n    to_integer(2.5)\n    resize(3.5)\n    0\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.matches("integer_convert").count() >= 4, "{debug}");
    assert!(debug.contains("InvalidConversion"), "{debug}");
    verify(&module).expect("numeric conversion Core verifies");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("numeric conversions specialize");
    verify_concrete(&concrete).expect("concrete numeric conversion Core verifies");
}

#[test]
fn concrete_types_carry_collector_independent_managed_classification() {
    let generic = lowered("defmodule Main do\n  def main() -> i32 do\n    0\n  end\nend\n");
    let roots = executable_reachability_roots(&generic).expect("entry point");
    let mut concrete = monomorphize(&generic, &roots).expect("concrete module");
    let i64_ty = TypeId(1);
    let string_ty = concrete
        .types
        .iter()
        .position(|ty| matches!(ty, Type::String))
        .map(|index| TypeId(index as u32))
        .unwrap_or_else(|| {
            let id = TypeId(concrete.types.len() as u32);
            concrete.types.push(Type::String);
            id
        });
    let mut add = |ty| {
        let id = TypeId(concrete.types.len() as u32);
        concrete.types.push(ty);
        id
    };
    let list = add(Type::List(i64_ty));
    let map = add(Type::Map {
        key: i64_ty,
        value: i64_ty,
    });
    let tuple = add(Type::Tuple(vec![i64_ty, string_ty]));
    let union = add(Type::Union(vec![i64_ty, list]));
    let array = add(Type::Array {
        item: string_ty,
        length: 2,
    });
    let slice = add(Type::Slice(string_ty));
    let function = add(Type::Function {
        parameters: vec![string_ty],
        result: i64_ty,
    });
    let declaration = DeclId(999);
    let structure = add(Type::Struct {
        declaration,
        arguments: Vec::new(),
    });
    concrete.structs.push(ConcreteStruct {
        declaration,
        name: "ManagedFields".to_owned(),
        arguments: Vec::new(),
        derives: Vec::new(),
        fields: vec![("name".to_owned(), string_ty), ("items".to_owned(), list)],
        origin: concrete.functions[0].span,
    });

    assert_eq!(
        managed_value_class(&concrete, i64_ty),
        Some(ManagedValueClass::Unmanaged)
    );
    assert_eq!(
        managed_value_class(&concrete, function),
        Some(ManagedValueClass::Unmanaged)
    );
    for ty in [string_ty, tuple, union, array, slice, structure] {
        assert_eq!(
            managed_value_class(&concrete, ty),
            Some(ManagedValueClass::ContainsBaseReferences)
        );
    }
    for ty in [list, map] {
        assert_eq!(
            managed_value_class(&concrete, ty),
            Some(ManagedValueClass::BaseReference)
        );
    }
    assert_eq!(managed_value_class(&concrete, TypeId(u32::MAX)), None);
}

#[test]
fn every_el_call_is_a_collection_point() {
    let module = lowered(
        "defmodule Main do\n  def identity(value: i64) -> i64 do\n    value\n  end\n  def main() -> i32 do\n    identity(1)\n    0\n  end\nend\n",
    );
    let operations = module.functions[1]
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .collect::<Vec<_>>();
    assert!(
        operations
            .iter()
            .any(|operation| matches!(operation, Operation::Call { .. }))
    );
    for operation in operations {
        let expected = if matches!(operation, Operation::Call { .. }) {
            CollectionEffect::MayCollect
        } else {
            CollectionEffect::CannotCollect
        };
        assert_eq!(operation_collection_effect(operation), expected);
    }
}

#[test]
fn lowers_and_specializes_named_function_values_and_indirect_calls() {
    let module = lowered(
        "defmodule Main do\n  def identity(value: a) -> a do\n    value\n  end\n  def increment(value: i32) -> i32 do\n    value + 1\n  end\n  def select(first: bool) -> (i32) -> i32 do\n    if first do\n      identity\n    else\n      increment\n    end\n  end\n  def apply(function: (i32) -> i32, value: i32) -> i32 do\n    function(value)\n  end\n  def main() -> i32 do\n    apply(select(false), 41)\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains(" = function f"), "{debug}");
    assert!(debug.contains("call_indirect"), "{debug}");
    let indirect = module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.operations)
        .find(|operation| matches!(operation, Operation::IndirectCall { .. }))
        .expect("indirect call operation");
    assert_eq!(
        operation_collection_effect(indirect),
        CollectionEffect::MayCollect
    );
    verify(&module).expect("function-value Generic Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("function targets specialize");
    verify_concrete(&concrete).expect("function-value Concrete Core verifies");
    assert!(
        concrete
            .functions
            .iter()
            .any(|function| function.name == "identity"
                && function.specialization_arguments.len() == 1),
        "generic function reference must retain its concrete specialization"
    );

    let mut invalid_reference = concrete.clone();
    let wrong_target = invalid_reference
        .functions
        .iter()
        .find(|function| function.name == "select")
        .expect("select specialization")
        .id;
    let reference = invalid_reference
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match operation {
            Operation::FunctionRef { function, .. } => Some(function),
            _ => None,
        })
        .expect("concrete function reference");
    *reference = wrong_target;
    assert!(
        verify_concrete(&invalid_reference)
            .expect_err("a function reference must match its exact target signature")
            .iter()
            .any(|error| error.contains("function value"))
    );

    let mut invalid_call = concrete.clone();
    let indirect = invalid_call
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.operations)
        .find_map(|operation| match operation {
            Operation::IndirectCall { function_ty, .. } => Some(function_ty),
            _ => None,
        })
        .expect("concrete indirect call");
    *indirect = TypeId(0);
    assert!(
        verify_concrete(&invalid_call)
            .expect_err("an indirect call requires an exact function type")
            .iter()
            .any(|error| error.contains("indirect call"))
    );
}

#[test]
fn non_empty_lists_are_collection_points_but_empty_lists_are_not() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    empty: [i32] = []\n    values: [i32] = [1 | empty]\n    0\n  end\nend\n",
    );
    let lists = module.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .filter(|operation| matches!(operation, Operation::List { .. }))
        .collect::<Vec<_>>();

    assert_eq!(lists.len(), 2);
    assert_eq!(
        operation_collection_effect(lists[0]),
        CollectionEffect::CannotCollect
    );
    assert_eq!(
        operation_collection_effect(lists[1]),
        CollectionEffect::MayCollect
    );
}

#[test]
fn computes_managed_roots_across_calls_slots_and_cleanup_block_parameters() {
    let generic = lowered(
        "defmodule Main do\n  def noop() -> unit do\n    unit\n  end\n  def consume(value: string) -> i32 do\n    1\n  end\n  def from_value(value: string) -> i32 do\n    noop()\n    consume(value)\n  end\n  def from_slot(value: string) -> i32 do\n    mut held: string = value\n    noop()\n    consume(held)\n  end\n  def through_loop(value: string) -> i32 do\n    mut count: i32 = 1\n    while count > 0 do\n      noop()\n      count := count - 1\n    end\n    consume(value)\n  end\n  def through_cleanup(value: string) -> string do\n    defer noop()\n    value\n  end\n  def main() -> i32 do\n    from_value(\"value\") + from_slot(through_cleanup(\"slot\")) + through_loop(\"loop\")\n  end\nend\n",
    );
    let roots = executable_reachability_roots(&generic).expect("entry point");
    let concrete = monomorphize(&generic, &roots).expect("concrete module");

    let from_value = concrete
        .functions
        .iter()
        .find(|function| function.name == "from_value")
        .expect("value fixture");
    let value_points = collection_point_roots(&concrete, from_value);
    assert_eq!(value_points.len(), 2);
    assert_eq!(value_points[0].values, vec![from_value.parameters[0].value]);
    assert!(value_points[0].slots.is_empty());
    assert_eq!(value_points[1].values, vec![from_value.parameters[0].value]);

    let from_slot = concrete
        .functions
        .iter()
        .find(|function| function.name == "from_slot")
        .expect("slot fixture");
    let slot_points = collection_point_roots(&concrete, from_slot);
    assert_eq!(slot_points.len(), 2);
    assert!(slot_points[0].values.is_empty());
    assert_eq!(slot_points[0].slots, vec![from_slot.slots[0].id]);
    assert_eq!(
        slot_points[1].values.len(),
        1,
        "the loaded slot is a call root"
    );
    assert!(slot_points[1].slots.is_empty());

    let through_loop = concrete
        .functions
        .iter()
        .find(|function| function.name == "through_loop")
        .expect("loop fixture");
    let loop_points = collection_point_roots(&concrete, through_loop);
    assert_eq!(loop_points.len(), 2);
    assert!(
        loop_points
            .iter()
            .all(|point| point.values.contains(&through_loop.parameters[0].value)),
        "the fixed point keeps a managed parameter live through the loop back edge"
    );

    let cleanup = concrete
        .functions
        .iter()
        .find(|function| function.name == "through_cleanup")
        .expect("cleanup fixture");
    let cleanup_points = collection_point_roots(&concrete, cleanup);
    assert_eq!(cleanup_points.len(), 1);
    assert_eq!(cleanup_points[0].values.len(), 1);
    assert!(
        cleanup
            .blocks
            .iter()
            .flat_map(|block| &block.parameters)
            .any(|parameter| cleanup_points[0].values.contains(&parameter.value)),
        "the saved managed result remains live while its deferred call runs"
    );
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
    let errors = verify_concrete(&abstract_layout).expect_err("abstract layout is rejected");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("has no concrete layout"))
    );
    assert!(
        errors
            .iter()
            .any(|error| error.contains("has no managed-value classification"))
    );
}

#[test]
fn lowers_calls_and_mutation_in_left_to_right_order() {
    let module = lowered(
        "defmodule Main do\n  def identity(value: a) -> a do\n    value\n  end\n  def main() -> i32 do\n    mut answer: i32 = identity(40)\n    answer := answer + 2\n    answer\n  end\nend\n",
    );

    assert_eq!(
        module.debug_text(),
        "fn f0 identity -> t4 {\n  b0:\n    return v0\n}\nfn f1 main -> t0 {\n  slot q0: t0\n  b0:\n    v0 = const Integer(40): t0\n    v1 = call f0(v0): t0\n    store q0, v1\n    v2 = load q0: t0\n    v3 = const Integer(2): t0\n    v4 = checked.Add v2, v3: t0 [IntegerOverflow => b1]\n    store q0, v4\n    v5 = load q0: t0\n    return v5\n  b1:\n    fail IntegerOverflow\n}\n"
    );
}

#[test]
fn lowers_pipeline_input_before_explicit_call_arguments() {
    let module = lowered(
        "defmodule Main do\n  def input() -> i32 do\n    20\n  end\n  def explicit() -> i32 do\n    22\n  end\n  def combine(left: i32, right: i32) -> i32 do\n    left + right\n  end\n  def main() -> i32 do\n    input() |> combine(explicit())\n  end\nend\n",
    );
    let main = &module.functions[3];
    let calls = main
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .filter_map(|operation| match operation {
            Operation::Call { function, .. } => Some(*function),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(calls, vec![FunctionId(0), FunctionId(1), FunctionId(2)]);
    verify(&module).expect("pipeline-free Generic Core verifies");
}

#[test]
fn lowers_defer_registration_values_before_lifo_cleanup() {
    let module = lowered(
        "defmodule Main do\n  def immediate() -> i32 do\n    1\n  end\n  def cleanup(value: i32) -> unit do\n    unit\n  end\n  def main() -> unit do\n    mut value: i32 = 10\n    defer cleanup(immediate())\n    defer do\n      cleanup(value)\n    end\n    value := 20\n    unit\n  end\nend\n",
    );
    let main = &module.functions[2];
    let calls = main
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .filter_map(|operation| match operation {
            Operation::Call {
                function,
                arguments,
                ..
            } => Some((*function, arguments.clone())),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(calls.len(), 3);
    assert_eq!(
        calls[0].0,
        FunctionId(0),
        "call argument runs at registration"
    );
    assert_eq!(calls[1].0, FunctionId(1), "last block action runs first");
    assert_eq!(calls[2].0, FunctionId(1), "first call action runs last");
    assert_ne!(
        calls[1].1, calls[2].1,
        "block capture and deferred call retain distinct registration values"
    );
    assert_eq!(
        main.blocks
            .iter()
            .filter(|block| !block.parameters.is_empty())
            .count(),
        2,
        "each registered action receives the saved scope result through its cleanup block"
    );
    verify(&module).expect("defer-free Generic Core verifies");
}

#[test]
fn routes_nested_fallthrough_and_return_values_through_cleanup_blocks() {
    let module = lowered(
        "defmodule Main do\n  def cleanup(value: i32) -> unit do\n    unit\n  end\n  def main(flag: bool) -> i32 do\n    defer cleanup(1)\n    if flag do\n      defer cleanup(2)\n      return 42\n    else\n      defer cleanup(3)\n      41\n    end\n  end\nend\n",
    );
    let main = &module.functions[1];
    let cleanup_calls = main
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .filter(|operation| {
            matches!(
                operation,
                Operation::Call {
                    function: FunctionId(0),
                    ..
                }
            )
        })
        .count();
    let cleanup_blocks = main
        .blocks
        .iter()
        .filter(|block| {
            block.operations.iter().any(|operation| {
                matches!(
                    operation,
                    Operation::Call {
                        function: FunctionId(0),
                        ..
                    }
                )
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(cleanup_calls, 4, "each reachable cleanup path is explicit");
    assert_eq!(
        cleanup_blocks.len(),
        4,
        "nested return and fallthrough cleanup blocks carry the i32 result"
    );
    assert!(
        cleanup_blocks.iter().all(|block| {
            matches!(block.parameters.as_slice(), [parameter] if parameter.ty == TypeId(0))
                && block
                    .operations
                    .iter()
                    .filter(|operation| {
                        matches!(
                            operation,
                            Operation::Call {
                                function: FunctionId(0),
                                ..
                            }
                        )
                    })
                    .count()
                    == 1
        }),
        "each cleanup block carries the saved result and invokes its action once"
    );
    verify(&module).expect("nested cleanup CFG verifies");
}

#[test]
fn cleanup_cfg_verifiers_reject_missing_and_mistyped_saved_results() {
    let source = "defmodule Main do\n  def cleanup() -> unit do\n    unit\n  end\n  def main() -> i32 do\n    defer cleanup()\n    42\n  end\nend\n";
    let module = lowered(source);
    let main = &module.functions[1];
    let cleanup_block = main
        .blocks
        .iter()
        .find(|block| {
            !block.parameters.is_empty()
                && block.operations.iter().any(|operation| {
                    matches!(
                        operation,
                        Operation::Call {
                            function: FunctionId(0),
                            ..
                        }
                    )
                })
        })
        .expect("cleanup block carries the saved result")
        .id;

    let mut missing = module.clone();
    let Terminator::Branch { arguments, .. } = missing.functions[1]
        .blocks
        .iter_mut()
        .find(|block| {
            matches!(block.terminator, Terminator::Branch { target, .. } if target == cleanup_block)
        })
        .map(|block| &mut block.terminator)
        .expect("incoming cleanup edge")
    else {
        panic!("incoming cleanup edge must be a branch")
    };
    arguments.clear();
    assert!(
        verify(&missing)
            .expect_err("missing saved cleanup result is rejected")
            .iter()
            .any(|error| error.contains("supplies 0 arguments, expected 1"))
    );

    let mut mistyped = module.clone();
    mistyped.functions[1]
        .blocks
        .iter_mut()
        .find(|block| block.id == cleanup_block)
        .expect("cleanup block")
        .parameters[0]
        .ty = TypeId(3);
    assert!(
        verify(&mistyped)
            .expect_err("mistyped saved cleanup result is rejected")
            .iter()
            .any(|error| error.contains("incorrectly typed argument"))
    );

    let roots = executable_reachability_roots(&module).expect("valid executable entry");
    let mut concrete = monomorphize(&module, &roots).expect("baseline cleanup graph specializes");
    let main = concrete
        .functions
        .iter_mut()
        .find(|function| function.name == "main")
        .expect("main specialization");
    let cleanup_block = main
        .blocks
        .iter()
        .find(|block| !block.parameters.is_empty())
        .expect("concrete cleanup block")
        .id;
    let Terminator::Branch { arguments, .. } = main
        .blocks
        .iter_mut()
        .find(|block| {
            matches!(block.terminator, Terminator::Branch { target, .. } if target == cleanup_block)
        })
        .map(|block| &mut block.terminator)
        .expect("concrete incoming cleanup edge")
    else {
        panic!("concrete incoming cleanup edge must be a branch")
    };
    arguments.clear();
    assert!(
        verify_concrete(&concrete)
            .expect_err("Concrete Core rejects a missing saved cleanup result")
            .iter()
            .any(|error| error.contains("supplies 0 arguments, expected 1"))
    );
}

#[test]
fn explicit_failure_terminators_bypass_cleanup_and_are_verified() {
    let module = lowered(
        "defmodule Main do\n  def cleanup() -> unit do\n    unit\n  end\n  def maximum() -> i32 do\n    2147483647\n  end\n  def one() -> i32 do\n    1\n  end\n  def main() -> i32 do\n    defer cleanup()\n    maximum() + one()\n  end\nend\n",
    );
    let main = &module.functions[3];
    let failures = main
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .find_map(|operation| match operation {
            Operation::CheckedArithmetic { failures, .. } => Some(failures.clone()),
            _ => None,
        })
        .expect("checked addition has an explicit failure edge");
    assert_eq!(
        failures
            .iter()
            .map(|(category, _)| *category)
            .collect::<Vec<_>>(),
        vec![CoreFailureCategory::IntegerOverflow]
    );
    let failure_target = failures[0].1;
    assert!(matches!(
        main.blocks
            .iter()
            .find(|block| block.id == failure_target)
            .map(|block| &block.terminator),
        Some(Terminator::Failure {
            category: CoreFailureCategory::IntegerOverflow,
            ..
        })
    ));
    assert!(main.blocks.iter().any(|block| {
        block.id != failure_target
            && block.operations.iter().any(|operation| {
                matches!(
                    operation,
                    Operation::Call {
                        function: FunctionId(0),
                        ..
                    }
                )
            })
    }));
    verify(&module).expect("explicit failure CFG verifies");

    let mut wrong_category = module.clone();
    let failure = wrong_category.functions[3]
        .blocks
        .iter_mut()
        .find(|block| block.id == failure_target)
        .expect("failure block");
    let Terminator::Failure { category, .. } = &mut failure.terminator else {
        panic!("failure terminator")
    };
    *category = CoreFailureCategory::DivisionByZero;
    assert!(
        verify(&wrong_category)
            .expect_err("mismatched failure category is rejected")
            .iter()
            .any(|error| error.contains("invalid failure target"))
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
fn lowers_list_reverse_as_an_allocating_verified_operation() {
    let module = lowered(
        "defmodule Main do\n  def reverse(values: [a]) -> [a] do\n    List.reverse(values)\n  end\n  def main() -> i32 do\n    values: [i32] = reverse([1, 42])\n    match values do\n      [value | _] -> value\n      [] -> 0\n    end\n  end\nend\n",
    );
    let reverse = module.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .find(|operation| matches!(operation, Operation::ListReverse { .. }))
        .expect("List.reverse has an explicit Core operation");

    assert_eq!(
        operation_collection_effect(reverse),
        CollectionEffect::MayCollect
    );
    assert!(module.debug_text().contains("list_reverse"));
    verify(&module).expect("list reverse Core IR verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("list reverse specializes");
    verify_concrete(&concrete).expect("specialized list reverse verifies");
}

#[test]
fn lowers_enum_count_at_and_fresh_list_materialization() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    list: [i32] = [10, 20]\n    array: [i32; 2] = #[30, 40]\n    slice = Slice.from_array(array)\n    data = String.bytes(\"AB\")\n    map: Map(i32, i32) = %{1 => 50, 2 => 60}\n    Enum.count(list)\n    Enum.count(array)\n    Enum.count(slice)\n    Enum.count(data)\n    Enum.count(map)\n    Enum.at(list, 1)\n    Enum.at(array, 2)\n    Enum.at(slice, 0)\n    Enum.at(data, 1)\n    Enum.at(map, 0)\n    Enum.to_list(list)\n    Enum.to_list(array)\n    Enum.to_list(slice)\n    Enum.to_list(data)\n    Enum.to_list(map)\n    0\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert_eq!(debug.matches("collection_length").count(), 5, "{debug}");
    assert_eq!(debug.matches("enum_at").count(), 5, "{debug}");
    assert_eq!(debug.matches("enum_to_list").count(), 2, "{debug}");
    assert!(debug.contains("bytes_to_list"), "{debug}");
    assert!(debug.contains("map_to_list"), "{debug}");
    let operations = module.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .collect::<Vec<_>>();
    for operation in operations
        .iter()
        .filter(|operation| matches!(operation, Operation::EnumToList { .. }))
    {
        assert_eq!(
            operation_collection_effect(operation),
            CollectionEffect::MayCollect
        );
    }
    for operation in operations
        .iter()
        .filter(|operation| matches!(operation, Operation::EnumAt { .. }))
    {
        assert_eq!(
            operation_collection_effect(operation),
            CollectionEffect::CannotCollect
        );
    }
    verify(&module).expect("Enum traversal Generic Core verifies");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("Enum traversal monomorphizes");
    verify_concrete(&concrete).expect("Enum traversal Concrete Core verifies");
}

#[test]
fn lowers_short_circuiting_enum_visits_as_collecting_operations() {
    let module = lowered(
        "defmodule Main do\n  def consume(value: i32) -> unit do\n    unit\n  end\n  def positive(value: i32) -> bool do\n    value > 0\n  end\n  def add(total: i32, value: i32) -> i32 do\n    total + value\n  end\n  def visit() -> bool do\n    values: [i32] = [1, 2]\n    Enum.each(values, consume)\n    Enum.any(values, positive)\n    Enum.reduce(values, 0 :: i32, add)\n    Enum.filter(values, positive)\n    Enum.map(values, positive)\n    Enum.all(values, positive)\n  end\n  def main() -> i32 do\n    visit()\n    0\n  end\nend\n",
    );
    let operations = module
        .functions
        .iter()
        .find(|function| function.name == "visit")
        .expect("visit function")
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .filter(|operation| matches!(operation, Operation::EnumVisit { .. }))
        .collect::<Vec<_>>();
    assert_eq!(operations.len(), 6);
    assert!(operations.iter().all(|operation| {
        operation_collection_effect(operation) == CollectionEffect::MayCollect
    }));
    let debug = module.debug_text();
    assert!(debug.contains("enum_Each"), "{debug}");
    assert!(debug.contains("enum_Any"), "{debug}");
    assert!(debug.contains("enum_All"), "{debug}");
    assert!(debug.contains("enum_Reduce"), "{debug}");
    assert!(debug.contains("enum_Filter"), "{debug}");
    assert!(debug.contains("enum_Map"), "{debug}");
    verify(&module).expect("Enum visits Generic Core verify");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("Enum visits monomorphize");
    verify_concrete(&concrete).expect("Enum visits Concrete Core verify");
    let mut malformed = concrete.clone();
    let visit = malformed
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.blocks)
        .flat_map(|block| &mut block.operations)
        .find(|operation| {
            matches!(
                operation,
                Operation::EnumVisit {
                    kind: EnumVisitKind::Map,
                    ..
                }
            )
        })
        .expect("map visitor operation");
    if let Operation::EnumVisit { function_ty, .. } = visit {
        *function_ty = TypeId(2);
    }
    assert!(
        verify_concrete(&malformed)
            .expect_err("mistyped Enum visitor is rejected")
            .iter()
            .any(|error| error.contains("Enum visit"))
    );
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
fn lowers_fixed_array_indexing_with_an_explicit_bounds_failure() {
    let module = lowered(
        "defmodule Main do\n  def get(values: [i32; 2], index: usize) -> i32 do\n    values[index]\n  end\n  def main() -> i32 do\n    get(#[40, 2], 1)\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("index"));
    assert!(debug.contains("IndexOutOfBounds"));
    assert!(
        module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .any(|block| matches!(
                block.terminator,
                Terminator::Failure {
                    category: CoreFailureCategory::IndexOutOfBounds,
                    ..
                }
            ))
    );
    verify(&module).expect("indexed array Core IR verifies");
}

#[test]
fn lowers_managed_slice_views_copy_length_and_indexing() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    array: [i32; 3] = #[10, 20, 30]\n    whole = Slice.from_array(array)\n    part = Slice.subslice(whole, 1, 2)\n    copy = Slice.copy(part)\n    length = Slice.length(copy)\n    copy[1]\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("slice_from_array"));
    assert!(debug.contains("subslice"));
    assert!(debug.contains("slice_copy"));
    assert!(debug.contains("collection_length"));
    assert_eq!(
        module
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .filter(|block| matches!(
                block.terminator,
                Terminator::Failure {
                    category: CoreFailureCategory::IndexOutOfBounds,
                    ..
                }
            ))
            .count(),
        2,
        "subslice and index share deterministic bounds-failure blocks by origin"
    );
    verify(&module).expect("slice Core IR verifies");
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
        "defmodule Main do\n  def divide(left: i32, right: i32) -> i32 do\n    left / right\n  end\n  def main() -> i32 do\n    mut n: i32 = 5\n    mut result: i32 = 1\n    while n > 1 do\n      result := result * n\n      n := n - 1\n    end\n    if false and divide(1, 0) == 0 do\n      return 1\n    end\n    if true or divide(1, 0) == 0 do\n      return result\n    end\n    0\n  end\nend\n",
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
    let method = module.implementations[0].method_declarations[0].1;
    assert!(
        module
            .functions
            .iter()
            .any(|function| function.declaration == method)
    );
    assert!(
        module
            .debug_text()
            .starts_with("impl i0 Render for t4 [Output=t1]\n")
    );
    verify(&module).expect("implementation metadata verifies");
}

#[test]
fn lowers_and_verifies_protocol_backed_concat() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    bits = Bytes.to_bits(Bytes.from_list([65]))\n    bits ++ bits\n    0\n  end\nend\n",
    );
    assert!(
        module.functions[0]
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .any(|operation| matches!(operation, Operation::Concat { .. }))
    );
    verify(&module).expect("Generic Core concat verifies");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("concat monomorphizes");
    verify_concrete(&concrete).expect("Concrete Core concat verifies");
}

#[test]
fn selects_explicit_operator_methods_after_generic_substitution() {
    let source = "defmodule Main do\n  defstruct Score do\n    value: i32\n  end\n  defimpl Eq, for: Score do\n    def eq(left: Score, right: Score) -> bool do\n      left.value == right.value\n    end\n  end\n  defimpl Ord, for: Score do\n    def compare(left: Score, right: Score) -> :less | :equal | :greater do\n      if left.value < right.value do\n        :less\n      else\n        if left.value > right.value do\n          :greater\n        else\n          :equal\n        end\n      end\n    end\n  end\n  defimpl Concat, for: Score do\n    def concat(left: Score, right: Score) -> Score do\n      %Score{value: left.value + right.value}\n    end\n  end\n  def same(left: a, right: a) -> bool when a: Eq do\n    left != right\n  end\n  def lower(left: a, right: a) -> bool when a: Ord do\n    left <= right\n  end\n  def append(left: a, right: a) -> a when a: Concat do\n    left ++ right\n  end\n  def main() -> i32 do\n    one = %Score{value: 1}\n    two = %Score{value: 2}\n    same(one, two)\n    lower(one, two)\n    append(one, two).value\n  end\nend\n";
    let generic = lowered(source);
    verify(&generic).expect("constrained operators verify before selection");
    let roots = executable_reachability_roots(&generic).expect("entry point");
    let concrete = monomorphize(&generic, &roots).expect("operator methods select");
    verify_concrete(&concrete).expect("selected operator calls verify");

    let comparisons = concrete
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.operations)
        .filter(|operation| matches!(operation, Operation::ProtocolCompare { .. }))
        .collect::<Vec<_>>();
    assert_eq!(comparisons.len(), 2);
    assert!(comparisons.iter().all(|operation| {
        operation_collection_effect(operation) == CollectionEffect::MayCollect
    }));
    assert!(
        ["<impl0>.eq", "<impl1>.compare", "<impl2>.concat"]
            .iter()
            .all(|name| concrete
                .functions
                .iter()
                .any(|function| function.name == *name))
    );
}

#[test]
fn reuses_generic_explicit_operator_methods_at_multiple_instantiations() {
    let source = "defmodule Main do\n  defstruct Box(a) do\n    value: a\n  end\n  defimpl Eq, for: Box(a) when a: Eq do\n    def eq(left: Box(a), right: Box(a)) -> bool do\n      left.value == right.value\n    end\n  end\n  def same(left: a, right: a) -> bool when a: Eq do\n    left == right\n  end\n  def main() -> i32 do\n    first: Box(i32) = %Box{value: 1}\n    second: Box(i32) = %Box{value: 2}\n    wide_first: Box(i64) = %Box{value: 1}\n    wide_second: Box(i64) = %Box{value: 2}\n    same(first, second)\n    same(wide_first, wide_second)\n    0\n  end\nend\n";
    let generic = lowered(source);
    let roots = executable_reachability_roots(&generic).expect("entry point");
    let concrete = monomorphize(&generic, &roots).expect("generic implementation specializes");
    verify_concrete(&concrete).expect("generic method specializations verify");

    let methods = concrete
        .functions
        .iter()
        .filter(|function| function.name == "<impl0>.eq")
        .collect::<Vec<_>>();
    assert_eq!(methods.len(), 2);
    assert_ne!(
        methods[0].specialization_arguments,
        methods[1].specialization_arguments
    );
    assert_eq!(
        concrete
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.operations)
            .filter(|operation| matches!(operation, Operation::ProtocolCompare { .. }))
            .count(),
        2
    );
}

#[test]
fn lowers_list_concat_to_order_preserving_core_cfg() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    left: [i32] = [1, 2]\n    right: [i32] = [3]\n    left ++ right\n    0\n  end\nend\n",
    );
    assert!(
        !module.functions[0]
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .any(|operation| matches!(operation, Operation::Concat { .. }))
    );
    assert!(
        module.functions[0]
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .any(|operation| matches!(operation, Operation::ListHead { .. }))
    );
    verify(&module).expect("list concat Core CFG verifies");
}

#[test]
fn lowers_for_to_static_iterable_cursor_cfg() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    values: [i32; 2] = #[1, 2]\n    for value in values do\n      value == 1\n      unit\n    end\n    0\n  end\nend\n",
    );
    let operations = module.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .collect::<Vec<_>>();
    assert!(
        operations
            .iter()
            .any(|operation| matches!(operation, Operation::CollectionLength { .. }))
    );
    assert!(
        operations
            .iter()
            .any(|operation| matches!(operation, Operation::EnumAt { .. }))
    );
    assert!(
        operations
            .iter()
            .any(|operation| matches!(operation, Operation::UnionProject { .. }))
    );
    verify(&module).expect("for Generic Core CFG verifies");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("for CFG monomorphizes");
    verify_concrete(&concrete).expect("for Concrete Core CFG verifies");
}

#[test]
fn specializes_generic_for_with_associated_items() {
    let module = lowered(
        "defmodule Main do\n  def consume(values: i) -> i32 when i: Iterable do\n    for value in values do\n      value\n      unit\n    end\n    0\n  end\n  def main() -> i32 do\n    values: [i32; 2] = #[1, 2]\n    consume(values)\n  end\nend\n",
    );
    verify(&module).expect("generic for Core verifies");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("generic for specializes");
    verify_concrete(&concrete).expect("specialized for Core verifies");
}

#[test]
fn normalizes_associated_item_projections_during_specialization() {
    let module = lowered(
        "defmodule Main do\n  def keep(values: i, value: Iterable.Item(i)) -> Iterable.Item(i) when i: Iterable do\n    value\n  end\n  def main() -> i32 do\n    values: [i32] = [1]\n    keep(values, 42)\n  end\nend\n",
    );
    verify(&module).expect("abstract projection Generic Core verifies");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("projection normalizes");
    verify_concrete(&concrete).expect("Concrete Core contains no projection");
}

#[test]
fn validates_derived_and_explicit_constraints_after_substitution() {
    for source in [
        "defmodule Main do\n  @derive [Eq]\n  defstruct Box(a) do\n    value: a\n  end\n  def same(left: a, right: a) -> bool when a: Eq do\n    left == right\n  end\n  def main() -> i32 do\n    left = %Box{value: 1}\n    right = %Box{value: 1}\n    if same(left, right) do\n      0\n    else\n      1\n    end\n  end\nend\n",
        "defmodule Main do\n  @derive [Eq, Ord]\n  defstruct Box(a) do\n    value: a\n  end\n  def main() -> i32 do\n    left = %Box{value: 1}\n    right = %Box{value: 2}\n    if left < right do\n      0\n    else\n      1\n    end\n  end\nend\n",
        "defmodule Main do\n  @derive [Eq, Ord]\n  defstruct Box(a) do\n    values: [a]\n  end\n  def main() -> i32 do\n    left: Box(i64) = %Box{values: [1]}\n    right: Box(i64) = %Box{values: [2]}\n    if left < right do\n      0\n    else\n      1\n    end\n  end\nend\n",
        "defmodule Main do\n  defstruct Box(a) do\n    value: a\n  end\n  defprotocol Marker do\n    def mark(value: Self) -> bool\n  end\n  defimpl Marker, for: Box(a) do\n    def mark(value: Box(a)) -> bool do\n      true\n    end\n  end\n  def keep(value: a) -> a when a: Marker do\n    value\n  end\n  def main() -> i32 do\n    value = %Box{value: 1}\n    keep(value)\n    0\n  end\nend\n",
    ] {
        let module = lowered(source);
        verify(&module).expect("constrained Generic Core verifies");
        let roots = executable_reachability_roots(&module).expect("entry point");
        let concrete = monomorphize(&module, &roots).expect("constraint holds after substitution");
        verify_concrete(&concrete).expect("constrained Concrete Core verifies");
    }
}

#[test]
fn normalizes_user_implementation_associated_types() {
    let module = lowered(
        "defmodule Main do\n  defstruct Box(a) do\n    value: a\n  end\n  defprotocol Container do\n    type Item\n    def get(value: Self) -> Item\n  end\n  defimpl Container, for: Box(a) do\n    type Item = a\n    def get(value: Box(a)) -> a do\n      value.value\n    end\n  end\n  def keep(value: c, item: Container.Item(c)) -> Container.Item(c) when c: Container do\n    item\n  end\n  def main() -> i32 do\n    value: Box(i32) = %Box{value: 1}\n    keep(value, 42)\n  end\nend\n",
    );
    verify(&module).expect("custom projection Generic Core verifies");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("custom projection normalizes");
    verify_concrete(&concrete).expect("custom projection Concrete Core verifies");
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
fn lowers_tagged_tuple_union_patterns_to_projection_then_destructuring() {
    let module = lowered(
        "defmodule Main do\n  @type Option(a) = {:some, a} | :none\n  def value(option: Option(i32)) -> i32 do\n    match option do\n      {:some, frequency} -> frequency + 1\n      :none -> 1\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("UnionMember"), "{debug}");
    assert!(debug.contains(" = project "), "{debug}");
    assert!(debug.contains("tuple_project"), "{debug}");
    verify(&module).expect("tagged tuple union pattern Core verifies");
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

#[test]
fn lowers_struct_construction_projection_and_update_as_reconstruction() {
    let module = lowered(
        "defmodule Main do\n  defstruct Pair(a) do\n    first: a\n    second: i32\n  end\n  def main() -> i32 do\n    mut pair: Pair(i32) = %Pair{second: 2, first: 40}\n    copy = pair\n    pair.second := pair.second + copy.second\n    pair.first + pair.second\n  end\nend\n",
    );
    let operations = module.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .collect::<Vec<_>>();
    assert_eq!(
        operations
            .iter()
            .filter(|operation| matches!(operation, Operation::Struct { .. }))
            .count(),
        2,
        "literal construction and field update each construct a value"
    );
    assert!(operations.iter().any(|operation| matches!(
        operation,
        Operation::Struct { fields, .. } if fields.iter().map(|(index, _)| *index).collect::<Vec<_>>() == vec![1, 0]
    )), "literal initializer evaluation order remains source order");
    verify(&module).expect("struct Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("struct specialization");
    verify_concrete(&concrete).expect("concrete struct Core verifies");
}

#[test]
fn lowers_managed_map_construction_and_size() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    values: Map(i32, i32) = %{1 => 10, 2 => 20}\n    if Map.size(values) == 2 do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let operations = module.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .collect::<Vec<_>>();
    let map = operations
        .iter()
        .find(|operation| matches!(operation, Operation::Map { .. }))
        .expect("map construction operation");
    assert_eq!(
        operation_collection_effect(map),
        CollectionEffect::MayCollect
    );
    assert!(operations.iter().any(|operation| matches!(
        operation,
        Operation::CollectionLength {
            known_length: None,
            ..
        }
    )));
    verify(&module).expect("map construction and size Core verifies");
}

#[test]
fn lowers_utf8_string_byte_size_as_verified_o1_length() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    if String.byte_size(\"é🙂\") == 7 do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let operations = module.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .collect::<Vec<_>>();
    assert!(operations.iter().any(|operation| matches!(
        operation,
        Operation::CollectionLength {
            known_length: None,
            ..
        }
    )));
    verify(&module).expect("string byte size Core verifies");
}

#[test]
fn lowers_rune_values_and_allocating_utf8_conversion() {
    let module = lowered(
        "defmodule Main do\n  def render(value: rune) -> string do\n    Rune.to_string(value)\n  end\n  def main() -> i32 do\n    if String.byte_size(render('🙂')) == 4 and 'a' < '🙂' do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("Rune('🙂')"), "{debug}");
    assert!(debug.contains("rune_to_string"), "{debug}");
    let conversion = module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.operations)
        .find(|operation| matches!(operation, Operation::RuneToString { .. }))
        .expect("rune conversion operation");
    assert_eq!(
        operation_collection_effect(conversion),
        CollectionEffect::MayCollect
    );
    verify(&module).expect("rune conversion Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("rune conversion specializes");
    verify_concrete(&concrete).expect("concrete rune conversion Core verifies");
}

#[test]
fn lowers_eager_string_codepoint_decoding_as_an_allocating_operation() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    if String.codepoints(\"Aé🙂\") == ['A', 'e', '́', '🙂'] do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("string_codepoints"), "{debug}");
    let operation = module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.operations)
        .find(|operation| matches!(operation, Operation::StringCodepoints { .. }))
        .expect("string codepoints operation");
    assert_eq!(
        operation_collection_effect(operation),
        CollectionEffect::MayCollect
    );
    verify(&module).expect("string codepoints Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("string codepoints specialize");
    verify_concrete(&concrete).expect("concrete string codepoints Core verifies");
}

#[test]
fn lowers_unicode_grapheme_length_as_a_noncollecting_operation() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    String.length(\"Aé🇸🇬👩‍👩‍👧‍👦\")\n    0\n  end\nend\n",
    );
    let operation = module.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .find(|operation| matches!(operation, Operation::StringLength { .. }))
        .expect("String.length has an explicit Core operation");
    assert_eq!(
        operation_collection_effect(operation),
        CollectionEffect::CannotCollect
    );
    assert!(module.debug_text().contains("string_length"));
    verify(&module).expect("String.length Generic Core verifies");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("String.length specializes");
    verify_concrete(&concrete).expect("String.length Concrete Core verifies");
}

#[test]
fn lowers_basic_string_operations_with_explicit_collection_effects() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    String.empty(\"\")\n    String.contains(\"café\", \"fé\")\n    String.split(\"a,,b,\", \",\")\n    0\n  end\nend\n",
    );
    let operations = module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.operations)
        .collect::<Vec<_>>();
    let empty = operations
        .iter()
        .find(|operation| matches!(operation, Operation::StringEmpty { .. }))
        .expect("String.empty operation");
    let contains = operations
        .iter()
        .find(|operation| matches!(operation, Operation::StringContains { .. }))
        .expect("String.contains operation");
    let split = operations
        .iter()
        .find(|operation| matches!(operation, Operation::StringSplit { .. }))
        .expect("String.split operation");
    assert_eq!(
        operation_collection_effect(empty),
        CollectionEffect::CannotCollect
    );
    assert_eq!(
        operation_collection_effect(contains),
        CollectionEffect::CannotCollect
    );
    assert_eq!(
        operation_collection_effect(split),
        CollectionEffect::MayCollect
    );
    let debug = module.debug_text();
    assert!(debug.contains("string_empty"), "{debug}");
    assert!(debug.contains("string_contains"), "{debug}");
    assert!(debug.contains("string_split"), "{debug}");
    verify(&module).expect("basic String Generic Core verifies");
    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("basic String operations specialize");
    verify_concrete(&concrete).expect("basic String Concrete Core verifies");
}

#[test]
fn lowers_eager_graphemes_through_retained_lazy_views() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    graphemes = String.graphemes(\"é🇸🇬\")\n    codepoints = Enum.to_list(String.codepoint_view(\"A🙂\"))\n    lazy_graphemes = Enum.to_list(String.grapheme_view(\"é🇸🇬\"))\n    if graphemes == lazy_graphemes and codepoints == ['A', '🙂'] do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let roots = executable_reachability_roots(&module).expect("entry point");
    let operations = module.functions.iter().flat_map(|function| {
        function
            .blocks
            .iter()
            .flat_map(|block| block.operations.iter())
    });
    let collected = operations
        .filter(|operation| matches!(operation, Operation::EnumToList { .. }))
        .count();
    assert_eq!(collected, 3);
    verify(&module).expect("lazy string view Generic Core verifies");
    let concrete = monomorphize(&module, &roots).expect("lazy string views specialize");
    verify_concrete(&concrete).expect("lazy string view Concrete Core verifies");
    assert!(
        concrete
            .types
            .iter()
            .any(|ty| matches!(ty, Type::CodepointView))
    );
    assert!(
        concrete
            .types
            .iter()
            .any(|ty| matches!(ty, Type::GraphemeView))
    );
}

#[test]
fn lowers_byte_aligned_bitstring_construction_with_a_failure_edge() {
    let module = lowered(
        "defmodule Main do\n  def packet(data: bytes, value: i32) -> bytes do\n    <<1::unsigned-big-size(8), value::signed-little-size(16), data::bytes>>\n  end\n  def main() -> i32 do\n    if Bytes.byte_size(packet(String.bytes(\"ok\"), 513)) == 5 do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("bitstring 3 segments"), "{debug}");
    assert!(debug.contains("fail BitstringSizeMismatch"), "{debug}");
    let operation = module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.operations)
        .find(|operation| matches!(operation, Operation::Bitstring { .. }))
        .expect("bitstring construction operation");
    assert_eq!(
        operation_collection_effect(operation),
        CollectionEffect::MayCollect
    );
    verify(&module).expect("bitstring Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("bitstring specializes");
    verify_concrete(&concrete).expect("concrete bitstring Core verifies");
}

#[test]
fn lowers_byte_aligned_bitstring_patterns_with_normal_failure_edges() {
    let module = lowered(
        "defmodule Main do\n  def parse(packet: bytes, prefix_size: usize) -> i32 do\n    match packet do\n      <<7::unsigned-big-size(8), prefix::bytes-size(prefix_size), rest::bytes-size(Bytes.byte_size(prefix))>> -> if Bytes.byte_size(rest) == prefix_size do\n        42\n      else\n        1\n      end\n      _ -> 0\n    end\n  end\n  def main() -> i32 do\n    parse(<<7::unsigned-big-size(8), String.bytes(\"ab\")::bytes, String.bytes(\"cd\")::bytes>>, 2)\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("bitstring_pattern_integer"), "{debug}");
    assert!(debug.contains("bitstring_pattern_bytes"), "{debug}");
    assert!(debug.contains("bitstring_pattern_check"), "{debug}");
    verify(&module).expect("bitstring-pattern Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("bitstring pattern specializes");
    verify_concrete(&concrete).expect("concrete bitstring-pattern Core verifies");
}

#[test]
fn lowers_utf8_validation_and_inspectable_error_offsets() {
    let module = lowered(
        "defmodule Main do\n  def valid(value: {:ok, string}) -> usize do\n    match value do\n      {:ok, text} -> String.byte_size(text)\n    end\n  end\n  def invalid(value: {:error, String.Utf8Error}) -> usize do\n    match value do\n      {:error, reason} -> String.utf8_error_offset(reason)\n    end\n  end\n  def inspect(data: bytes) -> usize do\n    match String.from_bytes(data) do\n      value: {:ok, string} -> valid(value)\n      value: {:error, String.Utf8Error} -> invalid(value)\n    end\n  end\n  def main() -> i32 do\n    if inspect(String.bytes(\"é\")) == 2 do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("string_from_bytes"), "{debug}");
    assert!(debug.contains("utf8_error_offset"), "{debug}");
    let conversion = module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.operations)
        .find(|operation| matches!(operation, Operation::StringFromBytes { .. }))
        .expect("UTF-8 validation operation");
    assert_eq!(
        operation_collection_effect(conversion),
        CollectionEffect::MayCollect
    );
    verify(&module).expect("UTF-8 validation Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("UTF-8 validation specializes");
    verify_concrete(&concrete).expect("concrete UTF-8 validation Core verifies");
}

#[test]
fn lowers_string_bytes_and_byte_slices_as_retained_views() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    data = String.bytes(\"é🙂\")\n    view = Bytes.slice(data, 1, 2)\n    if Bytes.byte_size(view) == 2 do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("string_bytes"), "{debug}");
    assert!(debug.contains("bytes_slice"), "{debug}");
    assert!(debug.contains("IndexOutOfBounds"), "{debug}");
    verify(&module).expect("byte-view Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("byte views specialize");
    verify_concrete(&concrete).expect("concrete byte-view Core verifies");
    let bytes = concrete
        .types
        .iter()
        .position(|ty| matches!(ty, Type::Bytes))
        .map(|index| TypeId(index as u32))
        .expect("concrete bytes type");
    assert_eq!(
        managed_value_class(&concrete, bytes),
        Some(ManagedValueClass::ContainsBaseReferences)
    );
}

#[test]
fn lowers_value_style_buffers_and_reuses_strict_utf8_validation() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    original = Buffer.append_string(Buffer.new(), \"hello\")\n    extended = Buffer.append_byte(original, 32)\n    complete = Buffer.append_bytes(extended, String.bytes(\"world\"))\n    snapshot = Buffer.to_bytes(complete)\n    Buffer.to_string(complete)\n    if Buffer.byte_size(original) == 5 and Bytes.byte_size(snapshot) == 11 do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("buffer_new"), "{debug}");
    assert!(debug.contains("buffer_append_String"), "{debug}");
    assert!(debug.contains("buffer_append_Byte"), "{debug}");
    assert!(debug.contains("buffer_append_Bytes"), "{debug}");
    assert!(debug.contains("buffer_to_bytes"), "{debug}");
    assert!(debug.contains("string_from_bytes"), "{debug}");
    for operation in module
        .functions
        .iter()
        .flat_map(|function| function.blocks.iter().flat_map(|block| &block.operations))
    {
        if matches!(
            operation,
            Operation::BufferAppend { .. }
                | Operation::BufferToBytes { .. }
                | Operation::StringFromBytes { .. }
        ) {
            assert_eq!(
                operation_collection_effect(operation),
                CollectionEffect::MayCollect
            );
        }
    }
    verify(&module).expect("Buffer Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("Buffer operations specialize");
    verify_concrete(&concrete).expect("concrete Buffer Core verifies");
    let buffer = concrete
        .types
        .iter()
        .position(|ty| matches!(ty, Type::Buffer))
        .map(|index| TypeId(index as u32))
        .expect("concrete Buffer type");
    assert_eq!(
        managed_value_class(&concrete, buffer),
        Some(ManagedValueClass::ContainsBaseReferences)
    );
}

#[test]
fn lowers_show_backed_interpolation_to_ordered_string_operations() {
    let module = lowered(
        "defmodule Main do\n  def message(value: i64) -> string do\n    \"value=#{value} ready=#{true}\"\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("integer_to_string"), "{debug}");
    assert!(debug.contains("boolean_to_string"), "{debug}");
    assert!(debug.contains("concat"), "{debug}");
    verify(&module).expect("interpolation Core verifies");
}

#[test]
fn lowers_arbitrary_bit_views_indexing_and_alignment_conversion() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    bits = Bytes.to_bits(String.bytes(\"abc\"))\n    view = Bits.slice(bits, 3, 16)\n    Bits.to_bytes(view)\n    if Bits.bit_size(view) == 16 and view[0] do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("bytes_to_bits"), "{debug}");
    assert!(debug.contains("bits_slice"), "{debug}");
    assert!(debug.contains("bits_to_bytes"), "{debug}");
    assert!(debug.contains("IndexOutOfBounds"), "{debug}");
    let conversion = module
        .functions
        .iter()
        .flat_map(|function| function.blocks.iter())
        .flat_map(|block| &block.operations)
        .find(|operation| matches!(operation, Operation::BitsToBytes { .. }))
        .expect("bits conversion operation");
    assert_eq!(
        operation_collection_effect(conversion),
        CollectionEffect::MayCollect
    );
    verify(&module).expect("bits Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("bits operations specialize");
    verify_concrete(&concrete).expect("concrete bits Core verifies");
    let bits = concrete
        .types
        .iter()
        .position(|ty| matches!(ty, Type::Bits))
        .map(|index| TypeId(index as u32))
        .expect("concrete bits type");
    assert_eq!(
        managed_value_class(&concrete, bits),
        Some(ManagedValueClass::ContainsBaseReferences)
    );
}

#[test]
fn lowers_allocating_bytes_list_conversions() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    values = Bytes.to_list(Bytes.from_list([0, 127, 255]))\n    if List.reverse(values) == [255, 127, 0] do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("bytes_from_list"), "{debug}");
    assert!(debug.contains("bytes_to_list"), "{debug}");
    for operation in module.functions.iter().flat_map(|function| {
        function
            .blocks
            .iter()
            .flat_map(|block| block.operations.iter())
    }) {
        if matches!(
            operation,
            Operation::BytesFromList { .. } | Operation::BytesToList { .. }
        ) {
            assert_eq!(
                operation_collection_effect(operation),
                CollectionEffect::MayCollect
            );
        }
    }
    verify(&module).expect("byte/list conversion Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("byte/list conversions specialize");
    verify_concrete(&concrete).expect("concrete byte/list conversion Core verifies");
}

#[test]
fn lowers_checked_byte_indexing_to_u8() {
    let module = lowered(
        "defmodule Main do\n  def first(data: bytes, index: usize) -> u8 do\n    data[index]\n  end\n  def main() -> i32 do\n    if first(String.bytes(\"abc\"), 0) == 97 do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let debug = module.debug_text();
    assert!(debug.contains("index"), "{debug}");
    assert!(debug.contains("IndexOutOfBounds"), "{debug}");
    assert!(module.functions.iter().any(|function| {
        function
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .any(|operation| {
                matches!(
                    operation,
                    Operation::ArrayIndex { ty, .. }
                        if matches!(module.types.get(ty.0 as usize), Some(Type::U8))
                )
            })
    }));
    verify(&module).expect("byte-index Core verifies");

    let roots = executable_reachability_roots(&module).expect("entry point");
    let concrete = monomorphize(&module, &roots).expect("byte index specializes");
    verify_concrete(&concrete).expect("concrete byte-index Core verifies");
}

#[test]
fn lowers_immutable_map_put_and_remove_as_allocating_operations() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    original: Map(i32, i32) = %{1 => 10, 2 => 20}\n    updated = Map.put(original, 2, 22)\n    removed = Map.remove(updated, 1)\n    fetched = Map.fetch(updated, 2)\n    if Map.size(removed) == 1 do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let operations = module.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .collect::<Vec<_>>();
    for operation in operations.iter().filter(|operation| {
        matches!(
            operation,
            Operation::MapPut { .. } | Operation::MapRemove { .. }
        )
    }) {
        assert_eq!(
            operation_collection_effect(operation),
            CollectionEffect::MayCollect
        );
    }
    assert_eq!(
        operations
            .iter()
            .filter(|operation| matches!(operation, Operation::MapPut { .. }))
            .count(),
        1
    );
    assert_eq!(
        operations
            .iter()
            .filter(|operation| matches!(operation, Operation::MapRemove { .. }))
            .count(),
        1
    );
    assert_eq!(
        operations
            .iter()
            .filter(|operation| matches!(operation, Operation::MapFetch { .. }))
            .count(),
        1
    );
    verify(&module).expect("map update Core verifies");
}

#[test]
fn lowers_map_order_view_and_structural_equality() {
    let module = lowered(
        "defmodule Main do\n  def main() -> i32 do\n    left: Map({i32, i32}, i32) = %{{1, 2} => 10, {3, 4} => 20}\n    right: Map({i32, i32}, i32) = %{{3, 4} => 20, {1, 2} => 10}\n    ordered = Enum.to_list(left)\n    if left == right do\n      0\n    else\n      1\n    end\n  end\nend\n",
    );
    let operations = module.functions[0]
        .blocks
        .iter()
        .flat_map(|block| &block.operations)
        .collect::<Vec<_>>();
    let to_list = operations
        .iter()
        .find(|operation| matches!(operation, Operation::MapToList { .. }))
        .expect("map order becomes an explicit Core operation");
    assert_eq!(
        operation_collection_effect(to_list),
        CollectionEffect::MayCollect
    );
    assert!(operations.iter().any(|operation| matches!(
        operation,
        Operation::Compare {
            operator: el_ir::ComparisonOperator::Equal,
            ..
        }
    )));
    verify(&module).expect("map order and equality Core verify");
}
