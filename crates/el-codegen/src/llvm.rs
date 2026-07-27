//! The private Inkwell boundary. No Inkwell or LLVM type escapes this module.

use crate::integer_checks::{FailureOrigin, failure_categories};
use crate::{CodegenProfile, InvalidTargetMetadata, TargetMetadata};
use el_ir::{
    ArithmeticOperator, Block, BlockId, ConcreteModule, Constant, CoreFunction, FunctionId,
    Operation, SlotId, Terminator, Type, TypeId, ValueId, verify_concrete,
};
use el_runtime::{FAILURE_SYMBOL, FailureCategory};
use el_span::Span;
use inkwell::IntPredicate;
use inkwell::OptimizationLevel;
use inkwell::basic_block::BasicBlock as LlvmBlock;
use inkwell::builder::{Builder, BuilderError};
use inkwell::context::Context;
use inkwell::intrinsics::Intrinsic;
use inkwell::module::Module;
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum};
use inkwell::values::{
    BasicMetadataValueEnum, BasicValueEnum, FunctionValue, IntValue, PhiValue, PointerValue,
};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

/// Verified LLVM text produced from a verified Concrete Core module.
///
/// Keeping the Inkwell module private prevents backend details from leaking into
/// compiler-stage APIs. Object emission can reuse the private lowering routine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedLlvmIr(String);

impl VerifiedLlvmIr {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

/// A failure at the Concrete Core to LLVM boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackendError {
    InvalidConcrete(Vec<String>),
    UnsupportedType(TypeId),
    UnsupportedOperation {
        function: FunctionId,
        block: BlockId,
        operation: &'static str,
    },
    UnsupportedTerminator {
        function: FunctionId,
        block: BlockId,
        terminator: &'static str,
    },
    MissingFunction(FunctionId),
    MissingBlock(BlockId),
    MissingValue(ValueId),
    MissingSlot(SlotId),
    IntegerOutOfRange {
        value: i128,
        ty: TypeId,
    },
    SourceOriginOutOfRange,
    MissingIntrinsic(&'static str),
    InvalidIntrinsicResult(&'static str),
    InvalidIntegerCheckPlan,
    Builder(String),
    Verification(String),
    HostTargetInitialization(String),
    HostTarget(String),
    HostTargetMachineUnavailable,
    NonUnicodeObjectPath,
    ObjectEmission(String),
    MissingExecutableEntry,
    InvalidExecutableEntry(FunctionId),
    HostTripleNotUtf8,
    InvalidTargetMetadata(InvalidTargetMetadata),
    Optimization(String),
}

impl fmt::Display for BackendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConcrete(errors) => {
                write!(formatter, "invalid Concrete Core IR: {}", errors.join("; "))
            }
            Self::UnsupportedType(ty) => write!(formatter, "unsupported LLVM type {ty:?}"),
            Self::UnsupportedOperation {
                function,
                block,
                operation,
            } => write!(
                formatter,
                "unsupported operation {operation} in {function:?} {block:?}"
            ),
            Self::UnsupportedTerminator {
                function,
                block,
                terminator,
            } => write!(
                formatter,
                "unsupported terminator {terminator} in {function:?} {block:?}"
            ),
            Self::MissingFunction(function) => {
                write!(formatter, "missing declared LLVM function {function:?}")
            }
            Self::MissingBlock(block) => write!(formatter, "missing LLVM block {block:?}"),
            Self::MissingValue(value) => write!(formatter, "missing LLVM value {value:?}"),
            Self::MissingSlot(slot) => write!(formatter, "missing LLVM slot {slot:?}"),
            Self::IntegerOutOfRange { value, ty } => {
                write!(formatter, "integer {value} is out of range for {ty:?}")
            }
            Self::SourceOriginOutOfRange => {
                write!(
                    formatter,
                    "source origin is not representable by the runtime ABI"
                )
            }
            Self::MissingIntrinsic(intrinsic) => {
                write!(formatter, "LLVM does not provide intrinsic {intrinsic}")
            }
            Self::InvalidIntrinsicResult(intrinsic) => {
                write!(
                    formatter,
                    "LLVM intrinsic {intrinsic} returned an invalid type"
                )
            }
            Self::InvalidIntegerCheckPlan => {
                write!(formatter, "integer operation has an invalid check plan")
            }
            Self::Builder(error) => write!(formatter, "LLVM builder error: {error}"),
            Self::Verification(error) => write!(formatter, "LLVM verification failed: {error}"),
            Self::HostTargetInitialization(error) => {
                write!(
                    formatter,
                    "could not initialize the LLVM host target: {error}"
                )
            }
            Self::HostTarget(error) => {
                write!(formatter, "could not select the LLVM host target: {error}")
            }
            Self::HostTargetMachineUnavailable => {
                formatter.write_str("LLVM could not create a machine for the host target")
            }
            Self::NonUnicodeObjectPath => {
                formatter.write_str("LLVM object output path is not valid Unicode")
            }
            Self::ObjectEmission(error) => {
                write!(formatter, "LLVM could not emit the host object: {error}")
            }
            Self::MissingExecutableEntry => formatter
                .write_str("Concrete Core roots do not contain the executable entry `Main.main`"),
            Self::InvalidExecutableEntry(function) => write!(
                formatter,
                "Concrete Core executable entry {function:?} is not `Main.main() -> i32`"
            ),
            Self::HostTripleNotUtf8 => {
                formatter.write_str("LLVM host target triple is not valid UTF-8")
            }
            Self::InvalidTargetMetadata(error) => write!(formatter, "{error}"),
            Self::Optimization(error) => write!(formatter, "LLVM optimization failed: {error}"),
        }
    }
}

