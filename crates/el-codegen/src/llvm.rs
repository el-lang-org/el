//! The private Inkwell boundary. No Inkwell or LLVM type escapes this module.

use crate::integer_checks::FailureOrigin;
use crate::{CodegenProfile, InvalidTargetMetadata, TargetMetadata};
use el_ir::{
    ArithmeticOperator, Block, BlockId, ComparisonOperator, ConcreteModule, Constant,
    CoreFailureCategory, CoreFunction, FunctionId, Operation, SlotId, SwitchValue, Terminator,
    Type, TypeId, ValueId, collection_point_roots, verify_concrete,
};
#[cfg(feature = "managed-runtime")]
use el_runtime::{ALLOCATE_SCANNED_SYMBOL, INITIALIZE_SYMBOL};
use el_runtime::{FAILURE_SYMBOL, FailureCategory};
use inkwell::AddressSpace;
use inkwell::IntPredicate;
use inkwell::OptimizationLevel;
use inkwell::basic_block::BasicBlock as LlvmBlock;
use inkwell::builder::{Builder, BuilderError};
use inkwell::context::Context;
use inkwell::intrinsics::Intrinsic;
use inkwell::module::{Linkage, Module};
use inkwell::passes::PassBuilderOptions;
use inkwell::targets::{
    CodeModel, FileType, InitializationConfig, RelocMode, Target, TargetMachine,
};
use inkwell::types::{BasicMetadataTypeEnum, BasicType, BasicTypeEnum, StructType};
use inkwell::values::{
    AggregateValueEnum, BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue,
    InstructionValue, IntValue, PhiValue, PointerValue, StructValue,
};
use std::collections::{BTreeMap, BTreeSet};
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
    InvalidUnionMember {
        union: TypeId,
        member: TypeId,
    },
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
            Self::InvalidUnionMember { union, member } => {
                write!(formatter, "{member:?} is not a member of union {union:?}")
            }
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

/// Lowers the supported Concrete Core scalar, control-flow, and aggregate slice
/// and verifies the resulting LLVM module before returning deterministic text.
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

