//! Canonical types, bidirectional checking, and the verified Typed AST.

use el_ast::{Node, Value};
use el_resolve::{
    DeclId, Function, ImplId, ModuleId, ResolvedProgram, Struct, SymbolId, TypeAlias, TypeSyntax,
};
use el_span::{Diagnostic, Span};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TypeId(pub u32);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Type {
    I32,
    I64,
    Bool,
    Unit,
    Atom(String),
    List(TypeId),
    Array {
        item: TypeId,
        length: u64,
    },
    Map {
        key: TypeId,
        value: TypeId,
    },
    Tuple(Vec<TypeId>),
    Function {
        parameters: Vec<TypeId>,
        result: TypeId,
    },
    Struct {
        declaration: DeclId,
        arguments: Vec<TypeId>,
    },
    Parameter {
        owner: DeclId,
        name: String,
    },
    Union(Vec<TypeId>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedProgram {
    pub module_name: String,
    pub types: Vec<Type>,
    pub structs: Vec<TypedStruct>,
    pub implementations: Vec<TypedImplementation>,
    pub functions: Vec<TypedFunction>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedImplementation {
    pub id: ImplId,
    pub protocol: String,
    pub target: TypeId,
    pub associated_types: Vec<(String, TypeId)>,
    pub methods: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedStruct {
    pub id: DeclId,
    pub name: String,
    pub span: Span,
    pub parameters: Vec<TypeId>,
    pub fields: Vec<TypedField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedField {
    pub name: String,
    pub span: Span,
    pub ty: TypeId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedFunction {
    pub id: DeclId,
    pub name: String,
    pub span: Span,
    pub parameters: Vec<TypedParameter>,
    pub type_parameters: Vec<TypeId>,
    pub constraints: Vec<(TypeId, String)>,
    pub result: TypeId,
    pub body: TypedBlock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedParameter {
    pub symbol: SymbolId,
    pub name: String,
    pub span: Span,
    pub ty: TypeId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedBlock {
    pub span: Span,
    pub ty: TypeId,
    pub items: Vec<TypedItem>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternFacts {
    pub reachable: bool,
    pub irrefutable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedPattern {
    pub kind: TypedPatternKind,
    pub ty: TypeId,
    pub span: Span,
    pub facts: PatternFacts,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedPatternKind {
    Wildcard,
    Binding {
        symbol: SymbolId,
        name: String,
    },
    Boolean(bool),
    Integer(i128),
    Atom(String),
    UnionMember {
        member: TypeId,
        symbol: SymbolId,
        name: String,
    },
    Tuple(Vec<TypedPattern>),
    ListEmpty,
    ListCons {
        head: Box<TypedPattern>,
        tail: Box<TypedPattern>,
    },
    Struct {
        declaration: DeclId,
        fields: Vec<(usize, TypedPattern)>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedMatchArm {
    pub pattern: TypedPattern,
    pub body: TypedBlock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedItem {
    Let {
        symbol: SymbolId,
        name: String,
        mutable: bool,
        ty: TypeId,
        initializer: TypedExpr,
        span: Span,
    },
    Assign {
        symbol: SymbolId,
        value: TypedExpr,
        span: Span,
    },
    Expr(TypedExpr),
    Return(TypedExpr),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedExpr {
    pub kind: TypedExprKind,
    pub ty: TypeId,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedExprKind {
    Integer(i128),
    Boolean(bool),
    Unit,
    Atom(String),
    List {
        elements: Vec<TypedExpr>,
        tail: Option<Box<TypedExpr>>,
    },
    Array(Vec<TypedExpr>),
    Map(Vec<(TypedExpr, TypedExpr)>),
    Tuple(Vec<TypedExpr>),
    If {
        condition: Box<TypedExpr>,
        then_block: TypedBlock,
        else_block: Option<TypedBlock>,
    },
    Match {
        subject: Box<TypedExpr>,
        arms: Vec<TypedMatchArm>,
        exhaustive: bool,
    },
    Ascription(Box<TypedExpr>),
    Local(SymbolId),
    Binary {
        operator: ArithmeticOperator,
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
    },
    Call {
        function: DeclId,
        substitutions: Vec<(TypeId, TypeId)>,
        arguments: Vec<TypedExpr>,
    },
    UnionInject {
        member: TypeId,
        value: Box<TypedExpr>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArithmeticOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

#[derive(Clone, Debug)]
struct Signature {
    parameters: Vec<TypeId>,
    type_parameters: Vec<TypeId>,
    constraints: Vec<(TypeId, String)>,
    result: TypeId,
}

#[derive(Clone, Debug)]
struct Local {
    symbol: SymbolId,
    ty: TypeId,
    mutable: bool,
    span: Span,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum PatternShape {
    Wildcard,
    Bool(bool),
    Integer(i128),
    Atom(String),
    Union(TypeId),
    Tuple(Vec<(TypeId, PatternShape)>),
    ListEmpty,
    ListCons(Box<(TypeId, PatternShape)>, Box<(TypeId, PatternShape)>),
    Struct(DeclId, Vec<(TypeId, PatternShape)>),
}

/// Resolves canonical types and checks every function body before producing a Typed AST.
pub fn check(program: &ResolvedProgram) -> Result<TypedProgram, Vec<Diagnostic>> {
    Checker::new(program).run()
}

/// Checks all resolved modules in one package-wide declaration and type context.
pub fn check_package(programs: &[ResolvedProgram]) -> Result<TypedProgram, Vec<Diagnostic>> {
    let Some(first) = programs.first() else {
        return Ok(TypedProgram {
            module_name: String::new(),
            types: vec![Type::I32, Type::I64, Type::Bool, Type::Unit],
            structs: Vec::new(),
            implementations: Vec::new(),
            functions: Vec::new(),
        });
    };
    let mut merged = ResolvedProgram {
        module_id: ModuleId(0),
        module_name: "<package>".to_owned(),
        module_span: first.module_span,
        aliases: programs
            .iter()
            .flat_map(|module| module.aliases.clone())
            .collect(),
        structs: programs
            .iter()
            .flat_map(|module| module.structs.clone())
            .collect(),
        protocols: programs
            .iter()
            .flat_map(|module| module.protocols.clone())
            .collect(),
        implementations: programs
            .iter()
            .flat_map(|module| module.implementations.clone())
            .collect(),
        functions: programs
            .iter()
            .flat_map(|module| module.functions.clone())
            .collect(),
    };
    for (index, implementation) in merged.implementations.iter_mut().enumerate() {
        implementation.id = ImplId(index as u32);
    }
    Checker::new(&merged).run()
}

struct Checker<'a> {
    program: &'a ResolvedProgram,
    types: Vec<Type>,
    signatures: BTreeMap<DeclId, Signature>,
    functions_by_name: BTreeMap<String, DeclId>,
    aliases: BTreeMap<DeclId, TypeAlias>,
    structs: BTreeMap<DeclId, Struct>,
    diagnostics: Vec<Diagnostic>,
    next_symbol: u32,
}

impl<'a> Checker<'a> {
    fn new(program: &'a ResolvedProgram) -> Self {
        let next_symbol = program
            .functions
            .iter()
            .flat_map(|function| function.parameters.iter())
            .map(|parameter| parameter.symbol.0 + 1)
            .max()
            .unwrap_or(0);
        Self {
            program,
            types: vec![Type::I32, Type::I64, Type::Bool, Type::Unit],
            signatures: BTreeMap::new(),
            functions_by_name: program
                .functions
                .iter()
                .map(|function| {
                    (
                        format!("{}.{}", function.module_name, function.name),
                        function.id,
                    )
                })
                .collect(),
            aliases: program
                .aliases
                .iter()
                .cloned()
                .map(|alias| (alias.id, alias))
                .collect(),
            structs: program
                .structs
                .iter()
                .cloned()
                .map(|structure| (structure.id, structure))
                .collect(),
            diagnostics: Vec::new(),
            next_symbol,
        }
    }

    fn run(mut self) -> Result<TypedProgram, Vec<Diagnostic>> {
        for alias in self.program.aliases.clone() {
            self.validate_alias(&alias);
        }
        let structs = self
            .program
            .structs
            .clone()
            .iter()
            .filter_map(|structure| self.check_struct_declaration(structure))
            .collect();
        for function in &self.program.functions {
            self.collect_signature(function);
        }
        let implementations = self
            .program
            .implementations
            .iter()
            .filter_map(|implementation| {
                let mut names = Vec::new();
                collect_type_variables(&implementation.target, &mut names);
                let owner = DeclId(u32::MAX - implementation.id.0);
                let parameters = names
                    .into_iter()
                    .map(|name| {
                        let id = self.intern(Type::Parameter {
                            owner,
                            name: name.clone(),
                        });
                        (name, id)
                    })
                    .collect::<BTreeMap<_, _>>();
                let target = self.resolve_type(&implementation.target, &parameters)?;
                let associated_types = implementation
                    .associated_types
                    .iter()
                    .filter_map(|(name, syntax)| {
                        self.resolve_type(syntax, &parameters)
                            .map(|ty| (name.clone(), ty))
                    })
                    .collect();
                Some(TypedImplementation {
                    id: implementation.id,
                    protocol: implementation.protocol.clone(),
                    target,
                    associated_types,
                    methods: implementation.methods.clone(),
                    span: implementation.span,
                })
            })
            .collect();
        let mut functions = Vec::new();
        for function in &self.program.functions {
            if let Some(typed) = self.check_function(function) {
                functions.push(typed);
            }
        }
        if !self.diagnostics.is_empty() {
            return Err(self.diagnostics);
        }
        let program = TypedProgram {
            module_name: self.program.module_name.clone(),
            types: self.types,
            structs,
            implementations,
            functions,
        };
        if let Err(errors) = verify(&program) {
            panic!("type checker produced invalid Typed AST: {errors:?}");
        }
        Ok(program)
    }

    fn validate_alias(&mut self, alias: &TypeAlias) {
        let parameters = alias
            .parameters
            .iter()
            .map(|name| {
                let id = self.intern(Type::Parameter {
                    owner: alias.id,
                    name: name.clone(),
                });
                (name.clone(), id)
            })
            .collect::<BTreeMap<_, _>>();
        let _ = self.resolve_type(&alias.value, &parameters);
    }

    fn check_struct_declaration(&mut self, structure: &Struct) -> Option<TypedStruct> {
        let parameters = structure
            .parameters
            .iter()
            .map(|name| {
                let id = self.intern(Type::Parameter {
                    owner: structure.id,
                    name: name.clone(),
                });
                (name.clone(), id)
            })
            .collect::<BTreeMap<_, _>>();
        let fields = structure
            .fields
            .iter()
            .filter_map(|field| {
                self.resolve_type(&field.ty, &parameters)
                    .map(|ty| TypedField {
                        name: field.name.clone(),
                        span: field.name_span,
                        ty,
                    })
            })
            .collect::<Vec<_>>();
        (fields.len() == structure.fields.len()).then(|| TypedStruct {
            id: structure.id,
            name: structure.name.clone(),
            span: structure.span,
            parameters: parameters.values().copied().collect(),
            fields,
        })
    }

    fn collect_signature(&mut self, function: &Function) {
        let mut parameters_by_name = BTreeMap::new();
        for name in &function.type_parameters {
            let id = self.intern(Type::Parameter {
                owner: function.id,
                name: name.clone(),
            });
            parameters_by_name.insert(name.clone(), id);
        }
        let parameters = function
            .parameters
            .iter()
            .filter_map(|parameter| self.resolve_type(&parameter.ty, &parameters_by_name))
            .collect();
        let result = function
            .return_type
            .as_ref()
            .and_then(|ty| self.resolve_type(ty, &parameters_by_name))
            .unwrap_or(TypeId(3));
        self.signatures.insert(
            function.id,
            Signature {
                parameters,
                type_parameters: parameters_by_name.values().copied().collect(),
                constraints: function
                    .constraints
                    .iter()
                    .filter_map(|constraint| {
                        parameters_by_name
                            .get(&constraint.parameter)
                            .map(|parameter| (*parameter, constraint.protocol.clone()))
                    })
                    .collect(),
                result,
            },
        );
    }

    fn resolve_type(
        &mut self,
        syntax: &TypeSyntax,
        parameters: &BTreeMap<String, TypeId>,
    ) -> Option<TypeId> {
        match syntax {
            TypeSyntax::Primitive { name, span } => match name.as_str() {
                "i32" => Some(TypeId(0)),
                "i64" => Some(TypeId(1)),
                "bool" => Some(TypeId(2)),
                "unit" => Some(TypeId(3)),
                _ => {
                    self.diagnostics.push(Diagnostic::error(
                        "E2100",
                        *span,
                        format!("primitive `{name}` is outside the Milestone 2 initial slice"),
                    ));
                    None
                }
            },
            TypeSyntax::Variable { name, span } => parameters.get(name).copied().or_else(|| {
                self.diagnostics.push(Diagnostic::error(
                    "E2101",
                    *span,
                    format!("unknown type parameter `{name}`"),
                ));
                None
            }),
            TypeSyntax::Named {
                declaration,
                arguments,
                span,
                ..
            } => {
                let arguments = arguments
                    .iter()
                    .filter_map(|argument| self.resolve_type(argument, parameters))
                    .collect::<Vec<_>>();
                if let Some(alias) = self.aliases.get(declaration).cloned() {
                    if arguments.len() != alias.parameters.len() {
                        return None;
                    }
                    let substitutions = alias
                        .parameters
                        .iter()
                        .cloned()
                        .zip(arguments)
                        .collect::<BTreeMap<_, _>>();
                    self.resolve_type(&alias.value, &substitutions)
                } else if self.structs.contains_key(declaration) {
                    Some(self.intern(Type::Struct {
                        declaration: *declaration,
                        arguments,
                    }))
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        "E2114",
                        *span,
                        "resolved type declaration is missing",
                    ));
                    None
                }
            }
            TypeSyntax::Union { members, span } => {
                let members = members
                    .iter()
                    .filter_map(|member| self.resolve_type(member, parameters))
                    .collect::<Vec<_>>();
                self.normalize_union(members, *span)
            }
            TypeSyntax::Atom { name, .. } => Some(self.intern(Type::Atom(name.clone()))),
            TypeSyntax::List { item, .. } => {
                let item = self.resolve_type(item, parameters)?;
                Some(self.intern(Type::List(item)))
            }
            TypeSyntax::Array { item, length, .. } => {
                let item = self.resolve_type(item, parameters)?;
                Some(self.intern(Type::Array {
                    item,
                    length: *length,
                }))
            }
            TypeSyntax::Map { key, value, .. } => {
                let key = self.resolve_type(key, parameters)?;
                let value = self.resolve_type(value, parameters)?;
                Some(self.intern(Type::Map { key, value }))
            }
            TypeSyntax::Tuple { elements, .. } => {
                let elements = elements
                    .iter()
                    .filter_map(|element| self.resolve_type(element, parameters))
                    .collect::<Vec<_>>();
                Some(self.intern(Type::Tuple(elements)))
            }
            TypeSyntax::Function {
                parameters: inputs,
                result,
                ..
            } => {
                let inputs = inputs
                    .iter()
                    .filter_map(|input| self.resolve_type(input, parameters))
                    .collect::<Vec<_>>();
                let result = self.resolve_type(result, parameters)?;
                Some(self.intern(Type::Function {
                    parameters: inputs,
                    result,
                }))
            }
        }
    }

    fn normalize_union(&mut self, members: Vec<TypeId>, span: Span) -> Option<TypeId> {
        let mut flattened = Vec::new();
        for member in members {
            match self.types[member.0 as usize].clone() {
                Type::Union(nested) => flattened.extend(nested),
                _ => flattened.push(member),
            }
        }
        flattened.sort_by_key(|member| self.type_key(*member));
        flattened.dedup();
        for left in 0..flattened.len() {
            for right in left + 1..flattened.len() {
                if let Some(witness) = self.overlap_witness(flattened[left], flattened[right]) {
                    let mut diagnostic = Diagnostic::error(
                        "E2115",
                        span,
                        format!(
                            "union members `{}` and `{}` may overlap",
                            self.type_name(flattened[left]),
                            self.type_name(flattened[right])
                        ),
                    );
                    if !witness.is_empty() {
                        diagnostic = diagnostic.with_note(format!("overlap witness: {witness}"));
                    }
                    self.diagnostics.push(diagnostic);
                    return None;
                }
            }
        }
        match flattened.as_slice() {
            [] => None,
            [member] => Some(*member),
            _ => Some(self.intern(Type::Union(flattened))),
        }
    }

    fn overlap_witness(&self, left: TypeId, right: TypeId) -> Option<String> {
        let mut substitutions = BTreeMap::new();
        unify_types(&self.types, left, right, &mut substitutions).then(|| {
            substitutions
                .into_iter()
                .map(|(parameter, value)| {
                    format!("{} = {}", self.type_name(parameter), self.type_name(value))
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
    }

    fn check_function(&mut self, function: &Function) -> Option<TypedFunction> {
        let signature = self.signatures.get(&function.id)?.clone();
        if signature.parameters.len() != function.parameters.len() {
            return None;
        }
        let parameters = function
            .parameters
            .iter()
            .zip(&signature.parameters)
            .map(|(parameter, ty)| TypedParameter {
                symbol: parameter.symbol,
                name: parameter.name.clone(),
                span: parameter.name_span,
                ty: *ty,
            })
            .collect::<Vec<_>>();
        let mut scopes = vec![BTreeMap::new()];
        for parameter in &parameters {
            scopes[0].insert(
                parameter.name.clone(),
                Local {
                    symbol: parameter.symbol,
                    ty: parameter.ty,
                    mutable: false,
                    span: parameter.span,
                },
            );
        }
        let body = self.check_block(
            &function.body,
            Some(signature.result),
            function.id,
            &mut scopes,
        )?;
        Some(TypedFunction {
            id: function.id,
            name: function.name.clone(),
            span: function.span,
            parameters,
            type_parameters: signature.type_parameters,
            constraints: signature.constraints,
            result: signature.result,
            body,
        })
    }

    fn check_block(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedBlock> {
        let mut items = Vec::new();
        let last = node.children.len().checked_sub(1);
        for (index, item) in node.children.iter().enumerate() {
            let item_expected = (Some(index) == last).then_some(expected).flatten();
            match item.kind.as_str() {
                "binding" => {
                    let Some(typed) = self.check_binding(item, owner, scopes) else {
                        continue;
                    };
                    items.push(typed);
                }
                "assignment" => {
                    let Some(typed) = self.check_assignment(item, owner, scopes) else {
                        continue;
                    };
                    items.push(typed);
                }
                "return_expr" => {
                    let expression = item.children.first()?;
                    if let Some(value) = self.check_expr(
                        expression,
                        Some(self.signatures[&owner].result),
                        owner,
                        scopes,
                    ) {
                        items.push(TypedItem::Return(value));
                    }
                }
                _ => {
                    if let Some(expression) = self.check_expr(item, item_expected, owner, scopes) {
                        items.push(TypedItem::Expr(expression));
                    }
                }
            }
        }
        let ty = match items.last() {
            Some(TypedItem::Expr(expression)) => expression.ty,
            Some(TypedItem::Return(_)) => expected.unwrap_or(TypeId(3)),
            _ => TypeId(3),
        };
        if let Some(expected) = expected
            && !matches!(items.last(), Some(TypedItem::Return(_)))
            && ty != expected
        {
            self.type_mismatch(node.span, expected, ty);
        }
        Some(TypedBlock {
            span: node.span,
            ty,
            items,
        })
    }

    fn check_binding(
        &mut self,
        node: &Node,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedItem> {
        let name_node = node.children.first()?;
        let name = text(name_node);
        let annotation_node = node.children.get(1).filter(|node| is_type_node(node));
        let initializer_node = node.children.last()?;
        let signature = self.signatures[&owner].clone();
        let parameter_names = signature
            .type_parameters
            .iter()
            .filter_map(|id| match &self.types[id.0 as usize] {
                Type::Parameter { name, .. } => Some((name.clone(), *id)),
                _ => None,
            })
            .collect();
        let annotation = annotation_node.and_then(|node| {
            let module = self.owner_module(owner)?.to_owned();
            let syntax = self.annotation_syntax(node, &module)?;
            self.resolve_type(&syntax, &parameter_names)
        });
        let initializer = self.check_expr(initializer_node, annotation, owner, scopes)?;
        if let Some(annotation) = annotation
            && initializer.ty != annotation
        {
            self.type_mismatch(initializer.span, annotation, initializer.ty);
            return None;
        }
        let scope = scopes.last_mut().expect("a function has a lexical scope");
        if let Some(previous) = scope.get(&name) {
            self.diagnostics.push(
                Diagnostic::error("E2102", name_node.span, format!("duplicate local `{name}`"))
                    .with_label(previous.span, "first declared here"),
            );
            return None;
        }
        let symbol = SymbolId(self.next_symbol);
        self.next_symbol += 1;
        let mutable = matches!(node.value, Some(Value::Text(ref value)) if value == "mutable");
        scope.insert(
            name.clone(),
            Local {
                symbol,
                ty: initializer.ty,
                mutable,
                span: name_node.span,
            },
        );
        Some(TypedItem::Let {
            symbol,
            name,
            mutable,
            ty: initializer.ty,
            initializer,
            span: node.span,
        })
    }

    fn annotation_syntax(&mut self, node: &Node, module: &str) -> Option<TypeSyntax> {
        match node.kind.as_str() {
            "primitive_type" => Some(TypeSyntax::Primitive {
                name: text(node),
                span: node.span,
            }),
            "type_variable" => Some(TypeSyntax::Variable {
                name: text(node),
                span: node.span,
            }),
            "atom" => match node.value.as_ref() {
                Some(Value::Atom { name, .. }) => Some(TypeSyntax::Atom {
                    name: name.clone(),
                    span: node.span,
                }),
                _ => None,
            },
            "list_or_array_type" if node.children.len() == 1 => Some(TypeSyntax::List {
                item: Box::new(self.annotation_syntax(&node.children[0], module)?),
                span: node.span,
            }),
            "list_or_array_type" => {
                let length = match node.children.get(1)?.value.as_ref() {
                    Some(Value::Text(text)) => text.replace('_', "").parse().ok()?,
                    _ => return None,
                };
                Some(TypeSyntax::Array {
                    item: Box::new(self.annotation_syntax(&node.children[0], module)?),
                    length,
                    span: node.span,
                })
            }
            "tuple_type" => Some(TypeSyntax::Tuple {
                elements: node
                    .children
                    .iter()
                    .filter_map(|child| self.annotation_syntax(child, module))
                    .collect(),
                span: node.span,
            }),
            "function_type" => {
                let (result, parameters) = node.children.split_last()?;
                Some(TypeSyntax::Function {
                    parameters: parameters
                        .iter()
                        .filter_map(|child| self.annotation_syntax(child, module))
                        .collect(),
                    result: Box::new(self.annotation_syntax(result, module)?),
                    span: node.span,
                })
            }
            "union_type" => Some(TypeSyntax::Union {
                members: node
                    .children
                    .iter()
                    .filter_map(|child| self.annotation_syntax(child, module))
                    .collect(),
                span: node.span,
            }),
            "named_type" => {
                let path = node.children.first()?;
                let written = path.children.iter().map(text).collect::<Vec<_>>().join(".");
                if written == "Map" {
                    if node.children.len() != 3 {
                        self.diagnostics.push(Diagnostic::error(
                            "E2118",
                            node.span,
                            format!(
                                "type `Map` expects 2 arguments but received {}",
                                node.children.len() - 1
                            ),
                        ));
                        return None;
                    }
                    return Some(TypeSyntax::Map {
                        key: Box::new(self.annotation_syntax(&node.children[1], module)?),
                        value: Box::new(self.annotation_syntax(&node.children[2], module)?),
                        span: node.span,
                    });
                }
                let declaration = self
                    .aliases
                    .values()
                    .find(|alias| {
                        written == alias.name && alias.module_name == module
                            || written == format!("{}.{}", alias.module_name, alias.name)
                    })
                    .map(|alias| (alias.id, alias.name.clone(), alias.parameters.len()))
                    .or_else(|| {
                        self.structs.values().find_map(|structure| {
                            (written == structure.name && structure.module_name == module
                                || written
                                    == format!("{}.{}", structure.module_name, structure.name))
                            .then(|| {
                                (
                                    structure.id,
                                    structure.name.clone(),
                                    structure.parameters.len(),
                                )
                            })
                        })
                    });
                let Some((declaration, name, arity)) = declaration else {
                    self.diagnostics.push(Diagnostic::error(
                        "E2117",
                        path.span,
                        format!("unknown type `{written}`"),
                    ));
                    return None;
                };
                if node.children.len() - 1 != arity {
                    self.diagnostics.push(Diagnostic::error(
                        "E2118",
                        node.span,
                        format!(
                            "type `{written}` expects {} arguments but received {}",
                            arity,
                            node.children.len() - 1
                        ),
                    ));
                    return None;
                }
                Some(TypeSyntax::Named {
                    declaration,
                    name,
                    arguments: node.children[1..]
                        .iter()
                        .filter_map(|child| self.annotation_syntax(child, module))
                        .collect(),
                    span: node.span,
                })
            }
            _ => None,
        }
    }

    fn check_assignment(
        &mut self,
        node: &Node,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedItem> {
        let target = node.children.first()?;
        let name = unqualified_name(target)?;
        let Some(local) = lookup(scopes, &name).cloned() else {
            self.diagnostics.push(Diagnostic::error(
                "E2103",
                target.span,
                format!("unknown local `{name}`"),
            ));
            return None;
        };
        if !local.mutable {
            self.diagnostics.push(Diagnostic::error(
                "E2104",
                target.span,
                format!("cannot assign to immutable local `{name}`"),
            ));
            return None;
        }
        let value = self.check_expr(node.children.last()?, Some(local.ty), owner, scopes)?;
        Some(TypedItem::Assign {
            symbol: local.symbol,
            value,
            span: node.span,
        })
    }

    fn check_expr(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        if let Some(expected) = expected
            && matches!(self.types[expected.0 as usize], Type::Union(_))
        {
            return self.check_union_injection(node, expected, owner, scopes);
        }
        let expression = match node.kind.as_str() {
            "integer" => self.check_integer(node, expected),
            "kw_true" | "kw_false" => Some(TypedExpr {
                kind: TypedExprKind::Boolean(matches!(node.value, Some(Value::Boolean(true)))),
                ty: TypeId(2),
                span: node.span,
            }),
            "kw_unit" => Some(TypedExpr {
                kind: TypedExprKind::Unit,
                ty: TypeId(3),
                span: node.span,
            }),
            "atom" => self.check_atom(node),
            "list_literal" => self.check_list(node, expected, owner, scopes),
            "array_literal" => self.check_array(node, expected, owner, scopes),
            "map_literal" => self.check_map(node, expected, owner, scopes),
            "if_expr" => self.check_if(node, expected, owner, scopes),
            "match_expr" => self.check_match(node, expected, owner, scopes),
            "tuple_literal" => self.check_tuple(node, expected, owner, scopes),
            "ascription_expr" => self.check_ascription(node, owner, scopes),
            "qualified_value" | "identifier" => self.check_name(node, scopes),
            "additive_expr" | "multiplicative_expr" => {
                self.check_binary(node, expected, owner, scopes)
            }
            "postfix_expr" => self.check_call(node, expected, owner, scopes),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E2105",
                    node.span,
                    format!(
                        "`{}` is not implemented in the Milestone 2 initial slice",
                        node.kind.as_str()
                    ),
                ));
                None
            }
        }?;
        if let Some(expected) = expected
            && expression.ty != expected
        {
            self.type_mismatch(node.span, expected, expression.ty);
            return None;
        }
        Some(expression)
    }

    fn check_union_injection(
        &mut self,
        node: &Node,
        union: TypeId,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let Type::Union(members) = self.types[union.0 as usize].clone() else {
            unreachable!("caller checks the expected type")
        };
        let candidates = match node.kind.as_str() {
            "integer" | "additive_expr" | "multiplicative_expr" => members
                .iter()
                .copied()
                .filter(|member| matches!(self.types[member.0 as usize], Type::I32 | Type::I64))
                .collect::<Vec<_>>(),
            "kw_true" | "kw_false" => members
                .iter()
                .copied()
                .filter(|member| matches!(self.types[member.0 as usize], Type::Bool))
                .collect(),
            "kw_unit" => members
                .iter()
                .copied()
                .filter(|member| matches!(self.types[member.0 as usize], Type::Unit))
                .collect(),
            "list_literal" => members
                .iter()
                .copied()
                .filter(|member| matches!(self.types[member.0 as usize], Type::List(_)))
                .collect(),
            "tuple_literal" => members
                .iter()
                .copied()
                .filter(|member| {
                    matches!(&self.types[member.0 as usize], Type::Tuple(elements) if elements.len() == node.children.len())
                })
                .collect(),
            _ => Vec::new(),
        };
        let value = if candidates.len() == 1 {
            self.check_expr(node, Some(candidates[0]), owner, scopes)?
        } else if candidates.len() > 1 {
            self.diagnostics.push(Diagnostic::error(
                "E2116",
                node.span,
                format!(
                    "literal is ambiguous in expected union `{}`",
                    self.type_name(union)
                ),
            ));
            return None;
        } else if node.kind.as_str() == "postfix_expr" {
            self.check_call(node, Some(union), owner, scopes)?
        } else {
            self.check_expr(node, None, owner, scopes)?
        };
        if value.ty == union {
            return Some(value);
        }
        if !members.contains(&value.ty) {
            self.type_mismatch(node.span, union, value.ty);
            return None;
        }
        let member = value.ty;
        Some(TypedExpr {
            kind: TypedExprKind::UnionInject {
                member,
                value: Box::new(value),
            },
            ty: union,
            span: node.span,
        })
    }

    fn check_atom(&mut self, node: &Node) -> Option<TypedExpr> {
        let Some(Value::Atom { name, .. }) = node.value.as_ref() else {
            return None;
        };
        let ty = self.intern(Type::Atom(name.clone()));
        Some(TypedExpr {
            kind: TypedExprKind::Atom(name.clone()),
            ty,
            span: node.span,
        })
    }

    fn check_list(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let expected_item = expected.and_then(|ty| match self.types[ty.0 as usize] {
            Type::List(item) => Some(item),
            _ => None,
        });
        if node.children.is_empty() && expected_item.is_none() {
            self.diagnostics.push(Diagnostic::error(
                "E2119",
                node.span,
                "cannot infer the item type of an empty list",
            ));
            return None;
        }
        let improper = matches!(node.value, Some(Value::Text(ref value)) if value == "improper");
        let element_count = node.children.len() - usize::from(improper);
        let mut elements = Vec::new();
        let mut item_type = expected_item;
        for element_node in &node.children[..element_count] {
            let element = self.check_expr(element_node, item_type, owner, scopes)?;
            item_type.get_or_insert(element.ty);
            elements.push(element);
        }
        let item_type = item_type.expect("an empty list requires an expected item type");
        let ty = self.intern(Type::List(item_type));
        let tail = if improper {
            Some(Box::new(self.check_expr(
                node.children.last()?,
                Some(ty),
                owner,
                scopes,
            )?))
        } else {
            None
        };
        Some(TypedExpr {
            kind: TypedExprKind::List { elements, tail },
            ty,
            span: node.span,
        })
    }

    fn check_tuple(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let expected_elements = expected.and_then(|ty| match &self.types[ty.0 as usize] {
            Type::Tuple(elements) if elements.len() == node.children.len() => {
                Some(elements.clone())
            }
            _ => None,
        });
        let elements = node
            .children
            .iter()
            .enumerate()
            .filter_map(|(index, child)| {
                self.check_expr(
                    child,
                    expected_elements.as_ref().map(|types| types[index]),
                    owner,
                    scopes,
                )
            })
            .collect::<Vec<_>>();
        if elements.len() != node.children.len() {
            return None;
        }
        let ty = self.intern(Type::Tuple(
            elements.iter().map(|element| element.ty).collect(),
        ));
        Some(TypedExpr {
            kind: TypedExprKind::Tuple(elements),
            ty,
            span: node.span,
        })
    }

    fn check_array(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let expected_item = expected.and_then(|ty| match self.types[ty.0 as usize] {
            Type::Array { item, length } if length == node.children.len() as u64 => Some(item),
            _ => None,
        });
        if node.children.is_empty() && expected_item.is_none() {
            self.diagnostics.push(Diagnostic::error(
                "E2121",
                node.span,
                "cannot infer the item type of an empty array",
            ));
            return None;
        }
        let mut item = expected_item;
        let mut elements = Vec::new();
        for child in &node.children {
            let element = self.check_expr(child, item, owner, scopes)?;
            item.get_or_insert(element.ty);
            elements.push(element);
        }
        let ty = self.intern(Type::Array {
            item: item.expect("a nonempty array supplies an item type"),
            length: elements.len() as u64,
        });
        Some(TypedExpr {
            kind: TypedExprKind::Array(elements),
            ty,
            span: node.span,
        })
    }

    fn check_map(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let expected_items = expected.and_then(|ty| match self.types[ty.0 as usize] {
            Type::Map { key, value } => Some((key, value)),
            _ => None,
        });
        if node.children.is_empty() && expected_items.is_none() {
            self.diagnostics.push(Diagnostic::error(
                "E2122",
                node.span,
                "cannot infer the key and value types of an empty map",
            ));
            return None;
        }
        let mut key_type = expected_items.map(|pair| pair.0);
        let mut value_type = expected_items.map(|pair| pair.1);
        let mut entries = Vec::new();
        for entry in &node.children {
            let key = self.check_expr(&entry.children[0], key_type, owner, scopes)?;
            let value = self.check_expr(&entry.children[1], value_type, owner, scopes)?;
            key_type.get_or_insert(key.ty);
            value_type.get_or_insert(value.ty);
            entries.push((key, value));
        }
        let key = key_type.expect("a nonempty map supplies a key type");
        let value = value_type.expect("a nonempty map supplies a value type");
        if !self.type_satisfies(key, "Eq", owner) || !self.type_satisfies(key, "Hash", owner) {
            self.diagnostics.push(Diagnostic::error(
                "E2123",
                node.span,
                format!(
                    "map key type `{}` must implement `Eq` and `Hash`",
                    self.type_name(key)
                ),
            ));
            return None;
        }
        let ty = self.intern(Type::Map { key, value });
        Some(TypedExpr {
            kind: TypedExprKind::Map(entries),
            ty,
            span: node.span,
        })
    }

    fn check_if(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let condition =
            Box::new(self.check_expr(&node.children[0], Some(TypeId(2)), owner, scopes)?);
        let has_else = node.children.len() == 3;
        if !has_else && expected.is_some() {
            self.diagnostics.push(Diagnostic::error(
                "E2124",
                node.span,
                "an `if` used as a value requires an `else` branch",
            ));
            return None;
        }
        scopes.push(BTreeMap::new());
        let then_block = self.check_block(
            &node.children[1],
            expected.or((!has_else).then_some(TypeId(3))),
            owner,
            scopes,
        );
        scopes.pop();
        let then_block = then_block?;
        let else_expected = expected.or(has_else.then_some(then_block.ty));
        let else_block = if has_else {
            scopes.push(BTreeMap::new());
            let checked = self.check_block(&node.children[2], else_expected, owner, scopes);
            scopes.pop();
            Some(checked?)
        } else {
            None
        };
        let ty = else_block.as_ref().map_or(TypeId(3), |block| block.ty);
        if then_block.ty != ty {
            self.type_mismatch(node.children[1].span, ty, then_block.ty);
            return None;
        }
        Some(TypedExpr {
            kind: TypedExprKind::If {
                condition,
                then_block,
                else_block,
            },
            ty,
            span: node.span,
        })
    }

    fn check_match(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let subject = Box::new(self.check_expr(&node.children[0], None, owner, scopes)?);
        let mut matrix = Vec::<Vec<PatternShape>>::new();
        let mut arms = Vec::new();
        let mut result_type = expected;
        for arm_node in &node.children[1..] {
            let pattern_node = &arm_node.children[0];
            let mut arm_scope = BTreeMap::new();
            let (mut pattern, shape) =
                self.check_pattern(pattern_node, subject.ty, owner, &mut arm_scope)?;
            let reachable = pattern_is_useful(
                &matrix,
                vec![shape.clone()],
                vec![subject.ty],
                &self.types,
                &self.structs,
            );
            if !reachable {
                self.diagnostics.push(Diagnostic::error(
                    "E2125",
                    pattern.span,
                    "unreachable match arm",
                ));
                continue;
            }
            pattern.facts.reachable = true;
            matrix.push(vec![shape]);
            scopes.push(arm_scope);
            let body = self.check_block(&arm_node.children[1], result_type, owner, scopes);
            scopes.pop();
            let body = body?;
            result_type.get_or_insert(body.ty);
            arms.push(TypedMatchArm { pattern, body });
        }
        let exhaustive = !pattern_is_useful(
            &matrix,
            vec![PatternShape::Wildcard],
            vec![subject.ty],
            &self.types,
            &self.structs,
        );
        if !exhaustive {
            self.diagnostics.push(Diagnostic::error(
                "E2126",
                node.span,
                "non-exhaustive match",
            ));
            return None;
        }
        Some(TypedExpr {
            kind: TypedExprKind::Match {
                subject,
                arms,
                exhaustive,
            },
            ty: result_type.unwrap_or(TypeId(3)),
            span: node.span,
        })
    }

    fn check_pattern(
        &mut self,
        node: &Node,
        subject: TypeId,
        owner: DeclId,
        bindings: &mut BTreeMap<String, Local>,
    ) -> Option<(TypedPattern, PatternShape)> {
        let (kind, irrefutable, shape) = match node.kind.as_str() {
            "wildcard_pattern" => (TypedPatternKind::Wildcard, true, PatternShape::Wildcard),
            "identifier" => {
                let name = text(node);
                if let Some(previous) = bindings.get(&name) {
                    self.diagnostics.push(
                        Diagnostic::error(
                            "E2130",
                            node.span,
                            format!("pattern binds `{name}` more than once"),
                        )
                        .with_label(previous.span, "first binding here"),
                    );
                    return None;
                }
                let symbol = SymbolId(self.next_symbol);
                self.next_symbol += 1;
                bindings.insert(
                    name.clone(),
                    Local {
                        symbol,
                        ty: subject,
                        mutable: false,
                        span: node.span,
                    },
                );
                (
                    TypedPatternKind::Binding { symbol, name },
                    true,
                    PatternShape::Wildcard,
                )
            }
            "kw_true" | "kw_false" if matches!(self.types[subject.0 as usize], Type::Bool) => {
                let value = matches!(node.value, Some(Value::Boolean(true)));
                (
                    TypedPatternKind::Boolean(value),
                    false,
                    PatternShape::Bool(value),
                )
            }
            "atom" => {
                let Some(Value::Atom { name, .. }) = node.value.as_ref() else {
                    return None;
                };
                let expected = self.intern(Type::Atom(name.clone()));
                if expected != subject {
                    self.type_mismatch(node.span, subject, expected);
                    return None;
                }
                (
                    TypedPatternKind::Atom(name.clone()),
                    false,
                    PatternShape::Atom(name.clone()),
                )
            }
            "integer" if matches!(self.types[subject.0 as usize], Type::I32 | Type::I64) => {
                let value = integer_value(node)?;
                (
                    TypedPatternKind::Integer(value),
                    false,
                    PatternShape::Integer(value),
                )
            }
            "typed_pattern" => {
                let name_node = &node.children[0];
                let module = self.owner_module(owner)?.to_owned();
                let syntax = self.annotation_syntax(&node.children[1], &module)?;
                let parameter_names = self.signatures[&owner]
                    .type_parameters
                    .iter()
                    .filter_map(|id| match &self.types[id.0 as usize] {
                        Type::Parameter { name, .. } => Some((name.clone(), *id)),
                        _ => None,
                    })
                    .collect();
                let member = self.resolve_type(&syntax, &parameter_names)?;
                let Type::Union(members) = &self.types[subject.0 as usize] else {
                    self.diagnostics.push(Diagnostic::error(
                        "E2127",
                        node.span,
                        "typed patterns require a structural union subject",
                    ));
                    return None;
                };
                if !members.contains(&member) {
                    self.diagnostics.push(Diagnostic::error(
                        "E2128",
                        node.span,
                        "typed pattern is not a member of the subject union",
                    ));
                    return None;
                }
                let name = text(name_node);
                let symbol = SymbolId(self.next_symbol);
                self.next_symbol += 1;
                bindings.insert(
                    name.clone(),
                    Local {
                        symbol,
                        ty: member,
                        mutable: false,
                        span: name_node.span,
                    },
                );
                (
                    TypedPatternKind::UnionMember {
                        member,
                        symbol,
                        name,
                    },
                    false,
                    PatternShape::Union(member),
                )
            }
            "tuple_pattern" => {
                let Type::Tuple(elements) = self.types[subject.0 as usize].clone() else {
                    self.diagnostics.push(Diagnostic::error(
                        "E2131",
                        node.span,
                        "tuple pattern requires a tuple subject",
                    ));
                    return None;
                };
                if node.children.len() != elements.len() {
                    self.diagnostics.push(Diagnostic::error(
                        "E2132",
                        node.span,
                        format!(
                            "tuple pattern has {} elements but the subject has {}",
                            node.children.len(),
                            elements.len()
                        ),
                    ));
                    return None;
                }
                let mut patterns = Vec::new();
                let mut shapes = Vec::new();
                for (child, ty) in node.children.iter().zip(elements) {
                    let (pattern, shape) = self.check_pattern(child, ty, owner, bindings)?;
                    patterns.push(pattern);
                    shapes.push((ty, shape));
                }
                let irrefutable = patterns.iter().all(|pattern| pattern.facts.irrefutable);
                (
                    TypedPatternKind::Tuple(patterns),
                    irrefutable,
                    PatternShape::Tuple(shapes),
                )
            }
            "list_pattern" => {
                let Type::List(item) = self.types[subject.0 as usize] else {
                    self.diagnostics.push(Diagnostic::error(
                        "E2133",
                        node.span,
                        "list pattern requires a list subject",
                    ));
                    return None;
                };
                if node.children.is_empty() {
                    (TypedPatternKind::ListEmpty, false, PatternShape::ListEmpty)
                } else {
                    let (head, head_shape) =
                        self.check_pattern(&node.children[0], item, owner, bindings)?;
                    let (tail, tail_shape) =
                        self.check_pattern(&node.children[1], subject, owner, bindings)?;
                    (
                        TypedPatternKind::ListCons {
                            head: Box::new(head),
                            tail: Box::new(tail),
                        },
                        false,
                        PatternShape::ListCons(
                            Box::new((item, head_shape)),
                            Box::new((subject, tail_shape)),
                        ),
                    )
                }
            }
            "struct_pattern" => {
                let Type::Struct {
                    declaration,
                    arguments,
                } = self.types[subject.0 as usize].clone()
                else {
                    self.diagnostics.push(Diagnostic::error(
                        "E2134",
                        node.span,
                        "struct pattern requires a struct subject",
                    ));
                    return None;
                };
                let structure = self.structs.get(&declaration)?.clone();
                let written = node.children[0]
                    .children
                    .iter()
                    .map(text)
                    .collect::<Vec<_>>()
                    .join(".");
                let expected_name = format!("{}.{}", structure.module_name, structure.name);
                if written != structure.name && written != expected_name {
                    self.diagnostics.push(Diagnostic::error(
                        "E2135",
                        node.children[0].span,
                        format!(
                            "pattern names `{written}`, but subject is `{}`",
                            structure.name
                        ),
                    ));
                    return None;
                }
                let substitutions = structure
                    .parameters
                    .iter()
                    .cloned()
                    .zip(arguments)
                    .collect::<BTreeMap<_, _>>();
                let mut seen = BTreeMap::<String, Span>::new();
                let mut fields = Vec::new();
                let mut shapes = Vec::with_capacity(structure.fields.len());
                for field in &structure.fields {
                    let ty = self.resolve_type(&field.ty, &substitutions)?;
                    shapes.push((ty, PatternShape::Wildcard));
                }
                for field_node in &node.children[1..] {
                    let name_node = &field_node.children[0];
                    let name = text(name_node);
                    if let Some(previous) = seen.insert(name.clone(), name_node.span) {
                        self.diagnostics.push(
                            Diagnostic::error(
                                "E2136",
                                name_node.span,
                                format!("duplicate struct pattern field `{name}`"),
                            )
                            .with_label(previous, "first field here"),
                        );
                        return None;
                    }
                    let Some(index) = structure.fields.iter().position(|field| field.name == name)
                    else {
                        self.diagnostics.push(Diagnostic::error(
                            "E2137",
                            name_node.span,
                            format!("unknown field `{name}` on `{}`", structure.name),
                        ));
                        return None;
                    };
                    let field_ty =
                        self.resolve_type(&structure.fields[index].ty, &substitutions)?;
                    let (pattern, field_shape) =
                        self.check_pattern(&field_node.children[1], field_ty, owner, bindings)?;
                    shapes[index].1 = field_shape;
                    fields.push((index, pattern));
                }
                fields.sort_by_key(|(index, _)| *index);
                let irrefutable = fields.iter().all(|(_, pattern)| pattern.facts.irrefutable);
                (
                    TypedPatternKind::Struct {
                        declaration,
                        fields,
                    },
                    irrefutable,
                    PatternShape::Struct(declaration, shapes),
                )
            }
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E2129",
                    node.span,
                    "pattern is not supported by the Milestone 2 checker",
                ));
                return None;
            }
        };
        Some((
            TypedPattern {
                kind,
                ty: subject,
                span: node.span,
                facts: PatternFacts {
                    reachable: true,
                    irrefutable,
                },
            },
            shape,
        ))
    }

    fn check_ascription(
        &mut self,
        node: &Node,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let parameters = self.signatures[&owner]
            .type_parameters
            .iter()
            .filter_map(|id| match &self.types[id.0 as usize] {
                Type::Parameter { name, .. } => Some((name.clone(), *id)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        let module = self.owner_module(owner)?.to_owned();
        let syntax = self.annotation_syntax(node.children.get(1)?, &module)?;
        let ty = self.resolve_type(&syntax, &parameters)?;
        let value = self.check_expr(&node.children[0], Some(ty), owner, scopes)?;
        Some(TypedExpr {
            kind: TypedExprKind::Ascription(Box::new(value)),
            ty,
            span: node.span,
        })
    }

    fn check_integer(&mut self, node: &Node, expected: Option<TypeId>) -> Option<TypedExpr> {
        let ty = expected
            .filter(|ty| matches!(self.types[ty.0 as usize], Type::I32 | Type::I64))
            .unwrap_or(TypeId(1));
        let Some(Value::Integer { radix, digits, .. }) = &node.value else {
            return None;
        };
        let value = u128::from_str_radix(digits, *radix).ok();
        let limit = match self.types[ty.0 as usize] {
            Type::I32 => i32::MAX as u128,
            Type::I64 => i64::MAX as u128,
            _ => unreachable!(),
        };
        if value.is_none_or(|value| value > limit) {
            self.diagnostics.push(Diagnostic::error(
                "E2106",
                node.span,
                format!(
                    "integer literal is out of range for `{}`",
                    self.type_name(ty)
                ),
            ));
            return None;
        }
        Some(TypedExpr {
            kind: TypedExprKind::Integer(value.expect("range checked") as i128),
            ty,
            span: node.span,
        })
    }

    fn check_name(&mut self, node: &Node, scopes: &[BTreeMap<String, Local>]) -> Option<TypedExpr> {
        let name = unqualified_name(node)?;
        if let Some(local) = lookup(scopes, &name) {
            return Some(TypedExpr {
                kind: TypedExprKind::Local(local.symbol),
                ty: local.ty,
                span: node.span,
            });
        }
        self.diagnostics.push(Diagnostic::error(
            "E2107",
            node.span,
            format!("unknown value `{name}`"),
        ));
        None
    }

    fn check_binary(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let operator = match node.value.as_ref() {
            Some(Value::Text(value)) if value == "+" => ArithmeticOperator::Add,
            Some(Value::Text(value)) if value == "-" => ArithmeticOperator::Subtract,
            Some(Value::Text(value)) if value == "*" => ArithmeticOperator::Multiply,
            Some(Value::Text(value)) if value == "/" => ArithmeticOperator::Divide,
            Some(Value::Text(value)) if value == "%" => ArithmeticOperator::Remainder,
            _ => return None,
        };
        let left = self.check_expr(&node.children[0], expected, owner, scopes)?;
        if !matches!(self.types[left.ty.0 as usize], Type::I32 | Type::I64) {
            self.diagnostics.push(Diagnostic::error(
                "E2108",
                node.span,
                "arithmetic requires an integer type",
            ));
            return None;
        }
        let right = self.check_expr(&node.children[1], Some(left.ty), owner, scopes)?;
        let ty = right.ty;
        Some(TypedExpr {
            kind: TypedExprKind::Binary {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
            span: node.span,
        })
    }

    fn check_call(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let callee = node.children.first()?;
        let name = self.call_name(callee, owner)?;
        let qualified = callee.kind.as_str() == "qualified_value" && callee.children.len() > 1;
        let local_name = unqualified_name(callee);
        if !qualified
            && local_name
                .as_ref()
                .is_some_and(|name| lookup(scopes, name).is_some())
        {
            self.diagnostics.push(Diagnostic::error(
                "E2109",
                callee.span,
                "indirect calls are not implemented in this slice",
            ));
            return None;
        }
        let Some(function) = self.functions_by_name.get(&name).copied() else {
            self.diagnostics.push(Diagnostic::error(
                "E2110",
                callee.span,
                format!("unknown function `{name}`"),
            ));
            return None;
        };
        let caller_module = self
            .program
            .functions
            .iter()
            .find(|candidate| candidate.id == owner)
            .map(|candidate| candidate.module_name.as_str())?;
        let called = self
            .program
            .functions
            .iter()
            .find(|candidate| candidate.id == function)?;
        if called.visibility == el_resolve::Visibility::Private
            && called.module_name != caller_module
        {
            self.diagnostics.push(Diagnostic::error(
                "E2138",
                callee.span,
                format!("function `{name}` is private"),
            ));
            return None;
        }
        let signature = self.signatures[&function].clone();
        let arguments_node = node
            .children
            .iter()
            .find(|node| node.kind.as_str() == "call_arguments")?;
        if arguments_node.children.len() != signature.parameters.len() {
            self.diagnostics.push(Diagnostic::error(
                "E2111",
                node.span,
                format!(
                    "function `{name}` expects {} arguments but received {}",
                    signature.parameters.len(),
                    arguments_node.children.len()
                ),
            ));
            return None;
        }
        let mut substitutions = BTreeMap::new();
        if let Some(expected) = expected {
            let _ = unify_types(&self.types, signature.result, expected, &mut substitutions);
        }
        let mut arguments = Vec::new();
        for (argument_node, parameter) in arguments_node.children.iter().zip(&signature.parameters)
        {
            let parameter_expected = self.apply_substitutions(*parameter, &substitutions);
            let argument = self.check_expr(
                argument_node,
                (!is_parameter(&self.types, parameter_expected)).then_some(parameter_expected),
                owner,
                scopes,
            )?;
            if !unify_types(&self.types, *parameter, argument.ty, &mut substitutions) {
                let expected = self.apply_substitutions(*parameter, &substitutions);
                self.type_mismatch(argument.span, expected, argument.ty);
                return None;
            }
            arguments.push(argument);
        }
        if let Some(missing) = signature
            .type_parameters
            .iter()
            .find(|id| !substitutions.contains_key(id))
        {
            self.diagnostics.push(Diagnostic::error(
                "E2112",
                node.span,
                format!(
                    "cannot infer generic type parameter `{}`",
                    self.type_name(*missing)
                ),
            ));
            return None;
        }
        for (parameter, protocol) in &signature.constraints {
            let concrete = self.apply_substitutions(*parameter, &substitutions);
            if !self.type_satisfies(concrete, protocol, owner) {
                self.diagnostics.push(Diagnostic::error(
                    "E2120",
                    node.span,
                    format!(
                        "type `{}` does not satisfy `{protocol}`",
                        self.type_name(concrete)
                    ),
                ));
                return None;
            }
        }
        let result = self.apply_substitutions(signature.result, &substitutions);
        Some(TypedExpr {
            kind: TypedExprKind::Call {
                function,
                substitutions: substitutions.into_iter().collect(),
                arguments,
            },
            ty: result,
            span: node.span,
        })
    }

    fn call_name(&mut self, node: &Node, owner: DeclId) -> Option<String> {
        let caller_module = self.owner_module(owner)?.to_owned();
        if let Some(name) = unqualified_name(node) {
            return Some(format!("{caller_module}.{name}"));
        }
        if node.kind.as_str() == "qualified_value" && node.children.len() >= 2 {
            let components = node.children.iter().map(text).collect::<Vec<_>>();
            let module = components[..components.len() - 1].join(".");
            let name = components.last()?;
            let qualified = format!("{module}.{name}");
            if self.functions_by_name.contains_key(&qualified) {
                return Some(qualified);
            }
            self.diagnostics.push(Diagnostic::error(
                "E2121",
                node.span,
                format!("unknown module `{module}` in this compilation"),
            ));
        }
        None
    }

    fn owner_module(&self, owner: DeclId) -> Option<&str> {
        self.program
            .functions
            .iter()
            .find(|function| function.id == owner)
            .map(|function| function.module_name.as_str())
    }

    fn intern(&mut self, ty: Type) -> TypeId {
        if let Some(index) = self.types.iter().position(|existing| existing == &ty) {
            TypeId(index as u32)
        } else {
            let id = TypeId(self.types.len() as u32);
            self.types.push(ty);
            id
        }
    }

    fn apply_substitutions(
        &mut self,
        ty: TypeId,
        substitutions: &BTreeMap<TypeId, TypeId>,
    ) -> TypeId {
        if let Some(replacement) = substitutions.get(&ty) {
            return *replacement;
        }
        match self.types[ty.0 as usize].clone() {
            Type::List(item) => {
                let item = self.apply_substitutions(item, substitutions);
                self.intern(Type::List(item))
            }
            Type::Array { item, length } => {
                let item = self.apply_substitutions(item, substitutions);
                self.intern(Type::Array { item, length })
            }
            Type::Map { key, value } => {
                let key = self.apply_substitutions(key, substitutions);
                let value = self.apply_substitutions(value, substitutions);
                self.intern(Type::Map { key, value })
            }
            Type::Tuple(elements) => {
                let elements = elements
                    .into_iter()
                    .map(|element| self.apply_substitutions(element, substitutions))
                    .collect();
                self.intern(Type::Tuple(elements))
            }
            Type::Function { parameters, result } => {
                let parameters = parameters
                    .into_iter()
                    .map(|parameter| self.apply_substitutions(parameter, substitutions))
                    .collect();
                let result = self.apply_substitutions(result, substitutions);
                self.intern(Type::Function { parameters, result })
            }
            Type::Union(members) => {
                let members = members
                    .into_iter()
                    .map(|member| self.apply_substitutions(member, substitutions))
                    .collect();
                self.intern(Type::Union(members))
            }
            Type::Struct {
                declaration,
                arguments,
            } => {
                let arguments = arguments
                    .into_iter()
                    .map(|argument| self.apply_substitutions(argument, substitutions))
                    .collect();
                self.intern(Type::Struct {
                    declaration,
                    arguments,
                })
            }
            _ => ty,
        }
    }

    fn type_satisfies(&self, ty: TypeId, protocol: &str, owner: DeclId) -> bool {
        match &self.types[ty.0 as usize] {
            Type::Parameter { .. } => self.signatures.get(&owner).is_some_and(|signature| {
                signature
                    .constraints
                    .iter()
                    .any(|(parameter, required)| *parameter == ty && required == protocol)
            }),
            Type::I32 | Type::I64 | Type::Bool | Type::Unit | Type::Atom(_) => {
                matches!(protocol, "Eq" | "Ord" | "Show" | "Hash")
            }
            Type::List(item) => match protocol {
                "Iterable" | "Concat" => true,
                "Eq" | "Ord" | "Show" | "Hash" => self.type_satisfies(*item, protocol, owner),
                _ => false,
            },
            Type::Array { item, .. } => match protocol {
                "Iterable" => true,
                "Eq" | "Ord" | "Show" | "Hash" => self.type_satisfies(*item, protocol, owner),
                _ => false,
            },
            Type::Map { key, value } => match protocol {
                "Iterable" => true,
                "Eq" | "Show" => {
                    self.type_satisfies(*key, "Eq", owner)
                        && self.type_satisfies(*key, "Hash", owner)
                        && self.type_satisfies(*value, protocol, owner)
                }
                _ => false,
            },
            Type::Tuple(elements) => {
                matches!(protocol, "Eq" | "Ord" | "Show" | "Hash")
                    && elements
                        .iter()
                        .all(|element| self.type_satisfies(*element, protocol, owner))
            }
            Type::Struct { .. } | Type::Function { .. } | Type::Union(_) => false,
        }
    }

    fn type_mismatch(&mut self, span: Span, expected: TypeId, actual: TypeId) {
        self.diagnostics.push(Diagnostic::error(
            "E2113",
            span,
            format!(
                "expected `{}`, found `{}`",
                self.type_name(expected),
                self.type_name(actual)
            ),
        ));
    }

    fn type_name(&self, id: TypeId) -> String {
        match &self.types[id.0 as usize] {
            Type::I32 => "i32".to_owned(),
            Type::I64 => "i64".to_owned(),
            Type::Bool => "bool".to_owned(),
            Type::Unit => "unit".to_owned(),
            Type::Atom(name) => format!(":{name}"),
            Type::List(item) => format!("[{}]", self.type_name(*item)),
            Type::Array { item, length } => format!("[{}; {length}]", self.type_name(*item)),
            Type::Map { key, value } => {
                format!("Map({}, {})", self.type_name(*key), self.type_name(*value))
            }
            Type::Tuple(elements) => format!(
                "{{{}}}",
                elements
                    .iter()
                    .map(|element| self.type_name(*element))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Type::Function { parameters, result } => format!(
                "({}) -> {}",
                parameters
                    .iter()
                    .map(|parameter| self.type_name(*parameter))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.type_name(*result)
            ),
            Type::Struct {
                declaration,
                arguments,
            } => {
                let name = self
                    .structs
                    .get(declaration)
                    .map_or("<struct>", |structure| structure.name.as_str());
                if arguments.is_empty() {
                    name.to_owned()
                } else {
                    format!(
                        "{name}({})",
                        arguments
                            .iter()
                            .map(|argument| self.type_name(*argument))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            Type::Parameter { name, .. } => name.clone(),
            Type::Union(members) => members
                .iter()
                .map(|member| self.type_name(*member))
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }

    fn type_key(&self, id: TypeId) -> String {
        match &self.types[id.0 as usize] {
            Type::I32 => "00:i32".to_owned(),
            Type::I64 => "00:i64".to_owned(),
            Type::Bool => "00:bool".to_owned(),
            Type::Unit => "00:unit".to_owned(),
            Type::Atom(name) => format!("01:{name}"),
            Type::List(item) => format!("02:[{}]", self.type_key(*item)),
            Type::Array { item, length } => format!("02a:[{};{length}]", self.type_key(*item)),
            Type::Map { key, value } => {
                format!("02m:Map({},{})", self.type_key(*key), self.type_key(*value))
            }
            Type::Tuple(elements) => format!(
                "03:{{{}}}",
                elements
                    .iter()
                    .map(|element| self.type_key(*element))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Type::Function { parameters, result } => format!(
                "04:({})->{}",
                parameters
                    .iter()
                    .map(|parameter| self.type_key(*parameter))
                    .collect::<Vec<_>>()
                    .join(","),
                self.type_key(*result)
            ),
            Type::Struct {
                declaration,
                arguments,
            } => format!(
                "05:{}({})",
                declaration.0,
                arguments
                    .iter()
                    .map(|argument| self.type_key(*argument))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Type::Parameter { owner, name } => format!("10:{}:{name}", owner.0),
            Type::Union(members) => format!(
                "20:{}",
                members
                    .iter()
                    .map(|member| self.type_key(*member))
                    .collect::<Vec<_>>()
                    .join("|")
            ),
        }
    }
}

fn unify_types(
    types: &[Type],
    left: TypeId,
    right: TypeId,
    substitutions: &mut BTreeMap<TypeId, TypeId>,
) -> bool {
    let left = follow_substitution(left, substitutions);
    let right = follow_substitution(right, substitutions);
    if left == right {
        if matches!(types[left.0 as usize], Type::Parameter { .. }) {
            substitutions.entry(left).or_insert(left);
        }
        return true;
    }
    if matches!(types[left.0 as usize], Type::Parameter { .. }) {
        return bind_type_variable(types, left, right, substitutions);
    }
    if matches!(types[right.0 as usize], Type::Parameter { .. }) {
        return bind_type_variable(types, right, left, substitutions);
    }
    match (&types[left.0 as usize], &types[right.0 as usize]) {
        (Type::Atom(left), Type::Atom(right)) => left == right,
        (Type::List(left), Type::List(right)) => unify_types(types, *left, *right, substitutions),
        (
            Type::Array {
                item: left,
                length: left_length,
            },
            Type::Array {
                item: right,
                length: right_length,
            },
        ) if left_length == right_length => unify_types(types, *left, *right, substitutions),
        (
            Type::Map {
                key: left_key,
                value: left_value,
            },
            Type::Map {
                key: right_key,
                value: right_value,
            },
        ) => {
            unify_types(types, *left_key, *right_key, substitutions)
                && unify_types(types, *left_value, *right_value, substitutions)
        }
        (Type::Tuple(left), Type::Tuple(right)) if left.len() == right.len() => left
            .iter()
            .zip(right)
            .all(|(left, right)| unify_types(types, *left, *right, substitutions)),
        (
            Type::Struct {
                declaration: left_declaration,
                arguments: left_arguments,
            },
            Type::Struct {
                declaration: right_declaration,
                arguments: right_arguments,
            },
        ) if left_declaration == right_declaration
            && left_arguments.len() == right_arguments.len() =>
        {
            left_arguments
                .iter()
                .zip(right_arguments)
                .all(|(left, right)| unify_types(types, *left, *right, substitutions))
        }
        (
            Type::Function {
                parameters: left_parameters,
                result: left_result,
            },
            Type::Function {
                parameters: right_parameters,
                result: right_result,
            },
        ) if left_parameters.len() == right_parameters.len() => {
            left_parameters
                .iter()
                .zip(right_parameters)
                .all(|(left, right)| unify_types(types, *left, *right, substitutions))
                && unify_types(types, *left_result, *right_result, substitutions)
        }
        (Type::Union(left), Type::Union(right)) if left.len() == right.len() => left
            .iter()
            .zip(right)
            .all(|(left, right)| unify_types(types, *left, *right, substitutions)),
        _ => false,
    }
}

fn integer_value(node: &Node) -> Option<i128> {
    let Some(Value::Integer { radix, digits, .. }) = &node.value else {
        return None;
    };
    i128::from_str_radix(digits, *radix).ok()
}

fn collect_type_variables(ty: &TypeSyntax, output: &mut Vec<String>) {
    match ty {
        TypeSyntax::Variable { name, .. } => {
            if !output.contains(name) {
                output.push(name.clone());
            }
        }
        TypeSyntax::Named { arguments, .. } => {
            for ty in arguments {
                collect_type_variables(ty, output);
            }
        }
        TypeSyntax::Union { members, .. } => {
            for ty in members {
                collect_type_variables(ty, output);
            }
        }
        TypeSyntax::List { item, .. } | TypeSyntax::Array { item, .. } => {
            collect_type_variables(item, output)
        }
        TypeSyntax::Map { key, value, .. } => {
            collect_type_variables(key, output);
            collect_type_variables(value, output);
        }
        TypeSyntax::Tuple { elements, .. } => {
            for ty in elements {
                collect_type_variables(ty, output);
            }
        }
        TypeSyntax::Function {
            parameters, result, ..
        } => {
            for ty in parameters {
                collect_type_variables(ty, output);
            }
            collect_type_variables(result, output);
        }
        TypeSyntax::Primitive { .. } | TypeSyntax::Atom { .. } => {}
    }
}

fn bind_type_variable(
    types: &[Type],
    variable: TypeId,
    value: TypeId,
    substitutions: &mut BTreeMap<TypeId, TypeId>,
) -> bool {
    if occurs_in(types, variable, value, substitutions) {
        return false;
    }
    substitutions.insert(variable, value);
    true
}

fn occurs_in(
    types: &[Type],
    variable: TypeId,
    value: TypeId,
    substitutions: &BTreeMap<TypeId, TypeId>,
) -> bool {
    let value = follow_substitution(value, substitutions);
    if variable == value {
        return true;
    }
    match &types[value.0 as usize] {
        Type::List(item) => occurs_in(types, variable, *item, substitutions),
        Type::Array { item, .. } => occurs_in(types, variable, *item, substitutions),
        Type::Map { key, value } => {
            occurs_in(types, variable, *key, substitutions)
                || occurs_in(types, variable, *value, substitutions)
        }
        Type::Tuple(elements) | Type::Union(elements) => elements
            .iter()
            .any(|element| occurs_in(types, variable, *element, substitutions)),
        Type::Function { parameters, result } => {
            parameters
                .iter()
                .any(|parameter| occurs_in(types, variable, *parameter, substitutions))
                || occurs_in(types, variable, *result, substitutions)
        }
        Type::Struct { arguments, .. } => arguments
            .iter()
            .any(|argument| occurs_in(types, variable, *argument, substitutions)),
        _ => false,
    }
}

fn follow_substitution(mut ty: TypeId, substitutions: &BTreeMap<TypeId, TypeId>) -> TypeId {
    while let Some(next) = substitutions.get(&ty) {
        if *next == ty {
            break;
        }
        ty = *next;
    }
    ty
}

fn is_type_node(node: &Node) -> bool {
    matches!(
        node.kind.as_str(),
        "primitive_type"
            | "type_variable"
            | "named_type"
            | "union_type"
            | "atom"
            | "list_or_array_type"
            | "tuple_type"
            | "function_type"
    )
}

fn text(node: &Node) -> String {
    match node.value.as_ref() {
        Some(Value::Text(value)) => value.clone(),
        _ => String::new(),
    }
}

fn unqualified_name(node: &Node) -> Option<String> {
    match node.kind.as_str() {
        "identifier" => Some(text(node)),
        "qualified_value" if node.children.len() == 1 => Some(text(&node.children[0])),
        _ => None,
    }
}

fn lookup<'a>(scopes: &'a [BTreeMap<String, Local>], name: &str) -> Option<&'a Local> {
    scopes.iter().rev().find_map(|scope| scope.get(name))
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PatternConstructor {
    Bool(bool),
    Integer(i128),
    Atom(String),
    Union(TypeId),
    Tuple(usize),
    ListEmpty,
    ListCons,
    Struct(DeclId),
}

fn pattern_is_useful(
    matrix: &[Vec<PatternShape>],
    query: Vec<PatternShape>,
    types_to_match: Vec<TypeId>,
    types: &[Type],
    structs: &BTreeMap<DeclId, Struct>,
) -> bool {
    if query.is_empty() {
        return matrix.is_empty();
    }
    let head_ty = types_to_match[0];
    let tail_types = &types_to_match[1..];
    match &query[0] {
        PatternShape::Wildcard => {
            let defaults = matrix
                .iter()
                .filter(|row| matches!(row.first(), Some(PatternShape::Wildcard)))
                .map(|row| row[1..].to_vec())
                .collect::<Vec<_>>();
            if !defaults.is_empty()
                && !pattern_is_useful(
                    &defaults,
                    query[1..].to_vec(),
                    tail_types.to_vec(),
                    types,
                    structs,
                )
            {
                return false;
            }
            if let Some(constructors) = complete_constructors(head_ty, matrix, types, structs) {
                constructors.into_iter().any(|constructor| {
                    let component_types =
                        constructor_component_types(&constructor, head_ty, matrix, types);
                    let specialized = specialize_matrix(matrix, &constructor);
                    let mut specialized_query = vec![PatternShape::Wildcard; component_types.len()];
                    specialized_query.extend_from_slice(&query[1..]);
                    let mut specialized_types = component_types;
                    specialized_types.extend_from_slice(tail_types);
                    pattern_is_useful(
                        &specialized,
                        specialized_query,
                        specialized_types,
                        types,
                        structs,
                    )
                })
            } else {
                pattern_is_useful(
                    &defaults,
                    query[1..].to_vec(),
                    tail_types.to_vec(),
                    types,
                    structs,
                )
            }
        }
        shape => {
            let constructor = shape_constructor(shape).expect("non-wildcard pattern constructor");
            let component_types = shape_components(shape)
                .iter()
                .map(|(ty, _)| *ty)
                .collect::<Vec<_>>();
            let specialized = specialize_matrix(matrix, &constructor);
            let mut specialized_query = shape_components(shape)
                .iter()
                .map(|(_, shape)| shape.clone())
                .collect::<Vec<_>>();
            specialized_query.extend_from_slice(&query[1..]);
            let mut specialized_types = component_types;
            specialized_types.extend_from_slice(tail_types);
            pattern_is_useful(
                &specialized,
                specialized_query,
                specialized_types,
                types,
                structs,
            )
        }
    }
}

fn complete_constructors(
    ty: TypeId,
    matrix: &[Vec<PatternShape>],
    types: &[Type],
    structs: &BTreeMap<DeclId, Struct>,
) -> Option<Vec<PatternConstructor>> {
    match &types[ty.0 as usize] {
        Type::Bool => Some(vec![
            PatternConstructor::Bool(false),
            PatternConstructor::Bool(true),
        ]),
        Type::Atom(name) => Some(vec![PatternConstructor::Atom(name.clone())]),
        Type::Tuple(elements) => Some(vec![PatternConstructor::Tuple(elements.len())]),
        Type::List(_) => Some(vec![
            PatternConstructor::ListEmpty,
            PatternConstructor::ListCons,
        ]),
        Type::Struct { declaration, .. } if structs.contains_key(declaration) => {
            Some(vec![PatternConstructor::Struct(*declaration)])
        }
        Type::Union(members) => Some(
            members
                .iter()
                .copied()
                .map(PatternConstructor::Union)
                .collect(),
        ),
        _ => {
            let _ = matrix;
            None
        }
    }
}

fn constructor_component_types(
    constructor: &PatternConstructor,
    ty: TypeId,
    matrix: &[Vec<PatternShape>],
    types: &[Type],
) -> Vec<TypeId> {
    match constructor {
        PatternConstructor::Tuple(_) => match &types[ty.0 as usize] {
            Type::Tuple(elements) => elements.clone(),
            _ => Vec::new(),
        },
        PatternConstructor::ListCons => match types[ty.0 as usize] {
            Type::List(item) => vec![item, ty],
            _ => Vec::new(),
        },
        PatternConstructor::Struct(declaration) => matrix
            .iter()
            .filter_map(|row| row.first())
            .find_map(|shape| match shape {
                PatternShape::Struct(found, fields) if found == declaration => {
                    Some(fields.iter().map(|(ty, _)| *ty).collect())
                }
                _ => None,
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn specialize_matrix(
    matrix: &[Vec<PatternShape>],
    constructor: &PatternConstructor,
) -> Vec<Vec<PatternShape>> {
    matrix
        .iter()
        .filter_map(|row| {
            let head = row.first()?;
            let mut result = if matches!(head, PatternShape::Wildcard) {
                vec![PatternShape::Wildcard; constructor_arity(constructor, matrix)]
            } else if shape_constructor(head).as_ref() == Some(constructor) {
                shape_components(head)
                    .iter()
                    .map(|(_, shape)| shape.clone())
                    .collect()
            } else {
                return None;
            };
            result.extend_from_slice(&row[1..]);
            Some(result)
        })
        .collect()
}

fn constructor_arity(constructor: &PatternConstructor, matrix: &[Vec<PatternShape>]) -> usize {
    match constructor {
        PatternConstructor::Tuple(arity) => *arity,
        PatternConstructor::ListCons => 2,
        PatternConstructor::Struct(declaration) => matrix
            .iter()
            .filter_map(|row| row.first())
            .find_map(|shape| match shape {
                PatternShape::Struct(found, fields) if found == declaration => Some(fields.len()),
                _ => None,
            })
            .unwrap_or(0),
        _ => 0,
    }
}

fn shape_constructor(shape: &PatternShape) -> Option<PatternConstructor> {
    match shape {
        PatternShape::Wildcard => None,
        PatternShape::Bool(value) => Some(PatternConstructor::Bool(*value)),
        PatternShape::Integer(value) => Some(PatternConstructor::Integer(*value)),
        PatternShape::Atom(value) => Some(PatternConstructor::Atom(value.clone())),
        PatternShape::Union(member) => Some(PatternConstructor::Union(*member)),
        PatternShape::Tuple(elements) => Some(PatternConstructor::Tuple(elements.len())),
        PatternShape::ListEmpty => Some(PatternConstructor::ListEmpty),
        PatternShape::ListCons(_, _) => Some(PatternConstructor::ListCons),
        PatternShape::Struct(declaration, _) => Some(PatternConstructor::Struct(*declaration)),
    }
}

fn shape_components(shape: &PatternShape) -> Vec<(TypeId, PatternShape)> {
    match shape {
        PatternShape::Tuple(elements) | PatternShape::Struct(_, elements) => elements.clone(),
        PatternShape::ListCons(head, tail) => vec![(**head).clone(), (**tail).clone()],
        _ => Vec::new(),
    }
}

fn is_parameter(types: &[Type], ty: TypeId) -> bool {
    matches!(types[ty.0 as usize], Type::Parameter { .. })
}

/// Checks the fully typed boundary independently from the type checker.
pub fn verify(program: &TypedProgram) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    let type_count = program.types.len() as u32;
    verify_type_table(&program.types, &mut errors);
    let declarations = program
        .functions
        .iter()
        .map(|function| function.id)
        .collect::<BTreeSet<_>>();
    for function in &program.functions {
        if function.result.0 >= type_count || function.body.ty.0 >= type_count {
            errors.push(format!(
                "function {:?} references an unknown type",
                function.id
            ));
        }
        if function.body.ty != function.result {
            errors.push(format!(
                "function {:?} body type differs from its result",
                function.id
            ));
        }
        let mut symbols = BTreeMap::new();
        let mut mutable = BTreeSet::new();
        for parameter in &function.parameters {
            if parameter.ty.0 >= type_count {
                errors.push(format!(
                    "parameter {:?} references an unknown type",
                    parameter.symbol
                ));
            }
            if symbols.insert(parameter.symbol, parameter.ty).is_some() {
                errors.push(format!("symbol {:?} is defined twice", parameter.symbol));
            }
        }
        for item in &function.body.items {
            verify_item(
                item,
                &program.types,
                type_count,
                &declarations,
                &mut symbols,
                &mut mutable,
                &mut errors,
            );
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn verify_type_table(types: &[Type], errors: &mut Vec<String>) {
    for (index, ty) in types.iter().enumerate() {
        let referenced = match ty {
            Type::List(item) | Type::Array { item, .. } => std::slice::from_ref(item),
            Type::Map { key, value } => {
                if key.0 as usize >= types.len() || value.0 as usize >= types.len() {
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
            if reference.0 as usize >= types.len() {
                errors.push(format!(
                    "type t{index} references unknown type {reference:?}"
                ));
            }
        }
        if let Type::Function { result, .. } = ty
            && result.0 as usize >= types.len()
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
                if member.0 as usize >= types.len() {
                    errors.push(format!(
                        "union type t{index} references unknown member {member:?}"
                    ));
                } else if matches!(types[member.0 as usize], Type::Union(_)) {
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
}

fn verify_item(
    item: &TypedItem,
    types: &[Type],
    type_count: u32,
    declarations: &BTreeSet<DeclId>,
    symbols: &mut BTreeMap<SymbolId, TypeId>,
    mutable_symbols: &mut BTreeSet<SymbolId>,
    errors: &mut Vec<String>,
) {
    match item {
        TypedItem::Let {
            symbol,
            ty,
            initializer,
            ..
        } => {
            verify_expr(
                initializer,
                types,
                type_count,
                declarations,
                symbols,
                errors,
            );
            if initializer.ty != *ty {
                errors.push(format!("binding {symbol:?} has inconsistent types"));
            }
            if symbols.insert(*symbol, *ty).is_some() {
                errors.push(format!("symbol {symbol:?} is defined twice"));
            }
            if matches!(item, TypedItem::Let { mutable: true, .. }) {
                mutable_symbols.insert(*symbol);
            }
        }
        TypedItem::Assign { symbol, value, .. } => {
            match symbols.get(symbol) {
                Some(ty) if *ty != value.ty => {
                    errors.push(format!("assignment to {symbol:?} has an incorrect type"))
                }
                None => errors.push(format!("assignment references unknown symbol {symbol:?}")),
                _ => {}
            }
            if !mutable_symbols.contains(symbol) {
                errors.push(format!("assignment targets immutable symbol {symbol:?}"));
            }
            verify_expr(value, types, type_count, declarations, symbols, errors);
        }
        TypedItem::Expr(expression) | TypedItem::Return(expression) => {
            verify_expr(expression, types, type_count, declarations, symbols, errors);
        }
    }
}

fn verify_expr(
    expression: &TypedExpr,
    types: &[Type],
    type_count: u32,
    declarations: &BTreeSet<DeclId>,
    symbols: &BTreeMap<SymbolId, TypeId>,
    errors: &mut Vec<String>,
) {
    if expression.ty.0 >= type_count {
        errors.push(format!(
            "expression references unknown type {:?}",
            expression.ty
        ));
    }
    match &expression.kind {
        TypedExprKind::Integer(_)
            if !matches!(
                types.get(expression.ty.0 as usize),
                Some(Type::I32 | Type::I64)
            ) =>
        {
            errors.push("integer expression has a non-integer type".to_owned());
        }
        TypedExprKind::Boolean(_)
            if !matches!(types.get(expression.ty.0 as usize), Some(Type::Bool)) =>
        {
            errors.push("boolean expression has a non-bool type".to_owned());
        }
        TypedExprKind::Unit if !matches!(types.get(expression.ty.0 as usize), Some(Type::Unit)) => {
            errors.push("unit expression has a non-unit type".to_owned());
        }
        TypedExprKind::Atom(name) if !matches!(types.get(expression.ty.0 as usize), Some(Type::Atom(expected)) if expected == name) =>
        {
            errors.push("atom expression has an incorrect type".to_owned());
        }
        TypedExprKind::Local(symbol) => match symbols.get(symbol) {
            Some(ty) if *ty != expression.ty => {
                errors.push(format!("local {symbol:?} has an incorrect type"))
            }
            None => errors.push(format!("unknown local {symbol:?}")),
            _ => {}
        },
        TypedExprKind::List { elements, tail } => {
            let item_type = match types.get(expression.ty.0 as usize) {
                Some(Type::List(item)) => Some(*item),
                _ => {
                    errors.push("list expression has a non-list type".to_owned());
                    None
                }
            };
            for element in elements {
                verify_expr(element, types, type_count, declarations, symbols, errors);
                if item_type.is_some_and(|item| element.ty != item) {
                    errors.push("list element has an incorrect type".to_owned());
                }
            }
            if let Some(tail) = tail {
                verify_expr(tail, types, type_count, declarations, symbols, errors);
                if tail.ty != expression.ty {
                    errors.push("list tail has an incorrect type".to_owned());
                }
            }
        }
        TypedExprKind::Tuple(elements) => {
            let element_types = match types.get(expression.ty.0 as usize) {
                Some(Type::Tuple(element_types)) if element_types.len() == elements.len() => {
                    Some(element_types)
                }
                _ => {
                    errors.push("tuple expression has an incorrect type".to_owned());
                    None
                }
            };
            for (index, element) in elements.iter().enumerate() {
                verify_expr(element, types, type_count, declarations, symbols, errors);
                if element_types.is_some_and(|types| element.ty != types[index]) {
                    errors.push("tuple element has an incorrect type".to_owned());
                }
            }
        }
        TypedExprKind::Array(elements) => {
            let item = match types.get(expression.ty.0 as usize) {
                Some(Type::Array { item, length }) if *length == elements.len() as u64 => {
                    Some(*item)
                }
                _ => {
                    errors.push("array expression has an incorrect type".to_owned());
                    None
                }
            };
            for element in elements {
                verify_expr(element, types, type_count, declarations, symbols, errors);
                if item.is_some_and(|item| item != element.ty) {
                    errors.push("array element has an incorrect type".to_owned());
                }
            }
        }
        TypedExprKind::Map(entries) => {
            let components = match types.get(expression.ty.0 as usize) {
                Some(Type::Map { key, value }) => Some((*key, *value)),
                _ => {
                    errors.push("map expression has a non-map type".to_owned());
                    None
                }
            };
            for (key, value) in entries {
                verify_expr(key, types, type_count, declarations, symbols, errors);
                verify_expr(value, types, type_count, declarations, symbols, errors);
                if components.is_some_and(|pair| pair != (key.ty, value.ty)) {
                    errors.push("map entry has incorrect types".to_owned());
                }
            }
        }
        TypedExprKind::If {
            condition,
            then_block,
            else_block,
        } => {
            verify_expr(condition, types, type_count, declarations, symbols, errors);
            if !matches!(types.get(condition.ty.0 as usize), Some(Type::Bool)) {
                errors.push("if condition has a non-bool type".to_owned());
            }
            let mut verify_nested = |block: &TypedBlock| {
                let mut nested_symbols = symbols.clone();
                let mut nested_mutable = BTreeSet::new();
                for item in &block.items {
                    verify_item(
                        item,
                        types,
                        type_count,
                        declarations,
                        &mut nested_symbols,
                        &mut nested_mutable,
                        errors,
                    );
                }
                if block.ty != expression.ty {
                    errors.push("if branch has an incorrect result type".to_owned());
                }
            };
            verify_nested(then_block);
            if let Some(block) = else_block {
                verify_nested(block);
            } else if !matches!(types.get(expression.ty.0 as usize), Some(Type::Unit)) {
                errors.push("if without else has a non-unit type".to_owned());
            }
        }
        TypedExprKind::Match {
            subject,
            arms,
            exhaustive,
        } => {
            verify_expr(subject, types, type_count, declarations, symbols, errors);
            if !*exhaustive {
                errors.push("typed match is not exhaustive".to_owned());
            }
            for arm in arms {
                if arm.pattern.ty != subject.ty || !arm.pattern.facts.reachable {
                    errors.push("match pattern facts are inconsistent".to_owned());
                }
                let mut nested_symbols = symbols.clone();
                verify_pattern(&arm.pattern, types, type_count, &mut nested_symbols, errors);
                let mut nested_mutable = BTreeSet::new();
                for item in &arm.body.items {
                    verify_item(
                        item,
                        types,
                        type_count,
                        declarations,
                        &mut nested_symbols,
                        &mut nested_mutable,
                        errors,
                    );
                }
                if arm.body.ty != expression.ty {
                    errors.push("match arm has an incorrect result type".to_owned());
                }
            }
        }
        TypedExprKind::Ascription(value) => {
            verify_expr(value, types, type_count, declarations, symbols, errors);
            if value.ty != expression.ty {
                errors.push("ascription changed the expression type".to_owned());
            }
        }
        TypedExprKind::Binary { left, right, .. } => {
            verify_expr(left, types, type_count, declarations, symbols, errors);
            verify_expr(right, types, type_count, declarations, symbols, errors);
            if left.ty != right.ty || left.ty != expression.ty {
                errors.push("binary operand types differ".to_owned());
            }
        }
        TypedExprKind::Call {
            function,
            arguments,
            ..
        } => {
            if !declarations.contains(function) {
                errors.push(format!("call references unknown function {function:?}"));
            }
            for argument in arguments {
                verify_expr(argument, types, type_count, declarations, symbols, errors);
            }
        }
        TypedExprKind::UnionInject { member, value } => {
            verify_expr(value, types, type_count, declarations, symbols, errors);
            if value.ty != *member {
                errors.push("union injection value does not match its member".to_owned());
            }
            if !matches!(types.get(expression.ty.0 as usize), Some(Type::Union(members)) if members.contains(member))
            {
                errors.push("union injection names a non-member type".to_owned());
            }
        }
        _ => {}
    }
}

fn verify_pattern(
    pattern: &TypedPattern,
    types: &[Type],
    type_count: u32,
    symbols: &mut BTreeMap<SymbolId, TypeId>,
    errors: &mut Vec<String>,
) {
    if pattern.ty.0 >= type_count {
        errors.push("pattern has an unknown type".to_owned());
        return;
    }
    match &pattern.kind {
        TypedPatternKind::Binding { symbol, .. } => {
            if symbols.insert(*symbol, pattern.ty).is_some() {
                errors.push("pattern binding symbol is defined twice".to_owned());
            }
        }
        TypedPatternKind::UnionMember { member, symbol, .. } => {
            if !matches!(&types[pattern.ty.0 as usize], Type::Union(members) if members.contains(member))
            {
                errors.push("union pattern names an invalid member".to_owned());
            }
            if symbols.insert(*symbol, *member).is_some() {
                errors.push("pattern binding symbol is defined twice".to_owned());
            }
        }
        TypedPatternKind::Tuple(elements) => {
            let expected = match &types[pattern.ty.0 as usize] {
                Type::Tuple(elements) => Some(elements),
                _ => None,
            };
            if expected.is_none_or(|expected| expected.len() != elements.len()) {
                errors.push("tuple pattern has an incorrect type or arity".to_owned());
            }
            for child in elements {
                verify_pattern(child, types, type_count, symbols, errors);
            }
        }
        TypedPatternKind::ListEmpty => {
            if !matches!(types[pattern.ty.0 as usize], Type::List(_)) {
                errors.push("empty-list pattern has a non-list type".to_owned());
            }
        }
        TypedPatternKind::ListCons { head, tail } => {
            if !matches!(types[pattern.ty.0 as usize], Type::List(item) if item == head.ty && tail.ty == pattern.ty)
            {
                errors.push("list pattern components have incorrect types".to_owned());
            }
            verify_pattern(head, types, type_count, symbols, errors);
            verify_pattern(tail, types, type_count, symbols, errors);
        }
        TypedPatternKind::Struct {
            declaration,
            fields,
        } => {
            if !matches!(types[pattern.ty.0 as usize], Type::Struct { declaration: found, .. } if found == *declaration)
            {
                errors.push("struct pattern has an incorrect nominal type".to_owned());
            }
            for (_, child) in fields {
                verify_pattern(child, types, type_count, symbols, errors);
            }
        }
        TypedPatternKind::Wildcard
        | TypedPatternKind::Boolean(_)
        | TypedPatternKind::Integer(_)
        | TypedPatternKind::Atom(_) => {}
    }
}

impl TypedProgram {
    #[must_use]
    pub fn debug_tree(&self) -> String {
        let mut output = format!("module {}\n", self.module_name);
        for function in &self.functions {
            output.push_str(&format!(
                "  function d{} {} -> {}\n",
                function.id.0,
                function.name,
                self.display_type(function.result)
            ));
            for parameter in &function.parameters {
                output.push_str(&format!(
                    "    parameter s{} {}: {}\n",
                    parameter.symbol.0,
                    parameter.name,
                    self.display_type(parameter.ty)
                ));
            }
            write_items(self, &mut output, &function.body.items, 2);
        }
        output
    }

    fn display_type(&self, id: TypeId) -> String {
        match &self.types[id.0 as usize] {
            Type::I32 => "i32".to_owned(),
            Type::I64 => "i64".to_owned(),
            Type::Bool => "bool".to_owned(),
            Type::Unit => "unit".to_owned(),
            Type::Atom(name) => format!(":{name}"),
            Type::List(item) => format!("[{}]", self.display_type(*item)),
            Type::Array { item, length } => format!("[{}; {length}]", self.display_type(*item)),
            Type::Map { key, value } => format!(
                "Map({}, {})",
                self.display_type(*key),
                self.display_type(*value)
            ),
            Type::Tuple(elements) => format!(
                "{{{}}}",
                elements
                    .iter()
                    .map(|element| self.display_type(*element))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Type::Function { parameters, result } => format!(
                "({}) -> {}",
                parameters
                    .iter()
                    .map(|parameter| self.display_type(*parameter))
                    .collect::<Vec<_>>()
                    .join(", "),
                self.display_type(*result)
            ),
            Type::Struct {
                declaration,
                arguments,
            } => {
                let name = self
                    .structs
                    .iter()
                    .find(|structure| structure.id == *declaration)
                    .map_or("<struct>", |structure| structure.name.as_str());
                if arguments.is_empty() {
                    name.to_owned()
                } else {
                    format!(
                        "{name}({})",
                        arguments
                            .iter()
                            .map(|argument| self.display_type(*argument))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            Type::Parameter { name, .. } => name.clone(),
            Type::Union(members) => members
                .iter()
                .map(|member| self.display_type(*member))
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }
}

fn write_items(program: &TypedProgram, output: &mut String, items: &[TypedItem], depth: usize) {
    let indent = "  ".repeat(depth);
    for item in items {
        match item {
            TypedItem::Let {
                symbol,
                name,
                mutable,
                ty,
                initializer,
                ..
            } => {
                output.push_str(&format!(
                    "{indent}let s{} {name} {}: {}\n",
                    symbol.0,
                    if *mutable { "mut" } else { "imm" },
                    program.display_type(*ty)
                ));
                write_expr(program, output, initializer, depth + 1);
            }
            TypedItem::Assign { symbol, value, .. } => {
                output.push_str(&format!("{indent}assign s{}\n", symbol.0));
                write_expr(program, output, value, depth + 1);
            }
            TypedItem::Expr(expression) => write_expr(program, output, expression, depth),
            TypedItem::Return(expression) => {
                output.push_str(&format!("{indent}return\n"));
                write_expr(program, output, expression, depth + 1);
            }
        }
    }
}

fn write_expr(program: &TypedProgram, output: &mut String, expression: &TypedExpr, depth: usize) {
    let indent = "  ".repeat(depth);
    let label = match &expression.kind {
        TypedExprKind::Integer(value) => format!("integer {value}"),
        TypedExprKind::Boolean(value) => format!("boolean {value}"),
        TypedExprKind::Unit => "unit".to_owned(),
        TypedExprKind::Atom(name) => format!("atom :{name}"),
        TypedExprKind::List { .. } => "list".to_owned(),
        TypedExprKind::Array(_) => "array".to_owned(),
        TypedExprKind::Map(_) => "map".to_owned(),
        TypedExprKind::Tuple(_) => "tuple".to_owned(),
        TypedExprKind::If { .. } => "if".to_owned(),
        TypedExprKind::Match { exhaustive, .. } => format!("match exhaustive={exhaustive}"),
        TypedExprKind::Ascription(_) => "ascription".to_owned(),
        TypedExprKind::Local(symbol) => format!("local s{}", symbol.0),
        TypedExprKind::Binary { operator, .. } => format!("binary {operator:?}"),
        TypedExprKind::Call {
            function,
            substitutions,
            ..
        } => format!(
            "call d{} [{}]",
            function.0,
            substitutions
                .iter()
                .map(|(from, to)| format!(
                    "{}={}",
                    program.display_type(*from),
                    program.display_type(*to)
                ))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TypedExprKind::UnionInject { member, .. } => {
            format!("inject {}", program.display_type(*member))
        }
    };
    output.push_str(&format!(
        "{indent}{label}: {}\n",
        program.display_type(expression.ty)
    ));
    match &expression.kind {
        TypedExprKind::Binary { left, right, .. } => {
            write_expr(program, output, left, depth + 1);
            write_expr(program, output, right, depth + 1);
        }
        TypedExprKind::List { elements, tail } => {
            for element in elements {
                write_expr(program, output, element, depth + 1);
            }
            if let Some(tail) = tail {
                write_expr(program, output, tail, depth + 1);
            }
        }
        TypedExprKind::Tuple(elements) => {
            for element in elements {
                write_expr(program, output, element, depth + 1);
            }
        }
        TypedExprKind::Array(elements) => {
            for element in elements {
                write_expr(program, output, element, depth + 1);
            }
        }
        TypedExprKind::Map(entries) => {
            for (key, value) in entries {
                write_expr(program, output, key, depth + 1);
                write_expr(program, output, value, depth + 1);
            }
        }
        TypedExprKind::If {
            condition,
            then_block,
            else_block,
        } => {
            write_expr(program, output, condition, depth + 1);
            write_items(program, output, &then_block.items, depth + 1);
            if let Some(block) = else_block {
                write_items(program, output, &block.items, depth + 1);
            }
        }
        TypedExprKind::Match { subject, arms, .. } => {
            write_expr(program, output, subject, depth + 1);
            for arm in arms {
                output.push_str(&format!(
                    "{}pattern {:?} reachable={} irrefutable={}\n",
                    "  ".repeat(depth + 1),
                    arm.pattern.kind,
                    arm.pattern.facts.reachable,
                    arm.pattern.facts.irrefutable
                ));
                write_items(program, output, &arm.body.items, depth + 2);
            }
        }
        TypedExprKind::Ascription(value) => {
            write_expr(program, output, value, depth + 1);
        }
        TypedExprKind::Call { arguments, .. } => {
            for argument in arguments {
                write_expr(program, output, argument, depth + 1);
            }
        }
        TypedExprKind::UnionInject { value, .. } => {
            write_expr(program, output, value, depth + 1);
        }
        _ => {}
    }
}