impl std::error::Error for BackendError {}

/// Lowers the currently supported primitive Concrete Core slice and verifies
/// the resulting LLVM module before returning deterministic textual IR.
pub fn lower_to_llvm_ir(core: &ConcreteModule) -> Result<VerifiedLlvmIr, BackendError> {
    let context = Context::create();
    let module = lower_verified_module(&context, core)?;
    Ok(VerifiedLlvmIr(module.print_to_string().to_string()))
}

/// Lowers verified Concrete Core IR and emits an object for the compiler host.
///
/// Target initialization and all LLVM target objects remain private to this
/// boundary. The caller receives only a structured error and the requested
/// object file on success.
pub fn emit_host_object(
    core: &ConcreteModule,
    output: &Path,
) -> Result<TargetMetadata, BackendError> {
    emit_host_object_with_profile(core, output, CodegenProfile::Development)
}

/// Emits a host object using the optimization profile selected by `el build`.
pub fn emit_host_object_with_profile(
    core: &ConcreteModule,
    output: &Path,
    profile: CodegenProfile,
) -> Result<TargetMetadata, BackendError> {
    // Inkwell 0.9.0 requires a Unicode path and otherwise panics internally.
    // Reject it explicitly so a host path can never crash the compiler.
    if output.to_str().is_none() {
        return Err(BackendError::NonUnicodeObjectPath);
    }

    Target::initialize_native(&InitializationConfig::default())
        .map_err(BackendError::HostTargetInitialization)?;
    let triple = TargetMachine::get_default_triple();
    let target = Target::from_triple(&triple)
        .map_err(|error| BackendError::HostTarget(error.to_string()))?;
    let machine = target
        .create_target_machine(
            &triple,
            "generic",
            "",
            optimization_level(profile),
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or(BackendError::HostTargetMachineUnavailable)?;

    let target_data = machine.get_target_data();
    let pointer_width = target_data.get_pointer_byte_size(None) * 8;
    let triple_text = triple
        .as_str()
        .to_str()
        .map_err(|_| BackendError::HostTripleNotUtf8)?;
    let metadata = TargetMetadata::new(triple_text, pointer_width)
        .map_err(BackendError::InvalidTargetMetadata)?;

    let context = Context::create();
    let module = lower_verified_module(&context, core)?;
    module.set_triple(&triple);
    module.set_data_layout(&target_data.get_data_layout());
    module
        .verify()
        .map_err(|error| BackendError::Verification(error.to_string()))?;
    let pass_options = PassBuilderOptions::create();
    pass_options.set_verify_each(true);
    module
        .run_passes(optimization_pipeline(profile), &machine, pass_options)
        .map_err(|error| BackendError::Optimization(error.to_string()))?;
    module
        .verify()
        .map_err(|error| BackendError::Verification(error.to_string()))?;
    machine
        .write_to_file(&module, FileType::Object, output)
        .map_err(|error| BackendError::ObjectEmission(error.to_string()))?;
    Ok(metadata)
}

fn optimization_level(profile: CodegenProfile) -> OptimizationLevel {
    match profile {
        CodegenProfile::Development => OptimizationLevel::Less,
        CodegenProfile::Release => OptimizationLevel::Aggressive,
    }
}

fn optimization_pipeline(profile: CodegenProfile) -> &'static str {
    match profile {
        CodegenProfile::Development => "default<O1>",
        CodegenProfile::Release => "default<O3>",
    }
}