/// Returns the selected compiler-host target facts without exposing LLVM types.
pub fn host_target_metadata() -> Result<TargetMetadata, BackendError> {
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
            OptimizationLevel::Less,
            RelocMode::PIC,
            CodeModel::Default,
        )
        .ok_or(BackendError::HostTargetMachineUnavailable)?;
    let triple_text = triple
        .as_str()
        .to_str()
        .map_err(|_| BackendError::HostTripleNotUtf8)?;
    TargetMetadata::new(
        triple_text,
        machine.get_target_data().get_pointer_byte_size(None) * 8,
    )
    .map_err(BackendError::InvalidTargetMetadata)
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
    #[cfg(feature = "managed-runtime")]
    initialize_runtime: FunctionValue<'ctx>,
    #[cfg(feature = "managed-runtime")]
    allocate_scanned: FunctionValue<'ctx>,
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
        #[cfg(feature = "managed-runtime")]
        let initialize_runtime = module.add_function(
            INITIALIZE_SYMBOL,
            context.void_type().fn_type(&[], false),
            None,
        );
        #[cfg(feature = "managed-runtime")]
        let allocate_scanned = module.add_function(
            ALLOCATE_SCANNED_SYMBOL,
            context.ptr_type(AddressSpace::default()).fn_type(
                &[
                    context.i64_type().into(),
                    context.i32_type().into(),
                    context.i64_type().into(),
                    context.i64_type().into(),
                ],
                false,
            ),
            None,
        );
        Self {
            context,
            core,
            module,
            functions: BTreeMap::new(),
            failure,
            #[cfg(feature = "managed-runtime")]
            initialize_runtime,
            #[cfg(feature = "managed-runtime")]
            allocate_scanned,
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
        #[cfg(feature = "managed-runtime")]
        built(builder.build_call(self.initialize_runtime, &[], ""))?;
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
            Some(Type::String) => Ok(self
                .context
                .struct_type(
                    &[
                        self.context.ptr_type(AddressSpace::default()).into(),
                        self.context.i64_type().into(),
                    ],
                    false,
                )
                .into()),
            Some(Type::List(_)) => Ok(self.context.ptr_type(AddressSpace::default()).into()),
            Some(Type::Atom(_)) => Ok(self.context.i8_type().into()),
            Some(Type::Tuple(elements)) => {
                let fields = elements
                    .iter()
                    .map(|element| self.basic_type(*element))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self.context.struct_type(&fields, false).into())
            }
            Some(Type::Union(members)) => {
                let mut fields = Vec::with_capacity(members.len() + 1);
                fields.push(self.context.i32_type().into());
                for member in members {
                    fields.push(self.basic_type(*member)?);
                }
                Ok(self.context.struct_type(&fields, false).into())
            }
            _ => Err(BackendError::UnsupportedType(ty)),
        }
    }

    fn union_tag(&self, union: TypeId, member: TypeId) -> Result<u32, BackendError> {
        let Some(Type::Union(members)) = self.core.types.get(union.0 as usize) else {
            return Err(BackendError::UnsupportedType(union));
        };
        members
            .iter()
            .position(|candidate| *candidate == member)
            .and_then(|index| u32::try_from(index).ok())
            .ok_or(BackendError::InvalidUnionMember { union, member })
    }

    fn list_node_type(&self, list: TypeId) -> Result<StructType<'ctx>, BackendError> {
        let Some(Type::List(item)) = self.core.types.get(list.0 as usize) else {
            return Err(BackendError::UnsupportedType(list));
        };
        Ok(self.context.struct_type(
            &[
                self.basic_type(*item)?,
                self.context.ptr_type(AddressSpace::default()).into(),
            ],
            false,
        ))
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

        let collection_points = collection_point_roots(self.core, function)
            .into_iter()
            .map(|point| ((point.block, point.operation_index), point))
            .collect::<BTreeMap<_, _>>();
        let value_types = core_value_types(function);
        let slot_types = function
            .slots
            .iter()
            .map(|slot| (slot.id, slot.ty))
            .collect::<BTreeMap<_, _>>();
        let rooted_values = collection_points
            .values()
            .flat_map(|point| point.values.iter().copied())
            .collect::<BTreeSet<_>>();
        let mut root_slots = BTreeMap::new();
        for value in rooted_values {
            let ty = value_types
                .get(&value)
                .copied()
                .ok_or(BackendError::MissingValue(value))?;
            let llvm_ty = self.basic_type(ty)?;
            let pointer = built(builder.build_alloca(llvm_ty, &format!("gc.root.v{}", value.0)))?;
            set_volatile(built(builder.build_store(pointer, llvm_ty.const_zero()))?)?;
            root_slots.insert(value, pointer);
        }
        let mut partial_list_roots = BTreeMap::new();
        for block in &function.blocks {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                if collection_points.contains_key(&(block.id, operation_index))
                    && matches!(operation, Operation::List { .. })
                {
                    let pointer = built(builder.build_alloca(
                        self.context.ptr_type(AddressSpace::default()),
                        &format!("gc.partial.b{}.o{}", block.id.0, operation_index),
                    ))?;
                    set_volatile(built(builder.build_store(
                        pointer,
                        self.context.ptr_type(AddressSpace::default()).const_null(),
                    ))?)?;
                    partial_list_roots.insert((block.id, operation_index), pointer);
                }
            }
        }

        for block in ordered_blocks {
            builder.position_at_end(self.block(&blocks, block.id)?);
            for (operation_index, operation) in block.operations.iter().enumerate() {
                self.lower_operation(
                    function.id,
                    block.id,
                    operation,
                    &builder,
                    &blocks,
                    &mut values,
                    &slots,
                    collection_points.get(&(block.id, operation_index)),
                    &root_slots,
                    &value_types,
                    &slot_types,
                    partial_list_roots
                        .get(&(block.id, operation_index))
                        .copied(),
                )?;
            }
            self.lower_terminator(function, block, &builder, &blocks, &values, &phis)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_operation(
        &self,
        function: FunctionId,
        block: BlockId,
        operation: &Operation,
        builder: &Builder<'ctx>,
        blocks: &BTreeMap<BlockId, LlvmBlock<'ctx>>,
        values: &mut BTreeMap<ValueId, BasicValueEnum<'ctx>>,
        slots: &BTreeMap<SlotId, PointerValue<'ctx>>,
        roots: Option<&el_ir::CollectionPointRoots>,
        root_slots: &BTreeMap<ValueId, PointerValue<'ctx>>,
        value_types: &BTreeMap<ValueId, TypeId>,
        slot_types: &BTreeMap<SlotId, TypeId>,
        _partial_list_root: Option<PointerValue<'ctx>>,
    ) -> Result<(), BackendError> {
        match operation {
            Operation::Constant {
                result,
                constant,
                ty,
                ..
            } => {
                let value = self.constant(function, *result, constant, *ty)?;
                values.insert(*result, value);
            }
            Operation::List {
                result,
                elements,
                tail,
                ty,
                origin,
            } => {
                let null = self.context.ptr_type(AddressSpace::default()).const_null();
                let initial = tail
                    .map(|tail| pointer_value(values, tail))
                    .transpose()?
                    .unwrap_or(null);
                if elements.is_empty() {
                    values.insert(*result, initial.into());
                } else {
                    #[cfg(not(feature = "managed-runtime"))]
                    {
                        let _ = (ty, origin);
                        return Err(BackendError::UnsupportedOperation {
                            function,
                            block,
                            operation: "list",
                        });
                    }
                    #[cfg(feature = "managed-runtime")]
                    {
                        let roots = roots.ok_or_else(|| {
                            BackendError::InvalidConcrete(vec![format!(
                                "missing live-root set for collection point {function:?} {block:?}"
                            )])
                        })?;
                        let partial = _partial_list_root.ok_or_else(|| {
                            BackendError::Builder(
                                "allocating list has no partial-list root".to_owned(),
                            )
                        })?;
                        self.preserve_roots(roots, builder, values, slots, root_slots, slot_types)?;
                        set_volatile(built(builder.build_store(partial, initial))?)?;

                        let node_type = self.list_node_type(*ty)?;
                        let native_size = node_type
                            .size_of()
                            .ok_or(BackendError::UnsupportedType(*ty))?;
                        let size = if native_size.get_type() == self.context.i64_type() {
                            native_size
                        } else {
                            built(builder.build_int_cast(
                                native_size,
                                self.context.i64_type(),
                                &format!("v{}.node_size", result.0),
                            ))?
                        };
                        let source = FailureOrigin::from_span(*origin)
                            .map_err(|()| BackendError::SourceOriginOutOfRange)?;
                        let mut current = initial;
                        for (index, element) in elements.iter().enumerate().rev() {
                            let call = built(
                                builder.build_call(
                                    self.allocate_scanned,
                                    &[
                                        size.into(),
                                        self.context
                                            .i32_type()
                                            .const_int(u64::from(source.file), false)
                                            .into(),
                                        self.context
                                            .i64_type()
                                            .const_int(source.start, false)
                                            .into(),
                                        self.context.i64_type().const_int(source.end, false).into(),
                                    ],
                                    &format!("v{}.node{index}", result.0),
                                ),
                            )?;
                            let node = match call.try_as_basic_value().basic() {
                                Some(BasicValueEnum::PointerValue(pointer)) => pointer,
                                _ => return Err(BackendError::MissingValue(*result)),
                            };
                            let item = built(builder.build_struct_gep(
                                node_type,
                                node,
                                0,
                                &format!("v{}.node{index}.item", result.0),
                            ))?;
                            let next = built(builder.build_struct_gep(
                                node_type,
                                node,
                                1,
                                &format!("v{}.node{index}.next", result.0),
                            ))?;
                            built(builder.build_store(item, value(values, *element)?))?;
                            built(builder.build_store(next, current))?;
                            current = node;
                            set_volatile(built(builder.build_store(partial, current))?)?;
                        }
                        self.clear_value_roots(roots, builder, root_slots, value_types)?;
                        set_volatile(built(builder.build_store(partial, null))?)?;
                        values.insert(*result, current.into());
                    }
                }
            }
            Operation::Tuple {
                result,
                elements,
                ty,
                ..
            } => {
                let mut aggregate = AggregateValueEnum::StructValue(
                    self.basic_type(*ty)?.into_struct_type().get_undef(),
                );
                for (index, element) in elements.iter().enumerate() {
                    aggregate = built(builder.build_insert_value(
                        aggregate,
                        value(values, *element)?,
                        index as u32,
                        &format!("v{}.field{index}", result.0),
                    ))?;
                }
                values.insert(*result, aggregate.into_struct_value().into());
            }
            Operation::TupleProject {
                result,
                tuple,
                index,
                ..
            } => {
                let projected = built(builder.build_extract_value(
                    struct_value(values, *tuple)?,
                    *index as u32,
                    &format!("v{}", result.0),
                ))?;
                values.insert(*result, projected);
            }
            Operation::ListHead {
                result, list, ty, ..
            } => {
                let list_ty = value_types
                    .get(list)
                    .copied()
                    .ok_or(BackendError::MissingValue(*list))?;
                let node_type = self.list_node_type(list_ty)?;
                let item = built(builder.build_struct_gep(
                    node_type,
                    pointer_value(values, *list)?,
                    0,
                    &format!("v{}.item", result.0),
                ))?;
                let loaded = built(builder.build_load(
                    self.basic_type(*ty)?,
                    item,
                    &format!("v{}", result.0),
                ))?;
                values.insert(*result, loaded);
            }
            Operation::ListTail {
                result, list, ty, ..
            } => {
                let node_type = self.list_node_type(*ty)?;
                let next = built(builder.build_struct_gep(
                    node_type,
                    pointer_value(values, *list)?,
                    1,
                    &format!("v{}.next", result.0),
                ))?;
                let loaded = built(builder.build_load(
                    self.basic_type(*ty)?,
                    next,
                    &format!("v{}", result.0),
                ))?;
                values.insert(*result, loaded);
            }
            Operation::CheckedArithmetic {
                result,
                operator,
                left,
                right,
                failures,
                ty,
                ..
            } => {
                let left = integer_value(values, *left)?;
                let right = integer_value(values, *right)?;
                let value = self.lower_checked_arithmetic(
                    *result, *operator, left, right, failures, *ty, builder, blocks,
                )?;
                values.insert(*result, value.into());
            }
            Operation::Compare {
                result,
                operator,
                left,
                right,
                ..
            } => {
                let predicate = match operator {
                    ComparisonOperator::Equal => IntPredicate::EQ,
                    ComparisonOperator::NotEqual => IntPredicate::NE,
                    ComparisonOperator::Less => IntPredicate::SLT,
                    ComparisonOperator::LessEqual => IntPredicate::SLE,
                    ComparisonOperator::Greater => IntPredicate::SGT,
                    ComparisonOperator::GreaterEqual => IntPredicate::SGE,
                };
                let compared = built(builder.build_int_compare(
                    predicate,
                    integer_value(values, *left)?,
                    integer_value(values, *right)?,
                    &format!("v{}", result.0),
                ))?;
                values.insert(*result, compared.into());
            }
            Operation::UnionInject {
                result,
                member,
                value: payload,
                ty,
                ..
            } => {
                let tag = self.union_tag(*ty, *member)?;
                let mut aggregate = AggregateValueEnum::StructValue(
                    self.basic_type(*ty)?.into_struct_type().get_undef(),
                );
                aggregate = built(builder.build_insert_value(
                    aggregate,
                    self.context.i32_type().const_int(u64::from(tag), false),
                    0,
                    &format!("v{}.tag", result.0),
                ))?;
                aggregate = built(builder.build_insert_value(
                    aggregate,
                    value(values, *payload)?,
                    tag + 1,
                    &format!("v{}.payload", result.0),
                ))?;
                values.insert(*result, aggregate.into_struct_value().into());
            }
            Operation::UnionProject {
                result,
                member,
                value: union,
                union_ty,
                ..
            } => {
                let tag = self.union_tag(*union_ty, *member)?;
                let projected = built(builder.build_extract_value(
                    struct_value(values, *union)?,
                    tag + 1,
                    &format!("v{}", result.0),
                ))?;
                values.insert(*result, projected);
            }
            Operation::Call {
                result,
                function: called,
                arguments,
                ..
            } => {
                let roots = roots.ok_or_else(|| {
                    BackendError::InvalidConcrete(vec![format!(
                        "missing live-root set for collection point {function:?} {block:?}"
                    )])
                })?;
                self.preserve_roots(roots, builder, values, slots, root_slots, slot_types)?;
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
                self.clear_value_roots(roots, builder, root_slots, value_types)?;
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

    fn preserve_roots(
        &self,
        roots: &el_ir::CollectionPointRoots,
        builder: &Builder<'ctx>,
        values: &BTreeMap<ValueId, BasicValueEnum<'ctx>>,
        slots: &BTreeMap<SlotId, PointerValue<'ctx>>,
        root_slots: &BTreeMap<ValueId, PointerValue<'ctx>>,
        slot_types: &BTreeMap<SlotId, TypeId>,
    ) -> Result<(), BackendError> {
        for root in &roots.values {
            let pointer = root_slots
                .get(root)
                .copied()
                .ok_or(BackendError::MissingValue(*root))?;
            set_volatile(built(builder.build_store(pointer, value(values, *root)?))?)?;
        }
        for slot in &roots.slots {
            let pointer = slots
                .get(slot)
                .copied()
                .ok_or(BackendError::MissingSlot(*slot))?;
            let ty = slot_types
                .get(slot)
                .copied()
                .ok_or(BackendError::MissingSlot(*slot))?;
            let loaded = built(builder.build_load(
                self.basic_type(ty)?,
                pointer,
                &format!("gc.root.q{}.touch", slot.0),
            ))?;
            let instruction = loaded.as_instruction_value().ok_or_else(|| {
                BackendError::Builder("GC root touch is not an instruction".to_owned())
            })?;
            set_volatile(instruction)?;
        }
        Ok(())
    }

    fn clear_value_roots(
        &self,
        roots: &el_ir::CollectionPointRoots,
        builder: &Builder<'ctx>,
        root_slots: &BTreeMap<ValueId, PointerValue<'ctx>>,
        value_types: &BTreeMap<ValueId, TypeId>,
    ) -> Result<(), BackendError> {
        for root in &roots.values {
            let pointer = root_slots
                .get(root)
                .copied()
                .ok_or(BackendError::MissingValue(*root))?;
            let ty = value_types
                .get(root)
                .copied()
                .ok_or(BackendError::MissingValue(*root))?;
            set_volatile(built(
                builder.build_store(pointer, self.basic_type(ty)?.const_zero()),
            )?)?;
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
            Terminator::Failure { category, origin } => {
                let origin = FailureOrigin::from_span(*origin)
                    .map_err(|()| BackendError::SourceOriginOutOfRange)?;
                let category = match category {
                    CoreFailureCategory::IntegerOverflow => FailureCategory::IntegerOverflow,
                    CoreFailureCategory::DivisionByZero => FailureCategory::DivisionByZero,
                };
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
            }
            Terminator::Unreachable { .. } => {
                built(builder.build_unreachable())?;
            }
            Terminator::Switch {
                subject,
                subject_ty,
                cases,
                default,
                ..
            } => {
                let source = value(values, *subject)?;
                let discriminant = if matches!(
                    self.core.types.get(subject_ty.0 as usize),
                    Some(Type::Union(_))
                ) {
                    match built(builder.build_extract_value(
                        struct_value(values, *subject)?,
                        0,
                        &format!("v{}.tag", subject.0),
                    ))? {
                        BasicValueEnum::IntValue(value) => value,
                        _ => return Err(BackendError::UnsupportedType(*subject_ty)),
                    }
                } else if matches!(
                    self.core.types.get(subject_ty.0 as usize),
                    Some(Type::List(_))
                ) {
                    built(builder.build_is_null(
                        pointer_value(values, *subject)?,
                        &format!("v{}.empty", subject.0),
                    ))?
                } else {
                    match source {
                        BasicValueEnum::IntValue(value) => value,
                        _ => return Err(BackendError::UnsupportedType(*subject_ty)),
                    }
                };
                let mut lowered_cases = Vec::with_capacity(cases.len());
                for (case, target) in cases {
                    let value = match case {
                        SwitchValue::Boolean(value) => {
                            discriminant.get_type().const_int(u64::from(*value), false)
                        }
                        SwitchValue::Integer(value) => {
                            discriminant.get_type().const_int(*value as u64, true)
                        }
                        SwitchValue::Atom(_) => discriminant.get_type().const_zero(),
                        SwitchValue::UnionMember(member) => discriminant
                            .get_type()
                            .const_int(u64::from(self.union_tag(*subject_ty, *member)?), false),
                        SwitchValue::ListEmpty => discriminant.get_type().const_int(1, false),
                        SwitchValue::ListCons => discriminant.get_type().const_zero(),
                    };
                    lowered_cases.push((value, self.block(blocks, *target)?));
                }
                let default = if let Some(default) = default {
                    self.block(blocks, *default)?
                } else {
                    let llvm_function = self
                        .functions
                        .get(&function.id)
                        .copied()
                        .ok_or(BackendError::MissingFunction(function.id))?;
                    let unreachable = self.context.append_basic_block(
                        llvm_function,
                        &format!("b{}.switch_unreachable", block.id.0),
                    );
                    let unreachable_builder = self.context.create_builder();
                    unreachable_builder.position_at_end(unreachable);
                    built(unreachable_builder.build_unreachable())?;
                    unreachable
                };
                built(builder.build_switch(discriminant, default, &lowered_cases))?;
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
        result: ValueId,
        operator: ArithmeticOperator,
        left: IntValue<'ctx>,
        right: IntValue<'ctx>,
        failures: &[(CoreFailureCategory, BlockId)],
        ty: TypeId,
        builder: &Builder<'ctx>,
        blocks: &BTreeMap<BlockId, LlvmBlock<'ctx>>,
    ) -> Result<IntValue<'ctx>, BackendError> {
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
                    result,
                    "overflow",
                    overflow,
                    CoreFailureCategory::IntegerOverflow,
                    failures,
                    builder,
                    blocks,
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
                    result,
                    "division_by_zero",
                    zero,
                    CoreFailureCategory::DivisionByZero,
                    failures,
                    builder,
                    blocks,
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
                    result,
                    "overflow",
                    overflow,
                    CoreFailureCategory::IntegerOverflow,
                    failures,
                    builder,
                    blocks,
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
        result: ValueId,
        label: &str,
        failed: IntValue<'ctx>,
        category: CoreFailureCategory,
        failures: &[(CoreFailureCategory, BlockId)],
        builder: &Builder<'ctx>,
        blocks: &BTreeMap<BlockId, LlvmBlock<'ctx>>,
    ) -> Result<(), BackendError> {
        let failure = failures
            .iter()
            .find_map(|(candidate, block)| (*candidate == category).then_some(*block))
            .ok_or(BackendError::InvalidIntegerCheckPlan)?;
        let continuation = self.context.append_basic_block(
            builder
                .get_insert_block()
                .and_then(|block| block.get_parent())
                .ok_or_else(|| BackendError::Builder("builder has no function".to_owned()))?,
            &format!("v{}.ok.{label}", result.0),
        );
        built(builder.build_conditional_branch(
            failed,
            self.block(blocks, failure)?,
            continuation,
        ))?;
        builder.position_at_end(continuation);
        Ok(())
    }

    fn constant(
        &self,
        function: FunctionId,
        result: ValueId,
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
            (Constant::String(value), Some(Type::String)) => {
                let bytes = self.context.const_string(value.as_bytes(), false);
                let global = self.module.add_global(
                    bytes.get_type(),
                    None,
                    &format!("el.s{}.{}", function.0, result.0),
                );
                global.set_initializer(&bytes);
                global.set_constant(true);
                global.set_linkage(Linkage::Private);
                let string = self
                    .basic_type(ty)?
                    .into_struct_type()
                    .const_named_struct(&[
                        global.as_pointer_value().into(),
                        self.context
                            .i64_type()
                            .const_int(value.len() as u64, false)
                            .into(),
                    ]);
                Ok(string.into())
            }
            (Constant::Atom(_), Some(Type::Atom(_))) => {
                Ok(self.context.i8_type().const_zero().into())
            }
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

fn core_value_types(function: &CoreFunction) -> BTreeMap<ValueId, TypeId> {
    function
        .parameters
        .iter()
        .chain(function.blocks.iter().flat_map(|block| &block.parameters))
        .map(|parameter| (parameter.value, parameter.ty))
        .chain(function.blocks.iter().flat_map(|block| {
            block
                .operations
                .iter()
                .filter_map(|operation| match operation {
                    Operation::Constant { result, ty, .. }
                    | Operation::List { result, ty, .. }
                    | Operation::Array { result, ty, .. }
                    | Operation::Map { result, ty, .. }
                    | Operation::Tuple { result, ty, .. }
                    | Operation::TupleProject { result, ty, .. }
                    | Operation::StructProject { result, ty, .. }
                    | Operation::ListHead { result, ty, .. }
                    | Operation::ListTail { result, ty, .. }
                    | Operation::CheckedArithmetic { result, ty, .. }
                    | Operation::Call { result, ty, .. }
                    | Operation::UnionInject { result, ty, .. }
                    | Operation::UnionProject { result, ty, .. }
                    | Operation::Load { result, ty, .. } => Some((*result, *ty)),
                    Operation::Compare { result, .. } => Some((*result, TypeId(2))),
                    Operation::Store { .. } => None,
                })
        }))
        .collect()
}

fn set_volatile(instruction: InstructionValue<'_>) -> Result<(), BackendError> {
    instruction
        .set_volatile(true)
        .map_err(|error| BackendError::Builder(error.to_string()))
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

fn struct_value<'ctx>(
    values: &BTreeMap<ValueId, BasicValueEnum<'ctx>>,
    id: ValueId,
) -> Result<StructValue<'ctx>, BackendError> {
    match value(values, id)? {
        BasicValueEnum::StructValue(value) => Ok(value),
        _ => Err(BackendError::MissingValue(id)),
    }
}

fn pointer_value<'ctx>(
    values: &BTreeMap<ValueId, BasicValueEnum<'ctx>>,
    id: ValueId,
) -> Result<PointerValue<'ctx>, BackendError> {
    match value(values, id)? {
        BasicValueEnum::PointerValue(value) => Ok(value),
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
        Operation::Compare { .. } => "compare",
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
        #[cfg(feature = "managed-runtime")]
        assert!(llvm.as_str().contains("call void @__el_runtime_init()"));
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
        let failures = core.functions[0]
            .blocks
            .iter()
            .flat_map(|block| &block.operations)
            .find_map(|operation| match operation {
                Operation::CheckedArithmetic { failures, .. } => Some(failures),
                _ => None,
            })
            .expect("division failure plan");
        assert_eq!(
            failures
                .iter()
                .map(|(category, _)| *category)
                .collect::<Vec<_>>(),
            vec![
                CoreFailureCategory::DivisionByZero,
                CoreFailureCategory::IntegerOverflow,
            ]
        );

        assert!(text.contains(&format!("b{}:", failures[0].1.0)));
        assert!(text.contains("call void @__el_runtime_fail(i32 2, i32 0, i64"));
        assert!(text.contains(&format!("b{}:", failures[1].1.0)));
        assert!(text.contains("call void @__el_runtime_fail(i32 1, i32 0, i64"));
        assert!(text.contains("sdiv i32"));
    }

    #[test]
    fn lowers_tuples_atoms_union_payloads_and_exhaustive_switches() {
        let core = concrete(
            "defmodule Main do\n  @type Parsed = {:ok, i32} | :error\n  def parse(valid: bool) -> Parsed do\n    if valid do\n      {:ok, 40}\n    else\n      :error\n    end\n  end\n  def payload(value: {:ok, i32}) -> i32 do\n    match value do\n      {:ok, number} -> number\n    end\n  end\n  def unwrap(value: Parsed) -> i32 do\n    match value do\n      ok: {:ok, i32} -> payload(ok)\n      _ -> 2\n    end\n  end\n  def main() -> i32 do\n    unwrap(parse(true)) + unwrap(parse(false))\n  end\nend\n",
        );

        let llvm = lower_to_llvm_ir(&core).expect("aggregate match lowering verifies");
        let text = llvm.as_str();

        assert!(text.contains("extractvalue"), "{text}");
        assert!(text.contains("switch"), "{text}");
        assert!(text.contains("{ i32, i8, { i8, i32 } }"), "{text}");
    }

    #[test]
    fn lowers_static_strings_and_exhaustive_integer_string_unions() {
        let core = concrete(
            "defmodule Main do\n  @type Scalar = i64 | string\n  def choose(text: bool) -> Scalar do\n    if text do\n      \"forty-two\"\n    else\n      42\n    end\n  end\n  def classify(value: Scalar) -> i32 do\n    match value do\n      number: i64 -> 40\n      text: string -> 2\n    end\n  end\n  def main() -> i32 do\n    classify(choose(true))\n  end\nend\n",
        );

        let llvm = lower_to_llvm_ir(&core).expect("string union lowering verifies");
        let text = llvm.as_str();
        assert!(
            text.contains("private constant [9 x i8] c\"forty-two\""),
            "{text}"
        );
        assert!(text.contains("{ ptr, i64 }"), "{text}");
        assert!(text.contains("switch i32"), "{text}");
        assert!(
            text.contains("store volatile { i32, i64, { ptr, i64 } }"),
            "a managed union payload is rooted across its consuming call:\n{text}"
        );
    }

    #[test]
    fn lowers_cleanup_blocks_with_saved_result_parameters() {
        let core = concrete(
            "defmodule Main do\n  def cleanup(value: i32) -> unit do\n    unit\n  end\n  def compute(flag: bool) -> i32 do\n    defer cleanup(1)\n    if flag do\n      defer cleanup(2)\n      return 42\n    else\n      41\n    end\n  end\n  def main() -> i32 do\n    compute(true)\n  end\nend\n",
        );
        let compute = core
            .functions
            .iter()
            .find(|function| function.name == "compute")
            .expect("compute specialization remains reachable");
        let cleanup = core
            .functions
            .iter()
            .find(|function| function.name == "cleanup")
            .expect("cleanup specialization remains reachable")
            .id;
        let cleanup_blocks = compute
            .blocks
            .iter()
            .filter(|block| {
                !block.parameters.is_empty()
                    && block.operations.iter().any(
                        |operation| matches!(operation, Operation::Call { function, .. } if *function == cleanup),
                    )
            })
            .collect::<Vec<_>>();
        assert_eq!(cleanup_blocks.len(), 3, "all cleanup paths remain explicit");

        let llvm = lower_to_llvm_ir(&core).expect("cleanup CFG lowers and verifies");
        let text = llvm.as_str();
        for block in cleanup_blocks {
            let parameter = block.parameters[0].value;
            assert!(
                text.contains(&format!("b{}:", block.id.0))
                    && text.contains(&format!("%v{} = phi i32", parameter.0)),
                "cleanup block {:?} must receive its saved result through an LLVM phi:\n{text}",
                block.id,
            );
        }
    }

    #[test]
    fn spills_live_managed_values_and_touches_live_slots_at_calls() {
        let core = concrete(
            "defmodule Main do\n  def noop() -> unit do\n    unit\n  end\n  def consume(value: string) -> i32 do\n    42\n  end\n  def from_slot(value: string) -> i32 do\n    mut held: string = value\n    noop()\n    consume(held)\n  end\n  def through_cleanup(value: string) -> string do\n    defer noop()\n    value\n  end\n  def main() -> i32 do\n    from_slot(through_cleanup(\"root\"))\n  end\nend\n",
        );

        let llvm = lower_to_llvm_ir(&core).expect("managed roots lower and verify");
        let text = llvm.as_str();

        assert!(
            text.contains("gc.root.v"),
            "dedicated root slots are emitted:\n{text}"
        );
        assert!(
            text.contains("store volatile { ptr, i64 }"),
            "managed values are materialized as aligned base-pointer aggregates:\n{text}"
        );
        assert!(
            text.contains("load volatile { ptr, i64 }, ptr %q0"),
            "a managed local live across a call remains in scanned stack storage:\n{text}"
        );
        assert!(
            text.contains("zeroinitializer, ptr %gc.root.v"),
            "temporary roots are cleared after collection points:\n{text}"
        );
    }

    #[test]
    fn rejects_malformed_cleanup_cfg_before_llvm_generation() {
        let mut core = concrete(
            "defmodule Main do\n  def cleanup() -> unit do\n    unit\n  end\n  def main() -> i32 do\n    defer cleanup()\n    42\n  end\nend\n",
        );
        let main = core
            .functions
            .iter_mut()
            .find(|function| function.name == "main")
            .expect("main specialization");
        let cleanup = main
            .blocks
            .iter()
            .find(|block| !block.parameters.is_empty())
            .expect("cleanup block")
            .id;
        let Terminator::Branch { arguments, .. } = main
            .blocks
            .iter_mut()
            .find(|block| {
                matches!(block.terminator, Terminator::Branch { target, .. } if target == cleanup)
            })
            .map(|block| &mut block.terminator)
            .expect("incoming cleanup edge")
        else {
            panic!("incoming cleanup edge must be a branch")
        };
        arguments.clear();

        assert!(matches!(
            lower_to_llvm_ir(&core),
            Err(BackendError::InvalidConcrete(errors))
                if errors.iter().any(|error| error.contains("supplies 0 arguments, expected 1"))
        ));
    }

    #[cfg(not(feature = "managed-runtime"))]
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

    #[cfg(feature = "managed-runtime")]
    #[test]
    fn lowers_scanned_list_nodes_and_structural_patterns() {
        let core = concrete(
            "defmodule Main do\n  def first(values: [i32]) -> i32 do\n    match values do\n      [head | _] -> head\n      [] -> 0\n    end\n  end\n  def main() -> i32 do\n    first([42])\n  end\nend\n",
        );

        let llvm = lower_to_llvm_ir(&core).expect("managed lists lower and verify");
        let text = llvm.as_str();
        assert!(
            text.contains("call ptr @__el_runtime_alloc_scanned"),
            "{text}"
        );
        assert!(text.contains("store i32 42"), "{text}");
        assert!(text.contains("icmp eq ptr"), "{text}");
        assert!(text.contains("load i32"), "{text}");
        assert!(
            text.contains("gc.partial") && text.contains("store volatile ptr"),
            "partially constructed list spines must remain rooted:\n{text}"
        );
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

    #[cfg(feature = "llvm")]
    #[test]
    fn optimized_object_emission_accepts_volatile_managed_roots() {
        let core = concrete(
            "defmodule Main do\n  def consume(value: string) -> i32 do\n    42\n  end\n  def main() -> i32 do\n    consume(\"root\")\n  end\nend\n",
        );
        let output = std::env::temp_dir().join(format!(
            "el-codegen-managed-root-release-test-{}.o",
            std::process::id()
        ));

        emit_host_object_with_profile(&core, &output, CodegenProfile::Release)
            .expect("O3 preserves and verifies volatile managed roots");
        assert!(std::fs::metadata(&output).is_ok());
        std::fs::remove_file(output).expect("remove emitted object");
    }
}
