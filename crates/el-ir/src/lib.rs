//! Shared Generic Core IR, initial lowering, and representation verification.

use el_resolve::{DeclId, ImplId, SymbolId, Visibility};
use el_span::Span;
pub use el_types::{ArithmeticOperator, ComparisonOperator, Type, TypeId};
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
    Array {
        result: ValueId,
        elements: Vec<ValueId>,
        ty: TypeId,
        origin: Span,
    },
    Map {
        result: ValueId,
        entries: Vec<(ValueId, ValueId)>,
        ty: TypeId,
        origin: Span,
    },
    Tuple {
        result: ValueId,
        elements: Vec<ValueId>,
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
            Lowerer::new(function, FunctionId(index as u32), &functions_by_decl).lower()
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
    ) -> Self {
        Self {
            function,
            id,
            functions,
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
    Bool,
    Unit,
    String,
    Atom(String),
    List(Box<Self>),
    Array {
        item: Box<Self>,
        length: u64,
    },
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
            Type::Bool => NormalizedType::Bool,
            Type::Unit => NormalizedType::Unit,
            Type::String => NormalizedType::String,
            Type::Atom(name) => NormalizedType::Atom(name.clone()),
            Type::List(item) => {
                NormalizedType::List(Box::new(self.normalize(*item, substitution)?))
            }
            Type::Array { item, length } => NormalizedType::Array {
                item: Box::new(self.normalize(*item, substitution)?),
                length: *length,
            },
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
            | Operation::Array { ty, .. }
            | Operation::Map { ty, .. }
            | Operation::Tuple { ty, .. }
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
            NormalizedType::Bool => return TypeId(2),
            NormalizedType::Unit => return TypeId(3),
            NormalizedType::String => Type::String,
            NormalizedType::Atom(name) => Type::Atom(name.clone()),
            NormalizedType::List(item) => Type::List(self.intern_normalized(item)),
            NormalizedType::Array { item, length } => Type::Array {
                item: self.intern_normalized(item),
                length: *length,
            },
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
        NormalizedType::List(item) | NormalizedType::Array { item, .. } => {
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
        | NormalizedType::Bool
        | NormalizedType::Unit
        | NormalizedType::String
        | NormalizedType::Atom(_) => {}
    }
}

fn operation_type_ids(operation: &Operation, output: &mut Vec<TypeId>) {
    match operation {
        Operation::Constant { ty, .. }
        | Operation::List { ty, .. }
        | Operation::Array { ty, .. }
        | Operation::Map { ty, .. }
        | Operation::Tuple { ty, .. }
        | Operation::TupleProject { ty, .. }
        | Operation::StructProject { ty, .. }
        | Operation::ListHead { ty, .. }
        | Operation::ListTail { ty, .. }
        | Operation::CheckedArithmetic { ty, .. }
        | Operation::Call { ty, .. }
        | Operation::Load { ty, .. } => output.push(*ty),
        Operation::Compare { operand_ty, .. } => output.push(*operand_ty),
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
                            | (SwitchValue::Integer(_), Some(Type::I32 | Type::I64))
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
                (Constant::Integer(_), Some(Type::I32 | Type::I64))
                | (Constant::Boolean(_), Some(Type::Bool))
                | (Constant::Unit, Some(Type::Unit))
                | (Constant::String(_), Some(Type::String)) => true,
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
                Some(Type::I32 | Type::I64)
            ) || (!ordered
                && matches!(types.get(operand_ty.0 as usize), Some(Type::Bool)));
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
