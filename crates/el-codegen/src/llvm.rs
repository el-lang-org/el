//! The private Inkwell boundary. No Inkwell or LLVM type escapes this module.

use crate::integer_checks::FailureOrigin;
use crate::{CodegenProfile, InvalidTargetMetadata, TargetMetadata};
use el_ir::{
    ArithmeticOperator, Block, BlockId, ComparisonOperator, ConcreteModule, Constant,
    CoreFailureCategory, CoreFunction, FunctionId, Operation, SlotId, SwitchValue, Terminator,
    Type, TypeId, ValueId, collection_point_roots, verify_concrete,
};
#[cfg(feature = "managed-runtime")]
use el_runtime::{
    ALLOCATE_ATOMIC_SYMBOL, ALLOCATE_SCANNED_SYMBOL, HASH_SEED_SYMBOL, INITIALIZE_SYMBOL,
};
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
use std::num::NonZeroU32;
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
    memcmp: FunctionValue<'ctx>,
    #[cfg(feature = "managed-runtime")]
    initialize_runtime: FunctionValue<'ctx>,
    #[cfg(feature = "managed-runtime")]
    allocate_scanned: FunctionValue<'ctx>,
    #[cfg(feature = "managed-runtime")]
    allocate_atomic: FunctionValue<'ctx>,
    #[cfg(feature = "managed-runtime")]
    hash_seed: FunctionValue<'ctx>,
}