fn lower_verified_module<'ctx>(
    context: &'ctx Context,
    core: &ConcreteModule,
) -> Result<Module<'ctx>, BackendError> {
    verify_concrete(core).map_err(BackendError::InvalidConcrete)?;
    let module = ModuleLowerer::new(context, core).lower()?;
    module
        .verify()
        .map_err(|error| BackendError::Verification(error.to_string()))?;
    Ok(module)
}

struct ModuleLowerer<'ctx, 'core> {
    context: &'ctx Context,
    core: &'core ConcreteModule,
    module: Module<'ctx>,
    functions: BTreeMap<FunctionId, FunctionValue<'ctx>>,
    failure: FunctionValue<'ctx>,
}

impl<'ctx, 'core> ModuleLowerer<'ctx, 'core> {
    fn new(context: &'ctx Context, core: &'core ConcreteModule) -> Self {
        let module = context.create_module("el");
        let failure_type = context.void_type().fn_type(
            &[
                context.i32_type().into(),
                context.i32_type().into(),
                context.i64_type().into(),
                context.i64_type().into(),
            ],
            false,
        );
        let failure = module.add_function(FAILURE_SYMBOL, failure_type, None);
        Self {
            context,
            core,
            module,
            functions: BTreeMap::new(),
            failure,
        }
    }

    fn lower(mut self) -> Result<Module<'ctx>, BackendError> {
        let mut functions = self.core.functions.iter().collect::<Vec<_>>();
        functions.sort_by_key(|function| function.id);
        for function in &functions {
            let parameter_types = function
                .parameters
                .iter()
                .map(|parameter| {
                    self.basic_type(parameter.ty)
                        .map(BasicMetadataTypeEnum::from)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let result = self.basic_type(function.result)?;
            let function_type = result.fn_type(&parameter_types, false);
            let value =
                self.module
                    .add_function(&format!("el.f{}", function.id.0), function_type, None);
            self.functions.insert(function.id, value);
        }
        for function in functions {
            self.lower_function(function)?;
        }
        self.lower_process_entry()?;
        Ok(self.module)
    }

    fn lower_process_entry(&self) -> Result<(), BackendError> {
        let entry = self
            .core
            .roots
            .functions
            .iter()
            .filter_map(|root| {
                self.core
                    .functions
                    .iter()
                    .find(|function| function.id == *root)
            })
            .find(|function| function.module_name == "Main" && function.name == "main")
            .ok_or(BackendError::MissingExecutableEntry)?;
        if !entry.is_exported()
            || !entry.parameters.is_empty()
            || !entry.type_parameters.is_empty()
            || !entry.constraints.is_empty()
            || !matches!(
                self.core.types.get(entry.result.0 as usize),
                Some(Type::I32)
            )
        {
            return Err(BackendError::InvalidExecutableEntry(entry.id));
        }

        let target = self
            .functions
            .get(&entry.id)
            .copied()
            .ok_or(BackendError::MissingFunction(entry.id))?;
        let shim_type = self.context.i32_type().fn_type(&[], false);
        let shim = self.module.add_function("main", shim_type, None);
        let block = self.context.append_basic_block(shim, "entry");
        let builder = self.context.create_builder();
        builder.position_at_end(block);
        let call = built(builder.build_call(target, &[], "el.exit_status"))?;
        let status = call
            .try_as_basic_value()
            .basic()
            .ok_or(BackendError::InvalidExecutableEntry(entry.id))?;
        built(builder.build_return(Some(&status)))?;
        Ok(())
    }

    fn basic_type(&self, ty: TypeId) -> Result<BasicTypeEnum<'ctx>, BackendError> {
        match self.core.types.get(ty.0 as usize) {
            Some(Type::I32) => Ok(self.context.i32_type().into()),
            Some(Type::I64) => Ok(self.context.i64_type().into()),
            Some(Type::Bool) => Ok(self.context.bool_type().into()),
            Some(Type::Unit) => Ok(self.context.struct_type(&[], false).into()),
            _ => Err(BackendError::UnsupportedType(ty)),
        }
    }

