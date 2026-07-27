//! Shared Generic Core IR, initial lowering, and representation verification.

use el_resolve::{DeclId, ImplId, SymbolId};
use el_span::Span;
use el_types::{
    ArithmeticOperator, Type, TypeId, TypedExpr, TypedExprKind, TypedItem, TypedPatternKind,
    TypedProgram,
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
    pub name: String,
    pub span: Span,
    pub parameters: Vec<CoreParameter>,
    pub type_parameters: Vec<TypeId>,
    pub constraints: Vec<(TypeId, String)>,
    pub result: TypeId,
    pub slots: Vec<Slot>,
    pub blocks: Vec<Block>,
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
        ty: TypeId,
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
        cases: Vec<(SwitchValue, BlockId)>,
        default: Option<BlockId>,
        origin: Span,
    },
    Return {
        value: ValueId,
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

fn pattern_arguments(bindings: &BTreeMap<SymbolId, (ValueId, TypeId)>) -> Vec<ValueId> {
    bindings.values().map(|(value, _)| *value).collect()
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
                    let value = self.lower_expr(initializer);
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
                    let lowered = self.lower_expr(value);
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
                    result = Some(self.lower_expr(expression));
                    return_origin = expression.span;
                }
                TypedItem::Return(expression) => {
                    result = Some(self.lower_expr(expression));
                    return_origin = expression.span;
                    break;
                }
            }
        }
        let value = result
            .unwrap_or_else(|| self.constant(Constant::Unit, TypeId(3), self.function.body.span));
        self.finish_current(Terminator::Return {
            value,
            origin: return_origin,
        });
        self.blocks.sort_by_key(|block| block.id);
        CoreFunction {
            id: self.id,
            declaration: self.function.id,
            name: self.function.name.clone(),
            span: self.function.span,
            parameters,
            type_parameters: self.function.type_parameters.clone(),
            constraints: self.function.constraints.clone(),
            result: self.function.result,
            slots: self.slots,
            blocks: self.blocks,
        }
    }

    fn lower_expr(&mut self, expression: &TypedExpr) -> ValueId {
        match &expression.kind {
            TypedExprKind::Integer(value) => {
                self.constant(Constant::Integer(*value), expression.ty, expression.span)
            }
            TypedExprKind::Boolean(value) => {
                self.constant(Constant::Boolean(*value), expression.ty, expression.span)
            }
            TypedExprKind::Unit => self.constant(Constant::Unit, expression.ty, expression.span),
            TypedExprKind::Atom(name) => {
                self.constant(Constant::Atom(name.clone()), expression.ty, expression.span)
            }
            TypedExprKind::List { elements, tail } => {
                let elements = elements
                    .iter()
                    .map(|element| self.lower_expr(element))
                    .collect();
                let tail = tail.as_ref().map(|tail| self.lower_expr(tail));
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
                    .collect();
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
                    .collect();
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
                    let key = self.lower_expr(key);
                    let value = self.lower_expr(value);
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
            TypedExprKind::Ascription(value) => self.lower_expr(value),
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
                let left = self.lower_expr(left);
                let right = self.lower_expr(right);
                let result = self.value();
                self.operations.push(Operation::CheckedArithmetic {
                    result,
                    operator: *operator,
                    left,
                    right,
                    ty: expression.ty,
                    origin: expression.span,
                });
                result
            }
            TypedExprKind::Call {
                function,
                substitutions,
                arguments,
            } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.lower_expr(argument))
                    .collect();
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
                let value = self.lower_expr(value);
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
            } => self.lower_if(
                condition,
                then_block,
                else_block.as_ref(),
                expression.ty,
                expression.span,
            ),
            TypedExprKind::Match { subject, arms, .. } => {
                self.lower_match(subject, arms, expression.ty, expression.span)
            }
        }
    }

    fn lower_if(
        &mut self,
        condition: &TypedExpr,
        then_block: &el_types::TypedBlock,
        else_block: Option<&el_types::TypedBlock>,
        ty: TypeId,
        origin: Span,
    ) -> ValueId {
        let condition = self.lower_expr(condition);
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
        self.finish_current(Terminator::Branch {
            target: join_target,
            arguments: vec![then_value],
            origin: then_block.span,
        });

        self.current_block = else_target;
        self.current_parameters.clear();
        self.bindings = outer_bindings.clone();
        let else_value = if let Some(block) = else_block {
            self.lower_block_value(block)
        } else {
            self.constant(Constant::Unit, TypeId(3), origin)
        };
        self.finish_current(Terminator::Branch {
            target: join_target,
            arguments: vec![else_value],
            origin,
        });

        self.current_block = join_target;
        self.bindings = outer_bindings;
        let result = self.value();
        self.current_parameters = vec![CoreParameter {
            value: result,
            ty,
            origin,
        }];
        result
    }

    fn lower_block_value(&mut self, block: &el_types::TypedBlock) -> ValueId {
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
                    let value = self.lower_expr(initializer);
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
                    let value = self.lower_expr(value);
                    let Binding::Slot(slot) = self.bindings[symbol] else {
                        panic!("verified mutable binding")
                    };
                    self.operations.push(Operation::Store {
                        slot,
                        value,
                        origin: *span,
                    });
                }
                TypedItem::Expr(value) | TypedItem::Return(value) => {
                    result = Some(self.lower_expr(value))
                }
            }
        }
        result.unwrap_or_else(|| self.constant(Constant::Unit, TypeId(3), block.span))
    }

    fn lower_match(
        &mut self,
        subject: &TypedExpr,
        arms: &[el_types::TypedMatchArm],
        ty: TypeId,
        origin: Span,
    ) -> ValueId {
        let subject_value = self.lower_expr(subject);
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
            let value = self.lower_block_value(&arm.body);
            self.finish_current(Terminator::Branch {
                target: join_target,
                arguments: vec![value],
                origin: arm.body.span,
            });
            self.current_block = failure_target;
            self.current_parameters.clear();
            self.bindings = outer_bindings.clone();
        }
        self.finish_current(Terminator::Unreachable { origin });
        self.current_block = join_target;
        self.current_parameters = vec![CoreParameter {
            value: self.value(),
            ty,
            origin,
        }];
        self.bindings = outer_bindings;
        self.current_parameters[0].value
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
                subject,
                SwitchValue::Boolean(*value),
                success,
                failure,
                pattern.span,
                pattern_arguments(bindings),
            ),
            TypedPatternKind::Integer(value) => self.pattern_switch(
                subject,
                SwitchValue::Integer(*value),
                success,
                failure,
                pattern.span,
                pattern_arguments(bindings),
            ),
            TypedPatternKind::Atom(value) => self.pattern_switch(
                subject,
                SwitchValue::Atom(value.clone()),
                success,
                failure,
                pattern.span,
                pattern_arguments(bindings),
            ),
            TypedPatternKind::UnionMember { member, symbol, .. } => {
                let matched = self.new_block();
                self.pattern_switch(
                    subject,
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
                subject,
                SwitchValue::ListEmpty,
                success,
                failure,
                pattern.span,
                pattern_arguments(bindings),
            ),
            TypedPatternKind::ListCons { head, tail } => {
                let matched = self.new_block();
                self.pattern_switch(
                    subject,
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
        subject: ValueId,
        value: SwitchValue,
        success: BlockId,
        failure: BlockId,
        origin: Span,
        success_arguments: Vec<ValueId>,
    ) {
        self.finish_current(Terminator::Switch {
            subject,
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
        for block in &function.blocks {
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
                Terminator::Return { .. } | Terminator::Unreachable { .. } => {}
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
                    cases,
                    default,
                    ..
                } => {
                    let subject_ty = values.get(subject).copied();
                    if subject_ty.is_none() {
                        errors.push(format!("switch in {:?} uses an undefined value", block.id));
                    }
                    let mut seen = BTreeSet::new();
                    for (case, target) in cases {
                        if !seen.insert(case_key(case)) {
                            errors.push(format!("switch in {:?} has a duplicate case", block.id));
                        }
                        let compatible = match (
                            case,
                            subject_ty.and_then(|ty| module.types.get(ty.0 as usize)),
                        ) {
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
                | (Constant::Unit, Some(Type::Unit)) => true,
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
            ty,
            ..
        } => {
            if member != ty {
                errors.push(format!(
                    "union projection {result:?} has inconsistent member type"
                ));
            }
            if !matches!(values.get(value).and_then(|union| types.get(union.0 as usize)), Some(Type::Union(members)) if members.contains(member))
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
            ty,
            ..
        } => format!(
            "v{} = checked.{operator:?} v{}, v{}: t{}",
            result.0, left.0, right.0, ty.0
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