impl<'ctx, 'core> ModuleLowerer<'ctx, 'core> {
    fn usize_type(&self) -> Result<inkwell::types::IntType<'ctx>, BackendError> {
        let bits = NonZeroU32::new(usize::BITS).expect("Rust pointer width is nonzero");
        self.context
            .custom_width_int_type(bits)
            .map_err(|error| BackendError::Builder(error.to_owned()))
    }

    fn element_pointer(
        &self,
        builder: &Builder<'ctx>,
        element: BasicTypeEnum<'ctx>,
        base: PointerValue<'ctx>,
        index: IntValue<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, BackendError> {
        let usize_ty = self.usize_type()?;
        let address = built(builder.build_ptr_to_int(base, usize_ty, &format!("{name}.address")))?;
        let size = element.size_of().ok_or_else(|| {
            BackendError::Builder("slice element has no statically known size".to_owned())
        })?;
        let size = if size.get_type() == usize_ty {
            size
        } else {
            built(builder.build_int_cast(size, usize_ty, &format!("{name}.size")))?
        };
        let offset = built(builder.build_int_mul(size, index, &format!("{name}.offset")))?;
        let address = built(builder.build_int_add(address, offset, &format!("{name}.indexed")))?;
        built(builder.build_int_to_ptr(
            address,
            self.context.ptr_type(AddressSpace::default()),
            name,
        ))
    }

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
        let memcmp = module.add_function(
            "memcmp",
            context.i32_type().fn_type(
                &[
                    context.ptr_type(AddressSpace::default()).into(),
                    context.ptr_type(AddressSpace::default()).into(),
                    context
                        .custom_width_int_type(NonZeroU32::new(usize::BITS).unwrap())
                        .expect("host usize type")
                        .into(),
                ],
                false,
            ),
            None,
        );
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
        #[cfg(feature = "managed-runtime")]
        let allocate_atomic = module.add_function(
            ALLOCATE_ATOMIC_SYMBOL,
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
        #[cfg(feature = "managed-runtime")]
        let hash_seed = module.add_function(
            HASH_SEED_SYMBOL,
            context
                .custom_width_int_type(NonZeroU32::new(usize::BITS).unwrap())
                .expect("host usize type")
                .fn_type(&[], false),
            None,
        );
        Self {
            context,
            core,
            module,
            functions: BTreeMap::new(),
            failure,
            #[cfg(feature = "managed-runtime")]
            memcmp,
            #[cfg(feature = "managed-runtime")]
            initialize_runtime,
            #[cfg(feature = "managed-runtime")]
            allocate_scanned,
            #[cfg(feature = "managed-runtime")]
            allocate_atomic,
            #[cfg(feature = "managed-runtime")]
            hash_seed,
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
            Some(Type::U8) => Ok(self.context.i8_type().into()),
            Some(Type::I32) => Ok(self.context.i32_type().into()),
            Some(Type::I64) => Ok(self.context.i64_type().into()),
            Some(Type::Usize) => Ok(self.usize_type()?.into()),
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
            Some(Type::Bytes) => Ok(self
                .context
                .struct_type(
                    &[
                        self.context.ptr_type(AddressSpace::default()).into(),
                        self.context.ptr_type(AddressSpace::default()).into(),
                        self.usize_type()?.into(),
                    ],
                    false,
                )
                .into()),
            Some(Type::List(_)) => Ok(self.context.ptr_type(AddressSpace::default()).into()),
            Some(Type::Array { item, length }) => {
                let item = self.basic_type(*item)?;
                let fields = (0..*length).map(|_| item).collect::<Vec<_>>();
                Ok(self.context.struct_type(&fields, false).into())
            }
            Some(Type::Slice(_)) => Ok(self
                .context
                .struct_type(
                    &[
                        self.context.ptr_type(AddressSpace::default()).into(),
                        self.context.ptr_type(AddressSpace::default()).into(),
                        self.usize_type()?.into(),
                    ],
                    false,
                )
                .into()),
            Some(Type::Map { .. }) => Ok(self.context.ptr_type(AddressSpace::default()).into()),
            Some(Type::Atom(_)) => Ok(self.context.i8_type().into()),
            Some(Type::Tuple(elements)) => {
                let fields = elements
                    .iter()
                    .map(|element| self.basic_type(*element))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self.context.struct_type(&fields, false).into())
            }
            Some(Type::Struct { declaration, .. }) => {
                let structure = self
                    .core
                    .structs
                    .iter()
                    .find(|structure| structure.declaration == *declaration)
                    .ok_or(BackendError::UnsupportedType(ty))?;
                let fields = structure
                    .fields
                    .iter()
                    .map(|(_, field)| self.basic_type(*field))
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

    fn map_node_type(&self, map: TypeId) -> Result<StructType<'ctx>, BackendError> {
        let Some(Type::Map { key, value }) = self.core.types.get(map.0 as usize) else {
            return Err(BackendError::UnsupportedType(map));
        };
        Ok(self.context.struct_type(
            &[
                self.usize_type()?.into(),
                self.basic_type(*key)?,
                self.basic_type(*value)?,
                self.context.ptr_type(AddressSpace::default()).into(),
            ],
            false,
        ))
    }

    #[cfg(feature = "managed-runtime")]
    fn map_key_equal(
        &self,
        builder: &Builder<'ctx>,
        left: BasicValueEnum<'ctx>,
        right: BasicValueEnum<'ctx>,
        ty: TypeId,
        name: &str,
    ) -> Result<IntValue<'ctx>, BackendError> {
        match self.core.types.get(ty.0 as usize) {
            Some(Type::I32 | Type::I64 | Type::Usize | Type::U8 | Type::Bool | Type::Atom(_)) => {
                built(builder.build_int_compare(
                    IntPredicate::EQ,
                    left.into_int_value(),
                    right.into_int_value(),
                    name,
                ))
            }
            Some(Type::Unit) => Ok(self.context.bool_type().const_all_ones()),
            Some(Type::Tuple(elements)) => {
                let left = left.into_struct_value();
                let right = right.into_struct_value();
                let mut equal = self.context.bool_type().const_all_ones();
                for (index, element) in elements.iter().enumerate() {
                    let left_element = built(builder.build_extract_value(
                        left,
                        index as u32,
                        &format!("{name}.left{index}"),
                    ))?;
                    let right_element = built(builder.build_extract_value(
                        right,
                        index as u32,
                        &format!("{name}.right{index}"),
                    ))?;
                    let component = self.map_key_equal(
                        builder,
                        left_element,
                        right_element,
                        *element,
                        &format!("{name}.element{index}"),
                    )?;
                    equal =
                        built(builder.build_and(equal, component, &format!("{name}.and{index}")))?;
                }
                Ok(equal)
            }
            Some(Type::Array { item, length }) => {
                let left = left.into_struct_value();
                let right = right.into_struct_value();
                let mut equal = self.context.bool_type().const_all_ones();
                for index in 0..*length {
                    let left_element = built(builder.build_extract_value(
                        left,
                        index as u32,
                        &format!("{name}.left{index}"),
                    ))?;
                    let right_element = built(builder.build_extract_value(
                        right,
                        index as u32,
                        &format!("{name}.right{index}"),
                    ))?;
                    let component = self.map_key_equal(
                        builder,
                        left_element,
                        right_element,
                        *item,
                        &format!("{name}.element{index}"),
                    )?;
                    equal =
                        built(builder.build_and(equal, component, &format!("{name}.and{index}")))?;
                }
                Ok(equal)
            }
            Some(Type::String) => {
                let left = left.into_struct_value();
                let right = right.into_struct_value();
                let left_data =
                    built(builder.build_extract_value(left, 0, &format!("{name}.left_data")))?
                        .into_pointer_value();
                let right_data =
                    built(builder.build_extract_value(right, 0, &format!("{name}.right_data")))?
                        .into_pointer_value();
                let left_length =
                    built(builder.build_extract_value(left, 1, &format!("{name}.left_length")))?
                        .into_int_value();
                let right_length =
                    built(builder.build_extract_value(right, 1, &format!("{name}.right_length")))?
                        .into_int_value();
                self.byte_sequence_equal(
                    builder,
                    left_data,
                    left_length,
                    right_data,
                    right_length,
                    name,
                )
            }
            Some(Type::Bytes) => {
                let left = left.into_struct_value();
                let right = right.into_struct_value();
                let left_data =
                    built(builder.build_extract_value(left, 1, &format!("{name}.left_data")))?
                        .into_pointer_value();
                let right_data =
                    built(builder.build_extract_value(right, 1, &format!("{name}.right_data")))?
                        .into_pointer_value();
                let left_length =
                    built(builder.build_extract_value(left, 2, &format!("{name}.left_length")))?
                        .into_int_value();
                let right_length =
                    built(builder.build_extract_value(right, 2, &format!("{name}.right_length")))?
                        .into_int_value();
                self.byte_sequence_equal(
                    builder,
                    left_data,
                    left_length,
                    right_data,
                    right_length,
                    name,
                )
            }
            Some(Type::Slice(item)) => {
                let left = left.into_struct_value();
                let right = right.into_struct_value();
                let left_data =
                    built(builder.build_extract_value(left, 1, &format!("{name}.left_data")))?
                        .into_pointer_value();
                let right_data =
                    built(builder.build_extract_value(right, 1, &format!("{name}.right_data")))?
                        .into_pointer_value();
                let left_length =
                    built(builder.build_extract_value(left, 2, &format!("{name}.left_length")))?
                        .into_int_value();
                let right_length =
                    built(builder.build_extract_value(right, 2, &format!("{name}.right_length")))?
                        .into_int_value();
                self.sequence_equal(
                    builder,
                    left_data,
                    left_length,
                    right_data,
                    right_length,
                    *item,
                    name,
                )
            }
            Some(Type::List(item)) => self.list_equal(
                builder,
                left.into_pointer_value(),
                right.into_pointer_value(),
                *item,
                name,
            ),
            Some(Type::Map { .. }) => self.map_equal(
                builder,
                left.into_pointer_value(),
                right.into_pointer_value(),
                ty,
                name,
            ),
            _ => Err(BackendError::UnsupportedType(ty)),
        }
    }

    #[cfg(feature = "managed-runtime")]
    fn byte_sequence_equal(
        &self,
        builder: &Builder<'ctx>,
        left: PointerValue<'ctx>,
        left_length: IntValue<'ctx>,
        right: PointerValue<'ctx>,
        right_length: IntValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>, BackendError> {
        let length_equal = built(builder.build_int_compare(
            IntPredicate::EQ,
            left_length,
            right_length,
            &format!("{name}.length_equal"),
        ))?;
        let zero = left_length.get_type().const_zero();
        let safe_length = built(builder.build_select(
            length_equal,
            left_length,
            zero,
            &format!("{name}.safe_length"),
        ))?
        .into_int_value();
        let compared = built(builder.build_call(
            self.memcmp,
            &[left.into(), right.into(), safe_length.into()],
            &format!("{name}.memcmp"),
        ))?
        .try_as_basic_value()
        .basic()
        .ok_or_else(|| BackendError::Builder("memcmp returned void".to_owned()))?
        .into_int_value();
        let contents_equal = built(builder.build_int_compare(
            IntPredicate::EQ,
            compared,
            self.context.i32_type().const_zero(),
            &format!("{name}.contents_equal"),
        ))?;
        built(builder.build_and(length_equal, contents_equal, name))
    }

    #[cfg(feature = "managed-runtime")]
    #[allow(clippy::too_many_arguments)]
    fn sequence_equal(
        &self,
        builder: &Builder<'ctx>,
        left: PointerValue<'ctx>,
        left_length: IntValue<'ctx>,
        right: PointerValue<'ctx>,
        right_length: IntValue<'ctx>,
        item: TypeId,
        name: &str,
    ) -> Result<IntValue<'ctx>, BackendError> {
        let function = builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or_else(|| BackendError::Builder("builder has no function".to_owned()))?;
        let result_slot =
            built(builder.build_alloca(self.context.bool_type(), &format!("{name}.result")))?;
        built(builder.build_store(result_slot, self.context.bool_type().const_all_ones()))?;
        let lengths_equal = built(builder.build_int_compare(
            IntPredicate::EQ,
            left_length,
            right_length,
            &format!("{name}.lengths_equal"),
        ))?;
        let preheader = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        let loop_block = self
            .context
            .append_basic_block(function, &format!("{name}.loop"));
        let body = self
            .context
            .append_basic_block(function, &format!("{name}.body"));
        let advance = self
            .context
            .append_basic_block(function, &format!("{name}.advance"));
        let mismatch = self
            .context
            .append_basic_block(function, &format!("{name}.mismatch"));
        let done = self
            .context
            .append_basic_block(function, &format!("{name}.done"));
        built(builder.build_conditional_branch(lengths_equal, loop_block, mismatch))?;
        builder.position_at_end(loop_block);
        let index = built(builder.build_phi(left_length.get_type(), &format!("{name}.index")))?;
        let zero = left_length.get_type().const_zero();
        index.add_incoming(&[(&zero, preheader)]);
        let exhausted = built(builder.build_int_compare(
            IntPredicate::EQ,
            index.as_basic_value().into_int_value(),
            left_length,
            &format!("{name}.exhausted"),
        ))?;
        built(builder.build_conditional_branch(exhausted, done, body))?;
        builder.position_at_end(body);
        let element_ty = self.basic_type(item)?;
        let left_ptr = self.element_pointer(
            builder,
            element_ty,
            left,
            index.as_basic_value().into_int_value(),
            &format!("{name}.left_ptr"),
        )?;
        let right_ptr = self.element_pointer(
            builder,
            element_ty,
            right,
            index.as_basic_value().into_int_value(),
            &format!("{name}.right_ptr"),
        )?;
        let left_item =
            built(builder.build_load(element_ty, left_ptr, &format!("{name}.left_item")))?;
        let right_item =
            built(builder.build_load(element_ty, right_ptr, &format!("{name}.right_item")))?;
        let equal = self.map_key_equal(
            builder,
            left_item,
            right_item,
            item,
            &format!("{name}.item_equal"),
        )?;
        built(builder.build_conditional_branch(equal, advance, mismatch))?;
        builder.position_at_end(advance);
        let next = built(builder.build_int_add(
            index.as_basic_value().into_int_value(),
            left_length.get_type().const_int(1, false),
            &format!("{name}.next"),
        ))?;
        let advance_end = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        built(builder.build_unconditional_branch(loop_block))?;
        index.add_incoming(&[(&next, advance_end)]);
        builder.position_at_end(mismatch);
        built(builder.build_store(result_slot, self.context.bool_type().const_zero()))?;
        built(builder.build_unconditional_branch(done))?;
        builder.position_at_end(done);
        Ok(
            built(builder.build_load(self.context.bool_type(), result_slot, name))?
                .into_int_value(),
        )
    }

    #[cfg(feature = "managed-runtime")]
    fn list_equal(
        &self,
        builder: &Builder<'ctx>,
        left: PointerValue<'ctx>,
        right: PointerValue<'ctx>,
        item: TypeId,
        name: &str,
    ) -> Result<IntValue<'ctx>, BackendError> {
        let function = builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or_else(|| BackendError::Builder("builder has no function".to_owned()))?;
        let pointer_ty = self.context.ptr_type(AddressSpace::default());
        let result_slot =
            built(builder.build_alloca(self.context.bool_type(), &format!("{name}.result")))?;
        built(builder.build_store(result_slot, self.context.bool_type().const_all_ones()))?;
        let preheader = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        let loop_block = self
            .context
            .append_basic_block(function, &format!("{name}.loop"));
        let inspect = self
            .context
            .append_basic_block(function, &format!("{name}.inspect"));
        let advance = self
            .context
            .append_basic_block(function, &format!("{name}.advance"));
        let mismatch = self
            .context
            .append_basic_block(function, &format!("{name}.mismatch"));
        let done = self
            .context
            .append_basic_block(function, &format!("{name}.done"));
        built(builder.build_unconditional_branch(loop_block))?;
        builder.position_at_end(loop_block);
        let left_cursor = built(builder.build_phi(pointer_ty, &format!("{name}.left")))?;
        let right_cursor = built(builder.build_phi(pointer_ty, &format!("{name}.right")))?;
        left_cursor.add_incoming(&[(&left, preheader)]);
        right_cursor.add_incoming(&[(&right, preheader)]);
        let left_null = built(builder.build_is_null(
            left_cursor.as_basic_value().into_pointer_value(),
            &format!("{name}.left_null"),
        ))?;
        let right_null = built(builder.build_is_null(
            right_cursor.as_basic_value().into_pointer_value(),
            &format!("{name}.right_null"),
        ))?;
        let both_null =
            built(builder.build_and(left_null, right_null, &format!("{name}.both_null")))?;
        let either_null =
            built(builder.build_or(left_null, right_null, &format!("{name}.either_null")))?;
        let check_mismatch = self
            .context
            .append_basic_block(function, &format!("{name}.check_mismatch"));
        built(builder.build_conditional_branch(both_null, done, check_mismatch))?;
        builder.position_at_end(check_mismatch);
        built(builder.build_conditional_branch(either_null, mismatch, inspect))?;
        builder.position_at_end(inspect);
        let node_ty = self.list_node_type(TypeId(
            self.core
                .types
                .iter()
                .position(|ty| ty == &Type::List(item))
                .ok_or(BackendError::UnsupportedType(item))? as u32,
        ))?;
        let left_node = left_cursor.as_basic_value().into_pointer_value();
        let right_node = right_cursor.as_basic_value().into_pointer_value();
        let left_item_ptr = built(builder.build_struct_gep(
            node_ty,
            left_node,
            0,
            &format!("{name}.left_item_ptr"),
        ))?;
        let right_item_ptr = built(builder.build_struct_gep(
            node_ty,
            right_node,
            0,
            &format!("{name}.right_item_ptr"),
        ))?;
        let left_item = built(builder.build_load(
            self.basic_type(item)?,
            left_item_ptr,
            &format!("{name}.left_item"),
        ))?;
        let right_item = built(builder.build_load(
            self.basic_type(item)?,
            right_item_ptr,
            &format!("{name}.right_item"),
        ))?;
        let equal = self.map_key_equal(
            builder,
            left_item,
            right_item,
            item,
            &format!("{name}.item_equal"),
        )?;
        built(builder.build_conditional_branch(equal, advance, mismatch))?;
        builder.position_at_end(advance);
        let left_next_ptr = built(builder.build_struct_gep(
            node_ty,
            left_node,
            1,
            &format!("{name}.left_next_ptr"),
        ))?;
        let right_next_ptr = built(builder.build_struct_gep(
            node_ty,
            right_node,
            1,
            &format!("{name}.right_next_ptr"),
        ))?;
        let left_next =
            built(builder.build_load(pointer_ty, left_next_ptr, &format!("{name}.left_next")))?
                .into_pointer_value();
        let right_next =
            built(builder.build_load(pointer_ty, right_next_ptr, &format!("{name}.right_next")))?
                .into_pointer_value();
        let advance_end = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        built(builder.build_unconditional_branch(loop_block))?;
        left_cursor.add_incoming(&[(&left_next, advance_end)]);
        right_cursor.add_incoming(&[(&right_next, advance_end)]);
        builder.position_at_end(mismatch);
        built(builder.build_store(result_slot, self.context.bool_type().const_zero()))?;
        built(builder.build_unconditional_branch(done))?;
        builder.position_at_end(done);
        Ok(
            built(builder.build_load(self.context.bool_type(), result_slot, name))?
                .into_int_value(),
        )
    }

    #[cfg(feature = "managed-runtime")]
    fn map_length(
        &self,
        builder: &Builder<'ctx>,
        map: PointerValue<'ctx>,
        ty: TypeId,
        name: &str,
    ) -> Result<IntValue<'ctx>, BackendError> {
        let function = builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or_else(|| BackendError::Builder("builder has no function".to_owned()))?;
        let preheader = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        let pointer_ty = self.context.ptr_type(AddressSpace::default());
        let usize_ty = self.usize_type()?;
        let node_ty = self.map_node_type(ty)?;
        let loop_block = self
            .context
            .append_basic_block(function, &format!("{name}.loop"));
        let body = self
            .context
            .append_basic_block(function, &format!("{name}.body"));
        let done = self
            .context
            .append_basic_block(function, &format!("{name}.done"));
        built(builder.build_unconditional_branch(loop_block))?;
        builder.position_at_end(loop_block);
        let cursor = built(builder.build_phi(pointer_ty, &format!("{name}.cursor")))?;
        let count = built(builder.build_phi(usize_ty, &format!("{name}.count")))?;
        cursor.add_incoming(&[(&map, preheader)]);
        count.add_incoming(&[(&usize_ty.const_zero(), preheader)]);
        let exhausted = built(builder.build_is_null(
            cursor.as_basic_value().into_pointer_value(),
            &format!("{name}.empty"),
        ))?;
        built(builder.build_conditional_branch(exhausted, done, body))?;
        builder.position_at_end(body);
        let next_ptr = built(builder.build_struct_gep(
            node_ty,
            cursor.as_basic_value().into_pointer_value(),
            3,
            &format!("{name}.next_ptr"),
        ))?;
        let next = built(builder.build_load(pointer_ty, next_ptr, &format!("{name}.next")))?
            .into_pointer_value();
        let next_count = built(builder.build_int_add(
            count.as_basic_value().into_int_value(),
            usize_ty.const_int(1, false),
            &format!("{name}.next_count"),
        ))?;
        let body_end = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        built(builder.build_unconditional_branch(loop_block))?;
        cursor.add_incoming(&[(&next, body_end)]);
        count.add_incoming(&[(&next_count, body_end)]);
        builder.position_at_end(done);
        Ok(count.as_basic_value().into_int_value())
    }

    #[cfg(feature = "managed-runtime")]
    fn map_equal(
        &self,
        builder: &Builder<'ctx>,
        left: PointerValue<'ctx>,
        right: PointerValue<'ctx>,
        ty: TypeId,
        name: &str,
    ) -> Result<IntValue<'ctx>, BackendError> {
        let Some(Type::Map {
            key: key_ty,
            value: value_ty,
        }) = self.core.types.get(ty.0 as usize)
        else {
            return Err(BackendError::UnsupportedType(ty));
        };
        let (key_ty, value_ty) = (*key_ty, *value_ty);
        let right_length = self.map_length(builder, right, ty, &format!("{name}.right_length"))?;
        let function = builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or_else(|| BackendError::Builder("builder has no function".to_owned()))?;
        let preheader = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        let pointer_ty = self.context.ptr_type(AddressSpace::default());
        let usize_ty = self.usize_type()?;
        let node_ty = self.map_node_type(ty)?;
        let result_slot =
            built(builder.build_alloca(self.context.bool_type(), &format!("{name}.result")))?;
        built(builder.build_store(result_slot, self.context.bool_type().const_all_ones()))?;
        let left_loop = self
            .context
            .append_basic_block(function, &format!("{name}.left_loop"));
        let left_body = self
            .context
            .append_basic_block(function, &format!("{name}.left_body"));
        let right_search = self
            .context
            .append_basic_block(function, &format!("{name}.right_search"));
        let right_inspect = self
            .context
            .append_basic_block(function, &format!("{name}.right_inspect"));
        let right_advance = self
            .context
            .append_basic_block(function, &format!("{name}.right_advance"));
        let value_check = self
            .context
            .append_basic_block(function, &format!("{name}.value_check"));
        let left_advance = self
            .context
            .append_basic_block(function, &format!("{name}.left_advance"));
        let finish_check = self
            .context
            .append_basic_block(function, &format!("{name}.finish_check"));
        let mismatch = self
            .context
            .append_basic_block(function, &format!("{name}.mismatch"));
        let done = self
            .context
            .append_basic_block(function, &format!("{name}.done"));
        built(builder.build_unconditional_branch(left_loop))?;
        builder.position_at_end(left_loop);
        let left_cursor = built(builder.build_phi(pointer_ty, &format!("{name}.left_cursor")))?;
        let matched = built(builder.build_phi(usize_ty, &format!("{name}.matched")))?;
        left_cursor.add_incoming(&[(&left, preheader)]);
        matched.add_incoming(&[(&usize_ty.const_zero(), preheader)]);
        let left_exhausted = built(builder.build_is_null(
            left_cursor.as_basic_value().into_pointer_value(),
            &format!("{name}.left_empty"),
        ))?;
        built(builder.build_conditional_branch(left_exhausted, finish_check, left_body))?;

        builder.position_at_end(left_body);
        let left_node = left_cursor.as_basic_value().into_pointer_value();
        let left_hash_ptr = built(builder.build_struct_gep(
            node_ty,
            left_node,
            0,
            &format!("{name}.left_hash_ptr"),
        ))?;
        let left_key_ptr = built(builder.build_struct_gep(
            node_ty,
            left_node,
            1,
            &format!("{name}.left_key_ptr"),
        ))?;
        let left_value_ptr = built(builder.build_struct_gep(
            node_ty,
            left_node,
            2,
            &format!("{name}.left_value_ptr"),
        ))?;
        let left_next_ptr = built(builder.build_struct_gep(
            node_ty,
            left_node,
            3,
            &format!("{name}.left_next_ptr"),
        ))?;
        let left_hash =
            built(builder.build_load(usize_ty, left_hash_ptr, &format!("{name}.left_hash")))?
                .into_int_value();
        let left_key = built(builder.build_load(
            self.basic_type(key_ty)?,
            left_key_ptr,
            &format!("{name}.left_key"),
        ))?;
        let left_value = built(builder.build_load(
            self.basic_type(value_ty)?,
            left_value_ptr,
            &format!("{name}.left_value"),
        ))?;
        let left_next =
            built(builder.build_load(pointer_ty, left_next_ptr, &format!("{name}.left_next")))?
                .into_pointer_value();
        let left_body_end = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        built(builder.build_unconditional_branch(right_search))?;

        builder.position_at_end(right_search);
        let right_cursor = built(builder.build_phi(pointer_ty, &format!("{name}.right_cursor")))?;
        right_cursor.add_incoming(&[(&right, left_body_end)]);
        let right_exhausted = built(builder.build_is_null(
            right_cursor.as_basic_value().into_pointer_value(),
            &format!("{name}.right_empty"),
        ))?;
        built(builder.build_conditional_branch(right_exhausted, mismatch, right_inspect))?;
        builder.position_at_end(right_inspect);
        let right_node = right_cursor.as_basic_value().into_pointer_value();
        let right_hash_ptr = built(builder.build_struct_gep(
            node_ty,
            right_node,
            0,
            &format!("{name}.right_hash_ptr"),
        ))?;
        let right_key_ptr = built(builder.build_struct_gep(
            node_ty,
            right_node,
            1,
            &format!("{name}.right_key_ptr"),
        ))?;
        let right_hash =
            built(builder.build_load(usize_ty, right_hash_ptr, &format!("{name}.right_hash")))?
                .into_int_value();
        let right_key = built(builder.build_load(
            self.basic_type(key_ty)?,
            right_key_ptr,
            &format!("{name}.right_key"),
        ))?;
        let same_hash = built(builder.build_int_compare(
            IntPredicate::EQ,
            left_hash,
            right_hash,
            &format!("{name}.same_hash"),
        ))?;
        let same_key = self.map_key_equal(
            builder,
            left_key,
            right_key,
            key_ty,
            &format!("{name}.same_key"),
        )?;
        let found_key =
            built(builder.build_and(same_hash, same_key, &format!("{name}.found_key")))?;
        built(builder.build_conditional_branch(found_key, value_check, right_advance))?;
        builder.position_at_end(right_advance);
        let right_next_ptr = built(builder.build_struct_gep(
            node_ty,
            right_node,
            3,
            &format!("{name}.right_next_ptr"),
        ))?;
        let right_next =
            built(builder.build_load(pointer_ty, right_next_ptr, &format!("{name}.right_next")))?
                .into_pointer_value();
        let right_advance_end = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        built(builder.build_unconditional_branch(right_search))?;
        right_cursor.add_incoming(&[(&right_next, right_advance_end)]);
        builder.position_at_end(value_check);
        let right_value_ptr = built(builder.build_struct_gep(
            node_ty,
            right_node,
            2,
            &format!("{name}.right_value_ptr"),
        ))?;
        let right_value = built(builder.build_load(
            self.basic_type(value_ty)?,
            right_value_ptr,
            &format!("{name}.right_value"),
        ))?;
        let values_equal = self.map_key_equal(
            builder,
            left_value,
            right_value,
            value_ty,
            &format!("{name}.value_equal"),
        )?;
        built(builder.build_conditional_branch(values_equal, left_advance, mismatch))?;
        builder.position_at_end(left_advance);
        let next_matched = built(builder.build_int_add(
            matched.as_basic_value().into_int_value(),
            usize_ty.const_int(1, false),
            &format!("{name}.next_matched"),
        ))?;
        let left_advance_end = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        built(builder.build_unconditional_branch(left_loop))?;
        left_cursor.add_incoming(&[(&left_next, left_advance_end)]);
        matched.add_incoming(&[(&next_matched, left_advance_end)]);
        builder.position_at_end(finish_check);
        let same_length = built(builder.build_int_compare(
            IntPredicate::EQ,
            matched.as_basic_value().into_int_value(),
            right_length,
            &format!("{name}.same_length"),
        ))?;
        built(builder.build_conditional_branch(same_length, done, mismatch))?;
        builder.position_at_end(mismatch);
        built(builder.build_store(result_slot, self.context.bool_type().const_zero()))?;
        built(builder.build_unconditional_branch(done))?;
        builder.position_at_end(done);
        Ok(
            built(builder.build_load(self.context.bool_type(), result_slot, name))?
                .into_int_value(),
        )
    }

    #[cfg(feature = "managed-runtime")]
    fn map_key_hash(
        &self,
        builder: &Builder<'ctx>,
        value: BasicValueEnum<'ctx>,
        ty: TypeId,
        seed: IntValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>, BackendError> {
        let usize_ty = self.usize_type()?;
        let mix =
            |builder: &Builder<'ctx>, state: IntValue<'ctx>, part: IntValue<'ctx>, suffix: &str| {
                let part = if part.get_type() == usize_ty {
                    Ok(part)
                } else {
                    built(builder.build_int_z_extend(
                        part,
                        usize_ty,
                        &format!("{name}.{suffix}.extend"),
                    ))
                }?;
                let xored = built(builder.build_xor(state, part, &format!("{name}.{suffix}.xor")))?;
                built(builder.build_int_mul(
                    xored,
                    usize_ty.const_int(0x9e37_79b1, false),
                    &format!("{name}.{suffix}.mix"),
                ))
            };
        match self.core.types.get(ty.0 as usize) {
            Some(Type::I32 | Type::I64 | Type::Usize | Type::U8 | Type::Bool | Type::Atom(_)) => {
                let integer = value.into_int_value();
                let integer = if integer.get_type().get_bit_width() > usize_ty.get_bit_width() {
                    built(builder.build_int_truncate(
                        integer,
                        usize_ty,
                        &format!("{name}.truncate"),
                    ))?
                } else if integer.get_type() != usize_ty {
                    built(builder.build_int_z_extend(integer, usize_ty, &format!("{name}.extend")))?
                } else {
                    integer
                };
                mix(builder, seed, integer, "scalar")
            }
            Some(Type::Unit) => mix(builder, seed, usize_ty.const_int(1, false), "unit"),
            Some(Type::Tuple(elements)) => {
                let aggregate = value.into_struct_value();
                let mut hash = seed;
                for (index, element) in elements.iter().enumerate() {
                    let field = built(builder.build_extract_value(
                        aggregate,
                        index as u32,
                        &format!("{name}.field{index}"),
                    ))?;
                    hash = self.map_key_hash(
                        builder,
                        field,
                        *element,
                        hash,
                        &format!("{name}.field{index}"),
                    )?;
                }
                Ok(hash)
            }
            Some(Type::Array { item, length }) => {
                let aggregate = value.into_struct_value();
                let mut hash = mix(builder, seed, usize_ty.const_int(*length, false), "length")?;
                for index in 0..*length {
                    let field = built(builder.build_extract_value(
                        aggregate,
                        index as u32,
                        &format!("{name}.item{index}"),
                    ))?;
                    hash = self.map_key_hash(
                        builder,
                        field,
                        *item,
                        hash,
                        &format!("{name}.item{index}"),
                    )?;
                }
                Ok(hash)
            }
            Some(Type::String) => {
                let value = value.into_struct_value();
                let data = built(builder.build_extract_value(value, 0, &format!("{name}.data")))?
                    .into_pointer_value();
                let length =
                    built(builder.build_extract_value(value, 1, &format!("{name}.length")))?
                        .into_int_value();
                self.byte_hash(builder, data, length, seed, name)
            }
            Some(Type::Bytes) => {
                let value = value.into_struct_value();
                let data = built(builder.build_extract_value(value, 1, &format!("{name}.data")))?
                    .into_pointer_value();
                let length =
                    built(builder.build_extract_value(value, 2, &format!("{name}.length")))?
                        .into_int_value();
                self.byte_hash(builder, data, length, seed, name)
            }
            Some(Type::Slice(item)) => {
                let value = value.into_struct_value();
                let data = built(builder.build_extract_value(value, 1, &format!("{name}.data")))?
                    .into_pointer_value();
                let length =
                    built(builder.build_extract_value(value, 2, &format!("{name}.length")))?
                        .into_int_value();
                self.sequence_hash(builder, data, length, *item, seed, name)
            }
            Some(Type::List(item)) => {
                self.list_hash(builder, value.into_pointer_value(), *item, seed, name)
            }
            _ => Err(BackendError::UnsupportedType(ty)),
        }
    }

    #[cfg(feature = "managed-runtime")]
    fn hash_mix(
        &self,
        builder: &Builder<'ctx>,
        state: IntValue<'ctx>,
        part: IntValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>, BackendError> {
        let usize_ty = self.usize_type()?;
        let part = if part.get_type().get_bit_width() > usize_ty.get_bit_width() {
            built(builder.build_int_truncate(part, usize_ty, &format!("{name}.truncate")))?
        } else if part.get_type() != usize_ty {
            built(builder.build_int_z_extend(part, usize_ty, &format!("{name}.extend")))?
        } else {
            part
        };
        let xored = built(builder.build_xor(state, part, &format!("{name}.xor")))?;
        built(builder.build_int_mul(
            xored,
            usize_ty.const_int(0x9e37_79b1, false),
            &format!("{name}.mix"),
        ))
    }

    #[cfg(feature = "managed-runtime")]
    fn byte_hash(
        &self,
        builder: &Builder<'ctx>,
        data: PointerValue<'ctx>,
        length: IntValue<'ctx>,
        seed: IntValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>, BackendError> {
        let function = builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or_else(|| BackendError::Builder("builder has no function".to_owned()))?;
        let preheader = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        let usize_ty = self.usize_type()?;
        let length = if length.get_type() == usize_ty {
            length
        } else {
            built(builder.build_int_cast(length, usize_ty, &format!("{name}.length_cast")))?
        };
        let initial = self.hash_mix(builder, seed, length, &format!("{name}.length"))?;
        let loop_block = self
            .context
            .append_basic_block(function, &format!("{name}.loop"));
        let body = self
            .context
            .append_basic_block(function, &format!("{name}.body"));
        let done = self
            .context
            .append_basic_block(function, &format!("{name}.done"));
        built(builder.build_unconditional_branch(loop_block))?;
        builder.position_at_end(loop_block);
        let index = built(builder.build_phi(usize_ty, &format!("{name}.index")))?;
        let hash = built(builder.build_phi(usize_ty, &format!("{name}.hash")))?;
        index.add_incoming(&[(&usize_ty.const_zero(), preheader)]);
        hash.add_incoming(&[(&initial, preheader)]);
        let exhausted = built(builder.build_int_compare(
            IntPredicate::EQ,
            index.as_basic_value().into_int_value(),
            length,
            &format!("{name}.exhausted"),
        ))?;
        built(builder.build_conditional_branch(exhausted, done, body))?;
        builder.position_at_end(body);
        let byte_ptr = self.element_pointer(
            builder,
            self.context.i8_type().into(),
            data,
            index.as_basic_value().into_int_value(),
            &format!("{name}.byte_ptr"),
        )?;
        let byte =
            built(builder.build_load(self.context.i8_type(), byte_ptr, &format!("{name}.byte")))?
                .into_int_value();
        let next_hash = self.hash_mix(
            builder,
            hash.as_basic_value().into_int_value(),
            byte,
            &format!("{name}.byte_hash"),
        )?;
        let next_index = built(builder.build_int_add(
            index.as_basic_value().into_int_value(),
            usize_ty.const_int(1, false),
            &format!("{name}.next_index"),
        ))?;
        let body_end = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        built(builder.build_unconditional_branch(loop_block))?;
        index.add_incoming(&[(&next_index, body_end)]);
        hash.add_incoming(&[(&next_hash, body_end)]);
        builder.position_at_end(done);
        Ok(hash.as_basic_value().into_int_value())
    }

    #[cfg(feature = "managed-runtime")]
    #[allow(clippy::too_many_arguments)]
    fn sequence_hash(
        &self,
        builder: &Builder<'ctx>,
        data: PointerValue<'ctx>,
        length: IntValue<'ctx>,
        item: TypeId,
        seed: IntValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>, BackendError> {
        let function = builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or_else(|| BackendError::Builder("builder has no function".to_owned()))?;
        let preheader = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        let usize_ty = self.usize_type()?;
        let initial = self.hash_mix(builder, seed, length, &format!("{name}.length"))?;
        let loop_block = self
            .context
            .append_basic_block(function, &format!("{name}.loop"));
        let body = self
            .context
            .append_basic_block(function, &format!("{name}.body"));
        let done = self
            .context
            .append_basic_block(function, &format!("{name}.done"));
        built(builder.build_unconditional_branch(loop_block))?;
        builder.position_at_end(loop_block);
        let index = built(builder.build_phi(usize_ty, &format!("{name}.index")))?;
        let hash = built(builder.build_phi(usize_ty, &format!("{name}.hash")))?;
        index.add_incoming(&[(&usize_ty.const_zero(), preheader)]);
        hash.add_incoming(&[(&initial, preheader)]);
        let exhausted = built(builder.build_int_compare(
            IntPredicate::EQ,
            index.as_basic_value().into_int_value(),
            length,
            &format!("{name}.exhausted"),
        ))?;
        built(builder.build_conditional_branch(exhausted, done, body))?;
        builder.position_at_end(body);
        let element_ty = self.basic_type(item)?;
        let item_ptr = self.element_pointer(
            builder,
            element_ty,
            data,
            index.as_basic_value().into_int_value(),
            &format!("{name}.item_ptr"),
        )?;
        let item_value = built(builder.build_load(element_ty, item_ptr, &format!("{name}.item")))?;
        let next_hash = self.map_key_hash(
            builder,
            item_value,
            item,
            hash.as_basic_value().into_int_value(),
            &format!("{name}.item_hash"),
        )?;
        let next_index = built(builder.build_int_add(
            index.as_basic_value().into_int_value(),
            usize_ty.const_int(1, false),
            &format!("{name}.next_index"),
        ))?;
        let body_end = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        built(builder.build_unconditional_branch(loop_block))?;
        index.add_incoming(&[(&next_index, body_end)]);
        hash.add_incoming(&[(&next_hash, body_end)]);
        builder.position_at_end(done);
        Ok(hash.as_basic_value().into_int_value())
    }

    #[cfg(feature = "managed-runtime")]
    fn list_hash(
        &self,
        builder: &Builder<'ctx>,
        list: PointerValue<'ctx>,
        item: TypeId,
        seed: IntValue<'ctx>,
        name: &str,
    ) -> Result<IntValue<'ctx>, BackendError> {
        let function = builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or_else(|| BackendError::Builder("builder has no function".to_owned()))?;
        let preheader = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        let pointer_ty = self.context.ptr_type(AddressSpace::default());
        let usize_ty = self.usize_type()?;
        let node_ty = self
            .context
            .struct_type(&[self.basic_type(item)?, pointer_ty.into()], false);
        let loop_block = self
            .context
            .append_basic_block(function, &format!("{name}.loop"));
        let body = self
            .context
            .append_basic_block(function, &format!("{name}.body"));
        let done = self
            .context
            .append_basic_block(function, &format!("{name}.done"));
        built(builder.build_unconditional_branch(loop_block))?;
        builder.position_at_end(loop_block);
        let cursor = built(builder.build_phi(pointer_ty, &format!("{name}.cursor")))?;
        let hash = built(builder.build_phi(usize_ty, &format!("{name}.hash")))?;
        let count = built(builder.build_phi(usize_ty, &format!("{name}.count")))?;
        cursor.add_incoming(&[(&list, preheader)]);
        hash.add_incoming(&[(&seed, preheader)]);
        count.add_incoming(&[(&usize_ty.const_zero(), preheader)]);
        let exhausted = built(builder.build_is_null(
            cursor.as_basic_value().into_pointer_value(),
            &format!("{name}.empty"),
        ))?;
        built(builder.build_conditional_branch(exhausted, done, body))?;
        builder.position_at_end(body);
        let node = cursor.as_basic_value().into_pointer_value();
        let item_ptr =
            built(builder.build_struct_gep(node_ty, node, 0, &format!("{name}.item_ptr")))?;
        let next_ptr =
            built(builder.build_struct_gep(node_ty, node, 1, &format!("{name}.next_ptr")))?;
        let item_value =
            built(builder.build_load(self.basic_type(item)?, item_ptr, &format!("{name}.item")))?;
        let next = built(builder.build_load(pointer_ty, next_ptr, &format!("{name}.next")))?
            .into_pointer_value();
        let next_hash = self.map_key_hash(
            builder,
            item_value,
            item,
            hash.as_basic_value().into_int_value(),
            &format!("{name}.item_hash"),
        )?;
        let next_count = built(builder.build_int_add(
            count.as_basic_value().into_int_value(),
            usize_ty.const_int(1, false),
            &format!("{name}.next_count"),
        ))?;
        let body_end = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        built(builder.build_unconditional_branch(loop_block))?;
        cursor.add_incoming(&[(&next, body_end)]);
        hash.add_incoming(&[(&next_hash, body_end)]);
        count.add_incoming(&[(&next_count, body_end)]);
        builder.position_at_end(done);
        self.hash_mix(
            builder,
            hash.as_basic_value().into_int_value(),
            count.as_basic_value().into_int_value(),
            &format!("{name}.final"),
        )
    }

    #[cfg(feature = "managed-runtime")]
    #[allow(clippy::too_many_arguments)]
    fn allocate_map_node(
        &self,
        builder: &Builder<'ctx>,
        node_type: StructType<'ctx>,
        hash: IntValue<'ctx>,
        key: BasicValueEnum<'ctx>,
        value: BasicValueEnum<'ctx>,
        origin: FailureOrigin,
        name: &str,
    ) -> Result<PointerValue<'ctx>, BackendError> {
        let native_size = node_type
            .size_of()
            .ok_or_else(|| BackendError::Builder("map node has no native size".to_owned()))?;
        let size = if native_size.get_type() == self.context.i64_type() {
            native_size
        } else {
            built(builder.build_int_cast(
                native_size,
                self.context.i64_type(),
                &format!("{name}.size"),
            ))?
        };
        let call = built(
            builder.build_call(
                self.allocate_scanned,
                &[
                    size.into(),
                    self.context
                        .i32_type()
                        .const_int(u64::from(origin.file), false)
                        .into(),
                    self.context
                        .i64_type()
                        .const_int(origin.start, false)
                        .into(),
                    self.context.i64_type().const_int(origin.end, false).into(),
                ],
                name,
            ),
        )?;
        let node = call
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| BackendError::Builder("map allocation returned void".to_owned()))?
            .into_pointer_value();
        for (field, field_value) in [
            hash.into(),
            key,
            value,
            self.context
                .ptr_type(AddressSpace::default())
                .const_null()
                .into(),
        ]
        .into_iter()
        .enumerate()
        {
            let destination = built(builder.build_struct_gep(
                node_type,
                node,
                field as u32,
                &format!("{name}.field{field}"),
            ))?;
            built(builder.build_store(destination, field_value))?;
        }
        Ok(node)
    }

    #[cfg(feature = "managed-runtime")]
    #[allow(clippy::too_many_arguments)]
    fn lower_map_change(
        &self,
        function: FunctionId,
        block: BlockId,
        builder: &Builder<'ctx>,
        source_map: PointerValue<'ctx>,
        key: BasicValueEnum<'ctx>,
        replacement: Option<BasicValueEnum<'ctx>>,
        ty: TypeId,
        origin: FailureOrigin,
        roots: &el_ir::CollectionPointRoots,
        values: &BTreeMap<ValueId, BasicValueEnum<'ctx>>,
        slots: &BTreeMap<SlotId, PointerValue<'ctx>>,
        root_slots: &BTreeMap<ValueId, PointerValue<'ctx>>,
        value_types: &BTreeMap<ValueId, TypeId>,
        slot_types: &BTreeMap<SlotId, TypeId>,
        partial: PointerValue<'ctx>,
        name: &str,
    ) -> Result<PointerValue<'ctx>, BackendError> {
        self.preserve_roots(roots, builder, values, slots, root_slots, slot_types)?;
        let pointer_ty = self.context.ptr_type(AddressSpace::default());
        let null = pointer_ty.const_null();
        set_volatile(built(builder.build_store(partial, null))?)?;
        let head_slot = built(builder.build_alloca(pointer_ty, &format!("{name}.head_slot")))?;
        let tail_slot = built(builder.build_alloca(pointer_ty, &format!("{name}.tail_slot")))?;
        let found_slot =
            built(builder.build_alloca(self.context.bool_type(), &format!("{name}.found_slot")))?;
        built(builder.build_store(head_slot, null))?;
        built(builder.build_store(tail_slot, null))?;
        built(builder.build_store(found_slot, self.context.bool_type().const_zero()))?;

        let Some(Type::Map {
            key: key_ty,
            value: value_ty,
        }) = self.core.types.get(ty.0 as usize)
        else {
            return Err(BackendError::UnsupportedType(ty));
        };
        let (key_ty, value_ty) = (*key_ty, *value_ty);
        let node_type = self.map_node_type(ty)?;
        let seed_call = built(builder.build_call(self.hash_seed, &[], &format!("{name}.seed")))?;
        let seed = seed_call
            .try_as_basic_value()
            .basic()
            .ok_or_else(|| BackendError::Builder("hash seed returned void".to_owned()))?
            .into_int_value();
        let key_hash =
            self.map_key_hash(builder, key, key_ty, seed, &format!("{name}.key_hash"))?;
        let llvm_function = builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or_else(|| BackendError::Builder("builder has no function".to_owned()))?;
        let preheader = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        let loop_block = self
            .context
            .append_basic_block(llvm_function, &format!("{name}.loop"));
        let body_block = self
            .context
            .append_basic_block(llvm_function, &format!("{name}.body"));
        let clone_block = self
            .context
            .append_basic_block(llvm_function, &format!("{name}.clone"));
        let install_block = self
            .context
            .append_basic_block(llvm_function, &format!("{name}.install"));
        let link_block = self
            .context
            .append_basic_block(llvm_function, &format!("{name}.link"));
        let continue_block = self
            .context
            .append_basic_block(llvm_function, &format!("{name}.continue"));
        let exhausted_block = self
            .context
            .append_basic_block(llvm_function, &format!("{name}.exhausted"));
        let append_block = self
            .context
            .append_basic_block(llvm_function, &format!("{name}.append"));
        let append_install = self
            .context
            .append_basic_block(llvm_function, &format!("{name}.append_install"));
        let append_link = self
            .context
            .append_basic_block(llvm_function, &format!("{name}.append_link"));
        let finish_block = self
            .context
            .append_basic_block(llvm_function, &format!("{name}.finish"));
        built(builder.build_unconditional_branch(loop_block))?;

        builder.position_at_end(loop_block);
        let cursor = built(builder.build_phi(pointer_ty, &format!("{name}.cursor")))?;
        cursor.add_incoming(&[(&source_map, preheader)]);
        let exhausted = built(builder.build_is_null(
            cursor.as_basic_value().into_pointer_value(),
            &format!("{name}.empty"),
        ))?;
        built(builder.build_conditional_branch(exhausted, exhausted_block, body_block))?;

        builder.position_at_end(body_block);
        let current = cursor.as_basic_value().into_pointer_value();
        let hash_pointer =
            built(builder.build_struct_gep(node_type, current, 0, &format!("{name}.hash_ptr")))?;
        let key_pointer =
            built(builder.build_struct_gep(node_type, current, 1, &format!("{name}.key_ptr")))?;
        let value_pointer =
            built(builder.build_struct_gep(node_type, current, 2, &format!("{name}.value_ptr")))?;
        let next_pointer =
            built(builder.build_struct_gep(node_type, current, 3, &format!("{name}.next_ptr")))?;
        let old_hash =
            built(builder.build_load(self.usize_type()?, hash_pointer, &format!("{name}.hash")))?
                .into_int_value();
        let old_key = built(builder.build_load(
            self.basic_type(key_ty)?,
            key_pointer,
            &format!("{name}.key"),
        ))?;
        let old_value = built(builder.build_load(
            self.basic_type(value_ty)?,
            value_pointer,
            &format!("{name}.value"),
        ))?;
        let next = built(builder.build_load(pointer_ty, next_pointer, &format!("{name}.next")))?
            .into_pointer_value();
        let same_hash = built(builder.build_int_compare(
            IntPredicate::EQ,
            old_hash,
            key_hash,
            &format!("{name}.same_hash"),
        ))?;
        let same_key =
            self.map_key_equal(builder, old_key, key, key_ty, &format!("{name}.same_key"))?;
        let equal = built(builder.build_and(same_hash, same_key, &format!("{name}.equal")))?;
        let value_to_copy = if let Some(replacement) = replacement {
            let selected = built(builder.build_select(
                equal,
                replacement,
                old_value,
                &format!("{name}.selected_value"),
            ))?;
            let found = built(builder.build_load(
                self.context.bool_type(),
                found_slot,
                &format!("{name}.found"),
            ))?
            .into_int_value();
            let now_found = built(builder.build_or(found, equal, &format!("{name}.now_found")))?;
            built(builder.build_store(found_slot, now_found))?;
            selected
        } else {
            old_value
        };
        if replacement.is_some() {
            built(builder.build_unconditional_branch(clone_block))?;
        } else {
            built(builder.build_conditional_branch(equal, continue_block, clone_block))?;
        }

        builder.position_at_end(clone_block);
        let node = self.allocate_map_node(
            builder,
            node_type,
            old_hash,
            old_key,
            value_to_copy,
            origin,
            &format!("{name}.node"),
        )?;
        let head = built(builder.build_load(pointer_ty, head_slot, &format!("{name}.head")))?
            .into_pointer_value();
        let empty_output = built(builder.build_is_null(head, &format!("{name}.output_empty")))?;
        built(builder.build_conditional_branch(empty_output, install_block, link_block))?;

        builder.position_at_end(install_block);
        built(builder.build_store(head_slot, node))?;
        built(builder.build_store(tail_slot, node))?;
        set_volatile(built(builder.build_store(partial, node))?)?;
        built(builder.build_unconditional_branch(continue_block))?;

        builder.position_at_end(link_block);
        let tail = built(builder.build_load(pointer_ty, tail_slot, &format!("{name}.tail")))?
            .into_pointer_value();
        let tail_next =
            built(builder.build_struct_gep(node_type, tail, 3, &format!("{name}.tail_next")))?;
        built(builder.build_store(tail_next, node))?;
        built(builder.build_store(tail_slot, node))?;
        built(builder.build_unconditional_branch(continue_block))?;

        builder.position_at_end(continue_block);
        let continue_end = builder
            .get_insert_block()
            .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
        built(builder.build_unconditional_branch(loop_block))?;
        cursor.add_incoming(&[(&next, continue_end)]);

        builder.position_at_end(exhausted_block);
        if replacement.is_some() {
            let found = built(builder.build_load(
                self.context.bool_type(),
                found_slot,
                &format!("{name}.found_final"),
            ))?
            .into_int_value();
            built(builder.build_conditional_branch(found, finish_block, append_block))?;
        } else {
            built(builder.build_unconditional_branch(finish_block))?;
        }

        if let Some(replacement) = replacement {
            builder.position_at_end(append_block);
            let appended = self.allocate_map_node(
                builder,
                node_type,
                key_hash,
                key,
                replacement,
                origin,
                &format!("{name}.appended"),
            )?;
            let head =
                built(builder.build_load(pointer_ty, head_slot, &format!("{name}.append_head")))?
                    .into_pointer_value();
            let empty_output = built(builder.build_is_null(head, &format!("{name}.append_empty")))?;
            built(builder.build_conditional_branch(empty_output, append_install, append_link))?;

            builder.position_at_end(append_install);
            built(builder.build_store(head_slot, appended))?;
            built(builder.build_store(tail_slot, appended))?;
            set_volatile(built(builder.build_store(partial, appended))?)?;
            built(builder.build_unconditional_branch(finish_block))?;

            builder.position_at_end(append_link);
            let tail =
                built(builder.build_load(pointer_ty, tail_slot, &format!("{name}.append_tail")))?
                    .into_pointer_value();
            let tail_next = built(builder.build_struct_gep(
                node_type,
                tail,
                3,
                &format!("{name}.append_tail_next"),
            ))?;
            built(builder.build_store(tail_next, appended))?;
            built(builder.build_store(tail_slot, appended))?;
            built(builder.build_unconditional_branch(finish_block))?;
        } else {
            for unreachable in [append_block, append_install, append_link] {
                builder.position_at_end(unreachable);
                built(builder.build_unreachable())?;
            }
        }

        builder.position_at_end(finish_block);
        let result = built(builder.build_load(pointer_ty, head_slot, &format!("{name}.result")))?
            .into_pointer_value();
        self.clear_value_roots(roots, builder, root_slots, value_types)?;
        set_volatile(built(builder.build_store(partial, null))?)?;
        let _ = (function, block);
        Ok(result)
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
        let mut partial_allocation_roots = BTreeMap::new();
        for block in &function.blocks {
            for (operation_index, operation) in block.operations.iter().enumerate() {
                if collection_points.contains_key(&(block.id, operation_index))
                    && matches!(
                        operation,
                        Operation::List { .. }
                            | Operation::ListReverse { .. }
                            | Operation::Map { .. }
                            | Operation::MapPut { .. }
                            | Operation::MapRemove { .. }
                            | Operation::MapToList { .. }
                            | Operation::BytesToList { .. }
                    )
                {
                    let pointer = built(builder.build_alloca(
                        self.context.ptr_type(AddressSpace::default()),
                        &format!("gc.partial.b{}.o{}", block.id.0, operation_index),
                    ))?;
                    set_volatile(built(builder.build_store(
                        pointer,
                        self.context.ptr_type(AddressSpace::default()).const_null(),
                    ))?)?;
                    partial_allocation_roots.insert((block.id, operation_index), pointer);
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
                    partial_allocation_roots
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
            Operation::ListReverse {
                result,
                list,
                ty,
                origin,
            } => {
                #[cfg(not(feature = "managed-runtime"))]
                {
                    let _ = (result, list, ty, origin);
                    return Err(BackendError::UnsupportedOperation {
                        function,
                        block,
                        operation: "list_reverse",
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
                        BackendError::Builder("list reverse has no partial-result root".to_owned())
                    })?;
                    self.preserve_roots(roots, builder, values, slots, root_slots, slot_types)?;
                    let pointer_ty = self.context.ptr_type(AddressSpace::default());
                    let null = pointer_ty.const_null();
                    set_volatile(built(builder.build_store(partial, null))?)?;

                    let llvm_function = builder
                        .get_insert_block()
                        .and_then(|block| block.get_parent())
                        .ok_or_else(|| {
                            BackendError::Builder("builder has no function".to_owned())
                        })?;
                    let preheader = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    let loop_block = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.reverse_loop", result.0));
                    let body_block = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.reverse_body", result.0));
                    let done_block = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.reverse_done", result.0));
                    built(builder.build_unconditional_branch(loop_block))?;

                    builder.position_at_end(loop_block);
                    let remaining_phi =
                        built(builder.build_phi(pointer_ty, &format!("v{}.remaining", result.0)))?;
                    let reversed_phi =
                        built(builder.build_phi(pointer_ty, &format!("v{}.reversed", result.0)))?;
                    let initial = pointer_value(values, *list)?;
                    remaining_phi.add_incoming(&[(&initial, preheader)]);
                    reversed_phi.add_incoming(&[(&null, preheader)]);
                    let empty = built(builder.build_is_null(
                        remaining_phi.as_basic_value().into_pointer_value(),
                        &format!("v{}.empty", result.0),
                    ))?;
                    built(builder.build_conditional_branch(empty, done_block, body_block))?;

                    builder.position_at_end(body_block);
                    let node_type = self.list_node_type(*ty)?;
                    let remaining = remaining_phi.as_basic_value().into_pointer_value();
                    let item_pointer = built(builder.build_struct_gep(
                        node_type,
                        remaining,
                        0,
                        &format!("v{}.source_item", result.0),
                    ))?;
                    let next_pointer = built(builder.build_struct_gep(
                        node_type,
                        remaining,
                        1,
                        &format!("v{}.source_next", result.0),
                    ))?;
                    let item = built(builder.build_load(
                        node_type.get_field_type_at_index(0).ok_or_else(|| {
                            BackendError::Builder("list node has no item field".to_owned())
                        })?,
                        item_pointer,
                        &format!("v{}.item", result.0),
                    ))?;
                    let next = built(builder.build_load(
                        pointer_ty,
                        next_pointer,
                        &format!("v{}.next", result.0),
                    ))?
                    .into_pointer_value();
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
                            &format!("v{}.node", result.0),
                        ),
                    )?;
                    let node = call
                        .try_as_basic_value()
                        .basic()
                        .ok_or(BackendError::MissingValue(*result))?
                        .into_pointer_value();
                    let destination_item = built(builder.build_struct_gep(
                        node_type,
                        node,
                        0,
                        &format!("v{}.destination_item", result.0),
                    ))?;
                    let destination_next = built(builder.build_struct_gep(
                        node_type,
                        node,
                        1,
                        &format!("v{}.destination_next", result.0),
                    ))?;
                    built(builder.build_store(destination_item, item))?;
                    built(builder.build_store(
                        destination_next,
                        reversed_phi.as_basic_value().into_pointer_value(),
                    ))?;
                    set_volatile(built(builder.build_store(partial, node))?)?;
                    let body_end = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    built(builder.build_unconditional_branch(loop_block))?;
                    remaining_phi.add_incoming(&[(&next, body_end)]);
                    reversed_phi.add_incoming(&[(&node, body_end)]);

                    builder.position_at_end(done_block);
                    let reversed = reversed_phi.as_basic_value().into_pointer_value();
                    self.clear_value_roots(roots, builder, root_slots, value_types)?;
                    set_volatile(built(builder.build_store(partial, null))?)?;
                    values.insert(*result, reversed.into());
                }
            }
            Operation::Map {
                result,
                entries,
                ty,
                origin,
            } => {
                let pointer_ty = self.context.ptr_type(AddressSpace::default());
                let null = pointer_ty.const_null();
                if entries.is_empty() {
                    values.insert(*result, null.into());
                } else {
                    #[cfg(not(feature = "managed-runtime"))]
                    {
                        let _ = (ty, origin);
                        return Err(BackendError::UnsupportedOperation {
                            function,
                            block,
                            operation: "map",
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
                                "allocating map has no partial-map root".to_owned(),
                            )
                        })?;
                        self.preserve_roots(roots, builder, values, slots, root_slots, slot_types)?;
                        set_volatile(built(builder.build_store(partial, null))?)?;
                        let node_type = self.map_node_type(*ty)?;
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
                        let Some(Type::Map { key: key_ty, .. }) =
                            self.core.types.get(ty.0 as usize)
                        else {
                            return Err(BackendError::UnsupportedType(*ty));
                        };
                        let key_ty = *key_ty;
                        let llvm_function = builder
                            .get_insert_block()
                            .and_then(|block| block.get_parent())
                            .ok_or_else(|| {
                                BackendError::Builder("builder has no function".to_owned())
                            })?;
                        let seed_call = built(builder.build_call(
                            self.hash_seed,
                            &[],
                            &format!("v{}.map_seed", result.0),
                        ))?;
                        let seed = seed_call
                            .try_as_basic_value()
                            .basic()
                            .ok_or_else(|| {
                                BackendError::Builder("hash seed returned void".to_owned())
                            })?
                            .into_int_value();
                        let mut head = null;
                        for (index, (key, entry_value)) in entries.iter().enumerate() {
                            let key_hash = self.map_key_hash(
                                builder,
                                value(values, *key)?,
                                key_ty,
                                seed,
                                &format!("v{}.map{index}.key_hash", result.0),
                            )?;
                            let preheader = builder.get_insert_block().ok_or_else(|| {
                                BackendError::Builder("builder has no block".to_owned())
                            })?;
                            let search = self.context.append_basic_block(
                                llvm_function,
                                &format!("v{}.map{index}.search", result.0),
                            );
                            let inspect = self.context.append_basic_block(
                                llvm_function,
                                &format!("v{}.map{index}.inspect", result.0),
                            );
                            let advance = self.context.append_basic_block(
                                llvm_function,
                                &format!("v{}.map{index}.advance", result.0),
                            );
                            let replace = self.context.append_basic_block(
                                llvm_function,
                                &format!("v{}.map{index}.replace", result.0),
                            );
                            let append = self.context.append_basic_block(
                                llvm_function,
                                &format!("v{}.map{index}.append", result.0),
                            );
                            let install_head = self.context.append_basic_block(
                                llvm_function,
                                &format!("v{}.map{index}.install_head", result.0),
                            );
                            let link_tail = self.context.append_basic_block(
                                llvm_function,
                                &format!("v{}.map{index}.link_tail", result.0),
                            );
                            let done = self.context.append_basic_block(
                                llvm_function,
                                &format!("v{}.map{index}.done", result.0),
                            );
                            built(builder.build_unconditional_branch(search))?;

                            builder.position_at_end(search);
                            let cursor = built(builder.build_phi(
                                pointer_ty,
                                &format!("v{}.map{index}.cursor", result.0),
                            ))?;
                            let previous = built(builder.build_phi(
                                pointer_ty,
                                &format!("v{}.map{index}.previous", result.0),
                            ))?;
                            cursor.add_incoming(&[(&head, preheader)]);
                            previous.add_incoming(&[(&null, preheader)]);
                            let exhausted = built(builder.build_is_null(
                                cursor.as_basic_value().into_pointer_value(),
                                &format!("v{}.map{index}.exhausted", result.0),
                            ))?;
                            built(builder.build_conditional_branch(exhausted, append, inspect))?;

                            builder.position_at_end(inspect);
                            let current = cursor.as_basic_value().into_pointer_value();
                            let stored_hash_pointer = built(builder.build_struct_gep(
                                node_type,
                                current,
                                0,
                                &format!("v{}.map{index}.hash_ptr", result.0),
                            ))?;
                            let stored_key_pointer = built(builder.build_struct_gep(
                                node_type,
                                current,
                                1,
                                &format!("v{}.map{index}.key_ptr", result.0),
                            ))?;
                            let stored_key = built(builder.build_load(
                                self.basic_type(key_ty)?,
                                stored_key_pointer,
                                &format!("v{}.map{index}.key", result.0),
                            ))?;
                            let stored_hash = built(builder.build_load(
                                self.usize_type()?,
                                stored_hash_pointer,
                                &format!("v{}.map{index}.hash", result.0),
                            ))?
                            .into_int_value();
                            let same_hash = built(builder.build_int_compare(
                                IntPredicate::EQ,
                                stored_hash,
                                key_hash,
                                &format!("v{}.map{index}.same_hash", result.0),
                            ))?;
                            let same_key = self.map_key_equal(
                                builder,
                                stored_key,
                                value(values, *key)?,
                                key_ty,
                                &format!("v{}.map{index}.same_key", result.0),
                            )?;
                            let equal = built(builder.build_and(
                                same_hash,
                                same_key,
                                &format!("v{}.map{index}.equal", result.0),
                            ))?;
                            built(builder.build_conditional_branch(equal, replace, advance))?;

                            builder.position_at_end(replace);
                            let stored_value = built(builder.build_struct_gep(
                                node_type,
                                current,
                                2,
                                &format!("v{}.map{index}.value_ptr", result.0),
                            ))?;
                            built(builder.build_store(stored_value, value(values, *entry_value)?))?;
                            let replace_end = builder.get_insert_block().ok_or_else(|| {
                                BackendError::Builder("builder has no block".to_owned())
                            })?;
                            built(builder.build_unconditional_branch(done))?;

                            builder.position_at_end(advance);
                            let next_pointer = built(builder.build_struct_gep(
                                node_type,
                                current,
                                3,
                                &format!("v{}.map{index}.next_ptr", result.0),
                            ))?;
                            let next = built(builder.build_load(
                                pointer_ty,
                                next_pointer,
                                &format!("v{}.map{index}.next", result.0),
                            ))?
                            .into_pointer_value();
                            let advance_end = builder.get_insert_block().ok_or_else(|| {
                                BackendError::Builder("builder has no block".to_owned())
                            })?;
                            built(builder.build_unconditional_branch(search))?;
                            cursor.add_incoming(&[(&next, advance_end)]);
                            previous.add_incoming(&[(&current, advance_end)]);

                            builder.position_at_end(append);
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
                                    &format!("v{}.map_node{index}", result.0),
                                ),
                            )?;
                            let node = call
                                .try_as_basic_value()
                                .basic()
                                .ok_or(BackendError::MissingValue(*result))?
                                .into_pointer_value();
                            for (field, field_value) in [
                                key_hash.into(),
                                value(values, *key)?,
                                value(values, *entry_value)?,
                                null.into(),
                            ]
                            .into_iter()
                            .enumerate()
                            {
                                let destination = built(builder.build_struct_gep(
                                    node_type,
                                    node,
                                    field as u32,
                                    &format!("v{}.map_node{index}.field{field}", result.0),
                                ))?;
                                built(builder.build_store(destination, field_value))?;
                            }
                            let was_empty = built(builder.build_is_null(
                                head,
                                &format!("v{}.map{index}.was_empty", result.0),
                            ))?;
                            built(builder.build_conditional_branch(
                                was_empty,
                                install_head,
                                link_tail,
                            ))?;

                            builder.position_at_end(install_head);
                            set_volatile(built(builder.build_store(partial, node))?)?;
                            let install_end = builder.get_insert_block().ok_or_else(|| {
                                BackendError::Builder("builder has no block".to_owned())
                            })?;
                            built(builder.build_unconditional_branch(done))?;

                            builder.position_at_end(link_tail);
                            let tail_next = built(builder.build_struct_gep(
                                node_type,
                                previous.as_basic_value().into_pointer_value(),
                                3,
                                &format!("v{}.map{index}.tail_next", result.0),
                            ))?;
                            built(builder.build_store(tail_next, node))?;
                            let link_end = builder.get_insert_block().ok_or_else(|| {
                                BackendError::Builder("builder has no block".to_owned())
                            })?;
                            built(builder.build_unconditional_branch(done))?;

                            builder.position_at_end(done);
                            let next_head =
                                built(builder.build_phi(
                                    pointer_ty,
                                    &format!("v{}.map{index}.head", result.0),
                                ))?;
                            next_head.add_incoming(&[
                                (&head, replace_end),
                                (&node, install_end),
                                (&head, link_end),
                            ]);
                            head = next_head.as_basic_value().into_pointer_value();
                            set_volatile(built(builder.build_store(partial, head))?)?;
                        }
                        self.clear_value_roots(roots, builder, root_slots, value_types)?;
                        set_volatile(built(builder.build_store(partial, null))?)?;
                        values.insert(*result, head.into());
                    }
                }
            }
            Operation::MapPut {
                result,
                map,
                key,
                value: replacement,
                ty,
                origin,
            } => {
                #[cfg(not(feature = "managed-runtime"))]
                {
                    let _ = (result, map, key, replacement, ty, origin);
                    return Err(BackendError::UnsupportedOperation {
                        function,
                        block,
                        operation: "map_put",
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
                        BackendError::Builder("map put has no partial-map root".to_owned())
                    })?;
                    let origin = FailureOrigin::from_span(*origin)
                        .map_err(|()| BackendError::SourceOriginOutOfRange)?;
                    let output = self.lower_map_change(
                        function,
                        block,
                        builder,
                        pointer_value(values, *map)?,
                        value(values, *key)?,
                        Some(value(values, *replacement)?),
                        *ty,
                        origin,
                        roots,
                        values,
                        slots,
                        root_slots,
                        value_types,
                        slot_types,
                        partial,
                        &format!("v{}.put", result.0),
                    )?;
                    values.insert(*result, output.into());
                }
            }
            Operation::MapRemove {
                result,
                map,
                key,
                ty,
                origin,
            } => {
                #[cfg(not(feature = "managed-runtime"))]
                {
                    let _ = (result, map, key, ty, origin);
                    return Err(BackendError::UnsupportedOperation {
                        function,
                        block,
                        operation: "map_remove",
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
                        BackendError::Builder("map remove has no partial-map root".to_owned())
                    })?;
                    let origin = FailureOrigin::from_span(*origin)
                        .map_err(|()| BackendError::SourceOriginOutOfRange)?;
                    let output = self.lower_map_change(
                        function,
                        block,
                        builder,
                        pointer_value(values, *map)?,
                        value(values, *key)?,
                        None,
                        *ty,
                        origin,
                        roots,
                        values,
                        slots,
                        root_slots,
                        value_types,
                        slot_types,
                        partial,
                        &format!("v{}.remove", result.0),
                    )?;
                    values.insert(*result, output.into());
                }
            }
            Operation::MapFetch {
                result,
                map,
                key,
                map_ty,
                ty,
                ..
            } => {
                #[cfg(not(feature = "managed-runtime"))]
                {
                    let _ = (result, map, key, map_ty, ty);
                    return Err(BackendError::UnsupportedOperation {
                        function,
                        block,
                        operation: "map_fetch",
                    });
                }
                #[cfg(feature = "managed-runtime")]
                {
                    let Some(Type::Map {
                        key: key_ty,
                        value: value_ty,
                    }) = self.core.types.get(map_ty.0 as usize)
                    else {
                        return Err(BackendError::UnsupportedType(*map_ty));
                    };
                    let (key_ty, value_ty) = (*key_ty, *value_ty);
                    let Some(Type::Union(members)) = self.core.types.get(ty.0 as usize) else {
                        return Err(BackendError::UnsupportedType(*ty));
                    };
                    let none_ty = members
                        .iter()
                        .copied()
                        .find(|member| {
                            matches!(self.core.types.get(member.0 as usize), Some(Type::Atom(name)) if name == "none")
                        })
                        .ok_or(BackendError::UnsupportedType(*ty))?;
                    let some_ty = members
                        .iter()
                        .copied()
                        .find(|member| {
                            matches!(self.core.types.get(member.0 as usize), Some(Type::Tuple(items)) if items.len() == 2
                                && items[1] == value_ty
                                && matches!(self.core.types.get(items[0].0 as usize), Some(Type::Atom(name)) if name == "some"))
                        })
                        .ok_or(BackendError::UnsupportedType(*ty))?;
                    let pointer_ty = self.context.ptr_type(AddressSpace::default());
                    let node_type = self.map_node_type(*map_ty)?;
                    let llvm_function = builder
                        .get_insert_block()
                        .and_then(|block| block.get_parent())
                        .ok_or_else(|| {
                            BackendError::Builder("builder has no function".to_owned())
                        })?;
                    let seed_call = built(builder.build_call(
                        self.hash_seed,
                        &[],
                        &format!("v{}.fetch_seed", result.0),
                    ))?;
                    let seed = seed_call
                        .try_as_basic_value()
                        .basic()
                        .ok_or_else(|| BackendError::Builder("hash seed returned void".to_owned()))?
                        .into_int_value();
                    let key_hash = self.map_key_hash(
                        builder,
                        value(values, *key)?,
                        key_ty,
                        seed,
                        &format!("v{}.fetch_hash", result.0),
                    )?;
                    let preheader = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    let search = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.fetch_search", result.0));
                    let inspect = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.fetch_inspect", result.0));
                    let advance = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.fetch_advance", result.0));
                    let found = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.fetch_found", result.0));
                    let missing = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.fetch_missing", result.0));
                    let done = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.fetch_done", result.0));
                    built(builder.build_unconditional_branch(search))?;

                    builder.position_at_end(search);
                    let cursor = built(
                        builder.build_phi(pointer_ty, &format!("v{}.fetch_cursor", result.0)),
                    )?;
                    cursor.add_incoming(&[(&pointer_value(values, *map)?, preheader)]);
                    let exhausted = built(builder.build_is_null(
                        cursor.as_basic_value().into_pointer_value(),
                        &format!("v{}.fetch_exhausted", result.0),
                    ))?;
                    built(builder.build_conditional_branch(exhausted, missing, inspect))?;

                    builder.position_at_end(inspect);
                    let current = cursor.as_basic_value().into_pointer_value();
                    let stored_hash_pointer = built(builder.build_struct_gep(
                        node_type,
                        current,
                        0,
                        &format!("v{}.fetch_hash_ptr", result.0),
                    ))?;
                    let stored_key_pointer = built(builder.build_struct_gep(
                        node_type,
                        current,
                        1,
                        &format!("v{}.fetch_key_ptr", result.0),
                    ))?;
                    let stored_key = built(builder.build_load(
                        self.basic_type(key_ty)?,
                        stored_key_pointer,
                        &format!("v{}.fetch_key", result.0),
                    ))?;
                    let stored_hash = built(builder.build_load(
                        self.usize_type()?,
                        stored_hash_pointer,
                        &format!("v{}.fetch_stored_hash", result.0),
                    ))?
                    .into_int_value();
                    let same_hash = built(builder.build_int_compare(
                        IntPredicate::EQ,
                        stored_hash,
                        key_hash,
                        &format!("v{}.fetch_same_hash", result.0),
                    ))?;
                    let same_key = self.map_key_equal(
                        builder,
                        stored_key,
                        value(values, *key)?,
                        key_ty,
                        &format!("v{}.fetch_same_key", result.0),
                    )?;
                    let equal = built(builder.build_and(
                        same_hash,
                        same_key,
                        &format!("v{}.fetch_equal", result.0),
                    ))?;
                    built(builder.build_conditional_branch(equal, found, advance))?;

                    builder.position_at_end(advance);
                    let next_pointer = built(builder.build_struct_gep(
                        node_type,
                        current,
                        3,
                        &format!("v{}.fetch_next_ptr", result.0),
                    ))?;
                    let next = built(builder.build_load(
                        pointer_ty,
                        next_pointer,
                        &format!("v{}.fetch_next", result.0),
                    ))?
                    .into_pointer_value();
                    let advance_end = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    built(builder.build_unconditional_branch(search))?;
                    cursor.add_incoming(&[(&next, advance_end)]);

                    builder.position_at_end(found);
                    let stored_value_pointer = built(builder.build_struct_gep(
                        node_type,
                        current,
                        2,
                        &format!("v{}.fetch_value_ptr", result.0),
                    ))?;
                    let stored_value = built(builder.build_load(
                        self.basic_type(value_ty)?,
                        stored_value_pointer,
                        &format!("v{}.fetch_value", result.0),
                    ))?;
                    let mut some = AggregateValueEnum::StructValue(
                        self.basic_type(some_ty)?.into_struct_type().get_undef(),
                    );
                    some = built(builder.build_insert_value(
                        some,
                        self.context.i8_type().const_zero(),
                        0,
                        &format!("v{}.fetch_some_tag", result.0),
                    ))?;
                    some = built(builder.build_insert_value(
                        some,
                        stored_value,
                        1,
                        &format!("v{}.fetch_some_value", result.0),
                    ))?;
                    let mut some_union = AggregateValueEnum::StructValue(
                        self.basic_type(*ty)?.into_struct_type().get_undef(),
                    );
                    let some_tag = self.union_tag(*ty, some_ty)?;
                    some_union = built(
                        builder.build_insert_value(
                            some_union,
                            self.context
                                .i32_type()
                                .const_int(u64::from(some_tag), false),
                            0,
                            &format!("v{}.fetch_some_union_tag", result.0),
                        ),
                    )?;
                    some_union = built(builder.build_insert_value(
                        some_union,
                        some.into_struct_value(),
                        some_tag + 1,
                        &format!("v{}.fetch_some_payload", result.0),
                    ))?;
                    let found_end = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    built(builder.build_unconditional_branch(done))?;

                    builder.position_at_end(missing);
                    let mut none_union = AggregateValueEnum::StructValue(
                        self.basic_type(*ty)?.into_struct_type().get_undef(),
                    );
                    let none_tag = self.union_tag(*ty, none_ty)?;
                    none_union = built(
                        builder.build_insert_value(
                            none_union,
                            self.context
                                .i32_type()
                                .const_int(u64::from(none_tag), false),
                            0,
                            &format!("v{}.fetch_none_union_tag", result.0),
                        ),
                    )?;
                    none_union = built(builder.build_insert_value(
                        none_union,
                        self.context.i8_type().const_zero(),
                        none_tag + 1,
                        &format!("v{}.fetch_none_payload", result.0),
                    ))?;
                    let missing_end = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    built(builder.build_unconditional_branch(done))?;

                    builder.position_at_end(done);
                    let output =
                        built(builder.build_phi(self.basic_type(*ty)?, &format!("v{}", result.0)))?;
                    let some_value = some_union.into_struct_value();
                    let none_value = none_union.into_struct_value();
                    output.add_incoming(&[(&some_value, found_end), (&none_value, missing_end)]);
                    values.insert(*result, output.as_basic_value());
                }
            }
            Operation::MapToList {
                result,
                map,
                map_ty,
                ty,
                origin,
            } => {
                #[cfg(not(feature = "managed-runtime"))]
                {
                    let _ = (result, map, map_ty, ty, origin);
                    return Err(BackendError::UnsupportedOperation {
                        function,
                        block,
                        operation: "map_to_list",
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
                        BackendError::Builder("map to-list has no partial-list root".to_owned())
                    })?;
                    self.preserve_roots(roots, builder, values, slots, root_slots, slot_types)?;
                    let Some(Type::Map {
                        key: key_ty,
                        value: value_ty,
                    }) = self.core.types.get(map_ty.0 as usize)
                    else {
                        return Err(BackendError::UnsupportedType(*map_ty));
                    };
                    let Some(Type::List(pair_ty)) = self.core.types.get(ty.0 as usize) else {
                        return Err(BackendError::UnsupportedType(*ty));
                    };
                    let pointer_ty = self.context.ptr_type(AddressSpace::default());
                    let null = pointer_ty.const_null();
                    set_volatile(built(builder.build_store(partial, null))?)?;
                    let head_slot = built(
                        builder.build_alloca(pointer_ty, &format!("v{}.to_list_head", result.0)),
                    )?;
                    let tail_slot = built(
                        builder.build_alloca(pointer_ty, &format!("v{}.to_list_tail", result.0)),
                    )?;
                    built(builder.build_store(head_slot, null))?;
                    built(builder.build_store(tail_slot, null))?;
                    let map_node = self.map_node_type(*map_ty)?;
                    let list_node = self.list_node_type(*ty)?;
                    let native_size = list_node
                        .size_of()
                        .ok_or(BackendError::UnsupportedType(*ty))?;
                    let size = if native_size.get_type() == self.context.i64_type() {
                        native_size
                    } else {
                        built(builder.build_int_cast(
                            native_size,
                            self.context.i64_type(),
                            &format!("v{}.to_list_size", result.0),
                        ))?
                    };
                    let source = FailureOrigin::from_span(*origin)
                        .map_err(|()| BackendError::SourceOriginOutOfRange)?;
                    let llvm_function = builder
                        .get_insert_block()
                        .and_then(|block| block.get_parent())
                        .ok_or_else(|| {
                            BackendError::Builder("builder has no function".to_owned())
                        })?;
                    let preheader = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    let loop_block = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.to_list_loop", result.0));
                    let body_block = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.to_list_body", result.0));
                    let install_block = self.context.append_basic_block(
                        llvm_function,
                        &format!("v{}.to_list_install", result.0),
                    );
                    let link_block = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.to_list_link", result.0));
                    let continue_block = self.context.append_basic_block(
                        llvm_function,
                        &format!("v{}.to_list_continue", result.0),
                    );
                    let done_block = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.to_list_done", result.0));
                    built(builder.build_unconditional_branch(loop_block))?;
                    builder.position_at_end(loop_block);
                    let cursor = built(
                        builder.build_phi(pointer_ty, &format!("v{}.to_list_cursor", result.0)),
                    )?;
                    cursor.add_incoming(&[(&pointer_value(values, *map)?, preheader)]);
                    let exhausted = built(builder.build_is_null(
                        cursor.as_basic_value().into_pointer_value(),
                        &format!("v{}.to_list_empty", result.0),
                    ))?;
                    built(builder.build_conditional_branch(exhausted, done_block, body_block))?;

                    builder.position_at_end(body_block);
                    let current = cursor.as_basic_value().into_pointer_value();
                    let key_ptr = built(builder.build_struct_gep(
                        map_node,
                        current,
                        1,
                        "map.to_list.key_ptr",
                    ))?;
                    let value_ptr = built(builder.build_struct_gep(
                        map_node,
                        current,
                        2,
                        "map.to_list.value_ptr",
                    ))?;
                    let next_ptr = built(builder.build_struct_gep(
                        map_node,
                        current,
                        3,
                        "map.to_list.next_ptr",
                    ))?;
                    let key = built(builder.build_load(
                        self.basic_type(*key_ty)?,
                        key_ptr,
                        "map.to_list.key",
                    ))?;
                    let item_value = built(builder.build_load(
                        self.basic_type(*value_ty)?,
                        value_ptr,
                        "map.to_list.value",
                    ))?;
                    let next = built(builder.build_load(pointer_ty, next_ptr, "map.to_list.next"))?
                        .into_pointer_value();
                    let mut pair = AggregateValueEnum::StructValue(
                        self.basic_type(*pair_ty)?.into_struct_type().get_undef(),
                    );
                    pair = built(builder.build_insert_value(pair, key, 0, "map.to_list.pair_key"))?;
                    pair = built(builder.build_insert_value(
                        pair,
                        item_value,
                        1,
                        "map.to_list.pair_value",
                    ))?;
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
                            &format!("v{}.to_list_node", result.0),
                        ),
                    )?;
                    let node = call
                        .try_as_basic_value()
                        .basic()
                        .ok_or(BackendError::MissingValue(*result))?
                        .into_pointer_value();
                    let item_ptr = built(builder.build_struct_gep(
                        list_node,
                        node,
                        0,
                        "map.to_list.item_ptr",
                    ))?;
                    let node_next = built(builder.build_struct_gep(
                        list_node,
                        node,
                        1,
                        "map.to_list.node_next",
                    ))?;
                    built(builder.build_store(item_ptr, pair.into_struct_value()))?;
                    built(builder.build_store(node_next, null))?;
                    let head =
                        built(builder.build_load(pointer_ty, head_slot, "map.to_list.head"))?
                            .into_pointer_value();
                    let empty = built(builder.build_is_null(head, "map.to_list.output_empty"))?;
                    built(builder.build_conditional_branch(empty, install_block, link_block))?;
                    builder.position_at_end(install_block);
                    built(builder.build_store(head_slot, node))?;
                    built(builder.build_store(tail_slot, node))?;
                    set_volatile(built(builder.build_store(partial, node))?)?;
                    built(builder.build_unconditional_branch(continue_block))?;
                    builder.position_at_end(link_block);
                    let tail =
                        built(builder.build_load(pointer_ty, tail_slot, "map.to_list.tail"))?
                            .into_pointer_value();
                    let tail_next = built(builder.build_struct_gep(
                        list_node,
                        tail,
                        1,
                        "map.to_list.tail_next",
                    ))?;
                    built(builder.build_store(tail_next, node))?;
                    built(builder.build_store(tail_slot, node))?;
                    built(builder.build_unconditional_branch(continue_block))?;
                    builder.position_at_end(continue_block);
                    let continue_end = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    built(builder.build_unconditional_branch(loop_block))?;
                    cursor.add_incoming(&[(&next, continue_end)]);
                    builder.position_at_end(done_block);
                    let output = built(builder.build_load(
                        pointer_ty,
                        head_slot,
                        &format!("v{}", result.0),
                    ))?
                    .into_pointer_value();
                    self.clear_value_roots(roots, builder, root_slots, value_types)?;
                    set_volatile(built(builder.build_store(partial, null))?)?;
                    values.insert(*result, output.into());
                }
            }
            Operation::Tuple {
                result,
                elements,
                ty,
                ..
            }
            | Operation::Array {
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
            Operation::Struct {
                result, fields, ty, ..
            } => {
                let mut aggregate = AggregateValueEnum::StructValue(
                    self.basic_type(*ty)?.into_struct_type().get_undef(),
                );
                for (index, field) in fields {
                    aggregate = built(builder.build_insert_value(
                        aggregate,
                        value(values, *field)?,
                        *index as u32,
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
            Operation::StructProject {
                result,
                structure,
                index,
                ..
            } => {
                let projected = built(builder.build_extract_value(
                    struct_value(values, *structure)?,
                    *index as u32,
                    &format!("v{}", result.0),
                ))?;
                values.insert(*result, projected);
            }
            Operation::ArrayIndex {
                result,
                array,
                index,
                length,
                failure,
                ty,
                ..
            } => {
                let index_value = integer_value(values, *index)?;
                let source = struct_value(values, *array)?;
                let bound = if let Some(length) = length {
                    index_value.get_type().const_int(*length, false)
                } else {
                    built(builder.build_extract_value(source, 2, &format!("v{}.length", result.0)))?
                        .into_int_value()
                };
                let out_of_bounds = built(builder.build_int_compare(
                    IntPredicate::UGE,
                    index_value,
                    bound,
                    &format!("v{}.out_of_bounds", result.0),
                ))?;
                let continuation = self.context.append_basic_block(
                    builder
                        .get_insert_block()
                        .and_then(|block| block.get_parent())
                        .ok_or_else(|| {
                            BackendError::Builder("builder has no function".to_owned())
                        })?,
                    &format!("v{}.index_ok", result.0),
                );
                built(builder.build_conditional_branch(
                    out_of_bounds,
                    self.block(blocks, *failure)?,
                    continuation,
                ))?;
                builder.position_at_end(continuation);

                if let Some(length) = length {
                    let mut selected = self.basic_type(*ty)?.const_zero();
                    for candidate in 0..*length {
                        let element = built(builder.build_extract_value(
                            source,
                            candidate as u32,
                            &format!("v{}.candidate{candidate}", result.0),
                        ))?;
                        let matches = built(builder.build_int_compare(
                            IntPredicate::EQ,
                            index_value,
                            index_value.get_type().const_int(candidate, false),
                            &format!("v{}.is{candidate}", result.0),
                        ))?;
                        selected = built(builder.build_select(
                            matches,
                            element,
                            selected,
                            &format!("v{}.select{candidate}", result.0),
                        ))?;
                    }
                    values.insert(*result, selected);
                } else {
                    let data = built(builder.build_extract_value(
                        source,
                        1,
                        &format!("v{}.data", result.0),
                    ))?
                    .into_pointer_value();
                    let pointer = self.element_pointer(
                        builder,
                        self.basic_type(*ty)?,
                        data,
                        index_value,
                        &format!("v{}.pointer", result.0),
                    )?;
                    let loaded = built(builder.build_load(
                        self.basic_type(*ty)?,
                        pointer,
                        &format!("v{}", result.0),
                    ))?;
                    values.insert(*result, loaded);
                }
            }
            Operation::SliceFromArray {
                result,
                array,
                length,
                ty,
                origin,
            } => {
                #[cfg(not(feature = "managed-runtime"))]
                {
                    let _ = (result, array, length, ty, origin);
                    return Err(BackendError::UnsupportedOperation {
                        function,
                        block,
                        operation: "slice_from_array",
                    });
                }
                #[cfg(feature = "managed-runtime")]
                {
                    let roots = roots.ok_or_else(|| {
                        BackendError::InvalidConcrete(vec![format!(
                            "missing live-root set for collection point {function:?} {block:?}"
                        )])
                    })?;
                    self.preserve_roots(roots, builder, values, slots, root_slots, slot_types)?;
                    let item_ty = match self.core.types.get(ty.0 as usize) {
                        Some(Type::Slice(item)) => self.basic_type(*item)?,
                        _ => return Err(BackendError::UnsupportedType(*ty)),
                    };
                    let base = if *length == 0 {
                        self.context.ptr_type(AddressSpace::default()).const_null()
                    } else {
                        let item_size = item_ty
                            .size_of()
                            .ok_or(BackendError::UnsupportedType(*ty))?;
                        let size = built(builder.build_int_mul(
                            item_size,
                            item_size.get_type().const_int(*length, false),
                            &format!("v{}.bytes", result.0),
                        ))?;
                        let is_zero = built(builder.build_int_compare(
                            IntPredicate::EQ,
                            size,
                            size.get_type().const_zero(),
                            &format!("v{}.empty_storage", result.0),
                        ))?;
                        let size = built(builder.build_select(
                            is_zero,
                            size.get_type().const_int(1, false),
                            size,
                            &format!("v{}.allocation_size", result.0),
                        ))?
                        .into_int_value();
                        let size = if size.get_type() == self.context.i64_type() {
                            size
                        } else {
                            built(builder.build_int_cast(
                                size,
                                self.context.i64_type(),
                                "slice.bytes.i64",
                            ))?
                        };
                        let source = FailureOrigin::from_span(*origin)
                            .map_err(|()| BackendError::SourceOriginOutOfRange)?;
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
                                &format!("v{}.base", result.0),
                            ),
                        )?;
                        let base = call
                            .try_as_basic_value()
                            .basic()
                            .ok_or(BackendError::MissingValue(*result))?
                            .into_pointer_value();
                        let aggregate = struct_value(values, *array)?;
                        for candidate in 0..*length {
                            let pointer = self.element_pointer(
                                builder,
                                item_ty,
                                base,
                                self.usize_type()?.const_int(candidate, false),
                                &format!("v{}.item{candidate}", result.0),
                            )?;
                            let item = built(builder.build_extract_value(
                                aggregate,
                                candidate as u32,
                                &format!("v{}.source{candidate}", result.0),
                            ))?;
                            built(builder.build_store(pointer, item))?;
                        }
                        base
                    };
                    self.clear_value_roots(roots, builder, root_slots, value_types)?;
                    let slice_ty = self.basic_type(*ty)?.into_struct_type();
                    let mut slice = AggregateValueEnum::StructValue(slice_ty.get_undef());
                    let fields: [BasicValueEnum<'ctx>; 3] = [
                        base.into(),
                        base.into(),
                        self.usize_type()?.const_int(*length, false).into(),
                    ];
                    for (field, value) in fields.into_iter().enumerate() {
                        slice = built(builder.build_insert_value(
                            slice,
                            value,
                            field as u32,
                            "slice.field",
                        ))?;
                    }
                    values.insert(*result, slice.into_struct_value().into());
                }
            }
            Operation::SliceSubslice {
                result,
                slice,
                start,
                length,
                failure,
                ty,
                ..
            } => {
                let source = struct_value(values, *slice)?;
                let source_length = built(builder.build_extract_value(
                    source,
                    2,
                    &format!("v{}.source_length", result.0),
                ))?
                .into_int_value();
                let start = integer_value(values, *start)?;
                let length = integer_value(values, *length)?;
                let start_invalid = built(builder.build_int_compare(
                    IntPredicate::UGT,
                    start,
                    source_length,
                    &format!("v{}.start_invalid", result.0),
                ))?;
                let remaining = built(builder.build_int_sub(
                    source_length,
                    start,
                    &format!("v{}.remaining", result.0),
                ))?;
                let length_invalid = built(builder.build_int_compare(
                    IntPredicate::UGT,
                    length,
                    remaining,
                    &format!("v{}.length_invalid", result.0),
                ))?;
                let invalid = built(builder.build_or(
                    start_invalid,
                    length_invalid,
                    &format!("v{}.out_of_bounds", result.0),
                ))?;
                let continuation = self.context.append_basic_block(
                    builder
                        .get_insert_block()
                        .and_then(|block| block.get_parent())
                        .ok_or_else(|| {
                            BackendError::Builder("builder has no function".to_owned())
                        })?,
                    &format!("v{}.subslice_ok", result.0),
                );
                built(builder.build_conditional_branch(
                    invalid,
                    self.block(blocks, *failure)?,
                    continuation,
                ))?;
                builder.position_at_end(continuation);
                let base = built(builder.build_extract_value(source, 0, "slice.base"))?;
                let data = built(builder.build_extract_value(source, 1, "slice.data"))?
                    .into_pointer_value();
                let item = match self.core.types.get(ty.0 as usize) {
                    Some(Type::Slice(item)) => self.basic_type(*item)?,
                    _ => return Err(BackendError::UnsupportedType(*ty)),
                };
                let new_data =
                    self.element_pointer(builder, item, data, start, "slice.new_data")?;
                let mut output = AggregateValueEnum::StructValue(
                    self.basic_type(*ty)?.into_struct_type().get_undef(),
                );
                let fields: [BasicValueEnum<'ctx>; 3] = [base, new_data.into(), length.into()];
                for (field, value) in fields.into_iter().enumerate() {
                    output = built(builder.build_insert_value(
                        output,
                        value,
                        field as u32,
                        "slice.field",
                    ))?;
                }
                values.insert(*result, output.into_struct_value().into());
            }
            Operation::SliceCopy {
                result,
                slice,
                ty,
                origin,
            } => {
                #[cfg(not(feature = "managed-runtime"))]
                {
                    let _ = (result, slice, ty, origin);
                    return Err(BackendError::UnsupportedOperation {
                        function,
                        block,
                        operation: "slice_copy",
                    });
                }
                #[cfg(feature = "managed-runtime")]
                {
                    let roots = roots.ok_or_else(|| {
                        BackendError::InvalidConcrete(vec![format!(
                            "missing live-root set for collection point {function:?} {block:?}"
                        )])
                    })?;
                    self.preserve_roots(roots, builder, values, slots, root_slots, slot_types)?;
                    let source = struct_value(values, *slice)?;
                    let data = built(builder.build_extract_value(source, 1, "slice.data"))?
                        .into_pointer_value();
                    let length = built(builder.build_extract_value(source, 2, "slice.length"))?
                        .into_int_value();
                    let item_ty = match self.core.types.get(ty.0 as usize) {
                        Some(Type::Slice(item)) => self.basic_type(*item)?,
                        _ => return Err(BackendError::UnsupportedType(*ty)),
                    };
                    let item_size = item_ty
                        .size_of()
                        .ok_or(BackendError::UnsupportedType(*ty))?;
                    let item_size = if item_size.get_type() == length.get_type() {
                        item_size
                    } else {
                        built(builder.build_int_cast(
                            item_size,
                            length.get_type(),
                            "slice.item_size",
                        ))?
                    };
                    let bytes =
                        built(builder.build_int_mul(item_size, length, "slice.copy_bytes"))?;
                    let one = bytes.get_type().const_int(1, false);
                    let is_empty = built(builder.build_int_compare(
                        IntPredicate::EQ,
                        length,
                        length.get_type().const_zero(),
                        "slice.empty",
                    ))?;
                    let allocation_size =
                        built(builder.build_select(is_empty, one, bytes, "slice.allocation_size"))?
                            .into_int_value();
                    let allocation_size = if allocation_size.get_type() == self.context.i64_type() {
                        allocation_size
                    } else {
                        built(builder.build_int_cast(
                            allocation_size,
                            self.context.i64_type(),
                            "slice.bytes.i64",
                        ))?
                    };
                    let source_origin = FailureOrigin::from_span(*origin)
                        .map_err(|()| BackendError::SourceOriginOutOfRange)?;
                    let call = built(
                        builder.build_call(
                            self.allocate_scanned,
                            &[
                                allocation_size.into(),
                                self.context
                                    .i32_type()
                                    .const_int(u64::from(source_origin.file), false)
                                    .into(),
                                self.context
                                    .i64_type()
                                    .const_int(source_origin.start, false)
                                    .into(),
                                self.context
                                    .i64_type()
                                    .const_int(source_origin.end, false)
                                    .into(),
                            ],
                            &format!("v{}.base", result.0),
                        ),
                    )?;
                    let new_base = call
                        .try_as_basic_value()
                        .basic()
                        .ok_or(BackendError::MissingValue(*result))?
                        .into_pointer_value();
                    built(builder.build_memcpy(new_base, 1, data, 1, bytes))?;
                    self.clear_value_roots(roots, builder, root_slots, value_types)?;
                    let mut output = AggregateValueEnum::StructValue(
                        self.basic_type(*ty)?.into_struct_type().get_undef(),
                    );
                    let fields: [BasicValueEnum<'ctx>; 3] =
                        [new_base.into(), new_base.into(), length.into()];
                    for (field, value) in fields.into_iter().enumerate() {
                        output = built(builder.build_insert_value(
                            output,
                            value,
                            field as u32,
                            "slice.field",
                        ))?;
                    }
                    values.insert(*result, output.into_struct_value().into());
                }
            }
            Operation::StringBytes {
                result, string, ty, ..
            } => {
                let source = struct_value(values, *string)?;
                let data =
                    built(builder.build_extract_value(source, 0, &format!("v{}.data", result.0)))?;
                let source_length = built(builder.build_extract_value(
                    source,
                    1,
                    &format!("v{}.source_length", result.0),
                ))?
                .into_int_value();
                let length = if source_length.get_type() == self.usize_type()? {
                    source_length
                } else {
                    built(builder.build_int_cast(
                        source_length,
                        self.usize_type()?,
                        &format!("v{}.length", result.0),
                    ))?
                };
                let mut output = AggregateValueEnum::StructValue(
                    self.basic_type(*ty)?.into_struct_type().get_undef(),
                );
                let fields: [BasicValueEnum<'ctx>; 3] = [data, data, length.into()];
                for (field, value) in fields.into_iter().enumerate() {
                    output = built(builder.build_insert_value(
                        output,
                        value,
                        field as u32,
                        "bytes.field",
                    ))?;
                }
                values.insert(*result, output.into_struct_value().into());
            }
            Operation::BytesFromList {
                result,
                list,
                ty,
                origin,
            } => {
                #[cfg(not(feature = "managed-runtime"))]
                {
                    let _ = (result, list, ty, origin);
                    return Err(BackendError::UnsupportedOperation {
                        function,
                        block,
                        operation: "bytes_from_list",
                    });
                }
                #[cfg(feature = "managed-runtime")]
                {
                    let roots = roots.ok_or_else(|| {
                        BackendError::InvalidConcrete(vec![format!(
                            "missing live-root set for collection point {function:?} {block:?}"
                        )])
                    })?;
                    self.preserve_roots(roots, builder, values, slots, root_slots, slot_types)?;
                    let pointer_ty = self.context.ptr_type(AddressSpace::default());
                    let source = pointer_value(values, *list)?;
                    let source_ty = value_types
                        .get(list)
                        .copied()
                        .ok_or(BackendError::MissingValue(*list))?;
                    let node_ty = self.list_node_type(source_ty)?;
                    let usize_ty = self.usize_type()?;
                    let function_value = builder
                        .get_insert_block()
                        .and_then(|block| block.get_parent())
                        .ok_or_else(|| {
                            BackendError::Builder("builder has no function".to_owned())
                        })?;
                    let preheader = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    let count_loop = self
                        .context
                        .append_basic_block(function_value, &format!("v{}.bytes_count", result.0));
                    let count_body = self.context.append_basic_block(
                        function_value,
                        &format!("v{}.bytes_count_body", result.0),
                    );
                    let allocate_block = self.context.append_basic_block(
                        function_value,
                        &format!("v{}.bytes_allocate", result.0),
                    );
                    built(builder.build_unconditional_branch(count_loop))?;
                    builder.position_at_end(count_loop);
                    let cursor_phi = built(builder.build_phi(pointer_ty, "bytes.count.cursor"))?;
                    let count_phi = built(builder.build_phi(usize_ty, "bytes.count"))?;
                    cursor_phi.add_incoming(&[(&source, preheader)]);
                    count_phi.add_incoming(&[(&usize_ty.const_zero(), preheader)]);
                    let cursor = cursor_phi.as_basic_value().into_pointer_value();
                    let count = count_phi.as_basic_value().into_int_value();
                    let done = built(builder.build_is_null(cursor, "bytes.count.done"))?;
                    built(builder.build_conditional_branch(done, allocate_block, count_body))?;
                    builder.position_at_end(count_body);
                    let next_ptr = built(builder.build_struct_gep(
                        node_ty,
                        cursor,
                        1,
                        "bytes.count.next_ptr",
                    ))?;
                    let next = built(builder.build_load(pointer_ty, next_ptr, "bytes.count.next"))?
                        .into_pointer_value();
                    let next_count = built(builder.build_int_add(
                        count,
                        usize_ty.const_int(1, false),
                        "bytes.count.next_count",
                    ))?;
                    let count_body_end = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    built(builder.build_unconditional_branch(count_loop))?;
                    cursor_phi.add_incoming(&[(&next, count_body_end)]);
                    count_phi.add_incoming(&[(&next_count, count_body_end)]);

                    builder.position_at_end(allocate_block);
                    let one = usize_ty.const_int(1, false);
                    let empty = built(builder.build_int_compare(
                        IntPredicate::EQ,
                        count,
                        usize_ty.const_zero(),
                        "bytes.empty",
                    ))?;
                    let allocation_size =
                        built(builder.build_select(empty, one, count, "bytes.allocation_size"))?
                            .into_int_value();
                    let allocation_size = if allocation_size.get_type() == self.context.i64_type() {
                        allocation_size
                    } else {
                        built(builder.build_int_cast(
                            allocation_size,
                            self.context.i64_type(),
                            "bytes.allocation_size.i64",
                        ))?
                    };
                    let source_origin = FailureOrigin::from_span(*origin)
                        .map_err(|()| BackendError::SourceOriginOutOfRange)?;
                    let call = built(
                        builder.build_call(
                            self.allocate_atomic,
                            &[
                                allocation_size.into(),
                                self.context
                                    .i32_type()
                                    .const_int(u64::from(source_origin.file), false)
                                    .into(),
                                self.context
                                    .i64_type()
                                    .const_int(source_origin.start, false)
                                    .into(),
                                self.context
                                    .i64_type()
                                    .const_int(source_origin.end, false)
                                    .into(),
                            ],
                            &format!("v{}.base", result.0),
                        ),
                    )?;
                    let base = call
                        .try_as_basic_value()
                        .basic()
                        .ok_or(BackendError::MissingValue(*result))?
                        .into_pointer_value();

                    let copy_loop = self
                        .context
                        .append_basic_block(function_value, &format!("v{}.bytes_copy", result.0));
                    let copy_body = self.context.append_basic_block(
                        function_value,
                        &format!("v{}.bytes_copy_body", result.0),
                    );
                    let copy_done = self.context.append_basic_block(
                        function_value,
                        &format!("v{}.bytes_copy_done", result.0),
                    );
                    let allocation_end = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    built(builder.build_unconditional_branch(copy_loop))?;
                    builder.position_at_end(copy_loop);
                    let copy_cursor_phi =
                        built(builder.build_phi(pointer_ty, "bytes.copy.cursor"))?;
                    let index_phi = built(builder.build_phi(usize_ty, "bytes.copy.index"))?;
                    copy_cursor_phi.add_incoming(&[(&source, allocation_end)]);
                    index_phi.add_incoming(&[(&usize_ty.const_zero(), allocation_end)]);
                    let copy_cursor = copy_cursor_phi.as_basic_value().into_pointer_value();
                    let copy_finished =
                        built(builder.build_is_null(copy_cursor, "bytes.copy.done"))?;
                    built(builder.build_conditional_branch(copy_finished, copy_done, copy_body))?;
                    builder.position_at_end(copy_body);
                    let item_ptr = built(builder.build_struct_gep(
                        node_ty,
                        copy_cursor,
                        0,
                        "bytes.copy.item_ptr",
                    ))?;
                    let byte = built(builder.build_load(
                        self.context.i8_type(),
                        item_ptr,
                        "bytes.copy.item",
                    ))?;
                    let destination = self.element_pointer(
                        builder,
                        self.context.i8_type().into(),
                        base,
                        index_phi.as_basic_value().into_int_value(),
                        "bytes.copy.destination",
                    )?;
                    built(builder.build_store(destination, byte))?;
                    let next_ptr = built(builder.build_struct_gep(
                        node_ty,
                        copy_cursor,
                        1,
                        "bytes.copy.next_ptr",
                    ))?;
                    let next = built(builder.build_load(pointer_ty, next_ptr, "bytes.copy.next"))?
                        .into_pointer_value();
                    let next_index = built(builder.build_int_add(
                        index_phi.as_basic_value().into_int_value(),
                        usize_ty.const_int(1, false),
                        "bytes.copy.next_index",
                    ))?;
                    let copy_body_end = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    built(builder.build_unconditional_branch(copy_loop))?;
                    copy_cursor_phi.add_incoming(&[(&next, copy_body_end)]);
                    index_phi.add_incoming(&[(&next_index, copy_body_end)]);
                    builder.position_at_end(copy_done);
                    self.clear_value_roots(roots, builder, root_slots, value_types)?;
                    let mut output = AggregateValueEnum::StructValue(
                        self.basic_type(*ty)?.into_struct_type().get_undef(),
                    );
                    for (field, value) in [
                        BasicValueEnum::from(base),
                        BasicValueEnum::from(base),
                        BasicValueEnum::from(count),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        output = built(builder.build_insert_value(
                            output,
                            value,
                            field as u32,
                            "bytes.field",
                        ))?;
                    }
                    values.insert(*result, output.into_struct_value().into());
                }
            }
            Operation::BytesToList {
                result,
                bytes,
                ty,
                origin,
            } => {
                #[cfg(not(feature = "managed-runtime"))]
                {
                    let _ = (result, bytes, ty, origin);
                    return Err(BackendError::UnsupportedOperation {
                        function,
                        block,
                        operation: "bytes_to_list",
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
                        BackendError::Builder("bytes to-list has no partial-result root".to_owned())
                    })?;
                    self.preserve_roots(roots, builder, values, slots, root_slots, slot_types)?;
                    let pointer_ty = self.context.ptr_type(AddressSpace::default());
                    let null = pointer_ty.const_null();
                    set_volatile(built(builder.build_store(partial, null))?)?;
                    let source = struct_value(values, *bytes)?;
                    let data = built(builder.build_extract_value(source, 1, "bytes.to_list.data"))?
                        .into_pointer_value();
                    let length =
                        built(builder.build_extract_value(source, 2, "bytes.to_list.length"))?
                            .into_int_value();
                    let function_value = builder
                        .get_insert_block()
                        .and_then(|block| block.get_parent())
                        .ok_or_else(|| {
                            BackendError::Builder("builder has no function".to_owned())
                        })?;
                    let preheader = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    let loop_block = self.context.append_basic_block(
                        function_value,
                        &format!("v{}.bytes_to_list_loop", result.0),
                    );
                    let body_block = self.context.append_basic_block(
                        function_value,
                        &format!("v{}.bytes_to_list_body", result.0),
                    );
                    let done_block = self.context.append_basic_block(
                        function_value,
                        &format!("v{}.bytes_to_list_done", result.0),
                    );
                    built(builder.build_unconditional_branch(loop_block))?;
                    builder.position_at_end(loop_block);
                    let index_phi =
                        built(builder.build_phi(length.get_type(), "bytes.to_list.index"))?;
                    let head_phi = built(builder.build_phi(pointer_ty, "bytes.to_list.head"))?;
                    index_phi.add_incoming(&[(&length, preheader)]);
                    head_phi.add_incoming(&[(&null, preheader)]);
                    let index = index_phi.as_basic_value().into_int_value();
                    let head = head_phi.as_basic_value().into_pointer_value();
                    let done = built(builder.build_int_compare(
                        IntPredicate::EQ,
                        index,
                        index.get_type().const_zero(),
                        "bytes.to_list.empty",
                    ))?;
                    built(builder.build_conditional_branch(done, done_block, body_block))?;
                    builder.position_at_end(body_block);
                    let source_index = built(builder.build_int_sub(
                        index,
                        index.get_type().const_int(1, false),
                        "bytes.to_list.source_index",
                    ))?;
                    let source_ptr = self.element_pointer(
                        builder,
                        self.context.i8_type().into(),
                        data,
                        source_index,
                        "bytes.to_list.source_ptr",
                    )?;
                    let byte = built(builder.build_load(
                        self.context.i8_type(),
                        source_ptr,
                        "bytes.to_list.byte",
                    ))?;
                    let node_ty = self.list_node_type(*ty)?;
                    let native_size = node_ty
                        .size_of()
                        .ok_or(BackendError::UnsupportedType(*ty))?;
                    let size = if native_size.get_type() == self.context.i64_type() {
                        native_size
                    } else {
                        built(builder.build_int_cast(
                            native_size,
                            self.context.i64_type(),
                            "bytes.to_list.node_size",
                        ))?
                    };
                    let source_origin = FailureOrigin::from_span(*origin)
                        .map_err(|()| BackendError::SourceOriginOutOfRange)?;
                    let call = built(
                        builder.build_call(
                            self.allocate_scanned,
                            &[
                                size.into(),
                                self.context
                                    .i32_type()
                                    .const_int(u64::from(source_origin.file), false)
                                    .into(),
                                self.context
                                    .i64_type()
                                    .const_int(source_origin.start, false)
                                    .into(),
                                self.context
                                    .i64_type()
                                    .const_int(source_origin.end, false)
                                    .into(),
                            ],
                            &format!("v{}.node", result.0),
                        ),
                    )?;
                    let node = call
                        .try_as_basic_value()
                        .basic()
                        .ok_or(BackendError::MissingValue(*result))?
                        .into_pointer_value();
                    let item_ptr = built(builder.build_struct_gep(
                        node_ty,
                        node,
                        0,
                        "bytes.to_list.item_ptr",
                    ))?;
                    let next_ptr = built(builder.build_struct_gep(
                        node_ty,
                        node,
                        1,
                        "bytes.to_list.next_ptr",
                    ))?;
                    built(builder.build_store(item_ptr, byte))?;
                    built(builder.build_store(next_ptr, head))?;
                    set_volatile(built(builder.build_store(partial, node))?)?;
                    let body_end = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    built(builder.build_unconditional_branch(loop_block))?;
                    index_phi.add_incoming(&[(&source_index, body_end)]);
                    head_phi.add_incoming(&[(&node, body_end)]);
                    builder.position_at_end(done_block);
                    let output = head_phi.as_basic_value().into_pointer_value();
                    self.clear_value_roots(roots, builder, root_slots, value_types)?;
                    set_volatile(built(builder.build_store(partial, null))?)?;
                    values.insert(*result, output.into());
                }
            }
            Operation::BytesSlice {
                result,
                bytes,
                start,
                length,
                failure,
                ty,
                ..
            } => {
                let source = struct_value(values, *bytes)?;
                let source_length = built(builder.build_extract_value(
                    source,
                    2,
                    &format!("v{}.source_length", result.0),
                ))?
                .into_int_value();
                let start = integer_value(values, *start)?;
                let length = integer_value(values, *length)?;
                let start_invalid = built(builder.build_int_compare(
                    IntPredicate::UGT,
                    start,
                    source_length,
                    &format!("v{}.start_invalid", result.0),
                ))?;
                let remaining = built(builder.build_int_sub(
                    source_length,
                    start,
                    &format!("v{}.remaining", result.0),
                ))?;
                let length_invalid = built(builder.build_int_compare(
                    IntPredicate::UGT,
                    length,
                    remaining,
                    &format!("v{}.length_invalid", result.0),
                ))?;
                let invalid = built(builder.build_or(
                    start_invalid,
                    length_invalid,
                    &format!("v{}.out_of_bounds", result.0),
                ))?;
                let continuation = self.context.append_basic_block(
                    builder
                        .get_insert_block()
                        .and_then(|block| block.get_parent())
                        .ok_or_else(|| {
                            BackendError::Builder("builder has no function".to_owned())
                        })?,
                    &format!("v{}.bytes_slice_ok", result.0),
                );
                built(builder.build_conditional_branch(
                    invalid,
                    self.block(blocks, *failure)?,
                    continuation,
                ))?;
                builder.position_at_end(continuation);
                let base = built(builder.build_extract_value(source, 0, "bytes.base"))?;
                let data = built(builder.build_extract_value(source, 1, "bytes.data"))?
                    .into_pointer_value();
                let new_data = self.element_pointer(
                    builder,
                    self.context.i8_type().into(),
                    data,
                    start,
                    "bytes.new_data",
                )?;
                let mut output = AggregateValueEnum::StructValue(
                    self.basic_type(*ty)?.into_struct_type().get_undef(),
                );
                let fields: [BasicValueEnum<'ctx>; 3] = [base, new_data.into(), length.into()];
                for (field, value) in fields.into_iter().enumerate() {
                    output = built(builder.build_insert_value(
                        output,
                        value,
                        field as u32,
                        "bytes.field",
                    ))?;
                }
                values.insert(*result, output.into_struct_value().into());
            }
            Operation::CollectionLength {
                result,
                value,
                known_length,
                ..
            } => {
                let source_ty = value_types
                    .get(value)
                    .copied()
                    .ok_or(BackendError::MissingValue(*value))?;
                let length = if let Some(length) = known_length {
                    self.usize_type()?.const_int(*length, false)
                } else if matches!(
                    self.core.types.get(source_ty.0 as usize),
                    Some(Type::Map { .. })
                ) {
                    let llvm_function = builder
                        .get_insert_block()
                        .and_then(|block| block.get_parent())
                        .ok_or_else(|| {
                            BackendError::Builder("builder has no function".to_owned())
                        })?;
                    let preheader = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    let loop_block = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.size_loop", result.0));
                    let body_block = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.size_body", result.0));
                    let done_block = self
                        .context
                        .append_basic_block(llvm_function, &format!("v{}.size_done", result.0));
                    built(builder.build_unconditional_branch(loop_block))?;
                    builder.position_at_end(loop_block);
                    let cursor = built(builder.build_phi(
                        self.context.ptr_type(AddressSpace::default()),
                        &format!("v{}.cursor", result.0),
                    ))?;
                    let count = built(
                        builder.build_phi(self.usize_type()?, &format!("v{}.count", result.0)),
                    )?;
                    let initial = pointer_value(values, *value)?;
                    let zero = self.usize_type()?.const_zero();
                    cursor.add_incoming(&[(&initial, preheader)]);
                    count.add_incoming(&[(&zero, preheader)]);
                    let empty = built(builder.build_is_null(
                        cursor.as_basic_value().into_pointer_value(),
                        &format!("v{}.empty", result.0),
                    ))?;
                    built(builder.build_conditional_branch(empty, done_block, body_block))?;
                    builder.position_at_end(body_block);
                    let node_type = self.map_node_type(source_ty)?;
                    let next_pointer = built(builder.build_struct_gep(
                        node_type,
                        cursor.as_basic_value().into_pointer_value(),
                        3,
                        &format!("v{}.next_ptr", result.0),
                    ))?;
                    let next = built(builder.build_load(
                        self.context.ptr_type(AddressSpace::default()),
                        next_pointer,
                        &format!("v{}.next", result.0),
                    ))?
                    .into_pointer_value();
                    let next_count = built(builder.build_int_add(
                        count.as_basic_value().into_int_value(),
                        self.usize_type()?.const_int(1, false),
                        &format!("v{}.next_count", result.0),
                    ))?;
                    let body_end = builder
                        .get_insert_block()
                        .ok_or_else(|| BackendError::Builder("builder has no block".to_owned()))?;
                    built(builder.build_unconditional_branch(loop_block))?;
                    cursor.add_incoming(&[(&next, body_end)]);
                    count.add_incoming(&[(&next_count, body_end)]);
                    builder.position_at_end(done_block);
                    count.as_basic_value().into_int_value()
                } else if matches!(
                    self.core.types.get(source_ty.0 as usize),
                    Some(Type::String)
                ) {
                    built(builder.build_extract_value(
                        struct_value(values, *value)?,
                        1,
                        &format!("v{}", result.0),
                    ))?
                    .into_int_value()
                } else {
                    built(builder.build_extract_value(
                        struct_value(values, *value)?,
                        2,
                        &format!("v{}", result.0),
                    ))?
                    .into_int_value()
                };
                values.insert(*result, length.into());
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
                operand_ty,
                ..
            } => {
                if matches!(
                    operator,
                    ComparisonOperator::Equal | ComparisonOperator::NotEqual
                ) && !matches!(
                    self.core.types.get(operand_ty.0 as usize),
                    Some(Type::I32 | Type::I64 | Type::Usize | Type::U8 | Type::Bool)
                ) {
                    #[cfg(not(feature = "managed-runtime"))]
                    return Err(BackendError::UnsupportedOperation {
                        function,
                        block,
                        operation: "structural_equal",
                    });
                    #[cfg(feature = "managed-runtime")]
                    {
                        let equal = self.map_key_equal(
                            builder,
                            value(values, *left)?,
                            value(values, *right)?,
                            *operand_ty,
                            &format!("v{}.equal", result.0),
                        )?;
                        let compared = if matches!(operator, ComparisonOperator::NotEqual) {
                            built(builder.build_not(equal, &format!("v{}", result.0)))?
                        } else {
                            equal
                        };
                        values.insert(*result, compared.into());
                        return Ok(());
                    }
                }
                let unsigned = matches!(
                    self.core.types.get(operand_ty.0 as usize),
                    Some(Type::U8 | Type::Usize)
                );
                let predicate = match operator {
                    ComparisonOperator::Equal => IntPredicate::EQ,
                    ComparisonOperator::NotEqual => IntPredicate::NE,
                    ComparisonOperator::Less if unsigned => IntPredicate::ULT,
                    ComparisonOperator::LessEqual if unsigned => IntPredicate::ULE,
                    ComparisonOperator::Greater if unsigned => IntPredicate::UGT,
                    ComparisonOperator::GreaterEqual if unsigned => IntPredicate::UGE,
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
                    CoreFailureCategory::IndexOutOfBounds => FailureCategory::IndexOutOfBounds,
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
            (Constant::Integer(value), Some(Type::U8)) => {
                let value = u8::try_from(*value)
                    .map_err(|_| BackendError::IntegerOutOfRange { value: *value, ty })?;
                Ok(self
                    .context
                    .i8_type()
                    .const_int(u64::from(value), false)
                    .into())
            }
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
            (Constant::Integer(value), Some(Type::Usize)) => {
                let value = usize::try_from(*value)
                    .map_err(|_| BackendError::IntegerOutOfRange { value: *value, ty })?;
                Ok(self.usize_type()?.const_int(value as u64, false).into())
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
                    | Operation::ListReverse { result, ty, .. }
                    | Operation::Array { result, ty, .. }
                    | Operation::ArrayIndex { result, ty, .. }
                    | Operation::SliceFromArray { result, ty, .. }
                    | Operation::SliceSubslice { result, ty, .. }
                    | Operation::SliceCopy { result, ty, .. }
                    | Operation::StringBytes { result, ty, .. }
                    | Operation::BytesFromList { result, ty, .. }
                    | Operation::BytesToList { result, ty, .. }
                    | Operation::BytesSlice { result, ty, .. }
                    | Operation::CollectionLength { result, ty, .. }
                    | Operation::Map { result, ty, .. }
                    | Operation::MapPut { result, ty, .. }
                    | Operation::MapRemove { result, ty, .. }
                    | Operation::MapFetch { result, ty, .. }
                    | Operation::MapToList { result, ty, .. }
                    | Operation::Tuple { result, ty, .. }
                    | Operation::Struct { result, ty, .. }
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
    fn lowers_fixed_arrays_usize_indices_and_bounds_failures() {
        let core = concrete(
            "defmodule Main do\n  def get(values: [i32; 2], index: usize) -> i32 do\n    values[index]\n  end\n  def main() -> i32 do\n    get(#[40, 2], 1)\n  end\nend\n",
        );

        let llvm = lower_to_llvm_ir(&core).expect("array indexing lowers and verifies");
        let text = llvm.as_str();
        assert!(text.contains("{ i32, i32 }"));
        assert!(text.contains("icmp uge i64"));
        assert!(text.contains("call void @__el_runtime_fail(i32 5"));
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
    fn lowers_struct_values_projections_and_reconstruction() {
        let core = concrete(
            "defmodule Main do\n  defstruct Pair(a) do\n    first: a\n    second: i32\n  end\n  def main() -> i32 do\n    mut pair: Pair(i32) = %Pair{second: 2, first: 40}\n    copy = pair\n    pair.second := pair.second + copy.second\n    pair.first + pair.second\n  end\nend\n",
        );

        let llvm = lower_to_llvm_ir(&core).expect("struct lowering verifies");
        let text = llvm.as_str();
        assert!(text.contains("insertvalue { i32, i32 }"), "{text}");
        assert!(text.contains("extractvalue { i32, i32 }"), "{text}");
        assert!(text.contains("store { i32, i32 }"), "{text}");
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

    #[cfg(feature = "managed-runtime")]
    #[test]
    fn lowers_list_reverse_with_a_rooted_partial_result_loop() {
        let core = concrete(
            "defmodule Main do\n  def main() -> i32 do\n    reversed: [i32] = List.reverse([1, 42])\n    match reversed do\n      [value | _] -> value\n      [] -> 0\n    end\n  end\nend\n",
        );

        let llvm = lower_to_llvm_ir(&core).expect("list reverse lowers and verifies");
        let text = llvm.as_str();
        assert!(text.contains("reverse_loop"), "{text}");
        assert!(text.contains("reverse_body"), "{text}");
        assert!(text.contains("gc.partial"), "{text}");
        assert!(
            text.matches("call ptr @__el_runtime_alloc_scanned").count() >= 2,
            "{text}"
        );
    }

    #[cfg(feature = "managed-runtime")]
    #[test]
    fn lowers_seeded_composite_maps_order_views_and_structural_equality() {
        let core = concrete(
            "defmodule Main do\n  def main() -> i32 do\n    first: Map({i32, i32}, i32) = %{{1, 2} => 10, {3, 4} => 20}\n    second: Map({i32, i32}, i32) = %{{3, 4} => 20, {1, 2} => 10}\n    ordered = Enum.to_list(first)\n    if first == second do\n      42\n    else\n      0\n    end\n  end\nend\n",
        );
        let llvm = lower_to_llvm_ir(&core).expect("seeded composite maps lower and verify");
        let text = llvm.as_str();
        assert!(text.contains("@__el_runtime_hash_seed"), "{text}");
        assert!(text.contains("to_list_loop"), "{text}");
        assert!(text.contains("right_search"), "{text}");
    }

    #[cfg(feature = "managed-runtime")]
    #[test]
    fn lowers_managed_slice_views_copy_and_checked_indexing() {
        let core = concrete(
            "defmodule Main do\n  def main() -> i32 do\n    array: [i32; 3] = #[10, 20, 30]\n    whole = Slice.from_array(array)\n    part = Slice.subslice(whole, 1, 2)\n    copy = Slice.copy(part)\n    copy[1]\n  end\nend\n",
        );

        let llvm = lower_to_llvm_ir(&core).expect("managed slices lower and verify");
        let text = llvm.as_str();
        assert!(
            text.matches("call ptr @__el_runtime_alloc_scanned").count() >= 2,
            "{text}"
        );
        assert!(text.contains("slice.new_data"), "{text}");
        assert!(text.contains("llvm.memcpy"), "{text}");
        assert!(
            text.contains("call void @__el_runtime_fail(i32 5"),
            "{text}"
        );
    }

    #[cfg(feature = "managed-runtime")]
    #[test]
    fn lowers_fresh_bytes_list_conversions_with_correct_scan_classes() {
        let core = concrete(
            "defmodule Main do\n  def main() -> i32 do\n    values = Bytes.to_list(Bytes.from_list([0, 127, 255]))\n    if values == [0, 127, 255] do\n      42\n    else\n      0\n    end\n  end\nend\n",
        );

        let llvm = lower_to_llvm_ir(&core).expect("byte/list conversions lower and verify");
        let text = llvm.as_str();
        assert!(
            text.contains("call ptr @__el_runtime_alloc_atomic"),
            "{text}"
        );
        assert!(text.contains("bytes_count"), "{text}");
        assert!(text.contains("bytes_copy"), "{text}");
        assert!(text.contains("bytes_to_list_loop"), "{text}");
        assert!(
            text.contains("call ptr @__el_runtime_alloc_scanned"),
            "{text}"
        );
        assert!(text.contains("gc.partial"), "{text}");
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