    fn lower_function(&self, function: &CoreFunction) -> Result<(), BackendError> {
        let llvm_function = self
            .functions
            .get(&function.id)
            .copied()
            .ok_or(BackendError::MissingFunction(function.id))?;
        let builder = self.context.create_builder();
        let mut blocks = BTreeMap::new();
        let mut ordered_blocks = function.blocks.iter().collect::<Vec<_>>();
        ordered_blocks.sort_by_key(|block| block.id);
        for block in &ordered_blocks {
            blocks.insert(
                block.id,
                self.context
                    .append_basic_block(llvm_function, &format!("b{}", block.id.0)),
            );
        }

        let mut values = BTreeMap::new();
        for (index, parameter) in function.parameters.iter().enumerate() {
            let value = llvm_function
                .get_nth_param(index as u32)
                .ok_or(BackendError::MissingValue(parameter.value))?;
            value.set_name(&format!("v{}", parameter.value.0));
            values.insert(parameter.value, value);
        }

        let mut phis = BTreeMap::new();
        for block in &ordered_blocks {
            builder.position_at_end(self.block(&blocks, block.id)?);
            for parameter in &block.parameters {
                let phi = built(builder.build_phi(
                    self.basic_type(parameter.ty)?,
                    &format!("v{}", parameter.value.0),
                ))?;
                values.insert(parameter.value, phi.as_basic_value());
                phis.insert(parameter.value, phi);
            }
        }

        let entry = self.block(&blocks, BlockId(0))?;
        builder.position_at_end(entry);
        let mut slots = BTreeMap::new();
        for slot in &function.slots {
            let pointer =
                built(builder.build_alloca(self.basic_type(slot.ty)?, &format!("q{}", slot.id.0)))?;
            slots.insert(slot.id, pointer);
        }

        for block in ordered_blocks {
            builder.position_at_end(self.block(&blocks, block.id)?);
            for operation in &block.operations {
                self.lower_operation(
                    function.id,
                    block.id,
                    operation,
                    &builder,
                    &mut values,
                    &slots,
                )?;
            }
            self.lower_terminator(function, block, &builder, &blocks, &values, &phis)?;
        }
        Ok(())
    }

