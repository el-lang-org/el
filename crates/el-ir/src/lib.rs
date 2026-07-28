//! Shared Generic Core IR, initial lowering, and representation verification.

use el_resolve::{DeclId, ImplId, SymbolId, Visibility};
use el_span::Span;
pub use el_types::{ArithmeticOperator, BufferAppendKind, ComparisonOperator, Type, TypeId};
use el_types::{
    LogicalOperator, TypedExpr, TypedExprKind, TypedItem, TypedPatternKind, TypedProgram,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FunctionId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlockId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ValueId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SlotId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CoreFailureCategory {
    IntegerOverflow,
    DivisionByZero,
    IndexOutOfBounds,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenericModule {
    pub types: Vec<Type>,
    pub structs: Vec<CoreStruct>,
    pub implementations: Vec<CoreImplementation>,
    pub functions: Vec<CoreFunction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoreImplementation {
    pub id: ImplId,
    pub protocol: String,
    pub target: TypeId,
    pub associated_types: Vec<(String, TypeId)>,
    pub methods: Vec<String>,
    pub origin: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoreStruct {
    pub declaration: DeclId,
    pub name: String,
    pub parameters: Vec<TypeId>,
    pub fields: Vec<(String, TypeId)>,
    pub origin: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoreFunction {
    pub id: FunctionId,
    pub declaration: DeclId,
    pub module_name: String,
    pub name: String,
    pub visibility: Visibility,
    pub span: Span,
    pub parameters: Vec<CoreParameter>,
    pub type_parameters: Vec<TypeId>,
    pub specialization_arguments: Vec<TypeId>,
    pub constraints: Vec<(TypeId, String)>,
    pub result: TypeId,
    pub slots: Vec<Slot>,
    pub blocks: Vec<Block>,
}

impl CoreFunction {
    /// Whether source resolution marked this function as module-public.
    ///
    /// Later compiler stages can enforce an exported-entry invariant without
    /// depending directly on resolver representation types.
    #[must_use]
    pub fn is_exported(&self) -> bool {
        self.visibility == Visibility::Public
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoreParameter {
    pub value: ValueId,
    pub ty: TypeId,
    pub origin: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Slot {
    pub id: SlotId,
    pub ty: TypeId,
    pub origin: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    pub id: BlockId,
    pub parameters: Vec<CoreParameter>,
    pub operations: Vec<Operation>,
    pub terminator: Terminator,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Operation {
    Constant {
        result: ValueId,
        constant: Constant,
        ty: TypeId,
        origin: Span,
    },
    List {
        result: ValueId,
        elements: Vec<ValueId>,
        tail: Option<ValueId>,
        ty: TypeId,
        origin: Span,
    },
    ListReverse {
        result: ValueId,
        list: ValueId,
        ty: TypeId,
        origin: Span,
    },
    Array {
        result: ValueId,
        elements: Vec<ValueId>,
        ty: TypeId,
        origin: Span,
    },
    ArrayIndex {
        result: ValueId,
        array: ValueId,
        index: ValueId,
        length: Option<u64>,
        failure: BlockId,
        ty: TypeId,
        origin: Span,
    },
    SliceFromArray {
        result: ValueId,
        array: ValueId,
        length: u64,
        ty: TypeId,
        origin: Span,
    },
    SliceSubslice {
        result: ValueId,
        slice: ValueId,
        start: ValueId,
        length: ValueId,
        failure: BlockId,
        ty: TypeId,
        origin: Span,
    },
    SliceCopy {
        result: ValueId,
        slice: ValueId,
        ty: TypeId,
        origin: Span,
    },
    StringBytes {
        result: ValueId,
        string: ValueId,
        ty: TypeId,
        origin: Span,
    },
    StringCodepoints {
        result: ValueId,
        string: ValueId,
        ty: TypeId,
        origin: Span,
    },
    StringFromBytes {
        result: ValueId,
        bytes: ValueId,
        ty: TypeId,
        origin: Span,
    },
    Utf8ErrorOffset {
        result: ValueId,
        error: ValueId,
        ty: TypeId,
        origin: Span,
    },
    RuneToString {
        result: ValueId,
        rune: ValueId,
        ty: TypeId,
        origin: Span,
    },
    BufferNew {
        result: ValueId,
        ty: TypeId,
        origin: Span,
    },
    BufferAppend {
        result: ValueId,
        buffer: ValueId,
        value: ValueId,
        kind: BufferAppendKind,
        ty: TypeId,
        origin: Span,
    },
    BufferToBytes {
        result: ValueId,
        buffer: ValueId,
        ty: TypeId,
        origin: Span,
    },
    BytesToBits {
        result: ValueId,
        bytes: ValueId,
        ty: TypeId,
        origin: Span,
    },
    BitsSlice {
        result: ValueId,
        bits: ValueId,
        start: ValueId,
        length: ValueId,
        failure: BlockId,
        ty: TypeId,
        origin: Span,
    },
    BitsToBytes {
        result: ValueId,
        bits: ValueId,
        ty: TypeId,
        origin: Span,
    },
    BytesFromList {
        result: ValueId,
        list: ValueId,
        ty: TypeId,
        origin: Span,
    },
    BytesToList {
        result: ValueId,
        bytes: ValueId,
        ty: TypeId,
        origin: Span,
    },
    BytesSlice {
        result: ValueId,
        bytes: ValueId,
        start: ValueId,
        length: ValueId,
        failure: BlockId,
        ty: TypeId,
        origin: Span,
    },
    CollectionLength {
        result: ValueId,
        value: ValueId,
        known_length: Option<u64>,
        ty: TypeId,
        origin: Span,
    },
    Map {
        result: ValueId,
        entries: Vec<(ValueId, ValueId)>,
        ty: TypeId,
        origin: Span,
    },
    MapPut {
        result: ValueId,
        map: ValueId,
        key: ValueId,
        value: ValueId,
        ty: TypeId,
        origin: Span,
    },
    MapRemove {
        result: ValueId,
        map: ValueId,
        key: ValueId,
        ty: TypeId,
        origin: Span,
    },
    MapFetch {
        result: ValueId,
        map: ValueId,
        key: ValueId,
        map_ty: TypeId,
        ty: TypeId,
        origin: Span,
    },
    MapToList {
        result: ValueId,
        map: ValueId,
        map_ty: TypeId,
        ty: TypeId,
        origin: Span,
    },
    Tuple {
        result: ValueId,
        elements: Vec<ValueId>,
        ty: TypeId,
        origin: Span,
    },
    Struct {
        result: ValueId,
        declaration: DeclId,
        fields: Vec<(usize, ValueId)>,
        ty: TypeId,
        origin: Span,
    },
    TupleProject {
        result: ValueId,
        tuple: ValueId,
        index: usize,
        ty: TypeId,
        origin: Span,
    },
    StructProject {
        result: ValueId,
        structure: ValueId,
        declaration: DeclId,
        index: usize,
        ty: TypeId,
        origin: Span,
    },
    ListHead {
        result: ValueId,
        list: ValueId,
        ty: TypeId,
        origin: Span,
    },
    ListTail {
        result: ValueId,
        list: ValueId,
        ty: TypeId,
        origin: Span,
    },
    CheckedArithmetic {
        result: ValueId,
        operator: ArithmeticOperator,
        left: ValueId,
        right: ValueId,
        failures: Vec<(CoreFailureCategory, BlockId)>,
        ty: TypeId,
        origin: Span,
    },
    Compare {
        result: ValueId,
        operator: ComparisonOperator,
        left: ValueId,
        right: ValueId,
        operand_ty: TypeId,
        origin: Span,
    },
    Call {
        result: ValueId,
        function: FunctionId,
        substitutions: Vec<(TypeId, TypeId)>,
        arguments: Vec<ValueId>,
        ty: TypeId,
        origin: Span,
    },
    UnionInject {
        result: ValueId,
        member: TypeId,
        value: ValueId,
        ty: TypeId,
        origin: Span,
    },
    UnionProject {
        result: ValueId,
        member: TypeId,
        value: ValueId,
        union_ty: TypeId,
        ty: TypeId,
        origin: Span,
    },
    Load {
        result: ValueId,
        slot: SlotId,
        ty: TypeId,
        origin: Span,
    },
    Store {
        slot: SlotId,
        value: ValueId,
        origin: Span,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Constant {
    Integer(i128),
    Boolean(bool),
    Unit,
    String(String),
    Rune(char),
    Atom(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Terminator {
    Branch {
        target: BlockId,
        arguments: Vec<ValueId>,
        origin: Span,
    },
    CondBranch {
        condition: ValueId,
        then_target: BlockId,
        else_target: BlockId,
        origin: Span,
    },
    Switch {
        subject: ValueId,
        subject_ty: TypeId,
        cases: Vec<(SwitchValue, BlockId)>,
        default: Option<BlockId>,
        origin: Span,
    },
    Return {
        value: ValueId,
        origin: Span,
    },
    Failure {
        category: CoreFailureCategory,
        origin: Span,
    },
    Unreachable {
        origin: Span,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SwitchValue {
    Boolean(bool),
    Integer(i128),
    Atom(String),
    UnionMember(TypeId),
    ListEmpty,
    ListCons,
}

#[derive(Clone, Copy, Debug)]
enum Binding {
    Value(ValueId),
    Slot(SlotId),
}

#[derive(Clone)]
enum CleanupAction<'a> {
    Call {
        function: DeclId,
        substitutions: &'a [(TypeId, TypeId)],
        arguments: Vec<ValueId>,
        origin: Span,
    },
    Block {
        captures: BTreeMap<SymbolId, Binding>,
        body: &'a el_types::TypedBlock,
    },
}

fn pattern_arguments(bindings: &BTreeMap<SymbolId, (ValueId, TypeId)>) -> Vec<ValueId> {
    bindings.values().map(|(value, _)| *value).collect()
}

fn arithmetic_failure_categories(operator: ArithmeticOperator) -> &'static [CoreFailureCategory] {
    match operator {
        ArithmeticOperator::Add | ArithmeticOperator::Subtract | ArithmeticOperator::Multiply => {
            &[CoreFailureCategory::IntegerOverflow]
        }
        ArithmeticOperator::Divide | ArithmeticOperator::Remainder => &[
            CoreFailureCategory::DivisionByZero,
            CoreFailureCategory::IntegerOverflow,
        ],
    }
}

/// Lowers verified Typed AST while preserving source evaluation order.
#[must_use]
pub fn lower(program: &TypedProgram) -> GenericModule {
    let functions_by_decl = program
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.id, FunctionId(index as u32)))
        .collect::<BTreeMap<_, _>>();
    let functions = program
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| {
            Lowerer::new(
                function,
                FunctionId(index as u32),
                &functions_by_decl,
                &program.types,
            )
            .lower()
        })
        .collect();
    let module = GenericModule {
        types: program.types.clone(),
        structs: program
            .structs
            .iter()
            .map(|structure| CoreStruct {
                declaration: structure.id,
                name: structure.name.clone(),
                parameters: structure.parameters.clone(),
                fields: structure
                    .fields
                    .iter()
                    .map(|field| (field.name.clone(), field.ty))
                    .collect(),
                origin: structure.span,
            })
            .collect(),
        implementations: program
            .implementations
            .iter()
            .map(|implementation| CoreImplementation {
                id: implementation.id,
                protocol: implementation.protocol.clone(),
                target: implementation.target,
                associated_types: implementation.associated_types.clone(),
                methods: implementation.methods.clone(),
                origin: implementation.span,
            })
            .collect(),
        functions,
    };
    if let Err(errors) = verify(&module) {
        panic!("lowering produced invalid Generic Core IR: {errors:?}");
    }
    module
}

struct Lowerer<'a> {
    function: &'a el_types::TypedFunction,
    id: FunctionId,
    functions: &'a BTreeMap<DeclId, FunctionId>,
    types: &'a [Type],
    next_value: u32,
    bindings: BTreeMap<SymbolId, Binding>,
    slots: Vec<Slot>,
    blocks: Vec<Block>,
    current_block: BlockId,
    current_parameters: Vec<CoreParameter>,
    next_block: u32,
    operations: Vec<Operation>,
    cleanup_scopes: Vec<Vec<CleanupAction<'a>>>,
}

impl<'a> Lowerer<'a> {
    fn new(
        function: &'a el_types::TypedFunction,
        id: FunctionId,
        functions: &'a BTreeMap<DeclId, FunctionId>,
        types: &'a [Type],
    ) -> Self {
        Self {
            function,
            id,
            functions,
            types,
            next_value: function.parameters.len() as u32,
            bindings: BTreeMap::new(),
            slots: Vec::new(),
            blocks: Vec::new(),
            current_block: BlockId(0),
            current_parameters: Vec::new(),
            next_block: 1,
            operations: Vec::new(),
            cleanup_scopes: Vec::new(),
        }
    }

    fn lower(mut self) -> CoreFunction {
        let parameters = self
            .function
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter)| {
                let value = ValueId(index as u32);
                self.bindings
                    .insert(parameter.symbol, Binding::Value(value));
                CoreParameter {
                    value,
                    ty: parameter.ty,
                    origin: parameter.span,
                }
            })
            .collect();
        self.cleanup_scopes.push(Vec::new());
        let mut result = None;
        let mut return_origin = self.function.body.span;
        for item in &self.function.body.items {
            match item {
                TypedItem::Let {
                    symbol,
                    mutable,
                    ty,
                    initializer,
                    span,
                    ..
                } => {
                    let Some(value) = self.lower_expr(initializer) else {
                        break;
                    };
                    if *mutable {
                        let slot = SlotId(self.slots.len() as u32);
                        self.slots.push(Slot {
                            id: slot,
                            ty: *ty,
                            origin: *span,
                        });
                        self.operations.push(Operation::Store {
                            slot,
                            value,
                            origin: *span,
                        });
                        self.bindings.insert(*symbol, Binding::Slot(slot));
                    } else {
                        self.bindings.insert(*symbol, Binding::Value(value));
                    }
                }
                TypedItem::Assign {
                    symbol,
                    value,
                    span,
                } => {
                    let Some(lowered) = self.lower_expr(value) else {
                        break;
                    };
                    let Binding::Slot(slot) = self.bindings[symbol] else {
                        panic!("Typed AST assignment target must be mutable");
                    };
                    self.operations.push(Operation::Store {
                        slot,
                        value: lowered,
                        origin: *span,
                    });
                }
                TypedItem::StructFieldAssign {
                    symbol,
                    declaration,
                    field,
                    field_types,
                    value,
                    span,
                } => {
                    let Some(updated) = self.lower_expr(value) else {
                        break;
                    };
                    let Binding::Slot(slot) = self.bindings[symbol] else {
                        panic!("verified mutable struct binding")
                    };
                    let structure_ty = self.slots[slot.0 as usize].ty;
                    let old = self.value();
                    self.operations.push(Operation::Load {
                        result: old,
                        slot,
                        ty: structure_ty,
                        origin: *span,
                    });
                    let mut fields = Vec::with_capacity(field_types.len());
                    for (index, ty) in field_types.iter().enumerate() {
                        let field_value = if index == *field {
                            updated
                        } else {
                            let projected = self.value();
                            self.operations.push(Operation::StructProject {
                                result: projected,
                                structure: old,
                                declaration: *declaration,
                                index,
                                ty: *ty,
                                origin: *span,
                            });
                            projected
                        };
                        fields.push((index, field_value));
                    }
                    let reconstructed = self.value();
                    self.operations.push(Operation::Struct {
                        result: reconstructed,
                        declaration: *declaration,
                        fields,
                        ty: structure_ty,
                        origin: *span,
                    });
                    self.operations.push(Operation::Store {
                        slot,
                        value: reconstructed,
                        origin: *span,
                    });
                }
                TypedItem::Expr(expression) => {
                    let Some(value) = self.lower_expr(expression) else {
                        result = None;
                        break;
                    };
                    result = Some(value);
                    return_origin = expression.span;
                }
                TypedItem::Return(expression) => {
                    if let Some(value) = self.lower_expr(expression) {
                        let Some(value) =
                            self.run_all_cleanups(value, self.function.result, expression.span)
                        else {
                            result = None;
                            break;
                        };
                        self.finish_current(Terminator::Return {
                            value,
                            origin: expression.span,
                        });
                    }
                    result = None;
                    break;
                }
                TypedItem::While {
                    condition,
                    body,
                    span,
                } => {
                    if !self.lower_while(condition, body, *span) {
                        result = None;
                        break;
                    }
                    result = None;
                }
                TypedItem::DeferCall {
                    function,
                    substitutions,
                    arguments,
                    span,
                } => {
                    let Some(arguments) = arguments
                        .iter()
                        .map(|argument| self.lower_expr(argument))
                        .collect::<Option<Vec<_>>>()
                    else {
                        result = None;
                        break;
                    };
                    self.cleanup_scopes
                        .last_mut()
                        .expect("function scope exists")
                        .push(CleanupAction::Call {
                            function: *function,
                            substitutions,
                            arguments,
                            origin: *span,
                        });
                    result = None;
                }
                TypedItem::DeferBlock { captures, body, .. } => {
                    let Some(captures) = self.lower_captures(captures) else {
                        result = None;
                        break;
                    };
                    self.cleanup_scopes
                        .last_mut()
                        .expect("function scope exists")
                        .push(CleanupAction::Block { captures, body });
                    result = None;
                }
            }
        }
        if !self
            .blocks
            .iter()
            .any(|block| block.id == self.current_block)
        {
            let value = result.unwrap_or_else(|| {
                self.constant(Constant::Unit, TypeId(3), self.function.body.span)
            });
            if let Some(value) = self.finish_scope(value, self.function.result, return_origin) {
                self.finish_current(Terminator::Return {
                    value,
                    origin: return_origin,
                });
            }
        }
        self.blocks.sort_by_key(|block| block.id);
        CoreFunction {
            id: self.id,
            declaration: self.function.id,
            module_name: self.function.module_name.clone(),
            name: self.function.name.clone(),
            visibility: self.function.visibility,
            span: self.function.span,
            parameters,
            type_parameters: self.function.type_parameters.clone(),
            specialization_arguments: Vec::new(),
            constraints: self.function.constraints.clone(),
            result: self.function.result,
            slots: self.slots,
            blocks: self.blocks,
        }
    }

    fn lower_expr(&mut self, expression: &'a TypedExpr) -> Option<ValueId> {
        Some(match &expression.kind {
            TypedExprKind::Integer(value) => {
                self.constant(Constant::Integer(*value), expression.ty, expression.span)
            }
            TypedExprKind::Boolean(value) => {
                self.constant(Constant::Boolean(*value), expression.ty, expression.span)
            }
            TypedExprKind::Unit => self.constant(Constant::Unit, expression.ty, expression.span),
            TypedExprKind::String(value) => self.constant(
                Constant::String(value.clone()),
                expression.ty,
                expression.span,
            ),
            TypedExprKind::Rune(value) => {
                self.constant(Constant::Rune(*value), expression.ty, expression.span)
            }
            TypedExprKind::Atom(name) => {
                self.constant(Constant::Atom(name.clone()), expression.ty, expression.span)
            }
            TypedExprKind::List { elements, tail } => {
                let elements = elements
                    .iter()
                    .map(|element| self.lower_expr(element))
                    .collect::<Option<Vec<_>>>()?;
                let tail = match tail {
                    Some(tail) => Some(self.lower_expr(tail)?),
                    None => None,
                };
                let result = self.value();
                self.operations.push(Operation::List {
                    result,
                    elements,
                    tail,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::ListReverse(value) => {
                let list = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::ListReverse {
                    result,
                    list,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::Tuple(elements) => {
                let elements = elements
                    .iter()
                    .map(|element| self.lower_expr(element))
                    .collect::<Option<Vec<_>>>()?;
                let result = self.value();
                self.operations.push(Operation::Tuple {
                    result,
                    elements,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::Struct {
                declaration,
                fields: source_fields,
                ..
            } => {
                let mut fields = Vec::with_capacity(source_fields.len());
                for (index, value) in source_fields {
                    fields.push((*index, self.lower_expr(value)?));
                }
                let result = self.value();
                self.operations.push(Operation::Struct {
                    result,
                    declaration: *declaration,
                    fields,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::StructProject {
                value,
                declaration,
                field,
            } => {
                let structure = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::StructProject {
                    result,
                    structure,
                    declaration: *declaration,
                    index: *field,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::Array(elements) => {
                let elements = elements
                    .iter()
                    .map(|element| self.lower_expr(element))
                    .collect::<Option<Vec<_>>>()?;
                let result = self.value();
                self.operations.push(Operation::Array {
                    result,
                    elements,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::Index {
                value,
                index,
                length,
            } => {
                let array = self.lower_expr(value)?;
                let index = self.lower_expr(index)?;
                let failure =
                    self.failure_target(CoreFailureCategory::IndexOutOfBounds, expression.span);
                let result = self.value();
                self.operations.push(Operation::ArrayIndex {
                    result,
                    array,
                    index,
                    length: *length,
                    failure,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::SliceFromArray { value, length } => {
                let array = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::SliceFromArray {
                    result,
                    array,
                    length: *length,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::SliceSubslice {
                value,
                start,
                length,
            } => {
                let slice = self.lower_expr(value)?;
                let start = self.lower_expr(start)?;
                let length = self.lower_expr(length)?;
                let failure =
                    self.failure_target(CoreFailureCategory::IndexOutOfBounds, expression.span);
                let result = self.value();
                self.operations.push(Operation::SliceSubslice {
                    result,
                    slice,
                    start,
                    length,
                    failure,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::SliceCopy(value) => {
                let slice = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::SliceCopy {
                    result,
                    slice,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::StringBytes(value) => {
                let string = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::StringBytes {
                    result,
                    string,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::StringCodepoints(value) => {
                let string = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::StringCodepoints {
                    result,
                    string,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::StringFromBytes(value) => {
                let bytes = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::StringFromBytes {
                    result,
                    bytes,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::Utf8ErrorOffset(value) => {
                let error = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::Utf8ErrorOffset {
                    result,
                    error,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::RuneToString(value) => {
                let rune = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::RuneToString {
                    result,
                    rune,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::BufferNew => {
                let result = self.value();
                self.operations.push(Operation::BufferNew {
                    result,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::BufferAppend {
                buffer,
                value,
                kind,
            } => {
                let buffer = self.lower_expr(buffer)?;
                let value = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::BufferAppend {
                    result,
                    buffer,
                    value,
                    kind: *kind,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::BufferToBytes(value) => {
                let buffer = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::BufferToBytes {
                    result,
                    buffer,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::BufferToString(value) => {
                let buffer = self.lower_expr(value)?;
                let bytes = self.value();
                let bytes_ty =
                    TypeId(self.types.iter().position(|ty| matches!(ty, Type::Bytes))? as u32);
                self.operations.push(Operation::BufferToBytes {
                    result: bytes,
                    buffer,
                    ty: bytes_ty,
                    origin: expression.span,
                });
                let result = self.value();
                self.operations.push(Operation::StringFromBytes {
                    result,
                    bytes,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::BytesToBits(value) => {
                let bytes = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::BytesToBits {
                    result,
                    bytes,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::BitsSlice {
                value,
                start,
                length,
            } => {
                let bits = self.lower_expr(value)?;
                let start = self.lower_expr(start)?;
                let length = self.lower_expr(length)?;
                let failure =
                    self.failure_target(CoreFailureCategory::IndexOutOfBounds, expression.span);
                let result = self.value();
                self.operations.push(Operation::BitsSlice {
                    result,
                    bits,
                    start,
                    length,
                    failure,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::BitsToBytes(value) => {
                let bits = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::BitsToBytes {
                    result,
                    bits,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::BytesFromList(value) => {
                let list = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::BytesFromList {
                    result,
                    list,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::BytesToList(value) => {
                let bytes = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::BytesToList {
                    result,
                    bytes,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::BytesSlice {
                value,
                start,
                length,
            } => {
                let bytes = self.lower_expr(value)?;
                let start = self.lower_expr(start)?;
                let length = self.lower_expr(length)?;
                let failure =
                    self.failure_target(CoreFailureCategory::IndexOutOfBounds, expression.span);
                let result = self.value();
                self.operations.push(Operation::BytesSlice {
                    result,
                    bytes,
                    start,
                    length,
                    failure,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::CollectionLength {
                value,
                known_length,
            } => {
                let source = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::CollectionLength {
                    result,
                    value: source,
                    known_length: *known_length,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::Map(source_entries) => {
                let mut entries = Vec::new();
                for (key, value) in source_entries {
                    let key = self.lower_expr(key)?;
                    let value = self.lower_expr(value)?;
                    entries.push((key, value));
                }
                let result = self.value();
                self.operations.push(Operation::Map {
                    result,
                    entries,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::MapPut { map, key, value } => {
                let map = self.lower_expr(map)?;
                let key = self.lower_expr(key)?;
                let value = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::MapPut {
                    result,
                    map,
                    key,
                    value,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::MapRemove { map, key } => {
                let map = self.lower_expr(map)?;
                let key = self.lower_expr(key)?;
                let result = self.value();
                self.operations.push(Operation::MapRemove {
                    result,
                    map,
                    key,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::MapFetch { map, key } => {
                let map_ty = map.ty;
                let map = self.lower_expr(map)?;
                let key = self.lower_expr(key)?;
                let result = self.value();
                self.operations.push(Operation::MapFetch {
                    result,
                    map,
                    key,
                    map_ty,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::MapToList(map) => {
                let map_ty = map.ty;
                let map = self.lower_expr(map)?;
                let result = self.value();
                self.operations.push(Operation::MapToList {
                    result,
                    map,
                    map_ty,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::Ascription(value) => return self.lower_expr(value),
            TypedExprKind::Local(symbol) => match self.bindings[symbol] {
                Binding::Value(value) => value,
                Binding::Slot(slot) => {
                    let result = self.value();
                    self.operations.push(Operation::Load {
                        result,
                        slot,
                        ty: expression.ty,
                        origin: expression.span,
                    });
                    result
                }
            },
            TypedExprKind::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.lower_expr(left)?;
                let right = self.lower_expr(right)?;
                let failures = self.checked_failure_targets(*operator, expression.span);
                let result = self.value();
                self.operations.push(Operation::CheckedArithmetic {
                    result,
                    operator: *operator,
                    left,
                    right,
                    failures,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::Comparison {
                operator,
                left,
                right,
            } => {
                let operand_ty = left.ty;
                let left = self.lower_expr(left)?;
                let right = self.lower_expr(right)?;
                let result = self.value();
                self.operations.push(Operation::Compare {
                    result,
                    operator: *operator,
                    left,
                    right,
                    operand_ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::Logical {
                operator,
                left,
                right,
            } => return self.lower_logical(*operator, left, right, expression.span),
            TypedExprKind::Call {
                function,
                substitutions,
                arguments,
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.lower_expr(argument))
                    .collect::<Option<Vec<_>>>()?;
                let result = self.value();
                self.operations.push(Operation::Call {
                    result,
                    function: self.functions[function],
                    substitutions: substitutions.clone(),
                    arguments,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::UnionInject { member, value } => {
                let value = self.lower_expr(value)?;
                let result = self.value();
                self.operations.push(Operation::UnionInject {
                    result,
                    member: *member,
                    value,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::If {
                condition,
                then_block,
                else_block,
            } => {
                return self.lower_if(
                    condition,
                    then_block,
                    else_block.as_ref(),
                    expression.ty,
                    expression.span,
                );
            }
            TypedExprKind::Match { subject, arms, .. } => {
                return self.lower_match(subject, arms, expression.ty, expression.span);
            }
        })
    }

    fn lower_if(
        &mut self,
        condition: &'a TypedExpr,
        then_block: &'a el_types::TypedBlock,
        else_block: Option<&'a el_types::TypedBlock>,
        ty: TypeId,
        origin: Span,
    ) -> Option<ValueId> {
        let condition = self.lower_expr(condition)?;
        let then_target = self.new_block();
        let else_target = self.new_block();
        let join_target = self.new_block();
        self.finish_current(Terminator::CondBranch {
            condition,
            then_target,
            else_target,
            origin,
        });

        let outer_bindings = self.bindings.clone();
        self.current_block = then_target;
        self.current_parameters.clear();
        self.bindings = outer_bindings.clone();
        let then_value = self.lower_block_value(then_block);
        if let Some(value) = then_value {
            self.finish_current(Terminator::Branch {
                target: join_target,
                arguments: vec![value],
                origin: then_block.span,
            });
        }

        self.current_block = else_target;
        self.current_parameters.clear();
        self.bindings = outer_bindings.clone();
        let else_value = if let Some(block) = else_block {
            self.lower_block_value(block)
        } else {
            Some(self.constant(Constant::Unit, TypeId(3), origin))
        };
        if let Some(value) = else_value {
            self.finish_current(Terminator::Branch {
                target: join_target,
                arguments: vec![value],
                origin,
            });
        }

        if then_value.is_none() && else_value.is_none() {
            self.bindings = outer_bindings;
            return None;
        }

        self.current_block = join_target;
        self.bindings = outer_bindings;
        let result = self.value();
        self.current_parameters = vec![CoreParameter {
            value: result,
            ty,
            origin,
        }];
        Some(result)
    }

    fn lower_block_value(&mut self, block: &'a el_types::TypedBlock) -> Option<ValueId> {
        self.cleanup_scopes.push(Vec::new());
        let mut result = None;
        for item in &block.items {
            match item {
                TypedItem::Let {
                    symbol,
                    mutable,
                    ty,
                    initializer,
                    span,
                    ..
                } => {
                    let value = self.lower_expr(initializer)?;
                    if *mutable {
                        let slot = SlotId(self.slots.len() as u32);
                        self.slots.push(Slot {
                            id: slot,
                            ty: *ty,
                            origin: *span,
                        });
                        self.operations.push(Operation::Store {
                            slot,
                            value,
                            origin: *span,
                        });
                        self.bindings.insert(*symbol, Binding::Slot(slot));
                    } else {
                        self.bindings.insert(*symbol, Binding::Value(value));
                    }
                }
                TypedItem::Assign {
                    symbol,
                    value,
                    span,
                } => {
                    let value = self.lower_expr(value)?;
                    let Binding::Slot(slot) = self.bindings[symbol] else {
                        panic!("verified mutable binding")
                    };
                    self.operations.push(Operation::Store {
                        slot,
                        value,
                        origin: *span,
                    });
                }
                TypedItem::StructFieldAssign {
                    symbol,
                    declaration,
                    field,
                    field_types,
                    value,
                    span,
                } => {
                    let updated = self.lower_expr(value)?;
                    let Binding::Slot(slot) = self.bindings[symbol] else {
                        panic!("verified mutable struct binding")
                    };
                    let structure_ty = self.slots[slot.0 as usize].ty;
                    let old = self.value();
                    self.operations.push(Operation::Load {
                        result: old,
                        slot,
                        ty: structure_ty,
                        origin: *span,
                    });
                    let mut fields = Vec::with_capacity(field_types.len());
                    for (index, ty) in field_types.iter().enumerate() {
                        let field_value = if index == *field {
                            updated
                        } else {
                            let projected = self.value();
                            self.operations.push(Operation::StructProject {
                                result: projected,
                                structure: old,
                                declaration: *declaration,
                                index,
                                ty: *ty,
                                origin: *span,
                            });
                            projected
                        };
                        fields.push((index, field_value));
                    }
                    let reconstructed = self.value();
                    self.operations.push(Operation::Struct {
                        result: reconstructed,
                        declaration: *declaration,
                        fields,
                        ty: structure_ty,
                        origin: *span,
                    });
                    self.operations.push(Operation::Store {
                        slot,
                        value: reconstructed,
                        origin: *span,
                    });
                }
                TypedItem::Expr(value) => result = Some(self.lower_expr(value)?),
                TypedItem::Return(expression) => {
                    let value = self.lower_expr(expression)?;
                    let Some(value) =
                        self.run_all_cleanups(value, self.function.result, expression.span)
                    else {
                        self.cleanup_scopes.pop();
                        return None;
                    };
                    self.finish_current(Terminator::Return {
                        value,
                        origin: block.span,
                    });
                    self.cleanup_scopes.pop();
                    return None;
                }
                TypedItem::While {
                    condition,
                    body,
                    span,
                } => {
                    if !self.lower_while(condition, body, *span) {
                        return None;
                    }
                    result = None;
                }
                TypedItem::DeferCall {
                    function,
                    substitutions,
                    arguments,
                    span,
                } => {
                    let arguments = arguments
                        .iter()
                        .map(|argument| self.lower_expr(argument))
                        .collect::<Option<Vec<_>>>()?;
                    self.cleanup_scopes
                        .last_mut()
                        .expect("lexical scope exists")
                        .push(CleanupAction::Call {
                            function: *function,
                            substitutions,
                            arguments,
                            origin: *span,
                        });
                    result = None;
                }
                TypedItem::DeferBlock { captures, body, .. } => {
                    let captures = self.lower_captures(captures)?;
                    self.cleanup_scopes
                        .last_mut()
                        .expect("lexical scope exists")
                        .push(CleanupAction::Block { captures, body });
                    result = None;
                }
            }
        }
        let result = result.unwrap_or_else(|| self.constant(Constant::Unit, TypeId(3), block.span));
        self.finish_scope(result, block.ty, block.span)
    }

    fn lower_captures(
        &mut self,
        captures: &[el_types::TypedCapture],
    ) -> Option<BTreeMap<SymbolId, Binding>> {
        let mut lowered = BTreeMap::new();
        for capture in captures {
            let value = self.read_binding(capture.source, capture.ty, capture.span)?;
            lowered.insert(capture.symbol, Binding::Value(value));
        }
        Some(lowered)
    }

    fn checked_failure_targets(
        &mut self,
        operator: ArithmeticOperator,
        origin: Span,
    ) -> Vec<(CoreFailureCategory, BlockId)> {
        arithmetic_failure_categories(operator)
            .iter()
            .copied()
            .map(|category| {
                let block = self.new_block();
                self.blocks.push(Block {
                    id: block,
                    parameters: Vec::new(),
                    operations: Vec::new(),
                    terminator: Terminator::Failure { category, origin },
                });
                (category, block)
            })
            .collect()
    }

    fn failure_target(&mut self, category: CoreFailureCategory, origin: Span) -> BlockId {
        let block = self.new_block();
        self.blocks.push(Block {
            id: block,
            parameters: Vec::new(),
            operations: Vec::new(),
            terminator: Terminator::Failure { category, origin },
        });
        block
    }

    fn read_binding(&mut self, symbol: SymbolId, ty: TypeId, origin: Span) -> Option<ValueId> {
        Some(match self.bindings.get(&symbol).copied()? {
            Binding::Value(value) => value,
            Binding::Slot(slot) => {
                let result = self.value();
                self.operations.push(Operation::Load {
                    result,
                    slot,
                    ty,
                    origin,
                });
                result
            }
        })
    }

    fn finish_scope(&mut self, result: ValueId, ty: TypeId, origin: Span) -> Option<ValueId> {
        let actions = self.cleanup_scopes.pop().expect("lexical scope exists");
        self.route_cleanup_actions(actions, result, ty, origin)
    }

    fn run_all_cleanups(
        &mut self,
        mut result: ValueId,
        ty: TypeId,
        origin: Span,
    ) -> Option<ValueId> {
        let scopes = self.cleanup_scopes.clone();
        for actions in scopes.into_iter().rev() {
            result = self.route_cleanup_actions(actions, result, ty, origin)?;
        }
        Some(result)
    }

    fn route_cleanup_actions(
        &mut self,
        actions: Vec<CleanupAction<'a>>,
        mut saved: ValueId,
        saved_ty: TypeId,
        origin: Span,
    ) -> Option<ValueId> {
        for action in actions.into_iter().rev() {
            let cleanup = self.new_block();
            self.finish_current(Terminator::Branch {
                target: cleanup,
                arguments: vec![saved],
                origin,
            });
            self.current_block = cleanup;
            saved = self.value();
            self.current_parameters = vec![CoreParameter {
                value: saved,
                ty: saved_ty,
                origin,
            }];
            match action {
                CleanupAction::Call {
                    function,
                    substitutions,
                    arguments,
                    origin,
                } => {
                    let result = self.value();
                    self.operations.push(Operation::Call {
                        result,
                        function: self.functions[&function],
                        substitutions: substitutions.to_vec(),
                        arguments,
                        ty: TypeId(3),
                        origin,
                    });
                }
                CleanupAction::Block { captures, body } => {
                    let outer_bindings = std::mem::replace(&mut self.bindings, captures);
                    let succeeded = self.lower_block_value(body).is_some();
                    self.bindings = outer_bindings;
                    if !succeeded {
                        return None;
                    }
                }
            }
        }
        Some(saved)
    }

    fn lower_logical(
        &mut self,
        operator: LogicalOperator,
        left: &'a TypedExpr,
        right: &'a TypedExpr,
        origin: Span,
    ) -> Option<ValueId> {
        let left = self.lower_expr(left)?;
        let right_target = self.new_block();
        let short_target = self.new_block();
        let join_target = self.new_block();
        let (then_target, else_target, short_value) = match operator {
            LogicalOperator::And => (right_target, short_target, false),
            LogicalOperator::Or => (short_target, right_target, true),
        };
        self.finish_current(Terminator::CondBranch {
            condition: left,
            then_target,
            else_target,
            origin,
        });

        self.current_block = short_target;
        self.current_parameters.clear();
        let short = self.constant(Constant::Boolean(short_value), TypeId(2), origin);
        self.finish_current(Terminator::Branch {
            target: join_target,
            arguments: vec![short],
            origin,
        });

        self.current_block = right_target;
        self.current_parameters.clear();
        if let Some(right) = self.lower_expr(right) {
            self.finish_current(Terminator::Branch {
                target: join_target,
                arguments: vec![right],
                origin,
            });
        }

        self.current_block = join_target;
        let result = self.value();
        self.current_parameters = vec![CoreParameter {
            value: result,
            ty: TypeId(2),
            origin,
        }];
        Some(result)
    }

    fn lower_while(
        &mut self,
        condition: &'a TypedExpr,
        body: &'a el_types::TypedBlock,
        origin: Span,
    ) -> bool {
        let condition_target = self.new_block();
        let body_target = self.new_block();
        let exit_target = self.new_block();
        self.finish_current(Terminator::Branch {
            target: condition_target,
            arguments: Vec::new(),
            origin,
        });

        let outer_bindings = self.bindings.clone();
        self.current_block = condition_target;
        self.current_parameters.clear();
        self.bindings = outer_bindings.clone();
        let Some(condition) = self.lower_expr(condition) else {
            return false;
        };
        self.finish_current(Terminator::CondBranch {
            condition,
            then_target: body_target,
            else_target: exit_target,
            origin,
        });

        self.current_block = body_target;
        self.current_parameters.clear();
        self.bindings = outer_bindings.clone();
        if self.lower_block_value(body).is_some() {
            self.finish_current(Terminator::Branch {
                target: condition_target,
                arguments: Vec::new(),
                origin: body.span,
            });
        }

        self.current_block = exit_target;
        self.current_parameters.clear();
        self.bindings = outer_bindings;
        true
    }

    fn lower_match(
        &mut self,
        subject: &'a TypedExpr,
        arms: &'a [el_types::TypedMatchArm],
        ty: TypeId,
        origin: Span,
    ) -> Option<ValueId> {
        let subject_value = self.lower_expr(subject)?;
        let join_target = self.new_block();
        let outer_bindings = self.bindings.clone();
        for arm in arms {
            let body_target = self.new_block();
            let failure_target = self.new_block();
            let mut pattern_bindings = BTreeMap::new();
            self.lower_pattern(
                &arm.pattern,
                subject_value,
                body_target,
                failure_target,
                &mut pattern_bindings,
            );
            self.current_block = body_target;
            self.current_parameters = pattern_bindings
                .values()
                .map(|(_, ty)| CoreParameter {
                    value: self.value(),
                    ty: *ty,
                    origin: arm.pattern.span,
                })
                .collect();
            self.bindings = outer_bindings.clone();
            for ((symbol, _), parameter) in pattern_bindings
                .into_iter()
                .zip(self.current_parameters.iter())
            {
                self.bindings
                    .insert(symbol, Binding::Value(parameter.value));
            }
            if let Some(value) = self.lower_block_value(&arm.body) {
                self.finish_current(Terminator::Branch {
                    target: join_target,
                    arguments: vec![value],
                    origin: arm.body.span,
                });
            }
            self.current_block = failure_target;
            self.current_parameters.clear();
            self.bindings = outer_bindings.clone();
        }
        let failure_is_reachable = self.blocks.iter().any(|block| match &block.terminator {
            Terminator::Branch { target, .. } => *target == self.current_block,
            Terminator::CondBranch {
                then_target,
                else_target,
                ..
            } => *then_target == self.current_block || *else_target == self.current_block,
            Terminator::Switch { cases, default, .. } => {
                cases
                    .iter()
                    .any(|(_, target)| *target == self.current_block)
                    || default.is_some_and(|target| target == self.current_block)
            }
            Terminator::Return { .. }
            | Terminator::Failure { .. }
            | Terminator::Unreachable { .. } => false,
        });
        if failure_is_reachable {
            self.finish_current(Terminator::Unreachable { origin });
        }
        self.current_block = join_target;
        self.current_parameters = vec![CoreParameter {
            value: self.value(),
            ty,
            origin,
        }];
        self.bindings = outer_bindings;
        Some(self.current_parameters[0].value)
    }

    fn lower_pattern(
        &mut self,
        pattern: &el_types::TypedPattern,
        subject: ValueId,
        success: BlockId,
        failure: BlockId,
        bindings: &mut BTreeMap<SymbolId, (ValueId, TypeId)>,
    ) {
        match &pattern.kind {
            TypedPatternKind::Wildcard => self.finish_current(Terminator::Branch {
                target: success,
                arguments: pattern_arguments(bindings),
                origin: pattern.span,
            }),
            TypedPatternKind::Binding { symbol, .. } => {
                bindings.insert(*symbol, (subject, pattern.ty));
                self.finish_current(Terminator::Branch {
                    target: success,
                    arguments: pattern_arguments(bindings),
                    origin: pattern.span,
                });
            }
            TypedPatternKind::Boolean(value) => self.pattern_switch(
                (subject, pattern.ty),
                SwitchValue::Boolean(*value),
                success,
                failure,
                pattern.span,
                pattern_arguments(bindings),
            ),
            TypedPatternKind::Integer(value) => self.pattern_switch(
                (subject, pattern.ty),
                SwitchValue::Integer(*value),
                success,
                failure,
                pattern.span,
                pattern_arguments(bindings),
            ),
            TypedPatternKind::Atom(value) => self.pattern_switch(
                (subject, pattern.ty),
                SwitchValue::Atom(value.clone()),
                success,
                failure,
                pattern.span,
                pattern_arguments(bindings),
            ),
            TypedPatternKind::UnionMember { member, symbol, .. } => {
                let matched = self.new_block();
                self.pattern_switch(
                    (subject, pattern.ty),
                    SwitchValue::UnionMember(*member),
                    matched,
                    failure,
                    pattern.span,
                    Vec::new(),
                );
                self.current_block = matched;
                self.current_parameters.clear();
                let value = self.value();
                self.operations.push(Operation::UnionProject {
                    result: value,
                    member: *member,
                    value: subject,
                    union_ty: pattern.ty,
                    ty: *member,
                    origin: pattern.span,
                });
                bindings.insert(*symbol, (value, *member));
                self.finish_current(Terminator::Branch {
                    target: success,
                    arguments: pattern_arguments(bindings),
                    origin: pattern.span,
                });
            }
            TypedPatternKind::Tuple(elements) => {
                let mut children = Vec::new();
                for (index, child) in elements.iter().enumerate() {
                    let value = self.value();
                    self.operations.push(Operation::TupleProject {
                        result: value,
                        tuple: subject,
                        index,
                        ty: child.ty,
                        origin: child.span,
                    });
                    children.push((child, value));
                }
                self.lower_pattern_sequence(&children, success, failure, bindings);
            }
            TypedPatternKind::ListEmpty => self.pattern_switch(
                (subject, pattern.ty),
                SwitchValue::ListEmpty,
                success,
                failure,
                pattern.span,
                pattern_arguments(bindings),
            ),
            TypedPatternKind::ListCons { head, tail } => {
                let matched = self.new_block();
                self.pattern_switch(
                    (subject, pattern.ty),
                    SwitchValue::ListCons,
                    matched,
                    failure,
                    pattern.span,
                    Vec::new(),
                );
                self.current_block = matched;
                self.current_parameters.clear();
                let head_value = self.value();
                self.operations.push(Operation::ListHead {
                    result: head_value,
                    list: subject,
                    ty: head.ty,
                    origin: head.span,
                });
                let tail_value = self.value();
                self.operations.push(Operation::ListTail {
                    result: tail_value,
                    list: subject,
                    ty: tail.ty,
                    origin: tail.span,
                });
                self.lower_pattern_sequence(
                    &[(head.as_ref(), head_value), (tail.as_ref(), tail_value)],
                    success,
                    failure,
                    bindings,
                );
            }
            TypedPatternKind::Struct {
                declaration,
                fields,
                ..
            } => {
                let mut children = Vec::new();
                for (index, child) in fields {
                    let value = self.value();
                    self.operations.push(Operation::StructProject {
                        result: value,
                        structure: subject,
                        declaration: *declaration,
                        index: *index,
                        ty: child.ty,
                        origin: child.span,
                    });
                    children.push((child, value));
                }
                self.lower_pattern_sequence(&children, success, failure, bindings);
            }
        }
    }

    fn lower_pattern_sequence(
        &mut self,
        children: &[(&el_types::TypedPattern, ValueId)],
        success: BlockId,
        failure: BlockId,
        bindings: &mut BTreeMap<SymbolId, (ValueId, TypeId)>,
    ) {
        if children.is_empty() {
            self.finish_current(Terminator::Branch {
                target: success,
                arguments: pattern_arguments(bindings),
                origin: self.function.span,
            });
            return;
        }
        for (index, (pattern, value)) in children.iter().enumerate() {
            let next = if index + 1 == children.len() {
                success
            } else {
                self.new_block()
            };
            self.lower_pattern(pattern, *value, next, failure, bindings);
            if next != success {
                self.current_block = next;
                self.current_parameters = bindings
                    .values()
                    .map(|(_, ty)| CoreParameter {
                        value: self.value(),
                        ty: *ty,
                        origin: pattern.span,
                    })
                    .collect();
                for ((_, binding), parameter) in
                    bindings.iter_mut().zip(self.current_parameters.iter())
                {
                    binding.0 = parameter.value;
                }
            }
        }
    }

    fn pattern_switch(
        &mut self,
        subject: (ValueId, TypeId),
        value: SwitchValue,
        success: BlockId,
        failure: BlockId,
        origin: Span,
        success_arguments: Vec<ValueId>,
    ) {
        let (subject, subject_ty) = subject;
        self.finish_current(Terminator::Switch {
            subject,
            subject_ty,
            cases: vec![(value, success)],
            default: Some(failure),
            origin,
        });
        if !success_arguments.is_empty() {
            let target = self.new_block();
            let block = self.blocks.last_mut().expect("switch was just emitted");
            let Terminator::Switch { cases, .. } = &mut block.terminator else {
                unreachable!("pattern switch terminator")
            };
            cases[0].1 = target;
            self.current_block = target;
            self.current_parameters.clear();
            self.finish_current(Terminator::Branch {
                target: success,
                arguments: success_arguments,
                origin,
            });
        }
    }

    fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.next_block);
        self.next_block += 1;
        id
    }

    fn finish_current(&mut self, terminator: Terminator) {
        self.blocks.push(Block {
            id: self.current_block,
            parameters: std::mem::take(&mut self.current_parameters),
            operations: std::mem::take(&mut self.operations),
            terminator,
        });
    }

    fn constant(&mut self, constant: Constant, ty: TypeId, origin: Span) -> ValueId {
        let result = self.value();
        self.operations.push(Operation::Constant {
            result,
            constant,
            ty,
            origin,
        });
        result
    }

    fn value(&mut self) -> ValueId {
        let value = ValueId(self.next_value);
        self.next_value += 1;
        value
    }
}

/// The deterministic set of compiler roots for an executable target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReachabilityRoots {
    pub functions: Vec<FunctionId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConcreteModule {
    pub types: Vec<Type>,
    pub structs: Vec<ConcreteStruct>,
    pub functions: Vec<CoreFunction>,
    pub roots: ReachabilityRoots,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConcreteStruct {
    pub declaration: DeclId,
    pub name: String,
    pub arguments: Vec<TypeId>,
    pub fields: Vec<(String, TypeId)>,
    pub origin: Span,
}

/// Collector-relevant physical category carried by a Concrete Core type.
///
/// This is deliberately independent of Boehm. Backends use it to decide which
/// live values require an unmodified base pointer at collection points.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedValueClass {
    Unmanaged,
    BaseReference,
    ContainsBaseReferences,
}

impl ManagedValueClass {
    #[must_use]
    pub const fn requires_root(self) -> bool {
        !matches!(self, Self::Unmanaged)
    }
}

/// Collection behavior of a Concrete Core operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollectionEffect {
    CannotCollect,
    MayCollect,
}

/// Every EL call is a possible collection point. Non-empty list and map
/// construction allocate one or more scanned nodes. Private runtime operations
/// carry their explicit effects when they enter the representation.
#[must_use]
pub const fn operation_collection_effect(operation: &Operation) -> CollectionEffect {
    if matches!(operation, Operation::Call { .. })
        || matches!(operation, Operation::List { elements, .. } if !elements.is_empty())
        || matches!(operation, Operation::Map { entries, .. } if !entries.is_empty())
        || matches!(
            operation,
            Operation::MapPut { .. } | Operation::MapRemove { .. }
        )
        || matches!(
            operation,
            Operation::ListReverse { .. }
                | Operation::MapToList { .. }
                | Operation::StringCodepoints { .. }
                | Operation::StringFromBytes { .. }
                | Operation::BytesFromList { .. }
                | Operation::BytesToList { .. }
                | Operation::RuneToString { .. }
                | Operation::BufferAppend { .. }
                | Operation::BufferToBytes { .. }
                | Operation::BitsToBytes { .. }
        )
        || matches!(
            operation,
            Operation::SliceFromArray { .. } | Operation::SliceCopy { .. }
        )
    {
        CollectionEffect::MayCollect
    } else {
        CollectionEffect::CannotCollect
    }
}

/// Managed Core values and addressable slots live when an operation may collect.
///
/// Results are ordered by block and operation index, and each root list is
/// sorted by its stable function-local ID. Values include call operands because
/// a callee may collect while consuming them. Slots are live when their current
/// contents can be loaded after the collection point before being overwritten.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionPointRoots {
    pub block: BlockId,
    pub operation_index: usize,
    pub values: Vec<ValueId>,
    pub slots: Vec<SlotId>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct LiveState {
    values: BTreeSet<ValueId>,
    slots: BTreeSet<SlotId>,
}

/// Computes the exact managed roots required at each possible collection point
/// in one verified Concrete Core function.
#[must_use]
pub fn collection_point_roots(
    module: &ConcreteModule,
    function: &CoreFunction,
) -> Vec<CollectionPointRoots> {
    let value_types = function_value_types(function);
    let slot_types = function
        .slots
        .iter()
        .map(|slot| (slot.id, slot.ty))
        .collect::<BTreeMap<_, _>>();
    let blocks = function
        .blocks
        .iter()
        .map(|block| (block.id, block))
        .collect::<BTreeMap<_, _>>();
    let mut live_in = blocks
        .keys()
        .map(|block| (*block, LiveState::default()))
        .collect::<BTreeMap<_, _>>();

    loop {
        let mut changed = false;
        for block in function.blocks.iter().rev() {
            let mut live = terminator_live_out(&block.terminator, &blocks, &live_in);
            add_terminator_uses(&block.terminator, &mut live.values);
            for operation in block.operations.iter().rev() {
                transfer_operation(operation, &mut live);
            }
            for parameter in &block.parameters {
                live.values.remove(&parameter.value);
            }
            if live_in.get(&block.id) != Some(&live) {
                live_in.insert(block.id, live);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    let mut points = Vec::new();
    for block in &function.blocks {
        let mut live = terminator_live_out(&block.terminator, &blocks, &live_in);
        add_terminator_uses(&block.terminator, &mut live.values);
        let mut reversed = Vec::new();
        for (operation_index, operation) in block.operations.iter().enumerate().rev() {
            transfer_operation(operation, &mut live);
            if operation_collection_effect(operation) == CollectionEffect::MayCollect {
                let values = live
                    .values
                    .iter()
                    .copied()
                    .filter(|value| {
                        value_types.get(value).is_some_and(|ty| {
                            managed_value_class(module, *ty)
                                .is_some_and(ManagedValueClass::requires_root)
                        })
                    })
                    .collect();
                let slots = live
                    .slots
                    .iter()
                    .copied()
                    .filter(|slot| {
                        slot_types.get(slot).is_some_and(|ty| {
                            managed_value_class(module, *ty)
                                .is_some_and(ManagedValueClass::requires_root)
                        })
                    })
                    .collect();
                reversed.push(CollectionPointRoots {
                    block: block.id,
                    operation_index,
                    values,
                    slots,
                });
            }
        }
        reversed.reverse();
        points.extend(reversed);
    }
    points.sort_by_key(|point| (point.block, point.operation_index));
    points
}

fn terminator_live_out(
    terminator: &Terminator,
    blocks: &BTreeMap<BlockId, &Block>,
    live_in: &BTreeMap<BlockId, LiveState>,
) -> LiveState {
    let mut output = LiveState::default();
    match terminator {
        Terminator::Branch {
            target, arguments, ..
        } => add_edge_live(*target, arguments, blocks, live_in, &mut output),
        Terminator::CondBranch {
            then_target,
            else_target,
            ..
        } => {
            add_edge_live(*then_target, &[], blocks, live_in, &mut output);
            add_edge_live(*else_target, &[], blocks, live_in, &mut output);
        }
        Terminator::Switch { cases, default, .. } => {
            for (_, target) in cases {
                add_edge_live(*target, &[], blocks, live_in, &mut output);
            }
            if let Some(target) = default {
                add_edge_live(*target, &[], blocks, live_in, &mut output);
            }
        }
        Terminator::Return { .. } | Terminator::Failure { .. } | Terminator::Unreachable { .. } => {
        }
    }
    output
}

fn add_edge_live(
    target: BlockId,
    arguments: &[ValueId],
    blocks: &BTreeMap<BlockId, &Block>,
    live_in: &BTreeMap<BlockId, LiveState>,
    output: &mut LiveState,
) {
    let Some(target_live) = live_in.get(&target) else {
        return;
    };
    output.slots.extend(&target_live.slots);
    output.values.extend(&target_live.values);
    if let Some(target_block) = blocks.get(&target) {
        for (index, parameter) in target_block.parameters.iter().enumerate() {
            if output.values.remove(&parameter.value)
                && let Some(argument) = arguments.get(index)
            {
                output.values.insert(*argument);
            }
        }
    }
}

fn add_terminator_uses(terminator: &Terminator, live: &mut BTreeSet<ValueId>) {
    match terminator {
        Terminator::Branch { .. } => {}
        Terminator::CondBranch { condition, .. } => {
            live.insert(*condition);
        }
        Terminator::Switch { subject, .. } => {
            live.insert(*subject);
        }
        Terminator::Return { value, .. } => {
            live.insert(*value);
        }
        Terminator::Failure { .. } | Terminator::Unreachable { .. } => {}
    }
}

fn transfer_operation(operation: &Operation, live: &mut LiveState) {
    if let Some(result) = operation_result(operation) {
        live.values.remove(&result);
    }
    match operation {
        Operation::Constant { .. } => {}
        Operation::List { elements, tail, .. } => {
            live.values.extend(elements);
            live.values.extend(tail);
        }
        Operation::ListReverse { list, .. } => {
            live.values.insert(*list);
        }
        Operation::Array { elements, .. } | Operation::Tuple { elements, .. } => {
            live.values.extend(elements);
        }
        Operation::ArrayIndex { array, index, .. } => {
            live.values.insert(*array);
            live.values.insert(*index);
        }
        Operation::SliceFromArray { array, .. } => {
            live.values.insert(*array);
        }
        Operation::SliceSubslice {
            slice,
            start,
            length,
            ..
        } => {
            live.values.extend([*slice, *start, *length]);
        }
        Operation::SliceCopy { slice, .. } => {
            live.values.insert(*slice);
        }
        Operation::StringBytes { string, .. } => {
            live.values.insert(*string);
        }
        Operation::StringCodepoints { string, .. } => {
            live.values.insert(*string);
        }
        Operation::StringFromBytes { bytes, .. } => {
            live.values.insert(*bytes);
        }
        Operation::Utf8ErrorOffset { error, .. } => {
            live.values.insert(*error);
        }
        Operation::RuneToString { rune, .. } => {
            live.values.insert(*rune);
        }
        Operation::BufferNew { .. } => {}
        Operation::BufferAppend { buffer, value, .. } => {
            live.values.extend([*buffer, *value]);
        }
        Operation::BufferToBytes { buffer, .. } => {
            live.values.insert(*buffer);
        }
        Operation::BytesToBits { bytes, .. } => {
            live.values.insert(*bytes);
        }
        Operation::BitsSlice {
            bits,
            start,
            length,
            ..
        } => {
            live.values.extend([*bits, *start, *length]);
        }
        Operation::BitsToBytes { bits, .. } => {
            live.values.insert(*bits);
        }
        Operation::BytesFromList { list, .. } => {
            live.values.insert(*list);
        }
        Operation::BytesToList { bytes, .. } => {
            live.values.insert(*bytes);
        }
        Operation::BytesSlice {
            bytes,
            start,
            length,
            ..
        } => {
            live.values.extend([*bytes, *start, *length]);
        }
        Operation::CollectionLength { value, .. } => {
            live.values.insert(*value);
        }
        Operation::Struct { fields, .. } => {
            live.values.extend(fields.iter().map(|(_, value)| value));
        }
        Operation::Map { entries, .. } => {
            live.values
                .extend(entries.iter().flat_map(|(key, value)| [*key, *value]));
        }
        Operation::MapPut {
            map, key, value, ..
        } => {
            live.values.extend([*map, *key, *value]);
        }
        Operation::MapRemove { map, key, .. } => {
            live.values.extend([*map, *key]);
        }
        Operation::MapFetch { map, key, .. } => {
            live.values.extend([*map, *key]);
        }
        Operation::MapToList { map, .. } => {
            live.values.insert(*map);
        }
        Operation::TupleProject { tuple, .. } => {
            live.values.insert(*tuple);
        }
        Operation::StructProject { structure, .. } => {
            live.values.insert(*structure);
        }
        Operation::ListHead { list, .. } | Operation::ListTail { list, .. } => {
            live.values.insert(*list);
        }
        Operation::CheckedArithmetic { left, right, .. }
        | Operation::Compare { left, right, .. } => {
            live.values.insert(*left);
            live.values.insert(*right);
        }
        Operation::Call { arguments, .. } => {
            live.values.extend(arguments);
        }
        Operation::UnionInject { value, .. } | Operation::UnionProject { value, .. } => {
            live.values.insert(*value);
        }
        Operation::Load { slot, .. } => {
            live.slots.insert(*slot);
        }
        Operation::Store { slot, value, .. } => {
            live.slots.remove(slot);
            live.values.insert(*value);
        }
    }
}

fn operation_result(operation: &Operation) -> Option<ValueId> {
    match operation {
        Operation::Constant { result, .. }
        | Operation::List { result, .. }
        | Operation::ListReverse { result, .. }
        | Operation::Array { result, .. }
        | Operation::ArrayIndex { result, .. }
        | Operation::SliceFromArray { result, .. }
        | Operation::SliceSubslice { result, .. }
        | Operation::SliceCopy { result, .. }
        | Operation::StringBytes { result, .. }
        | Operation::StringCodepoints { result, .. }
        | Operation::StringFromBytes { result, .. }
        | Operation::Utf8ErrorOffset { result, .. }
        | Operation::RuneToString { result, .. }
        | Operation::BufferNew { result, .. }
        | Operation::BufferAppend { result, .. }
        | Operation::BufferToBytes { result, .. }
        | Operation::BytesToBits { result, .. }
        | Operation::BitsSlice { result, .. }
        | Operation::BitsToBytes { result, .. }
        | Operation::BytesFromList { result, .. }
        | Operation::BytesToList { result, .. }
        | Operation::BytesSlice { result, .. }
        | Operation::CollectionLength { result, .. }
        | Operation::Map { result, .. }
        | Operation::MapPut { result, .. }
        | Operation::MapRemove { result, .. }
        | Operation::MapFetch { result, .. }
        | Operation::MapToList { result, .. }
        | Operation::Tuple { result, .. }
        | Operation::Struct { result, .. }
        | Operation::TupleProject { result, .. }
        | Operation::StructProject { result, .. }
        | Operation::ListHead { result, .. }
        | Operation::ListTail { result, .. }
        | Operation::CheckedArithmetic { result, .. }
        | Operation::Compare { result, .. }
        | Operation::Call { result, .. }
        | Operation::UnionInject { result, .. }
        | Operation::UnionProject { result, .. }
        | Operation::Load { result, .. } => Some(*result),
        Operation::Store { .. } => None,
    }
}

/// Classifies a fully concrete type without exposing collector implementation
/// details across the IR boundary.
#[must_use]
pub fn managed_value_class(module: &ConcreteModule, ty: TypeId) -> Option<ManagedValueClass> {
    classify_managed_type(module, ty, &mut BTreeSet::new())
}

fn classify_managed_type(
    module: &ConcreteModule,
    ty: TypeId,
    visiting: &mut BTreeSet<TypeId>,
) -> Option<ManagedValueClass> {
    let value = module.types.get(ty.0 as usize)?;
    if !visiting.insert(ty) {
        return Some(ManagedValueClass::ContainsBaseReferences);
    }
    let class = match value {
        Type::I32
        | Type::I64
        | Type::Usize
        | Type::Rune
        | Type::Utf8Error
        | Type::U8
        | Type::Bool
        | Type::Unit
        | Type::Atom(_)
        | Type::Function { .. } => ManagedValueClass::Unmanaged,
        // String is a view-like pointer/length value and must retain its base.
        Type::String | Type::Bytes | Type::Bits | Type::Buffer | Type::Slice(_) => {
            ManagedValueClass::ContainsBaseReferences
        }
        Type::List(_) | Type::Map { .. } => ManagedValueClass::BaseReference,
        Type::Array { item, .. } => aggregate_managed_class(module, [*item], visiting)?,
        Type::Tuple(elements) | Type::Union(elements) => {
            aggregate_managed_class(module, elements.iter().copied(), visiting)?
        }
        Type::Struct { declaration, .. } => {
            let structure = module
                .structs
                .iter()
                .find(|structure| structure.declaration == *declaration)?;
            aggregate_managed_class(
                module,
                structure.fields.iter().map(|(_, field)| *field),
                visiting,
            )?
        }
        Type::Parameter { .. } => return None,
    };
    visiting.remove(&ty);
    Some(class)
}

fn aggregate_managed_class(
    module: &ConcreteModule,
    children: impl IntoIterator<Item = TypeId>,
    visiting: &mut BTreeSet<TypeId>,
) -> Option<ManagedValueClass> {
    for child in children {
        if classify_managed_type(module, child, visiting)?.requires_root() {
            return Some(ManagedValueClass::ContainsBaseReferences);
        }
    }
    Some(ManagedValueClass::Unmanaged)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MonomorphizationError {
    UnknownRoot(FunctionId),
    UnknownFunction(FunctionId),
    UnknownDeclaration(DeclId),
    UnknownStruct(DeclId),
    MissingSubstitution {
        declaration: DeclId,
        parameter: TypeId,
    },
    ConstrainedFunction(DeclId),
    InvalidType(TypeId),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum NormalizedType {
    I32,
    I64,
    Usize,
    Bool,
    Unit,
    String,
    Bytes,
    Bits,
    Buffer,
    Rune,
    Utf8Error,
    U8,
    Atom(String),
    List(Box<Self>),
    Array {
        item: Box<Self>,
        length: u64,
    },
    Slice(Box<Self>),
    Map {
        key: Box<Self>,
        value: Box<Self>,
    },
    Tuple(Vec<Self>),
    Function {
        parameters: Vec<Self>,
        result: Box<Self>,
    },
    Struct {
        declaration: DeclId,
        arguments: Vec<Self>,
    },
    Union(Vec<Self>),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct FunctionSpecializationKey {
    declaration: DeclId,
    substitution: Vec<(TypeId, NormalizedType)>,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct LayoutSpecializationKey {
    declaration: DeclId,
    arguments: Vec<NormalizedType>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EntryPointError {
    Missing,
    Private { span: Span },
    HasParameters { span: Span, count: usize },
    WrongResult { span: Span, found: TypeId },
    Generic { span: Span },
}

/// Selects the initial executable reachability root required by EL v1.
///
/// The input must already be verified Generic Core IR. Additional
/// compiler-defined roots can be appended here as later runtime features need
/// them; the returned function IDs are always in deterministic order.
pub fn executable_reachability_roots(
    module: &GenericModule,
) -> Result<ReachabilityRoots, EntryPointError> {
    let Some(entry) = module
        .functions
        .iter()
        .find(|function| function.module_name == "Main" && function.name == "main")
    else {
        return Err(EntryPointError::Missing);
    };
    if entry.visibility != Visibility::Public {
        return Err(EntryPointError::Private { span: entry.span });
    }
    if !entry.parameters.is_empty() {
        return Err(EntryPointError::HasParameters {
            span: entry.span,
            count: entry.parameters.len(),
        });
    }
    if !entry.type_parameters.is_empty() || !entry.constraints.is_empty() {
        return Err(EntryPointError::Generic { span: entry.span });
    }
    if !matches!(module.types.get(entry.result.0 as usize), Some(Type::I32)) {
        return Err(EntryPointError::WrongResult {
            span: entry.span,
            found: entry.result,
        });
    }

    let mut functions = vec![entry.id];
    functions.sort_unstable();
    functions.dedup();
    Ok(ReachabilityRoots { functions })
}

/// Specializes the reachable unconstrained Generic Core graph and its nominal
/// layouts in deterministic declaration/substitution order.
pub fn monomorphize(
    module: &GenericModule,
    roots: &ReachabilityRoots,
) -> Result<ConcreteModule, MonomorphizationError> {
    let concrete = Monomorphizer::new(module).run(roots)?;
    if let Err(errors) = verify_concrete(&concrete) {
        panic!("monomorphization produced invalid Concrete Core IR: {errors:?}");
    }
    Ok(concrete)
}

struct Monomorphizer<'a> {
    module: &'a GenericModule,
    functions_by_id: BTreeMap<FunctionId, &'a CoreFunction>,
    functions_by_decl: BTreeMap<DeclId, &'a CoreFunction>,
    structs_by_decl: BTreeMap<DeclId, &'a CoreStruct>,
    layouts: BTreeSet<LayoutSpecializationKey>,
    types: Vec<Type>,
}

impl<'a> Monomorphizer<'a> {
    fn new(module: &'a GenericModule) -> Self {
        Self {
            module,
            functions_by_id: module
                .functions
                .iter()
                .map(|function| (function.id, function))
                .collect(),
            functions_by_decl: module
                .functions
                .iter()
                .map(|function| (function.declaration, function))
                .collect(),
            structs_by_decl: module
                .structs
                .iter()
                .map(|structure| (structure.declaration, structure))
                .collect(),
            layouts: BTreeSet::new(),
            types: vec![Type::I32, Type::I64, Type::Bool, Type::Unit],
        }
    }

    fn run(mut self, roots: &ReachabilityRoots) -> Result<ConcreteModule, MonomorphizationError> {
        let mut pending = BTreeSet::new();
        let mut root_keys = Vec::new();
        for root in &roots.functions {
            let function = self
                .functions_by_id
                .get(root)
                .ok_or(MonomorphizationError::UnknownRoot(*root))?;
            let key = FunctionSpecializationKey {
                declaration: function.declaration,
                substitution: Vec::new(),
            };
            pending.insert(key.clone());
            root_keys.push(key);
        }

        let mut discovered = BTreeSet::new();
        while let Some(key) = pending.pop_first() {
            if !discovered.insert(key.clone()) {
                continue;
            }
            let function = self.function_for_key(&key)?;
            if !function.constraints.is_empty() {
                return Err(MonomorphizationError::ConstrainedFunction(
                    function.declaration,
                ));
            }
            let substitution = key.substitution.iter().cloned().collect::<BTreeMap<_, _>>();
            self.discover_function_layouts(function, &substitution)?;
            for block in &function.blocks {
                for operation in &block.operations {
                    if let Operation::Call {
                        function,
                        substitutions,
                        ..
                    } = operation
                    {
                        let called = self
                            .functions_by_id
                            .get(function)
                            .ok_or(MonomorphizationError::UnknownFunction(*function))?;
                        pending.insert(self.call_key(called, substitutions, &substitution)?);
                    }
                }
            }
        }

        let mut pending_layouts = self.layouts.clone();
        let mut concrete_layout_keys = BTreeSet::new();
        while let Some(key) = pending_layouts.pop_first() {
            if !concrete_layout_keys.insert(key.clone()) {
                continue;
            }
            let structure = self
                .structs_by_decl
                .get(&key.declaration)
                .ok_or(MonomorphizationError::UnknownStruct(key.declaration))?;
            let substitution = structure
                .parameters
                .iter()
                .copied()
                .zip(key.arguments.iter().cloned())
                .collect::<BTreeMap<_, _>>();
            for (_, field) in &structure.fields {
                let normalized = self.normalize(*field, &substitution)?;
                let mut found = BTreeSet::new();
                collect_layout_keys(&normalized, &mut found);
                for layout in found {
                    if !concrete_layout_keys.contains(&layout) {
                        pending_layouts.insert(layout);
                    }
                }
            }
        }

        let ids = discovered
            .iter()
            .enumerate()
            .map(|(index, key)| (key.clone(), FunctionId(index as u32)))
            .collect::<BTreeMap<_, _>>();
        let mut functions = Vec::with_capacity(discovered.len());
        for key in &discovered {
            functions.push(self.specialize_function(key, &ids)?);
        }
        let concrete_roots = ReachabilityRoots {
            functions: root_keys.iter().map(|key| ids[key]).collect::<Vec<_>>(),
        };

        let mut structs = Vec::with_capacity(concrete_layout_keys.len());
        for key in concrete_layout_keys {
            structs.push(self.specialize_struct(&key)?);
        }
        Ok(ConcreteModule {
            types: self.types,
            structs,
            functions,
            roots: concrete_roots,
        })
    }

    fn function_for_key(
        &self,
        key: &FunctionSpecializationKey,
    ) -> Result<&'a CoreFunction, MonomorphizationError> {
        self.functions_by_decl
            .get(&key.declaration)
            .copied()
            .ok_or(MonomorphizationError::UnknownDeclaration(key.declaration))
    }

    fn call_key(
        &self,
        called: &CoreFunction,
        call_substitutions: &[(TypeId, TypeId)],
        caller_substitution: &BTreeMap<TypeId, NormalizedType>,
    ) -> Result<FunctionSpecializationKey, MonomorphizationError> {
        let written = call_substitutions
            .iter()
            .copied()
            .collect::<BTreeMap<_, _>>();
        let mut substitution = Vec::with_capacity(called.type_parameters.len());
        for parameter in &called.type_parameters {
            let value = written.get(parameter).copied().ok_or(
                MonomorphizationError::MissingSubstitution {
                    declaration: called.declaration,
                    parameter: *parameter,
                },
            )?;
            substitution.push((*parameter, self.normalize(value, caller_substitution)?));
        }
        Ok(FunctionSpecializationKey {
            declaration: called.declaration,
            substitution,
        })
    }

    fn discover_function_layouts(
        &mut self,
        function: &CoreFunction,
        substitution: &BTreeMap<TypeId, NormalizedType>,
    ) -> Result<(), MonomorphizationError> {
        let mut types = function
            .parameters
            .iter()
            .map(|parameter| parameter.ty)
            .chain(std::iter::once(function.result))
            .chain(function.slots.iter().map(|slot| slot.ty))
            .collect::<Vec<_>>();
        for block in &function.blocks {
            types.extend(block.parameters.iter().map(|parameter| parameter.ty));
            for operation in &block.operations {
                operation_type_ids(operation, &mut types);
            }
            if let Terminator::Switch {
                subject_ty, cases, ..
            } = &block.terminator
            {
                types.push(*subject_ty);
                types.extend(cases.iter().filter_map(|(value, _)| match value {
                    SwitchValue::UnionMember(ty) => Some(*ty),
                    _ => None,
                }));
            }
        }
        for ty in types {
            let normalized = self.normalize(ty, substitution)?;
            collect_layout_keys(&normalized, &mut self.layouts);
        }
        Ok(())
    }

    fn normalize(
        &self,
        ty: TypeId,
        substitution: &BTreeMap<TypeId, NormalizedType>,
    ) -> Result<NormalizedType, MonomorphizationError> {
        if let Some(replacement) = substitution.get(&ty) {
            return Ok(replacement.clone());
        }
        let normalized = match self
            .module
            .types
            .get(ty.0 as usize)
            .ok_or(MonomorphizationError::InvalidType(ty))?
        {
            Type::I32 => NormalizedType::I32,
            Type::I64 => NormalizedType::I64,
            Type::Usize => NormalizedType::Usize,
            Type::Bool => NormalizedType::Bool,
            Type::Unit => NormalizedType::Unit,
            Type::String => NormalizedType::String,
            Type::Bytes => NormalizedType::Bytes,
            Type::Bits => NormalizedType::Bits,
            Type::Buffer => NormalizedType::Buffer,
            Type::Rune => NormalizedType::Rune,
            Type::Utf8Error => NormalizedType::Utf8Error,
            Type::U8 => NormalizedType::U8,
            Type::Atom(name) => NormalizedType::Atom(name.clone()),
            Type::List(item) => {
                NormalizedType::List(Box::new(self.normalize(*item, substitution)?))
            }
            Type::Array { item, length } => NormalizedType::Array {
                item: Box::new(self.normalize(*item, substitution)?),
                length: *length,
            },
            Type::Slice(item) => {
                NormalizedType::Slice(Box::new(self.normalize(*item, substitution)?))
            }
            Type::Map { key, value } => NormalizedType::Map {
                key: Box::new(self.normalize(*key, substitution)?),
                value: Box::new(self.normalize(*value, substitution)?),
            },
            Type::Tuple(elements) => NormalizedType::Tuple(
                elements
                    .iter()
                    .map(|element| self.normalize(*element, substitution))
                    .collect::<Result<_, _>>()?,
            ),
            Type::Function { parameters, result } => NormalizedType::Function {
                parameters: parameters
                    .iter()
                    .map(|parameter| self.normalize(*parameter, substitution))
                    .collect::<Result<_, _>>()?,
                result: Box::new(self.normalize(*result, substitution)?),
            },
            Type::Struct {
                declaration,
                arguments,
            } => NormalizedType::Struct {
                declaration: *declaration,
                arguments: arguments
                    .iter()
                    .map(|argument| self.normalize(*argument, substitution))
                    .collect::<Result<_, _>>()?,
            },
            Type::Union(members) => NormalizedType::Union(
                members
                    .iter()
                    .map(|member| self.normalize(*member, substitution))
                    .collect::<Result<_, _>>()?,
            ),
            Type::Parameter { owner, .. } => {
                return Err(MonomorphizationError::MissingSubstitution {
                    declaration: *owner,
                    parameter: ty,
                });
            }
        };
        Ok(normalized)
    }

    fn specialize_function(
        &mut self,
        key: &FunctionSpecializationKey,
        ids: &BTreeMap<FunctionSpecializationKey, FunctionId>,
    ) -> Result<CoreFunction, MonomorphizationError> {
        let source = self.function_for_key(key)?.clone();
        let substitution = key.substitution.iter().cloned().collect::<BTreeMap<_, _>>();
        let parameters = source
            .parameters
            .iter()
            .map(|parameter| {
                Ok(CoreParameter {
                    value: parameter.value,
                    ty: self.materialize_type(parameter.ty, &substitution)?,
                    origin: parameter.origin,
                })
            })
            .collect::<Result<_, MonomorphizationError>>()?;
        let slots = source
            .slots
            .iter()
            .map(|slot| {
                Ok(Slot {
                    id: slot.id,
                    ty: self.materialize_type(slot.ty, &substitution)?,
                    origin: slot.origin,
                })
            })
            .collect::<Result<_, MonomorphizationError>>()?;
        let mut blocks = source.blocks.clone();
        for block in &mut blocks {
            for parameter in &mut block.parameters {
                parameter.ty = self.materialize_type(parameter.ty, &substitution)?;
            }
            for operation in &mut block.operations {
                self.specialize_operation(operation, &substitution, ids)?;
            }
            if let Terminator::Switch {
                subject_ty, cases, ..
            } = &mut block.terminator
            {
                *subject_ty = self.materialize_type(*subject_ty, &substitution)?;
                for (value, _) in cases {
                    if let SwitchValue::UnionMember(ty) = value {
                        *ty = self.materialize_type(*ty, &substitution)?;
                    }
                }
            }
        }
        let specialization_arguments = key
            .substitution
            .iter()
            .map(|(_, argument)| self.intern_normalized(argument))
            .collect();
        Ok(CoreFunction {
            id: ids[key],
            declaration: source.declaration,
            module_name: source.module_name,
            name: source.name,
            visibility: source.visibility,
            span: source.span,
            parameters,
            type_parameters: Vec::new(),
            specialization_arguments,
            constraints: Vec::new(),
            result: self.materialize_type(source.result, &substitution)?,
            slots,
            blocks,
        })
    }

    fn specialize_operation(
        &mut self,
        operation: &mut Operation,
        substitution: &BTreeMap<TypeId, NormalizedType>,
        ids: &BTreeMap<FunctionSpecializationKey, FunctionId>,
    ) -> Result<(), MonomorphizationError> {
        match operation {
            Operation::Constant { ty, .. }
            | Operation::List { ty, .. }
            | Operation::ListReverse { ty, .. }
            | Operation::Array { ty, .. }
            | Operation::ArrayIndex { ty, .. }
            | Operation::SliceFromArray { ty, .. }
            | Operation::SliceSubslice { ty, .. }
            | Operation::SliceCopy { ty, .. }
            | Operation::StringBytes { ty, .. }
            | Operation::StringCodepoints { ty, .. }
            | Operation::StringFromBytes { ty, .. }
            | Operation::Utf8ErrorOffset { ty, .. }
            | Operation::RuneToString { ty, .. }
            | Operation::BufferNew { ty, .. }
            | Operation::BufferAppend { ty, .. }
            | Operation::BufferToBytes { ty, .. }
            | Operation::BytesToBits { ty, .. }
            | Operation::BitsSlice { ty, .. }
            | Operation::BitsToBytes { ty, .. }
            | Operation::BytesFromList { ty, .. }
            | Operation::BytesToList { ty, .. }
            | Operation::BytesSlice { ty, .. }
            | Operation::CollectionLength { ty, .. }
            | Operation::Map { ty, .. }
            | Operation::MapPut { ty, .. }
            | Operation::MapRemove { ty, .. }
            | Operation::Tuple { ty, .. }
            | Operation::Struct { ty, .. }
            | Operation::TupleProject { ty, .. }
            | Operation::StructProject { ty, .. }
            | Operation::ListHead { ty, .. }
            | Operation::ListTail { ty, .. }
            | Operation::CheckedArithmetic { ty, .. }
            | Operation::Load { ty, .. } => {
                *ty = self.materialize_type(*ty, substitution)?;
            }
            Operation::Compare { operand_ty, .. } => {
                *operand_ty = self.materialize_type(*operand_ty, substitution)?;
            }
            Operation::MapFetch { map_ty, ty, .. } => {
                *map_ty = self.materialize_type(*map_ty, substitution)?;
                *ty = self.materialize_type(*ty, substitution)?;
            }
            Operation::MapToList { map_ty, ty, .. } => {
                *map_ty = self.materialize_type(*map_ty, substitution)?;
                *ty = self.materialize_type(*ty, substitution)?;
            }
            Operation::Call {
                function,
                substitutions,
                ty,
                ..
            } => {
                let called = self
                    .functions_by_id
                    .get(function)
                    .ok_or(MonomorphizationError::UnknownFunction(*function))?;
                let key = self.call_key(called, substitutions, substitution)?;
                *function = ids[&key];
                substitutions.clear();
                *ty = self.materialize_type(*ty, substitution)?;
            }
            Operation::UnionInject { member, ty, .. } => {
                *member = self.materialize_type(*member, substitution)?;
                *ty = self.materialize_type(*ty, substitution)?;
            }
            Operation::UnionProject {
                member,
                union_ty,
                ty,
                ..
            } => {
                *member = self.materialize_type(*member, substitution)?;
                *union_ty = self.materialize_type(*union_ty, substitution)?;
                *ty = self.materialize_type(*ty, substitution)?;
            }
            Operation::Store { .. } => {}
        }
        Ok(())
    }

    fn materialize_type(
        &mut self,
        ty: TypeId,
        substitution: &BTreeMap<TypeId, NormalizedType>,
    ) -> Result<TypeId, MonomorphizationError> {
        let normalized = self.normalize(ty, substitution)?;
        Ok(self.intern_normalized(&normalized))
    }

    fn intern_normalized(&mut self, ty: &NormalizedType) -> TypeId {
        let materialized = match ty {
            NormalizedType::I32 => return TypeId(0),
            NormalizedType::I64 => return TypeId(1),
            NormalizedType::Usize => Type::Usize,
            NormalizedType::Bool => return TypeId(2),
            NormalizedType::Unit => return TypeId(3),
            NormalizedType::String => Type::String,
            NormalizedType::Bytes => Type::Bytes,
            NormalizedType::Bits => Type::Bits,
            NormalizedType::Buffer => Type::Buffer,
            NormalizedType::Rune => Type::Rune,
            NormalizedType::Utf8Error => Type::Utf8Error,
            NormalizedType::U8 => Type::U8,
            NormalizedType::Atom(name) => Type::Atom(name.clone()),
            NormalizedType::List(item) => Type::List(self.intern_normalized(item)),
            NormalizedType::Array { item, length } => Type::Array {
                item: self.intern_normalized(item),
                length: *length,
            },
            NormalizedType::Slice(item) => Type::Slice(self.intern_normalized(item)),
            NormalizedType::Map { key, value } => Type::Map {
                key: self.intern_normalized(key),
                value: self.intern_normalized(value),
            },
            NormalizedType::Tuple(elements) => Type::Tuple(
                elements
                    .iter()
                    .map(|element| self.intern_normalized(element))
                    .collect(),
            ),
            NormalizedType::Function { parameters, result } => Type::Function {
                parameters: parameters
                    .iter()
                    .map(|parameter| self.intern_normalized(parameter))
                    .collect(),
                result: self.intern_normalized(result),
            },
            NormalizedType::Struct {
                declaration,
                arguments,
            } => Type::Struct {
                declaration: *declaration,
                arguments: arguments
                    .iter()
                    .map(|argument| self.intern_normalized(argument))
                    .collect(),
            },
            NormalizedType::Union(members) => Type::Union(
                members
                    .iter()
                    .map(|member| self.intern_normalized(member))
                    .collect(),
            ),
        };
        if let Some(index) = self
            .types
            .iter()
            .position(|existing| existing == &materialized)
        {
            TypeId(index as u32)
        } else {
            let id = TypeId(self.types.len() as u32);
            self.types.push(materialized);
            id
        }
    }

    fn specialize_struct(
        &mut self,
        key: &LayoutSpecializationKey,
    ) -> Result<ConcreteStruct, MonomorphizationError> {
        let source = self
            .structs_by_decl
            .get(&key.declaration)
            .copied()
            .ok_or(MonomorphizationError::UnknownStruct(key.declaration))?
            .clone();
        let substitution = source
            .parameters
            .iter()
            .copied()
            .zip(key.arguments.iter().cloned())
            .collect::<BTreeMap<_, _>>();
        Ok(ConcreteStruct {
            declaration: source.declaration,
            name: source.name,
            arguments: key
                .arguments
                .iter()
                .map(|argument| self.intern_normalized(argument))
                .collect(),
            fields: source
                .fields
                .iter()
                .map(|(name, ty)| Ok((name.clone(), self.materialize_type(*ty, &substitution)?)))
                .collect::<Result<_, MonomorphizationError>>()?,
            origin: source.origin,
        })
    }
}

fn collect_layout_keys(ty: &NormalizedType, layouts: &mut BTreeSet<LayoutSpecializationKey>) {
    match ty {
        NormalizedType::List(item)
        | NormalizedType::Array { item, .. }
        | NormalizedType::Slice(item) => {
            collect_layout_keys(item, layouts);
        }
        NormalizedType::Map { key, value } => {
            collect_layout_keys(key, layouts);
            collect_layout_keys(value, layouts);
        }
        NormalizedType::Tuple(elements) | NormalizedType::Union(elements) => {
            for element in elements {
                collect_layout_keys(element, layouts);
            }
        }
        NormalizedType::Function { parameters, result } => {
            for parameter in parameters {
                collect_layout_keys(parameter, layouts);
            }
            collect_layout_keys(result, layouts);
        }
        NormalizedType::Struct {
            declaration,
            arguments,
        } => {
            layouts.insert(LayoutSpecializationKey {
                declaration: *declaration,
                arguments: arguments.clone(),
            });
            for argument in arguments {
                collect_layout_keys(argument, layouts);
            }
        }
        NormalizedType::I32
        | NormalizedType::I64
        | NormalizedType::Usize
        | NormalizedType::Bool
        | NormalizedType::Unit
        | NormalizedType::String
        | NormalizedType::Bytes
        | NormalizedType::Bits
        | NormalizedType::Buffer
        | NormalizedType::Rune
        | NormalizedType::Utf8Error
        | NormalizedType::U8
        | NormalizedType::Atom(_) => {}
    }
}

fn operation_type_ids(operation: &Operation, output: &mut Vec<TypeId>) {
    match operation {
        Operation::Constant { ty, .. }
        | Operation::List { ty, .. }
        | Operation::ListReverse { ty, .. }
        | Operation::Array { ty, .. }
        | Operation::ArrayIndex { ty, .. }
        | Operation::SliceFromArray { ty, .. }
        | Operation::SliceSubslice { ty, .. }
        | Operation::SliceCopy { ty, .. }
        | Operation::StringBytes { ty, .. }
        | Operation::StringCodepoints { ty, .. }
        | Operation::StringFromBytes { ty, .. }
        | Operation::Utf8ErrorOffset { ty, .. }
        | Operation::RuneToString { ty, .. }
        | Operation::BufferNew { ty, .. }
        | Operation::BufferAppend { ty, .. }
        | Operation::BufferToBytes { ty, .. }
        | Operation::BytesToBits { ty, .. }
        | Operation::BitsSlice { ty, .. }
        | Operation::BitsToBytes { ty, .. }
        | Operation::BytesFromList { ty, .. }
        | Operation::BytesToList { ty, .. }
        | Operation::BytesSlice { ty, .. }
        | Operation::CollectionLength { ty, .. }
        | Operation::Map { ty, .. }
        | Operation::MapPut { ty, .. }
        | Operation::MapRemove { ty, .. }
        | Operation::Tuple { ty, .. }
        | Operation::Struct { ty, .. }
        | Operation::TupleProject { ty, .. }
        | Operation::StructProject { ty, .. }
        | Operation::ListHead { ty, .. }
        | Operation::ListTail { ty, .. }
        | Operation::CheckedArithmetic { ty, .. }
        | Operation::Call { ty, .. }
        | Operation::Load { ty, .. } => output.push(*ty),
        Operation::Compare { operand_ty, .. } => output.push(*operand_ty),
        Operation::MapFetch { map_ty, ty, .. } => {
            output.push(*map_ty);
            output.push(*ty);
        }
        Operation::MapToList { map_ty, ty, .. } => {
            output.push(*map_ty);
            output.push(*ty);
        }
        Operation::UnionInject { member, ty, .. } => {
            output.push(*member);
            output.push(*ty);
        }
        Operation::UnionProject {
            member,
            union_ty,
            ty,
            ..
        } => {
            output.push(*member);
            output.push(*union_ty);
            output.push(*ty);
        }
        Operation::Store { .. } => {}
    }
}

/// Verifies the fully concrete, reachable representation accepted by backends.
pub fn verify_concrete(module: &ConcreteModule) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let type_count = module.types.len() as u32;

    for (index, ty) in module.types.iter().enumerate() {
        if matches!(ty, Type::Parameter { .. }) {
            errors.push(format!(
                "Concrete Core type t{index} contains a residual type parameter"
            ));
        }
        if managed_value_class(module, TypeId(index as u32)).is_none() {
            errors.push(format!(
                "Concrete Core type t{index} has no managed-value classification"
            ));
        }
    }

    let mut layout_keys = BTreeSet::new();
    for structure in &module.structs {
        let key = (structure.declaration, structure.arguments.clone());
        if !layout_keys.insert(key) {
            errors.push(format!(
                "concrete layout {:?} with arguments {:?} is emitted twice",
                structure.declaration, structure.arguments
            ));
        }
        for ty in structure
            .arguments
            .iter()
            .chain(structure.fields.iter().map(|(_, ty)| ty))
        {
            if ty.0 >= type_count {
                errors.push(format!(
                    "concrete layout {:?} references unknown type {ty:?}",
                    structure.declaration
                ));
            }
        }
    }
    for (index, ty) in module.types.iter().enumerate() {
        if let Type::Struct {
            declaration,
            arguments,
        } = ty
            && !layout_keys.contains(&(*declaration, arguments.clone()))
        {
            errors.push(format!(
                "Concrete Core type t{index} has no concrete layout"
            ));
        }
    }

    let signatures = module
        .functions
        .iter()
        .map(|function| {
            (
                function.id,
                (
                    function
                        .parameters
                        .iter()
                        .map(|parameter| parameter.ty)
                        .collect::<Vec<_>>(),
                    function.result,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    if signatures.len() != module.functions.len() {
        errors.push("Concrete Core defines a function ID more than once".to_owned());
    }
    let mut specialization_keys = BTreeSet::new();
    for function in &module.functions {
        if !function.type_parameters.is_empty() {
            errors.push(format!(
                "function {:?} contains residual type parameters",
                function.id
            ));
        }
        if !function.constraints.is_empty() {
            errors.push(format!(
                "function {:?} contains a residual constraint",
                function.id
            ));
        }
        if !specialization_keys.insert((
            function.declaration,
            function.specialization_arguments.clone(),
        )) {
            errors.push(format!(
                "function {:?} duplicates an existing specialization",
                function.id
            ));
        }
        for argument in &function.specialization_arguments {
            if argument.0 >= type_count {
                errors.push(format!(
                    "function {:?} has an unknown specialization argument",
                    function.id
                ));
            }
        }
        let values = function_value_types(function);
        for block in &function.blocks {
            for operation in &block.operations {
                if let Operation::Call {
                    function: called,
                    substitutions,
                    arguments,
                    ty,
                    ..
                } = operation
                {
                    if !substitutions.is_empty() {
                        errors.push(format!(
                            "call in {:?} contains a residual type substitution",
                            function.id
                        ));
                    }
                    if let Some((parameters, result)) = signatures.get(called)
                        && (arguments.len() != parameters.len()
                            || arguments
                                .iter()
                                .zip(parameters)
                                .any(|(argument, expected)| values.get(argument) != Some(expected))
                            || ty != result)
                    {
                        errors.push(format!(
                            "call in {:?} does not have the exact concrete signature",
                            function.id
                        ));
                    }
                }
                if let Operation::StructProject {
                    structure,
                    declaration,
                    index,
                    ty,
                    ..
                } = operation
                {
                    let layout = values
                        .get(structure)
                        .and_then(|source| module.types.get(source.0 as usize))
                        .and_then(|source| match source {
                            Type::Struct {
                                declaration: found,
                                arguments,
                            } if found == declaration => module.structs.iter().find(|layout| {
                                layout.declaration == *declaration && layout.arguments == *arguments
                            }),
                            _ => None,
                        });
                    if layout.and_then(|layout| layout.fields.get(*index).map(|field| field.1))
                        != Some(*ty)
                    {
                        errors.push(format!(
                            "struct projection in {:?} has no exact concrete layout field",
                            function.id
                        ));
                    }
                }
            }
        }
    }

    let root_set = module
        .roots
        .functions
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if root_set.len() != module.roots.functions.len()
        || !module
            .roots
            .functions
            .windows(2)
            .all(|pair| pair[0] < pair[1])
    {
        errors.push("Concrete Core roots are not unique and sorted".to_owned());
    }
    for root in &root_set {
        if !signatures.contains_key(root) {
            errors.push(format!("Concrete Core references unknown root {root:?}"));
        }
    }
    let mut reachable = root_set.clone();
    let mut pending = root_set;
    while let Some(function) = pending.pop_first() {
        let Some(body) = module
            .functions
            .iter()
            .find(|candidate| candidate.id == function)
        else {
            continue;
        };
        for called in body.blocks.iter().flat_map(|block| {
            block
                .operations
                .iter()
                .filter_map(|operation| match operation {
                    Operation::Call { function, .. } => Some(*function),
                    _ => None,
                })
        }) {
            if reachable.insert(called) {
                pending.insert(called);
            }
        }
    }
    for function in &module.functions {
        if !reachable.contains(&function.id) {
            errors.push(format!(
                "function {:?} is not reachable from a Concrete Core root",
                function.id
            ));
        }
    }

    let mut shared_functions = module.functions.clone();
    for function in &mut shared_functions {
        function.specialization_arguments.clear();
    }
    let shared = GenericModule {
        types: module.types.clone(),
        structs: module
            .structs
            .iter()
            .map(|structure| CoreStruct {
                declaration: structure.declaration,
                name: structure.name.clone(),
                parameters: Vec::new(),
                fields: structure.fields.clone(),
                origin: structure.origin,
            })
            .collect(),
        implementations: Vec::new(),
        functions: shared_functions,
    };
    if let Err(mut shared_errors) = verify(&shared) {
        errors.append(&mut shared_errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn function_value_types(function: &CoreFunction) -> BTreeMap<ValueId, TypeId> {
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
                    | Operation::StringCodepoints { result, ty, .. }
                    | Operation::StringFromBytes { result, ty, .. }
                    | Operation::Utf8ErrorOffset { result, ty, .. }
                    | Operation::RuneToString { result, ty, .. }
                    | Operation::BufferNew { result, ty, .. }
                    | Operation::BufferAppend { result, ty, .. }
                    | Operation::BufferToBytes { result, ty, .. }
                    | Operation::BytesToBits { result, ty, .. }
                    | Operation::BitsSlice { result, ty, .. }
                    | Operation::BitsToBytes { result, ty, .. }
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

/// Verifies ownership, definitions, operation typing, slot initialization, and returns.
pub fn verify(module: &GenericModule) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let type_count = module.types.len() as u32;
    for (index, ty) in module.types.iter().enumerate() {
        let referenced = match ty {
            Type::List(item) | Type::Array { item, .. } => std::slice::from_ref(item),
            Type::Map { key, value } => {
                if key.0 >= type_count || value.0 >= type_count {
                    errors.push(format!("type t{index} references an unknown map component"));
                }
                &[]
            }
            Type::Tuple(elements) | Type::Union(elements) => elements.as_slice(),
            Type::Function { parameters, .. } => parameters.as_slice(),
            Type::Struct { arguments, .. } => arguments.as_slice(),
            _ => &[],
        };
        for reference in referenced {
            if reference.0 >= type_count {
                errors.push(format!(
                    "type t{index} references unknown type {reference:?}"
                ));
            }
        }
        if let Type::Function { result, .. } = ty
            && result.0 >= type_count
        {
            errors.push(format!(
                "function type t{index} references unknown result {result:?}"
            ));
        }
        if let Type::Union(members) = ty {
            if members.len() < 2 {
                errors.push(format!("union type t{index} has fewer than two members"));
            }
            let mut unique = BTreeSet::new();
            for member in members {
                if member.0 >= type_count {
                    errors.push(format!(
                        "union type t{index} references unknown member {member:?}"
                    ));
                } else if matches!(module.types[member.0 as usize], Type::Union(_)) {
                    errors.push(format!("union type t{index} contains an unflattened union"));
                }
                if !unique.insert(*member) {
                    errors.push(format!(
                        "union type t{index} contains duplicate member {member:?}"
                    ));
                }
            }
        }
    }
    let signatures = module
        .functions
        .iter()
        .map(|function| (function.id, (function.parameters.len(), function.result)))
        .collect::<BTreeMap<_, _>>();
    let structs_by_decl = module
        .structs
        .iter()
        .map(|structure| (structure.declaration, structure))
        .collect::<BTreeMap<_, _>>();
    let mut implementation_ids = BTreeSet::new();
    for implementation in &module.implementations {
        if !implementation_ids.insert(implementation.id) {
            errors.push(format!(
                "implementation {:?} is defined twice",
                implementation.id
            ));
        }
        if implementation.target.0 >= type_count {
            errors.push(format!(
                "implementation {:?} has unknown target type",
                implementation.id
            ));
        }
        for (_, ty) in &implementation.associated_types {
            if ty.0 >= type_count {
                errors.push(format!(
                    "implementation {:?} has unknown associated type",
                    implementation.id
                ));
            }
        }
    }
    for function in &module.functions {
        if !function.specialization_arguments.is_empty() {
            errors.push(format!(
                "Generic Core function {:?} contains concrete specialization arguments",
                function.id
            ));
        }
        let mut values = BTreeMap::new();
        for parameter in &function.parameters {
            if parameter.ty.0 >= type_count {
                errors.push(format!(
                    "parameter {:?} has unknown type {:?}",
                    parameter.value, parameter.ty
                ));
            }
            if values.insert(parameter.value, parameter.ty).is_some() {
                errors.push(format!(
                    "parameter value {:?} is defined twice",
                    parameter.value
                ));
            }
        }
        let mut slots = BTreeMap::new();
        for slot in &function.slots {
            if slot.ty.0 >= type_count {
                errors.push(format!("slot {:?} has unknown type {:?}", slot.id, slot.ty));
            }
            if slots.insert(slot.id, slot.ty).is_some() {
                errors.push(format!("slot {:?} is declared twice", slot.id));
            }
        }
        let blocks = function
            .blocks
            .iter()
            .map(|block| (block.id, block))
            .collect::<BTreeMap<_, _>>();
        if blocks.len() != function.blocks.len() {
            errors.push(format!("function {:?} defines a block twice", function.id));
        }
        if !blocks.contains_key(&BlockId(0)) {
            errors.push(format!("function {:?} has no entry block", function.id));
        }
        let mut predecessors = BTreeSet::new();
        let mut failure_predecessors = BTreeSet::new();
        for block in &function.blocks {
            for operation in &block.operations {
                if let Operation::CheckedArithmetic { failures, .. } = operation {
                    predecessors.extend(failures.iter().map(|(_, target)| *target));
                    failure_predecessors.extend(failures.iter().map(|(_, target)| *target));
                }
                if let Operation::ArrayIndex { failure, .. }
                | Operation::SliceSubslice { failure, .. }
                | Operation::BitsSlice { failure, .. }
                | Operation::BytesSlice { failure, .. } = operation
                {
                    predecessors.insert(*failure);
                    failure_predecessors.insert(*failure);
                }
            }
            match &block.terminator {
                Terminator::Branch { target, .. } => {
                    predecessors.insert(*target);
                }
                Terminator::CondBranch {
                    then_target,
                    else_target,
                    ..
                } => {
                    predecessors.insert(*then_target);
                    predecessors.insert(*else_target);
                }
                Terminator::Switch { cases, default, .. } => {
                    predecessors.extend(cases.iter().map(|(_, target)| *target));
                    predecessors.extend(default.iter().copied());
                }
                Terminator::Return { .. }
                | Terminator::Failure { .. }
                | Terminator::Unreachable { .. } => {}
            }
        }
        for block in &function.blocks {
            for parameter in &block.parameters {
                if parameter.ty.0 >= type_count {
                    errors.push(format!("block {:?} parameter has unknown type", block.id));
                }
                if values.insert(parameter.value, parameter.ty).is_some() {
                    errors.push(format!("value {:?} is defined twice", parameter.value));
                }
            }
        }
        let mut initialized_by_block = BTreeMap::from([(BlockId(0), BTreeSet::new())]);
        let operation_context = OperationVerifyContext {
            types: &module.types,
            signatures: &signatures,
            structs: &structs_by_decl,
            slots: &slots,
        };
        let mut ordered = function.blocks.iter().collect::<Vec<_>>();
        ordered.sort_by_key(|block| block.id);
        for block in ordered {
            let mut initialized = initialized_by_block
                .get(&block.id)
                .cloned()
                .unwrap_or_default();
            for operation in &block.operations {
                verify_operation(
                    operation,
                    &operation_context,
                    &mut initialized,
                    &mut values,
                    &mut errors,
                );
                if let Operation::CheckedArithmetic {
                    operator,
                    failures,
                    origin,
                    ..
                } = operation
                {
                    let actual = failures
                        .iter()
                        .map(|(category, _)| *category)
                        .collect::<Vec<_>>();
                    if actual != arithmetic_failure_categories(*operator) {
                        errors.push(format!(
                            "checked arithmetic in {:?} has an invalid failure plan",
                            block.id
                        ));
                    }
                    let mut targets = BTreeSet::new();
                    for (category, target) in failures {
                        if !targets.insert(*target) {
                            errors.push(format!(
                                "checked arithmetic in {:?} reuses a failure block",
                                block.id
                            ));
                        }
                        match blocks.get(target) {
                            Some(target_block)
                                if target_block.parameters.is_empty()
                                    && target_block.operations.is_empty()
                                    && matches!(
                                        target_block.terminator,
                                        Terminator::Failure {
                                            category: found,
                                            origin: found_origin,
                                        } if found == *category && found_origin == *origin
                                    ) => {}
                            _ => errors.push(format!(
                                "checked arithmetic in {:?} has an invalid failure target",
                                block.id
                            )),
                        }
                    }
                }
                if let Operation::ArrayIndex {
                    failure, origin, ..
                }
                | Operation::SliceSubslice {
                    failure, origin, ..
                }
                | Operation::BitsSlice {
                    failure, origin, ..
                }
                | Operation::BytesSlice {
                    failure, origin, ..
                } = operation
                {
                    match blocks.get(failure) {
                        Some(target_block)
                            if target_block.parameters.is_empty()
                                && target_block.operations.is_empty()
                                && matches!(
                                    target_block.terminator,
                                    Terminator::Failure {
                                        category: CoreFailureCategory::IndexOutOfBounds,
                                        origin: found_origin,
                                    } if found_origin == *origin
                                ) => {}
                        _ => errors.push(format!(
                            "bounds-checked operation in {:?} has an invalid failure target",
                            block.id
                        )),
                    }
                }
            }
            let mut propagate = |target: BlockId| {
                initialized_by_block
                    .entry(target)
                    .and_modify(|known| known.retain(|slot| initialized.contains(slot)))
                    .or_insert_with(|| initialized.clone());
            };
            match &block.terminator {
                Terminator::Return { value, .. } => match values.get(value) {
                    Some(ty) if *ty == function.result => {}
                    Some(ty) => errors.push(format!(
                        "function {:?} returns {ty:?}, expected {:?}",
                        function.id, function.result
                    )),
                    None => errors.push(format!(
                        "function {:?} returns undefined value {value:?}",
                        function.id
                    )),
                },
                Terminator::Branch {
                    target, arguments, ..
                } => {
                    verify_edge(block.id, *target, arguments, &blocks, &values, &mut errors);
                    propagate(*target);
                }
                Terminator::CondBranch {
                    condition,
                    then_target,
                    else_target,
                    ..
                } => {
                    if values.get(condition) != Some(&TypeId(2)) {
                        errors.push(format!(
                            "conditional branch in {:?} has a non-bool condition",
                            block.id
                        ));
                    }
                    verify_edge(block.id, *then_target, &[], &blocks, &values, &mut errors);
                    verify_edge(block.id, *else_target, &[], &blocks, &values, &mut errors);
                    propagate(*then_target);
                    propagate(*else_target);
                }
                Terminator::Switch {
                    subject,
                    subject_ty,
                    cases,
                    default,
                    ..
                } => {
                    let actual_subject_ty = values.get(subject).copied();
                    if actual_subject_ty.is_none() {
                        errors.push(format!("switch in {:?} uses an undefined value", block.id));
                    } else if actual_subject_ty != Some(*subject_ty) {
                        errors.push(format!(
                            "switch in {:?} has an incorrect subject type",
                            block.id
                        ));
                    }
                    let mut seen = BTreeSet::new();
                    for (case, target) in cases {
                        if !seen.insert(case_key(case)) {
                            errors.push(format!("switch in {:?} has a duplicate case", block.id));
                        }
                        let compatible = match (case, module.types.get(subject_ty.0 as usize)) {
                            (SwitchValue::Boolean(_), Some(Type::Bool))
                            | (SwitchValue::Integer(_), Some(Type::I32 | Type::I64 | Type::U8))
                            | (
                                SwitchValue::ListEmpty | SwitchValue::ListCons,
                                Some(Type::List(_)),
                            ) => true,
                            (SwitchValue::Atom(value), Some(Type::Atom(expected))) => {
                                value == expected
                            }
                            (SwitchValue::UnionMember(member), Some(Type::Union(members))) => {
                                members.contains(member)
                            }
                            _ => false,
                        };
                        if !compatible {
                            errors.push(format!(
                                "switch in {:?} has a case incompatible with its subject",
                                block.id
                            ));
                        }
                        verify_edge(block.id, *target, &[], &blocks, &values, &mut errors);
                        propagate(*target);
                    }
                    if let Some(target) = default {
                        verify_edge(block.id, *target, &[], &blocks, &values, &mut errors);
                        propagate(*target);
                    }
                }
                Terminator::Failure { .. } if !failure_predecessors.contains(&block.id) => {
                    errors.push(format!(
                        "function {:?} has an unjustified failure terminator",
                        function.id
                    ));
                }
                Terminator::Failure { .. } => {}
                Terminator::Unreachable { .. }
                    if block.id == BlockId(0) || !predecessors.contains(&block.id) =>
                {
                    errors.push(format!(
                        "function {:?} has an unjustified unreachable terminator",
                        function.id
                    ));
                }
                Terminator::Unreachable { .. } => {}
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn verify_edge(
    source: BlockId,
    target: BlockId,
    arguments: &[ValueId],
    blocks: &BTreeMap<BlockId, &Block>,
    values: &BTreeMap<ValueId, TypeId>,
    errors: &mut Vec<String>,
) {
    let Some(target_block) = blocks.get(&target) else {
        errors.push(format!(
            "block {source:?} branches to unknown block {target:?}"
        ));
        return;
    };
    if arguments.len() != target_block.parameters.len() {
        errors.push(format!(
            "edge {source:?} -> {target:?} supplies {} arguments, expected {}",
            arguments.len(),
            target_block.parameters.len()
        ));
        return;
    }
    for (argument, parameter) in arguments.iter().zip(&target_block.parameters) {
        if values.get(argument) != Some(&parameter.ty) {
            errors.push(format!(
                "edge {source:?} -> {target:?} has an incorrectly typed argument"
            ));
        }
    }
}

fn case_key(value: &SwitchValue) -> String {
    match value {
        SwitchValue::Boolean(value) => format!("b:{value}"),
        SwitchValue::Integer(value) => format!("i:{value}"),
        SwitchValue::Atom(value) => format!("a:{value}"),
        SwitchValue::UnionMember(value) => format!("u:{}", value.0),
        SwitchValue::ListEmpty => "l:empty".to_owned(),
        SwitchValue::ListCons => "l:cons".to_owned(),
    }
}

struct OperationVerifyContext<'a> {
    types: &'a [Type],
    signatures: &'a BTreeMap<FunctionId, (usize, TypeId)>,
    structs: &'a BTreeMap<DeclId, &'a CoreStruct>,
    slots: &'a BTreeMap<SlotId, TypeId>,
}

fn verify_operation(
    operation: &Operation,
    context: &OperationVerifyContext<'_>,
    initialized: &mut BTreeSet<SlotId>,
    values: &mut BTreeMap<ValueId, TypeId>,
    errors: &mut Vec<String>,
) {
    let OperationVerifyContext {
        types,
        signatures,
        structs,
        slots,
    } = context;
    let type_count = types.len() as u32;
    let define = |id: ValueId,
                  ty: TypeId,
                  values: &mut BTreeMap<ValueId, TypeId>,
                  errors: &mut Vec<String>| {
        if ty.0 >= type_count {
            errors.push(format!("value {id:?} has unknown type {ty:?}"));
        }
        if values.insert(id, ty).is_some() {
            errors.push(format!("value {id:?} is defined twice"));
        }
    };
    match operation {
        Operation::Constant {
            result,
            constant,
            ty,
            ..
        } => {
            let valid = match (constant, types.get(ty.0 as usize)) {
                (Constant::Integer(_), Some(Type::I32 | Type::I64 | Type::Usize | Type::U8))
                | (Constant::Boolean(_), Some(Type::Bool))
                | (Constant::Unit, Some(Type::Unit))
                | (Constant::String(_), Some(Type::String)) => true,
                (Constant::Rune(_), Some(Type::Rune)) => true,
                (Constant::Atom(name), Some(Type::Atom(expected))) => name == expected,
                _ => false,
            };
            if !valid {
                errors.push(format!("constant {result:?} has incompatible type {ty:?}"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::List {
            result,
            elements,
            tail,
            ty,
            ..
        } => {
            let Some(Type::List(item)) = types.get(ty.0 as usize) else {
                errors.push(format!("list {result:?} has a non-list result type"));
                define(*result, *ty, values, errors);
                return;
            };
            for element in elements {
                if values.get(element) != Some(item) {
                    errors.push(format!("list {result:?} has an incorrectly typed element"));
                }
            }
            if tail.is_some_and(|tail| values.get(&tail) != Some(ty)) {
                errors.push(format!("list {result:?} has an incorrectly typed tail"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::ListReverse {
            result, list, ty, ..
        } => {
            if values.get(list) != Some(ty)
                || !matches!(types.get(ty.0 as usize), Some(Type::List(_)))
            {
                errors.push(format!("list reverse {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::Tuple {
            result,
            elements,
            ty,
            ..
        } => {
            let Some(Type::Tuple(element_types)) = types.get(ty.0 as usize) else {
                errors.push(format!("tuple {result:?} has a non-tuple result type"));
                define(*result, *ty, values, errors);
                return;
            };
            if elements.len() != element_types.len()
                || elements
                    .iter()
                    .zip(element_types)
                    .any(|(element, ty)| values.get(element) != Some(ty))
            {
                errors.push(format!("tuple {result:?} has incorrectly typed elements"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::Struct {
            result,
            declaration,
            fields,
            ty,
            ..
        } => {
            let field_count = structs
                .get(declaration)
                .map(|structure| structure.fields.len());
            if !matches!(types.get(ty.0 as usize), Some(Type::Struct { declaration: found, .. }) if found == declaration)
                || field_count.is_none()
            {
                errors.push(format!("struct {result:?} has an invalid nominal type"));
            }
            let mut seen = BTreeSet::new();
            for (index, value) in fields {
                if field_count.is_none_or(|count| *index >= count)
                    || !seen.insert(*index)
                    || !values.contains_key(value)
                {
                    errors.push(format!("struct {result:?} has an invalid field"));
                }
            }
            if field_count.is_some_and(|count| seen.len() != count) {
                errors.push(format!("struct {result:?} does not initialize every field"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::TupleProject {
            result,
            tuple,
            index,
            ty,
            ..
        } => {
            if !matches!(values.get(tuple).and_then(|source| types.get(source.0 as usize)), Some(Type::Tuple(elements)) if elements.get(*index) == Some(ty))
            {
                errors.push(format!(
                    "tuple projection {result:?} has an invalid source or index"
                ));
            }
            define(*result, *ty, values, errors);
        }
        Operation::StructProject {
            result,
            structure,
            declaration,
            index,
            ty,
            ..
        } => {
            if !matches!(values.get(structure).and_then(|source| types.get(source.0 as usize)), Some(Type::Struct { declaration: found, .. }) if found == declaration)
                || structs
                    .get(declaration)
                    .is_none_or(|structure| *index >= structure.fields.len())
            {
                errors.push(format!(
                    "struct projection {result:?} has an invalid source or field"
                ));
            }
            define(*result, *ty, values, errors);
        }
        Operation::ListHead {
            result, list, ty, ..
        } => {
            if !matches!(values.get(list).and_then(|source| types.get(source.0 as usize)), Some(Type::List(item)) if item == ty)
            {
                errors.push(format!("list head {result:?} has an invalid source"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::ListTail {
            result, list, ty, ..
        } => {
            if values.get(list) != Some(ty)
                || !matches!(types.get(ty.0 as usize), Some(Type::List(_)))
            {
                errors.push(format!("list tail {result:?} has an invalid source"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::Array {
            result,
            elements,
            ty,
            ..
        } => {
            let Some(Type::Array { item, length }) = types.get(ty.0 as usize) else {
                errors.push(format!("array {result:?} has a non-array result type"));
                define(*result, *ty, values, errors);
                return;
            };
            if *length != elements.len() as u64
                || elements
                    .iter()
                    .any(|element| values.get(element) != Some(item))
            {
                errors.push(format!("array {result:?} has incorrectly typed elements"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::ArrayIndex {
            result,
            array,
            index,
            length,
            ty,
            ..
        } => {
            let source = values
                .get(array)
                .and_then(|source| types.get(source.0 as usize));
            let valid = match (source, length) {
                (
                    Some(Type::Array {
                        item,
                        length: source_length,
                    }),
                    Some(length),
                ) => item == ty && source_length == length,
                (Some(Type::Slice(item)), None) => item == ty,
                (Some(Type::Bytes), None) => matches!(types.get(ty.0 as usize), Some(Type::U8)),
                (Some(Type::Bits), None) => matches!(types.get(ty.0 as usize), Some(Type::Bool)),
                _ => false,
            };
            if !valid {
                errors.push(format!("index {result:?} has an invalid source or length"));
            }
            if !matches!(
                values.get(index).and_then(|ty| types.get(ty.0 as usize)),
                Some(Type::Usize)
            ) {
                errors.push(format!("array index {result:?} has a non-usize index"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::SliceFromArray {
            result,
            array,
            length,
            ty,
            ..
        } => {
            let valid = matches!(
                (values.get(array).and_then(|source| types.get(source.0 as usize)), types.get(ty.0 as usize)),
                (Some(Type::Array { item: source, length: source_length }), Some(Type::Slice(item)))
                    if source == item && source_length == length
            );
            if !valid {
                errors.push(format!("slice construction {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::SliceSubslice {
            result,
            slice,
            start,
            length,
            ty,
            ..
        } => {
            if values.get(slice) != Some(ty)
                || !matches!(types.get(ty.0 as usize), Some(Type::Slice(_)))
                || !matches!(
                    values.get(start).and_then(|ty| types.get(ty.0 as usize)),
                    Some(Type::Usize)
                )
                || !matches!(
                    values.get(length).and_then(|ty| types.get(ty.0 as usize)),
                    Some(Type::Usize)
                )
            {
                errors.push(format!("subslice {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::SliceCopy {
            result, slice, ty, ..
        } => {
            if values.get(slice) != Some(ty)
                || !matches!(types.get(ty.0 as usize), Some(Type::Slice(_)))
            {
                errors.push(format!("slice copy {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::StringBytes {
            result, string, ty, ..
        } => {
            if !matches!(
                values
                    .get(string)
                    .and_then(|source| types.get(source.0 as usize)),
                Some(Type::String)
            ) || !matches!(types.get(ty.0 as usize), Some(Type::Bytes))
            {
                errors.push(format!("string bytes {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::StringCodepoints {
            result, string, ty, ..
        } => {
            if !matches!(
                values
                    .get(string)
                    .and_then(|source| types.get(source.0 as usize)),
                Some(Type::String)
            ) || !matches!(
                types.get(ty.0 as usize),
                Some(Type::List(item)) if matches!(types.get(item.0 as usize), Some(Type::Rune))
            ) {
                errors.push(format!("string codepoints {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::StringFromBytes {
            result, bytes, ty, ..
        } => {
            if !matches!(
                values
                    .get(bytes)
                    .and_then(|source| types.get(source.0 as usize)),
                Some(Type::Bytes)
            ) || !is_utf8_result_type(types, *ty)
            {
                errors.push(format!("string from-bytes {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::Utf8ErrorOffset {
            result, error, ty, ..
        } => {
            if !matches!(
                values
                    .get(error)
                    .and_then(|source| types.get(source.0 as usize)),
                Some(Type::Utf8Error)
            ) || !matches!(types.get(ty.0 as usize), Some(Type::Usize))
            {
                errors.push(format!("UTF-8 error offset {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::RuneToString {
            result, rune, ty, ..
        } => {
            if !matches!(
                values
                    .get(rune)
                    .and_then(|source| types.get(source.0 as usize)),
                Some(Type::Rune)
            ) || !matches!(types.get(ty.0 as usize), Some(Type::String))
            {
                errors.push(format!("rune to-string {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::BufferNew { result, ty, .. } => {
            if !matches!(types.get(ty.0 as usize), Some(Type::Buffer)) {
                errors.push(format!("buffer construction {result:?} has invalid type"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::BufferAppend {
            result,
            buffer,
            value,
            kind,
            ty,
            ..
        } => {
            let valid_value = match kind {
                BufferAppendKind::Byte => matches!(
                    values
                        .get(value)
                        .and_then(|source| types.get(source.0 as usize)),
                    Some(Type::U8)
                ),
                BufferAppendKind::Bytes => matches!(
                    values
                        .get(value)
                        .and_then(|source| types.get(source.0 as usize)),
                    Some(Type::Bytes)
                ),
                BufferAppendKind::String => matches!(
                    values
                        .get(value)
                        .and_then(|source| types.get(source.0 as usize)),
                    Some(Type::String)
                ),
            };
            if values.get(buffer) != Some(ty)
                || !matches!(types.get(ty.0 as usize), Some(Type::Buffer))
                || !valid_value
            {
                errors.push(format!("buffer append {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::BufferToBytes {
            result, buffer, ty, ..
        } => {
            if !matches!(
                values
                    .get(buffer)
                    .and_then(|source| types.get(source.0 as usize)),
                Some(Type::Buffer)
            ) || !matches!(types.get(ty.0 as usize), Some(Type::Bytes))
            {
                errors.push(format!("buffer to-bytes {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::BytesToBits {
            result, bytes, ty, ..
        } => {
            if !matches!(
                values
                    .get(bytes)
                    .and_then(|source| types.get(source.0 as usize)),
                Some(Type::Bytes)
            ) || !matches!(types.get(ty.0 as usize), Some(Type::Bits))
            {
                errors.push(format!("bytes to-bits {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::BitsSlice {
            result,
            bits,
            start,
            length,
            ty,
            ..
        } => {
            if values.get(bits) != Some(ty)
                || !matches!(types.get(ty.0 as usize), Some(Type::Bits))
                || !matches!(
                    values.get(start).and_then(|ty| types.get(ty.0 as usize)),
                    Some(Type::Usize)
                )
                || !matches!(
                    values.get(length).and_then(|ty| types.get(ty.0 as usize)),
                    Some(Type::Usize)
                )
            {
                errors.push(format!("bits slice {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::BitsToBytes {
            result, bits, ty, ..
        } => {
            if !matches!(
                values
                    .get(bits)
                    .and_then(|source| types.get(source.0 as usize)),
                Some(Type::Bits)
            ) || !is_option_bytes_type(types, *ty)
            {
                errors.push(format!("bits to-bytes {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::BytesFromList {
            result, list, ty, ..
        } => {
            if !matches!(
                values.get(list).and_then(|source| types.get(source.0 as usize)),
                Some(Type::List(item)) if matches!(types.get(item.0 as usize), Some(Type::U8))
            ) || !matches!(types.get(ty.0 as usize), Some(Type::Bytes))
            {
                errors.push(format!("bytes from-list {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::BytesToList {
            result, bytes, ty, ..
        } => {
            if !matches!(
                values
                    .get(bytes)
                    .and_then(|source| types.get(source.0 as usize)),
                Some(Type::Bytes)
            ) || !matches!(
                types.get(ty.0 as usize),
                Some(Type::List(item)) if matches!(types.get(item.0 as usize), Some(Type::U8))
            ) {
                errors.push(format!("bytes to-list {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::BytesSlice {
            result,
            bytes,
            start,
            length,
            ty,
            ..
        } => {
            if values.get(bytes) != Some(ty)
                || !matches!(types.get(ty.0 as usize), Some(Type::Bytes))
                || !matches!(
                    values.get(start).and_then(|ty| types.get(ty.0 as usize)),
                    Some(Type::Usize)
                )
                || !matches!(
                    values.get(length).and_then(|ty| types.get(ty.0 as usize)),
                    Some(Type::Usize)
                )
            {
                errors.push(format!("bytes slice {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::CollectionLength {
            result,
            value,
            known_length,
            ty,
            ..
        } => {
            let source = values.get(value).and_then(|ty| types.get(ty.0 as usize));
            let valid_source = match (source, known_length) {
                (Some(Type::Array { length, .. }), Some(known)) => length == known,
                (
                    Some(
                        Type::String
                        | Type::Bytes
                        | Type::Bits
                        | Type::Buffer
                        | Type::Slice(_)
                        | Type::Map { .. },
                    ),
                    None,
                ) => true,
                _ => false,
            };
            if !valid_source || !matches!(types.get(ty.0 as usize), Some(Type::Usize)) {
                errors.push(format!("collection length {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::Map {
            result,
            entries,
            ty,
            ..
        } => {
            let Some(Type::Map { key, value }) = types.get(ty.0 as usize) else {
                errors.push(format!("map {result:?} has a non-map result type"));
                define(*result, *ty, values, errors);
                return;
            };
            if entries.iter().any(|(entry_key, entry_value)| {
                values.get(entry_key) != Some(key) || values.get(entry_value) != Some(value)
            }) {
                errors.push(format!("map {result:?} has incorrectly typed entries"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::MapPut {
            result,
            map,
            key,
            value,
            ty,
            ..
        } => {
            let valid = matches!(
                types.get(ty.0 as usize),
                Some(Type::Map {
                    key: expected_key,
                    value: expected_value,
                }) if values.get(map) == Some(ty)
                    && values.get(key) == Some(expected_key)
                    && values.get(value) == Some(expected_value)
            );
            if !valid {
                errors.push(format!("map put {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::MapRemove {
            result,
            map,
            key,
            ty,
            ..
        } => {
            let valid = matches!(
                types.get(ty.0 as usize),
                Some(Type::Map { key: expected_key, .. })
                    if values.get(map) == Some(ty) && values.get(key) == Some(expected_key)
            );
            if !valid {
                errors.push(format!("map remove {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::MapFetch {
            result,
            map,
            key,
            map_ty,
            ty,
            ..
        } => {
            let map_value = match types.get(map_ty.0 as usize) {
                Some(Type::Map {
                    key: expected_key,
                    value,
                }) if values.get(map) == Some(map_ty) && values.get(key) == Some(expected_key) => {
                    Some(*value)
                }
                _ => None,
            };
            let valid_map = map_value.is_some();
            let valid_result = map_value.is_some_and(|value| {
                matches!(types.get(ty.0 as usize), Some(Type::Union(members)) if members.len() == 2
                    && members.iter().any(|member| {
                        matches!(types.get(member.0 as usize), Some(Type::Atom(name)) if name == "none")
                    })
                    && members.iter().any(|member| {
                        matches!(types.get(member.0 as usize), Some(Type::Tuple(items)) if items.len() == 2
                            && items[1] == value
                            && matches!(types.get(items[0].0 as usize), Some(Type::Atom(name)) if name == "some"))
                    }))
            });
            if !valid_map || !valid_result {
                errors.push(format!("map fetch {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::MapToList {
            result,
            map,
            map_ty,
            ty,
            ..
        } => {
            let valid = match (types.get(map_ty.0 as usize), types.get(ty.0 as usize)) {
                (Some(Type::Map { key, value }), Some(Type::List(pair)))
                    if values.get(map) == Some(map_ty) =>
                {
                    matches!(types.get(pair.0 as usize), Some(Type::Tuple(items)) if items.as_slice() == [*key, *value])
                }
                _ => false,
            };
            if !valid {
                errors.push(format!("map to-list {result:?} has invalid types"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::CheckedArithmetic {
            result,
            left,
            right,
            ty,
            ..
        } => {
            if values.get(left) != Some(ty)
                || values.get(right) != Some(ty)
                || !matches!(ty.0, 0 | 1)
            {
                errors.push(format!("arithmetic result {result:?} has invalid operands"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::Compare {
            result,
            operator,
            left,
            right,
            operand_ty,
            ..
        } => {
            let ordered = !matches!(
                operator,
                ComparisonOperator::Equal | ComparisonOperator::NotEqual
            );
            let supported = matches!(
                types.get(operand_ty.0 as usize),
                Some(Type::I32 | Type::I64 | Type::Usize | Type::U8) | Some(Type::Rune)
            ) || (!ordered && standard_eq_type(types, *operand_ty));
            if values.get(left) != Some(operand_ty)
                || values.get(right) != Some(operand_ty)
                || !supported
            {
                errors.push(format!("comparison result {result:?} has invalid operands"));
            }
            define(*result, TypeId(2), values, errors);
        }
        Operation::Call {
            result,
            function,
            arguments,
            ty,
            ..
        } => {
            match signatures.get(function) {
                Some((arity, _)) if *arity == arguments.len() => {}
                Some((arity, _)) => errors.push(format!(
                    "call to {function:?} has {} arguments, expected {arity}",
                    arguments.len()
                )),
                None => errors.push(format!("call references unknown function {function:?}")),
            }
            for argument in arguments {
                if !values.contains_key(argument) {
                    errors.push(format!("call uses undefined value {argument:?}"));
                }
            }
            define(*result, *ty, values, errors);
        }
        Operation::UnionInject {
            result,
            member,
            value,
            ty,
            ..
        } => {
            if values.get(value) != Some(member) {
                errors.push(format!(
                    "union injection {result:?} has an incorrectly typed value"
                ));
            }
            if !matches!(types.get(ty.0 as usize), Some(Type::Union(members)) if members.contains(member))
            {
                errors.push(format!(
                    "union injection {result:?} names an invalid member"
                ));
            }
            define(*result, *ty, values, errors);
        }
        Operation::UnionProject {
            result,
            member,
            value,
            union_ty,
            ty,
            ..
        } => {
            if member != ty {
                errors.push(format!(
                    "union projection {result:?} has inconsistent member type"
                ));
            }
            if values.get(value) != Some(union_ty)
                || !matches!(types.get(union_ty.0 as usize), Some(Type::Union(members)) if members.contains(member))
            {
                errors.push(format!("union projection {result:?} has an invalid source"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::Load {
            result, slot, ty, ..
        } => {
            if slots.get(slot) != Some(ty) {
                errors.push(format!("load from {slot:?} has incorrect type"));
            }
            if !initialized.contains(slot) {
                errors.push(format!("load from uninitialized slot {slot:?}"));
            }
            define(*result, *ty, values, errors);
        }
        Operation::Store { slot, value, .. } => match (slots.get(slot), values.get(value)) {
            (Some(slot_ty), Some(value_ty)) if slot_ty == value_ty => {
                initialized.insert(*slot);
            }
            (Some(_), Some(_)) => errors.push(format!("store to {slot:?} has incorrect type")),
            (None, _) => errors.push(format!("store references unknown slot {slot:?}")),
            (_, None) => errors.push(format!("store uses undefined value {value:?}")),
        },
    }
}

fn standard_eq_type(types: &[Type], ty: TypeId) -> bool {
    match types.get(ty.0 as usize) {
        Some(
            Type::I32
            | Type::I64
            | Type::Usize
            | Type::Bool
            | Type::Unit
            | Type::String
            | Type::Bytes
            | Type::Bits
            | Type::Rune
            | Type::Utf8Error
            | Type::U8
            | Type::Atom(_),
        ) => true,
        Some(Type::Buffer) => false,
        Some(Type::List(item) | Type::Slice(item)) => standard_eq_type(types, *item),
        Some(Type::Array { item, .. }) => standard_eq_type(types, *item),
        Some(Type::Tuple(items)) => items.iter().all(|item| standard_eq_type(types, *item)),
        Some(Type::Map { key, value }) => {
            standard_hash_type(types, *key) && standard_eq_type(types, *value)
        }
        _ => false,
    }
}

fn is_utf8_result_type(types: &[Type], ty: TypeId) -> bool {
    matches!(types.get(ty.0 as usize), Some(Type::Union(members)) if {
        let has = |tag: &str, payload: fn(&Type) -> bool| members.iter().any(|member| {
            matches!(types.get(member.0 as usize), Some(Type::Tuple(fields)) if fields.len() == 2
                && matches!(types.get(fields[0].0 as usize), Some(Type::Atom(found)) if found == tag)
                && types.get(fields[1].0 as usize).is_some_and(payload))
        });
        has("ok", |value| matches!(value, Type::String))
            && has("error", |value| matches!(value, Type::Utf8Error))
    })
}

fn is_option_bytes_type(types: &[Type], ty: TypeId) -> bool {
    matches!(types.get(ty.0 as usize), Some(Type::Union(members)) if {
        let has_none = members.iter().any(|member| {
            matches!(types.get(member.0 as usize), Some(Type::Atom(found)) if found == "none")
        });
        let has_some = members.iter().any(|member| {
            matches!(types.get(member.0 as usize), Some(Type::Tuple(fields)) if fields.len() == 2
                && matches!(types.get(fields[0].0 as usize), Some(Type::Atom(found)) if found == "some")
                && matches!(types.get(fields[1].0 as usize), Some(Type::Bytes)))
        });
        has_none && has_some
    })
}

fn standard_hash_type(types: &[Type], ty: TypeId) -> bool {
    match types.get(ty.0 as usize) {
        Some(Type::Map { .. } | Type::Function { .. } | Type::Union(_) | Type::Struct { .. })
        | Some(Type::Parameter { .. })
        | None => false,
        Some(Type::List(item) | Type::Slice(item)) => standard_hash_type(types, *item),
        Some(Type::Array { item, .. }) => standard_hash_type(types, *item),
        Some(Type::Tuple(items)) => items.iter().all(|item| standard_hash_type(types, *item)),
        Some(_) => true,
    }
}

impl GenericModule {
    #[must_use]
    pub fn debug_text(&self) -> String {
        let mut output = String::new();
        for implementation in &self.implementations {
            output.push_str(&format!(
                "impl i{} {} for t{}",
                implementation.id.0, implementation.protocol, implementation.target.0
            ));
            if !implementation.associated_types.is_empty() {
                output.push_str(&format!(
                    " [{}]",
                    implementation
                        .associated_types
                        .iter()
                        .map(|(name, ty)| format!("{name}=t{}", ty.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            output.push('\n');
        }
        for function in &self.functions {
            output.push_str(&format!(
                "fn f{} {} -> t{} {{\n",
                function.id.0, function.name, function.result.0
            ));
            for slot in &function.slots {
                output.push_str(&format!("  slot q{}: t{}\n", slot.id.0, slot.ty.0));
            }
            for block in &function.blocks {
                if block.parameters.is_empty() {
                    output.push_str(&format!("  b{}:\n", block.id.0));
                } else {
                    output.push_str(&format!(
                        "  b{}({}):\n",
                        block.id.0,
                        block
                            .parameters
                            .iter()
                            .map(|parameter| format!("v{}: t{}", parameter.value.0, parameter.ty.0))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
                for operation in &block.operations {
                    output.push_str("    ");
                    output.push_str(&display_operation(operation));
                    output.push('\n');
                }
                match &block.terminator {
                    Terminator::Return { value, .. } => {
                        output.push_str(&format!("    return v{}\n", value.0))
                    }
                    Terminator::Branch {
                        target, arguments, ..
                    } => output.push_str(&format!(
                        "    branch b{}({})\n",
                        target.0,
                        arguments
                            .iter()
                            .map(|value| format!("v{}", value.0))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )),
                    Terminator::CondBranch {
                        condition,
                        then_target,
                        else_target,
                        ..
                    } => output.push_str(&format!(
                        "    branch_if v{}, b{}, b{}\n",
                        condition.0, then_target.0, else_target.0
                    )),
                    Terminator::Switch {
                        subject,
                        cases,
                        default,
                        ..
                    } => output.push_str(&format!(
                        "    switch v{} [{}]{}\n",
                        subject.0,
                        cases
                            .iter()
                            .map(|(value, target)| format!("{value:?} => b{}", target.0))
                            .collect::<Vec<_>>()
                            .join(", "),
                        default
                            .map_or_else(String::new, |target| format!(" default b{}", target.0))
                    )),
                    Terminator::Failure { category, .. } => {
                        output.push_str(&format!("    fail {category:?}\n"))
                    }
                    Terminator::Unreachable { .. } => output.push_str("    unreachable\n"),
                }
            }
            output.push_str("}\n");
        }
        output
    }
}

fn display_operation(operation: &Operation) -> String {
    match operation {
        Operation::Constant {
            result,
            constant,
            ty,
            ..
        } => format!("v{} = const {constant:?}: t{}", result.0, ty.0),
        Operation::List {
            result,
            elements,
            tail,
            ty,
            ..
        } => format!(
            "v{} = list [{}]{}: t{}",
            result.0,
            elements
                .iter()
                .map(|value| format!("v{}", value.0))
                .collect::<Vec<_>>()
                .join(", "),
            tail.map_or_else(String::new, |tail| format!(" | v{}", tail.0)),
            ty.0
        ),
        Operation::ListReverse {
            result, list, ty, ..
        } => format!("v{} = list_reverse v{}: t{}", result.0, list.0, ty.0),
        Operation::Struct {
            result,
            declaration,
            fields,
            ty,
            ..
        } => format!(
            "v{} = struct d{} {{{}}}: t{}",
            result.0,
            declaration.0,
            fields
                .iter()
                .map(|(index, value)| format!("{index}: v{}", value.0))
                .collect::<Vec<_>>()
                .join(", "),
            ty.0
        ),
        Operation::TupleProject {
            result,
            tuple,
            index,
            ty,
            ..
        } => format!(
            "v{} = tuple_project v{}[{}]: t{}",
            result.0, tuple.0, index, ty.0
        ),
        Operation::StructProject {
            result,
            structure,
            declaration,
            index,
            ty,
            ..
        } => format!(
            "v{} = struct_project d{} v{}[{}]: t{}",
            result.0, declaration.0, structure.0, index, ty.0
        ),
        Operation::ListHead {
            result, list, ty, ..
        } => format!("v{} = list_head v{}: t{}", result.0, list.0, ty.0),
        Operation::ListTail {
            result, list, ty, ..
        } => format!("v{} = list_tail v{}: t{}", result.0, list.0, ty.0),
        Operation::Tuple {
            result,
            elements,
            ty,
            ..
        } => format!(
            "v{} = tuple {{{}}}: t{}",
            result.0,
            elements
                .iter()
                .map(|value| format!("v{}", value.0))
                .collect::<Vec<_>>()
                .join(", "),
            ty.0
        ),
        Operation::Array {
            result,
            elements,
            ty,
            ..
        } => format!(
            "v{} = array #[{}]: t{}",
            result.0,
            elements
                .iter()
                .map(|value| format!("v{}", value.0))
                .collect::<Vec<_>>()
                .join(", "),
            ty.0
        ),
        Operation::ArrayIndex {
            result,
            array,
            index,
            length,
            failure,
            ty,
            ..
        } => format!(
            "v{} = index v{}[v{}] length {:?}: t{} [IndexOutOfBounds => b{}]",
            result.0, array.0, index.0, length, ty.0, failure.0
        ),
        Operation::SliceFromArray {
            result,
            array,
            length,
            ty,
            ..
        } => format!(
            "v{} = slice_from_array v{} length {}: t{}",
            result.0, array.0, length, ty.0
        ),
        Operation::SliceSubslice {
            result,
            slice,
            start,
            length,
            failure,
            ty,
            ..
        } => format!(
            "v{} = subslice v{} v{} v{}: t{} [IndexOutOfBounds => b{}]",
            result.0, slice.0, start.0, length.0, ty.0, failure.0
        ),
        Operation::SliceCopy {
            result, slice, ty, ..
        } => {
            format!("v{} = slice_copy v{}: t{}", result.0, slice.0, ty.0)
        }
        Operation::StringBytes {
            result, string, ty, ..
        } => format!("v{} = string_bytes v{}: t{}", result.0, string.0, ty.0),
        Operation::StringCodepoints {
            result, string, ty, ..
        } => format!("v{} = string_codepoints v{}: t{}", result.0, string.0, ty.0),
        Operation::StringFromBytes {
            result, bytes, ty, ..
        } => format!("v{} = string_from_bytes v{}: t{}", result.0, bytes.0, ty.0),
        Operation::Utf8ErrorOffset {
            result, error, ty, ..
        } => format!("v{} = utf8_error_offset v{}: t{}", result.0, error.0, ty.0),
        Operation::RuneToString {
            result, rune, ty, ..
        } => format!("v{} = rune_to_string v{}: t{}", result.0, rune.0, ty.0),
        Operation::BufferNew { result, ty, .. } => {
            format!("v{} = buffer_new: t{}", result.0, ty.0)
        }
        Operation::BufferAppend {
            result,
            buffer,
            value,
            kind,
            ty,
            ..
        } => format!(
            "v{} = buffer_append_{kind:?} v{} v{}: t{}",
            result.0, buffer.0, value.0, ty.0
        ),
        Operation::BufferToBytes {
            result, buffer, ty, ..
        } => format!("v{} = buffer_to_bytes v{}: t{}", result.0, buffer.0, ty.0),
        Operation::BytesToBits {
            result, bytes, ty, ..
        } => format!("v{} = bytes_to_bits v{}: t{}", result.0, bytes.0, ty.0),
        Operation::BitsSlice {
            result,
            bits,
            start,
            length,
            failure,
            ty,
            ..
        } => format!(
            "v{} = bits_slice v{} v{} v{}: t{} [IndexOutOfBounds => b{}]",
            result.0, bits.0, start.0, length.0, ty.0, failure.0
        ),
        Operation::BitsToBytes {
            result, bits, ty, ..
        } => format!("v{} = bits_to_bytes v{}: t{}", result.0, bits.0, ty.0),
        Operation::BytesFromList {
            result, list, ty, ..
        } => format!("v{} = bytes_from_list v{}: t{}", result.0, list.0, ty.0),
        Operation::BytesToList {
            result, bytes, ty, ..
        } => format!("v{} = bytes_to_list v{}: t{}", result.0, bytes.0, ty.0),
        Operation::BytesSlice {
            result,
            bytes,
            start,
            length,
            failure,
            ty,
            ..
        } => format!(
            "v{} = bytes_slice v{} v{} v{}: t{} [IndexOutOfBounds => b{}]",
            result.0, bytes.0, start.0, length.0, ty.0, failure.0
        ),
        Operation::CollectionLength {
            result,
            value,
            known_length,
            ty,
            ..
        } => format!(
            "v{} = collection_length v{} known {:?}: t{}",
            result.0, value.0, known_length, ty.0
        ),
        Operation::Map {
            result,
            entries,
            ty,
            ..
        } => format!(
            "v{} = map %{{{}}}: t{}",
            result.0,
            entries
                .iter()
                .map(|(key, value)| format!("v{} => v{}", key.0, value.0))
                .collect::<Vec<_>>()
                .join(", "),
            ty.0
        ),
        Operation::MapPut {
            result,
            map,
            key,
            value,
            ty,
            ..
        } => format!(
            "v{} = map_put v{} v{} v{}: t{}",
            result.0, map.0, key.0, value.0, ty.0
        ),
        Operation::MapRemove {
            result,
            map,
            key,
            ty,
            ..
        } => format!(
            "v{} = map_remove v{} v{}: t{}",
            result.0, map.0, key.0, ty.0
        ),
        Operation::MapFetch {
            result,
            map,
            key,
            map_ty,
            ty,
            ..
        } => format!(
            "v{} = map_fetch v{} v{}: t{} -> t{}",
            result.0, map.0, key.0, map_ty.0, ty.0
        ),
        Operation::MapToList {
            result,
            map,
            map_ty,
            ty,
            ..
        } => format!(
            "v{} = map_to_list v{}: t{} -> t{}",
            result.0, map.0, map_ty.0, ty.0
        ),
        Operation::CheckedArithmetic {
            result,
            operator,
            left,
            right,
            failures,
            ty,
            ..
        } => format!(
            "v{} = checked.{operator:?} v{}, v{}: t{} [{}]",
            result.0,
            left.0,
            right.0,
            ty.0,
            failures
                .iter()
                .map(|(category, target)| format!("{category:?} => b{}", target.0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Operation::Compare {
            result,
            operator,
            left,
            right,
            operand_ty,
            ..
        } => format!(
            "v{} = compare.{operator:?} v{}, v{}: t{} -> t2",
            result.0, left.0, right.0, operand_ty.0
        ),
        Operation::Call {
            result,
            function,
            arguments,
            ty,
            ..
        } => format!(
            "v{} = call f{}({}): t{}",
            result.0,
            function.0,
            arguments
                .iter()
                .map(|value| format!("v{}", value.0))
                .collect::<Vec<_>>()
                .join(", "),
            ty.0
        ),
        Operation::UnionInject {
            result,
            member,
            value,
            ty,
            ..
        } => format!(
            "v{} = inject t{} v{}: t{}",
            result.0, member.0, value.0, ty.0
        ),
        Operation::UnionProject {
            result,
            member,
            value,
            ty,
            ..
        } => format!(
            "v{} = project t{} v{}: t{}",
            result.0, member.0, value.0, ty.0
        ),
        Operation::Load {
            result, slot, ty, ..
        } => format!("v{} = load q{}: t{}", result.0, slot.0, ty.0),
        Operation::Store { slot, value, .. } => format!("store q{}, v{}", slot.0, value.0),
    }
}