    fn lower_operation(
        &self,
        function: FunctionId,
        block: BlockId,
        operation: &Operation,
        builder: &Builder<'ctx>,
        values: &mut BTreeMap<ValueId, BasicValueEnum<'ctx>>,
        slots: &BTreeMap<SlotId, PointerValue<'ctx>>,
    ) -> Result<(), BackendError> {
        match operation {
            Operation::Constant {
                result,
                constant,
                ty,
                ..
            } => {
                let value = self.constant(constant, *ty)?;
                values.insert(*result, value);
            }
            Operation::CheckedArithmetic {
                result,
                operator,
                left,
                right,
                ty,
                origin,
            } => {
                let left = integer_value(values, *left)?;
                let right = integer_value(values, *right)?;
                let value = self.lower_checked_arithmetic(
                    function, *result, *operator, left, right, *ty, *origin, builder,
                )?;
                values.insert(*result, value.into());
            }
            Operation::Call {
                result,
                function: called,
                arguments,
                ..
            } => {
                let target = self
                    .functions
                    .get(called)
                    .copied()
                    .ok_or(BackendError::MissingFunction(*called))?;
                let arguments = arguments
                    .iter()
                    .map(|argument| value(values, *argument).map(BasicMetadataValueEnum::from))
                    .collect::<Result<Vec<_>, _>>()?;
                let call =
                    built(builder.build_call(target, &arguments, &format!("v{}", result.0)))?;
                let result_value = call
                    .try_as_basic_value()
                    .basic()
                    .ok_or(BackendError::MissingValue(*result))?;
                values.insert(*result, result_value);
            }
            Operation::Load {
                result, slot, ty, ..
            } => {
                let pointer = slots
                    .get(slot)
                    .copied()
                    .ok_or(BackendError::MissingSlot(*slot))?;
                let loaded = built(builder.build_load(
                    self.basic_type(*ty)?,
                    pointer,
                    &format!("v{}", result.0),
                ))?;
                values.insert(*result, loaded);
            }
            Operation::Store {
                slot, value: id, ..
            } => {
                let pointer = slots
                    .get(slot)
                    .copied()
                    .ok_or(BackendError::MissingSlot(*slot))?;
                built(builder.build_store(pointer, value(values, *id)?))?;
            }
            other => {
                return Err(BackendError::UnsupportedOperation {
                    function,
                    block,
                    operation: operation_name(other),
                });
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_terminator(
        &self,
        function: &CoreFunction,
        block: &Block,
        builder: &Builder<'ctx>,
        blocks: &BTreeMap<BlockId, LlvmBlock<'ctx>>,
        values: &BTreeMap<ValueId, BasicValueEnum<'ctx>>,
        phis: &BTreeMap<ValueId, PhiValue<'ctx>>,
    ) -> Result<(), BackendError> {
        match &block.terminator {
            Terminator::Branch {
                target, arguments, ..
            } => {
                let source = builder.get_insert_block().ok_or_else(|| {
                    BackendError::Builder("builder has no current block".to_owned())
                })?;
                self.add_phi_incoming(*target, arguments, source, function, values, phis)?;
                built(builder.build_unconditional_branch(self.block(blocks, *target)?))?;
            }
            Terminator::CondBranch {
                condition,
                then_target,
                else_target,
                ..
            } => {
                built(builder.build_conditional_branch(
                    integer_value(values, *condition)?,
                    self.block(blocks, *then_target)?,
                    self.block(blocks, *else_target)?,
                ))?;
            }
            Terminator::Return { value: id, .. } => {
                let result = value(values, *id)?;
                built(builder.build_return(Some(&result)))?;
            }
            Terminator::Unreachable { .. } => {
                built(builder.build_unreachable())?;
            }
            Terminator::Switch { .. } => {
                return Err(BackendError::UnsupportedTerminator {
                    function: function.id,
                    block: block.id,
                    terminator: "switch",
                });
            }
        }
        Ok(())
    }

    fn add_phi_incoming(
        &self,
        target: BlockId,
        arguments: &[ValueId],
        source: LlvmBlock<'ctx>,
        function: &CoreFunction,
        values: &BTreeMap<ValueId, BasicValueEnum<'ctx>>,
        phis: &BTreeMap<ValueId, PhiValue<'ctx>>,
    ) -> Result<(), BackendError> {
        let target = function
            .blocks
            .iter()
            .find(|block| block.id == target)
            .ok_or(BackendError::MissingBlock(target))?;
        for (argument, parameter) in arguments.iter().zip(&target.parameters) {
            let incoming = value(values, *argument)?;
            let phi = phis
                .get(&parameter.value)
                .copied()
                .ok_or(BackendError::MissingValue(parameter.value))?;
            phi.add_incoming(&[(&incoming, source)]);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_checked_arithmetic(
        &self,
        function: FunctionId,
        result: ValueId,
        operator: ArithmeticOperator,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        ty: TypeId,
        origin: Span,
        builder: &Builder<'ctx>,
    ) -> Result<IntValue<'ctx>, BackendError> {
        let origin =
            FailureOrigin::from_span(origin).map_err(|()| BackendError::SourceOriginOutOfRange)?;
        let categories = failure_categories(operator);
        match operator {
            ArithmeticOperator::Add
            | ArithmeticOperator::Subtract
            | ArithmeticOperator::Multiply => {
                let intrinsic = match operator {
                    ArithmeticOperator::Add => "llvm.sadd.with.overflow",
                    ArithmeticOperator::Subtract => "llvm.ssub.with.overflow",
                    ArithmeticOperator::Multiply => "llvm.smul.with.overflow",
                    ArithmeticOperator::Divide | ArithmeticOperator::Remainder => unreachable!(),
                };
                let declaration = Intrinsic::find(intrinsic)
                    .and_then(|intrinsic| {
                        intrinsic.get_declaration(&self.module, &[left.get_type().into()])
                    })
                    .ok_or(BackendError::MissingIntrinsic(intrinsic))?;
                let call = built(builder.build_call(
                    declaration,
                    &[left.into(), right.into()],
                    &format!("v{}.checked", result.0),
                ))?;
                let aggregate = match call.try_as_basic_value().basic() {
                    Some(BasicValueEnum::StructValue(value)) => value,
                    _ => return Err(BackendError::InvalidIntrinsicResult(intrinsic)),
                };
                let checked = match built(builder.build_extract_value(
                    aggregate,
                    0,
                    &format!("v{}", result.0),
                ))? {
                    BasicValueEnum::IntValue(value) => value,
                    _ => return Err(BackendError::InvalidIntrinsicResult(intrinsic)),
                };
                let overflow = match built(builder.build_extract_value(
                    aggregate,
                    1,
                    &format!("v{}.overflow", result.0),
                ))? {
                    BasicValueEnum::IntValue(value) => value,
                    _ => return Err(BackendError::InvalidIntrinsicResult(intrinsic)),
                };
                self.branch_on_failure(
                    function,
                    result,
                    "overflow",
                    overflow,
                    categories.overflow,
                    origin,
                    builder,
                )?;
                Ok(checked)
            }
            ArithmeticOperator::Divide | ArithmeticOperator::Remainder => {
                let zero = built(builder.build_int_compare(
                    IntPredicate::EQ,
                    right,
                    right.get_type().const_zero(),
                    &format!("v{}.zero", result.0),
                ))?;
                self.branch_on_failure(
                    function,
                    result,
                    "division_by_zero",
                    zero,
                    categories
                        .zero
                        .ok_or(BackendError::InvalidIntegerCheckPlan)?,
                    origin,
                    builder,
                )?;

                let minimum = match self.core.types.get(ty.0 as usize) {
                    Some(Type::I32) => left.get_type().const_int(i32::MIN as u32 as u64, false),
                    Some(Type::I64) => left.get_type().const_int(i64::MIN as u64, false),
                    _ => return Err(BackendError::UnsupportedType(ty)),
                };
                let negative_one = left.get_type().const_all_ones();
                let left_is_minimum = built(builder.build_int_compare(
                    IntPredicate::EQ,
                    left,
                    minimum,
                    &format!("v{}.minimum", result.0),
                ))?;
                let right_is_negative_one = built(builder.build_int_compare(
                    IntPredicate::EQ,
                    right,
                    negative_one,
                    &format!("v{}.negative_one", result.0),
                ))?;
                let overflow = built(builder.build_and(
                    left_is_minimum,
                    right_is_negative_one,
                    &format!("v{}.overflow", result.0),
                ))?;
                self.branch_on_failure(
                    function,
                    result,
                    "overflow",
                    overflow,
                    categories.overflow,
                    origin,
                    builder,
                )?;
                let name = format!("v{}", result.0);
                match operator {
                    ArithmeticOperator::Divide => {
                        built(builder.build_int_signed_div(left, right, &name))
                    }
                    ArithmeticOperator::Remainder => {
                        built(builder.build_int_signed_rem(left, right, &name))
                    }
                    ArithmeticOperator::Add
                    | ArithmeticOperator::Subtract
                    | ArithmeticOperator::Multiply => unreachable!(),
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn branch_on_failure(
        &self,
        function: FunctionId,
        result: ValueId,
        label: &str,
        failed: IntValue<'ctx>,
        category: FailureCategory,
        origin: FailureOrigin,
        builder: &Builder<'ctx>,
    ) -> Result<(), BackendError> {
        let function = self
            .functions
            .get(&function)
            .copied()
            .ok_or(BackendError::MissingFunction(function))?;
        let failure = self
            .context
            .append_basic_block(function, &format!("v{}.fail.{label}", result.0));
        let continuation = self
            .context
            .append_basic_block(function, &format!("v{}.ok.{label}", result.0));
        built(builder.build_conditional_branch(failed, failure, continuation))?;

        builder.position_at_end(failure);
        let i32_type = self.context.i32_type();
        let i64_type = self.context.i64_type();
        built(builder.build_call(
            self.failure,
            &[
                i32_type.const_int(u64::from(category.code()), false).into(),
                i32_type.const_int(u64::from(origin.file), false).into(),
                i64_type.const_int(origin.start, false).into(),
                i64_type.const_int(origin.end, false).into(),
            ],
            "",
        ))?;
        built(builder.build_unreachable())?;

        builder.position_at_end(continuation);
        Ok(())
    }

    fn constant(
        &self,
        constant: &Constant,
        ty: TypeId,
    ) -> Result<BasicValueEnum<'ctx>, BackendError> {
        match (constant, self.core.types.get(ty.0 as usize)) {
            (Constant::Integer(value), Some(Type::I32)) => {
                let value = i32::try_from(*value)
                    .map_err(|_| BackendError::IntegerOutOfRange { value: *value, ty })?;
                Ok(self
                    .context
                    .i32_type()
                    .const_int(value as u32 as u64, false)
                    .into())
            }
            (Constant::Integer(value), Some(Type::I64)) => {
                let value = i64::try_from(*value)
                    .map_err(|_| BackendError::IntegerOutOfRange { value: *value, ty })?;
                Ok(self
                    .context
                    .i64_type()
                    .const_int(value as u64, false)
                    .into())
            }
            (Constant::Boolean(value), Some(Type::Bool)) => Ok(self
                .context
                .bool_type()
                .const_int(u64::from(*value), false)
                .into()),
            (Constant::Unit, Some(Type::Unit)) => Ok(self.basic_type(ty)?.const_zero()),
            _ => Err(BackendError::UnsupportedType(ty)),
        }
    }

    fn block(
        &self,
        blocks: &BTreeMap<BlockId, LlvmBlock<'ctx>>,
        id: BlockId,
    ) -> Result<LlvmBlock<'ctx>, BackendError> {
        blocks
            .get(&id)
            .copied()
            .ok_or(BackendError::MissingBlock(id))
    }
}

fn built<T>(result: Result<T, BuilderError>) -> Result<T, BackendError> {
    result.map_err(|error| BackendError::Builder(error.to_string()))
}

fn value<'ctx>(
    values: &BTreeMap<ValueId, BasicValueEnum<'ctx>>,
    id: ValueId,
) -> Result<BasicValueEnum<'ctx>, BackendError> {
    values
        .get(&id)
        .copied()
        .ok_or(BackendError::MissingValue(id))
}

fn integer_value<'ctx>(
    values: &BTreeMap<ValueId, BasicValueEnum<'ctx>>,
    id: ValueId,
) -> Result<inkwell::values::IntValue<'ctx>, BackendError> {
    match value(values, id)? {
        BasicValueEnum::IntValue(value) => Ok(value),
        _ => Err(BackendError::MissingValue(id)),
    }
}

fn operation_name(operation: &Operation) -> &'static str {
    match operation {
        Operation::Constant { .. } => "constant",
        Operation::List { .. } => "list",
        Operation::Array { .. } => "array",
        Operation::Map { .. } => "map",
        Operation::Tuple { .. } => "tuple",
        Operation::TupleProject { .. } => "tuple_project",
        Operation::StructProject { .. } => "struct_project",
        Operation::ListHead { .. } => "list_head",
        Operation::ListTail { .. } => "list_tail",
        Operation::CheckedArithmetic { .. } => "checked_arithmetic",
        Operation::Call { .. } => "call",
        Operation::UnionInject { .. } => "union_inject",
        Operation::UnionProject { .. } => "union_project",
        Operation::Load { .. } => "load",
        Operation::Store { .. } => "store",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use el_ir::{executable_reachability_roots, lower, monomorphize};
    use el_parser::parse;
    use el_resolve::resolve;
    use el_span::SourceMap;
    use el_types::check;

    fn concrete(source: &str) -> ConcreteModule {
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/main.el", source);
        let parsed = parse(file, source).expect("fixture parses");
        let resolved = resolve(&parsed).expect("fixture resolves");
        let typed = check(&resolved).expect("fixture type checks");
        let generic = lower(&typed);
        let roots = executable_reachability_roots(&generic).expect("fixture entry point");
        monomorphize(&generic, &roots).expect("fixture monomorphizes")
    }

    #[test]
    fn lowers_arithmetic_calls_slots_blocks_and_returns() {
        let core = concrete(
            "defmodule Main do\n  def double(value: i32) -> i32 do\n    value * 2\n  end\n  def main() -> i32 do\n    mut total: i32 = 20\n    total := double(total)\n    if true do\n      total + 2\n    else\n      0\n    end\n  end\nend\n",
        );

        let llvm = lower_to_llvm_ir(&core).expect("supported Core lowers and verifies");

        assert!(llvm.as_str().contains("define i32 @el.f0(i32"));
        assert!(llvm.as_str().contains("alloca i32"));
        assert!(llvm.as_str().contains("call i32 @el.f0"));
        assert!(llvm.as_str().contains("phi i32"));
        assert!(llvm.as_str().contains("ret i32"));
        assert!(llvm.as_str().contains("define i32 @main()"));
        assert!(llvm.as_str().contains("call i32 @el.f1()"));
        assert!(llvm.as_str().contains("@llvm.smul.with.overflow.i32"));
        assert!(llvm.as_str().contains("@llvm.sadd.with.overflow.i32"));
        assert!(
            llvm.as_str()
                .contains("declare void @__el_runtime_fail(i32, i32, i64, i64)")
        );
        assert!(
            llvm.as_str()
                .contains("call void @__el_runtime_fail(i32 1, i32 0, i64")
        );
    }

    #[test]
    fn checks_division_zero_before_signed_overflow_with_the_same_origin() {
        let core = concrete(
            "defmodule Main do\n  def divide(value: i32, divisor: i32) -> i32 do\n    value / divisor\n  end\n  def main() -> i32 do\n    divide(10, 2)\n  end\nend\n",
        );

        let llvm = lower_to_llvm_ir(&core).expect("checked division lowers and verifies");
        let text = llvm.as_str();

        assert!(text.contains("v2.fail.division_by_zero"));
        assert!(text.contains("call void @__el_runtime_fail(i32 2, i32 0, i64"));
        assert!(text.contains("v2.fail.overflow"));
        assert!(text.contains("call void @__el_runtime_fail(i32 1, i32 0, i64"));
        assert!(text.contains("sdiv i32"));
    }

    #[test]
    fn rejects_operations_outside_the_first_backend_slice() {
        let core = concrete(
            "defmodule Main do\n  def main() -> i32 do\n    values: [i32] = [1]\n    0\n  end\nend\n",
        );

        assert!(matches!(
            lower_to_llvm_ir(&core),
            Err(BackendError::UnsupportedOperation {
                operation: "list",
                ..
            })
        ));
    }

    #[test]
    fn process_entry_forwards_the_el_entry_result() {
        let core = concrete("defmodule Main do\n  def main() -> i32 do\n    40 + 2\n  end\nend\n");

        let llvm = lower_to_llvm_ir(&core).expect("entry shim lowers and verifies");
        let text = llvm.as_str();

        assert!(text.contains("define i32 @main()"));
        assert!(text.contains("%el.exit_status = call i32 @el.f0()"));
        assert!(text.contains("ret i32 %el.exit_status"));
    }

    #[test]
    fn rejects_a_root_that_is_not_the_executable_entry() {
        let mut core = concrete("defmodule Main do\n  def main() -> i32 do\n    42\n  end\nend\n");
        core.functions[0].name = "not_main".to_owned();

        assert_eq!(
            lower_to_llvm_ir(&core),
            Err(BackendError::MissingExecutableEntry)
        );
    }

    #[cfg(feature = "llvm")]
    #[test]
    fn initializes_the_host_target_and_emits_an_object() {
        let core = concrete("defmodule Main do\n  def main() -> i32 do\n    40 + 2\n  end\nend\n");
        let output =
            std::env::temp_dir().join(format!("el-codegen-object-test-{}.o", std::process::id()));

        let target = emit_host_object(&core, &output).expect("host object emits");
        let metadata = std::fs::metadata(&output).expect("emitted object exists");
        assert!(metadata.len() > 0);
        assert!(!target.llvm_target_triple().is_empty());
        assert!(matches!(target.pointer_width(), 32 | 64));
        std::fs::remove_file(output).expect("remove emitted object");
    }
}
