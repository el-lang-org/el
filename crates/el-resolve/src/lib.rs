//! Declaration collection and stable source-name resolution.

use el_ast::{Node, Program, Value};
use el_span::{Diagnostic, Span};
use std::collections::BTreeMap;

macro_rules! id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(pub u32);
    };
}

id!(ModuleId);
id!(DeclId);
id!(SymbolId);
id!(ImplId);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Visibility {
    Public,
    Private,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypeSyntax {
    Primitive {
        name: String,
        span: Span,
    },
    Variable {
        name: String,
        span: Span,
    },
    SelfType {
        span: Span,
    },
    Projection {
        protocol: String,
        associated: String,
        argument: Box<TypeSyntax>,
        span: Span,
    },
    Named {
        declaration: DeclId,
        name: String,
        arguments: Vec<TypeSyntax>,
        span: Span,
    },
    Union {
        members: Vec<TypeSyntax>,
        span: Span,
    },
    Atom {
        name: String,
        span: Span,
    },
    List {
        item: Box<TypeSyntax>,
        span: Span,
    },
    Array {
        item: Box<TypeSyntax>,
        length: u64,
        span: Span,
    },
    Slice {
        item: Box<TypeSyntax>,
        span: Span,
    },
    Map {
        key: Box<TypeSyntax>,
        value: Box<TypeSyntax>,
        span: Span,
    },
    Tuple {
        elements: Vec<TypeSyntax>,
        span: Span,
    },
    Function {
        parameters: Vec<TypeSyntax>,
        result: Box<TypeSyntax>,
        span: Span,
    },
}

impl TypeSyntax {
    #[must_use]
    pub const fn span(&self) -> Span {
        match self {
            Self::Primitive { span, .. }
            | Self::Variable { span, .. }
            | Self::SelfType { span }
            | Self::Projection { span, .. }
            | Self::Named { span, .. }
            | Self::Union { span, .. }
            | Self::Atom { span, .. }
            | Self::List { span, .. }
            | Self::Array { span, .. }
            | Self::Slice { span, .. }
            | Self::Map { span, .. }
            | Self::Tuple { span, .. }
            | Self::Function { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Parameter {
    pub symbol: SymbolId,
    pub name: String,
    pub name_span: Span,
    pub ty: TypeSyntax,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Function {
    pub id: DeclId,
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub visibility: Visibility,
    pub module_name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: Option<TypeSyntax>,
    pub type_parameters: Vec<String>,
    pub constraints: Vec<Constraint>,
    pub body: Node,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Constraint {
    pub parameter: String,
    pub protocol: String,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Protocol {
    pub id: DeclId,
    pub name: String,
    pub span: Span,
    pub associated_types: Vec<String>,
    pub methods: Vec<String>,
    pub method_signatures: Vec<Node>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Implementation {
    pub id: ImplId,
    pub protocol: String,
    pub target: TypeSyntax,
    pub associated_types: Vec<(String, TypeSyntax)>,
    pub methods: Vec<String>,
    pub method_declarations: Vec<(String, DeclId)>,
    pub constraints: Vec<Constraint>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeAlias {
    pub id: DeclId,
    pub module_name: String,
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub parameters: Vec<String>,
    pub value: TypeSyntax,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructField {
    pub name: String,
    pub name_span: Span,
    pub ty: TypeSyntax,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Struct {
    pub id: DeclId,
    pub module_name: String,
    pub name: String,
    pub name_span: Span,
    pub span: Span,
    pub parameters: Vec<String>,
    pub derives: Vec<String>,
    pub fields: Vec<StructField>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedProgram {
    pub module_id: ModuleId,
    pub module_name: String,
    pub module_span: Span,
    pub aliases: Vec<TypeAlias>,
    pub structs: Vec<Struct>,
    pub protocols: Vec<Protocol>,
    pub implementations: Vec<Implementation>,
    pub functions: Vec<Function>,
}

impl ResolvedProgram {
    #[must_use]
    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|function| function.name == name)
    }

    #[must_use]
    pub fn debug_tree(&self) -> String {
        let mut output = format!("module m{} {}\n", self.module_id.0, self.module_name);
        for alias in &self.aliases {
            output.push_str(&format!("  alias d{} {}", alias.id.0, alias.name));
            if !alias.parameters.is_empty() {
                output.push_str(&format!(" <{}>", alias.parameters.join(", ")));
            }
            output.push('\n');
        }
        for structure in &self.structs {
            output.push_str(&format!("  struct d{} {}", structure.id.0, structure.name));
            if !structure.parameters.is_empty() {
                output.push_str(&format!(" <{}>", structure.parameters.join(", ")));
            }
            output.push('\n');
        }
        for protocol in &self.protocols {
            output.push_str(&format!(
                "  protocol d{} {}\n",
                protocol.id.0, protocol.name
            ));
            for associated in &protocol.associated_types {
                output.push_str(&format!("    associated {associated}\n"));
            }
            for method in &protocol.methods {
                output.push_str(&format!("    method {method}\n"));
            }
        }
        for implementation in &self.implementations {
            output.push_str(&format!(
                "  impl i{} {} for {}\n",
                implementation.id.0,
                implementation.protocol,
                type_name(&implementation.target)
            ));
        }
        for function in &self.functions {
            output.push_str(&format!(
                "  function d{} {} {:?}",
                function.id.0, function.name, function.visibility
            ));
            if !function.type_parameters.is_empty() {
                output.push_str(&format!(" <{}>", function.type_parameters.join(", ")));
            }
            output.push('\n');
            for parameter in &function.parameters {
                output.push_str(&format!(
                    "    parameter s{} {}: {}\n",
                    parameter.symbol.0,
                    parameter.name,
                    type_name(&parameter.ty)
                ));
            }
        }
        output
    }
}

/// Collects the single parsed module in deterministic source order.
pub fn resolve(program: &Program) -> Result<ResolvedProgram, Vec<Diagnostic>> {
    resolve_with_catalog(program, ModuleId(0), &BTreeMap::new())
}

/// Resolves a package of modules with one deterministic declaration identity space.
pub fn resolve_package(programs: &[Program]) -> Result<Vec<ResolvedProgram>, Vec<Diagnostic>> {
    let mut catalog = BTreeMap::new();
    let mut next = 0_u32;
    for program in programs {
        let module = &program.root.children[0];
        let Some(name_node) = module
            .children
            .iter()
            .find(|node| node.kind.as_str() == "type_path")
        else {
            continue;
        };
        let module_name = path_name(name_node);
        for node in &module.children {
            if node.kind.as_str() == "protocol_impl" {
                for method in node
                    .children
                    .iter()
                    .filter(|item| item.kind.as_str() == "function_decl")
                {
                    let Some(name) = child(method, "identifier").map(text) else {
                        continue;
                    };
                    catalog.insert(
                        format!("{module_name}.<impl@{}>.{name}", node.span.start()),
                        (DeclId(next), method.span, 0),
                    );
                    next += 1;
                }
                continue;
            }
            if !matches!(
                node.kind.as_str(),
                "type_alias" | "struct_decl" | "protocol_decl" | "function_decl"
            ) {
                continue;
            }
            let name_kind = if matches!(
                node.kind.as_str(),
                "type_alias" | "struct_decl" | "protocol_decl"
            ) {
                "type_name"
            } else {
                "identifier"
            };
            let Some(name) = child(node, name_kind).map(text) else {
                continue;
            };
            let arity = child(node, "type_params").map_or(0, |params| params.children.len());
            catalog.insert(
                format!("{module_name}.{name}"),
                (DeclId(next), node.span, arity),
            );
            next += 1;
        }
    }
    let mut diagnostics = Vec::new();
    let mut modules = Vec::new();
    let mut module_names = BTreeMap::new();
    for (index, program) in programs.iter().enumerate() {
        match resolve_with_catalog(program, ModuleId(index as u32), &catalog) {
            Ok(module) => {
                if let Some(previous) =
                    module_names.insert(module.module_name.clone(), module.module_span)
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "E2023",
                            module.module_span,
                            format!("duplicate module `{}`", module.module_name),
                        )
                        .with_label(previous, "first module here"),
                    );
                }
                modules.push(module);
            }
            Err(mut errors) => diagnostics.append(&mut errors),
        }
    }
    let package_aliases = modules
        .iter()
        .flat_map(|module| module.aliases.iter().cloned())
        .collect::<Vec<_>>();
    reject_alias_cycles(&package_aliases, &mut diagnostics);
    let package_structs = modules
        .iter()
        .flat_map(|module| module.structs.iter().cloned())
        .collect::<Vec<_>>();
    reject_infinite_struct_layouts(&package_structs, &package_aliases, &mut diagnostics);
    if diagnostics.is_empty() {
        Ok(modules)
    } else {
        Err(diagnostics)
    }
}

/// Resolves a qualified cross-module function and enforces its visibility.
pub fn resolve_package_function<'a>(
    modules: &'a [ResolvedProgram],
    from: &str,
    path: &str,
    span: Span,
) -> Result<&'a Function, Box<Diagnostic>> {
    let Some((module_name, function_name)) = path.rsplit_once('.') else {
        return Err(Box::new(Diagnostic::error(
            "E2024",
            span,
            format!("cross-module function `{path}` must be qualified"),
        )));
    };
    let module = modules
        .iter()
        .find(|module| module.module_name == module_name)
        .ok_or_else(|| {
            Box::new(Diagnostic::error(
                "E2025",
                span,
                format!("unknown module `{module_name}`"),
            ))
        })?;
    let function = module.function(function_name).ok_or_else(|| {
        Box::new(Diagnostic::error(
            "E2026",
            span,
            format!("unknown function `{path}`"),
        ))
    })?;
    if function.visibility == Visibility::Private && from != module_name {
        return Err(Box::new(Diagnostic::error(
            "E2027",
            span,
            format!("function `{path}` is private"),
        )));
    }
    Ok(function)
}

fn resolve_with_catalog(
    program: &Program,
    module_id: ModuleId,
    catalog: &BTreeMap<String, (DeclId, Span, usize)>,
) -> Result<ResolvedProgram, Vec<Diagnostic>> {
    let module = &program.root.children[0];
    let Some(name_node) = module
        .children
        .iter()
        .find(|node| node.kind.as_str() == "type_path")
    else {
        return Err(vec![Diagnostic::error(
            "E2000",
            module.span,
            "a module must have a name",
        )]);
    };
    let module_name = path_name(name_node);
    let mut diagnostics = Vec::new();
    let mut function_names = BTreeMap::<String, Span>::new();
    let mut type_names = BTreeMap::<String, (DeclId, Span, usize)>::new();
    let mut declaration_ids = BTreeMap::new();
    let mut next_declaration = 0_u32;
    for node in &module.children {
        if !matches!(
            node.kind.as_str(),
            "type_alias" | "struct_decl" | "protocol_decl" | "function_decl"
        ) {
            continue;
        }
        let declaration_name = if matches!(
            node.kind.as_str(),
            "type_alias" | "struct_decl" | "protocol_decl"
        ) {
            child(node, "type_name").map(text)
        } else {
            child(node, "identifier").map(text)
        };
        let id = declaration_name
            .as_ref()
            .and_then(|name| catalog.get(&format!("{module_name}.{name}")))
            .map_or(DeclId(next_declaration), |entry| entry.0);
        next_declaration += 1;
        declaration_ids.insert(node.span.start(), id);
        if matches!(node.kind.as_str(), "type_alias" | "struct_decl") {
            let name_node = child(node, "type_name").expect("parser validates alias names");
            let name = text(name_node);
            let arity = child(node, "type_params").map_or(0, |params| params.children.len());
            if let Some((_, previous, _)) =
                type_names.insert(name.clone(), (id, name_node.span, arity))
            {
                diagnostics.push(
                    Diagnostic::error("E2004", name_node.span, format!("duplicate type `{name}`"))
                        .with_label(previous, "first declared here"),
                );
            }
        }
    }
    for (qualified, entry) in catalog {
        if qualified.starts_with(&format!("{module_name}.")) {
            continue;
        }
        type_names.entry(qualified.clone()).or_insert(*entry);
    }

    let mut protocol_names = BTreeMap::<String, Span>::new();
    let mut protocols = Vec::new();
    for node in module
        .children
        .iter()
        .filter(|node| node.kind.as_str() == "protocol_decl")
    {
        let name_node = child(node, "type_name").expect("parser validates protocol names");
        let name = text(name_node);
        if is_core_protocol(&name) {
            diagnostics.push(Diagnostic::error(
                "E2014",
                name_node.span,
                format!("protocol `{name}` is already defined by the closed core prelude"),
            ));
            continue;
        }
        if let Some(previous) = protocol_names.insert(name.clone(), name_node.span) {
            diagnostics.push(
                Diagnostic::error(
                    "E2011",
                    name_node.span,
                    format!("duplicate protocol `{name}`"),
                )
                .with_label(previous, "first declared here"),
            );
            continue;
        }
        let associated_types = node
            .children
            .iter()
            .filter(|item| item.kind.as_str() == "assoc_type_decl")
            .filter_map(|item| child(item, "type_name").map(text))
            .collect::<Vec<_>>();
        let method_signatures = node
            .children
            .iter()
            .filter(|item| item.kind.as_str() == "protocol_signature")
            .cloned()
            .collect::<Vec<_>>();
        let methods = method_signatures
            .iter()
            .filter_map(|item| child(item, "identifier").map(text))
            .collect::<Vec<_>>();
        for (names, kind, code) in [
            (&associated_types, "associated type", "E2034"),
            (&methods, "protocol method", "E2035"),
        ] {
            let mut seen = BTreeMap::new();
            for name in names {
                if seen.insert(name, node.span).is_some() {
                    diagnostics.push(Diagnostic::error(
                        code,
                        node.span,
                        format!("duplicate {kind} `{name}`"),
                    ));
                }
            }
        }
        protocols.push(Protocol {
            id: declaration_ids[&node.span.start()],
            name,
            span: node.span,
            associated_types,
            methods,
            method_signatures,
        });
    }

    let mut aliases = Vec::new();
    for node in module
        .children
        .iter()
        .filter(|node| node.kind.as_str() == "type_alias")
    {
        let name_node = child(node, "type_name").expect("parser validates alias names");
        let name = text(name_node);
        if type_names
            .get(&name)
            .is_none_or(|(_, span, _)| *span != name_node.span)
        {
            continue;
        }
        let parameters = child(node, "type_params")
            .map(|params| params.children.iter().map(text).collect::<Vec<_>>())
            .unwrap_or_default();
        let duplicate_parameter = parameters
            .iter()
            .enumerate()
            .find_map(|(index, name)| parameters[..index].contains(name).then_some(name.clone()));
        if let Some(name) = duplicate_parameter {
            diagnostics.push(Diagnostic::error(
                "E2005",
                node.span,
                format!("duplicate type parameter `{name}`"),
            ));
            continue;
        }
        let value_node = node.children.last().expect("alias has a value");
        if let Some(value) = parse_type(value_node, &type_names, &module_name, &mut diagnostics) {
            aliases.push(TypeAlias {
                id: declaration_ids[&node.span.start()],
                module_name: module_name.clone(),
                name,
                name_span: name_node.span,
                span: node.span,
                parameters,
                value,
            });
        }
    }
    reject_alias_cycles(&aliases, &mut diagnostics);

    let mut structs = Vec::new();
    for (node_index, node) in module
        .children
        .iter()
        .enumerate()
        .filter(|(_, node)| node.kind.as_str() == "struct_decl")
    {
        let name_node = child(node, "type_name").expect("parser validates struct names");
        let name = text(name_node);
        if type_names
            .get(&name)
            .is_none_or(|(_, span, _)| *span != name_node.span)
        {
            continue;
        }
        let parameters = child(node, "type_params")
            .map(|params| params.children.iter().map(text).collect::<Vec<_>>())
            .unwrap_or_default();
        let derives = node
            .children
            .iter()
            .find(|attribute| attribute.kind.as_str() == "derive_attr")
            .or_else(|| {
                node_index
                    .checked_sub(1)
                    .and_then(|index| module.children.get(index))
                    .filter(|attribute| attribute.kind.as_str() == "derive_attr")
            })
            .map(|attribute| {
                attribute
                    .children
                    .iter()
                    .filter(|path| path.kind.as_str() == "type_path")
                    .map(path_name)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let mut seen_derives = BTreeMap::new();
        for derive in &derives {
            if !matches!(derive.as_str(), "Eq" | "Ord" | "Show" | "Hash") {
                diagnostics.push(Diagnostic::error(
                    "E2028",
                    node.span,
                    format!("`@derive` does not support protocol `{derive}`"),
                ));
            } else if seen_derives.insert(derive.clone(), node.span).is_some() {
                diagnostics.push(Diagnostic::error(
                    "E2029",
                    node.span,
                    format!("duplicate derived protocol `{derive}`"),
                ));
            }
        }
        let mut field_names = BTreeMap::new();
        let mut fields = Vec::new();
        for field in node
            .children
            .iter()
            .filter(|child| child.kind.as_str() == "field_decl")
        {
            let field_name_node = child(field, "identifier").expect("field name");
            let field_name = text(field_name_node);
            if let Some(previous) = field_names.insert(field_name.clone(), field_name_node.span) {
                diagnostics.push(
                    Diagnostic::error(
                        "E2009",
                        field_name_node.span,
                        format!("duplicate field `{field_name}`"),
                    )
                    .with_label(previous, "first declared here"),
                );
                continue;
            }
            if let Some(ty) = field
                .children
                .get(1)
                .and_then(|ty| parse_type(ty, &type_names, &module_name, &mut diagnostics))
            {
                fields.push(StructField {
                    name: field_name,
                    name_span: field_name_node.span,
                    ty,
                });
            }
        }
        structs.push(Struct {
            id: declaration_ids[&node.span.start()],
            module_name: module_name.clone(),
            name,
            name_span: name_node.span,
            span: node.span,
            parameters,
            derives,
            fields,
        });
    }
    reject_infinite_struct_layouts(&structs, &aliases, &mut diagnostics);

    let mut functions = Vec::new();
    let mut next_symbol = 0_u32;

    for node in module
        .children
        .iter()
        .filter(|node| node.kind.as_str() == "function_decl")
    {
        let name_node = child(node, "identifier").expect("parser validates function names");
        let name = text(name_node);
        if let Some(previous) = function_names.insert(name.clone(), name_node.span) {
            diagnostics.push(
                Diagnostic::error(
                    "E2001",
                    name_node.span,
                    format!("duplicate function `{name}`"),
                )
                .with_label(previous, "first declared here"),
            );
            continue;
        }
        let mut parameters = Vec::new();
        let mut parameter_names = BTreeMap::<String, Span>::new();
        for parameter in node
            .children
            .iter()
            .filter(|node| node.kind.as_str() == "parameter")
        {
            let parameter_name_node = child(parameter, "identifier").expect("parameter name");
            let parameter_name = text(parameter_name_node);
            if let Some(previous) =
                parameter_names.insert(parameter_name.clone(), parameter_name_node.span)
            {
                diagnostics.push(
                    Diagnostic::error(
                        "E2002",
                        parameter_name_node.span,
                        format!("duplicate parameter `{parameter_name}`"),
                    )
                    .with_label(previous, "first declared here"),
                );
                continue;
            }
            let Some(type_node) = parameter.children.get(1) else {
                continue;
            };
            if let Some(ty) = parse_type(type_node, &type_names, &module_name, &mut diagnostics) {
                parameters.push(Parameter {
                    symbol: SymbolId(next_symbol),
                    name: parameter_name,
                    name_span: parameter_name_node.span,
                    ty,
                });
                next_symbol += 1;
            }
        }
        let return_type = node
            .children
            .iter()
            .find(|node| node.kind.as_str() == "return_type")
            .and_then(|node| node.children.first())
            .and_then(|node| parse_type(node, &type_names, &module_name, &mut diagnostics));
        let mut type_parameters = Vec::new();
        for parameter in &parameters {
            collect_type_parameter(&parameter.ty, &mut type_parameters);
        }
        if let Some(return_type) = &return_type {
            collect_type_parameter(return_type, &mut type_parameters);
        }
        let constraints: Vec<Constraint> = node
            .children
            .iter()
            .find(|child| child.kind.as_str() == "when_clause")
            .map(|when| {
                when.children
                    .iter()
                    .filter(|child| child.kind.as_str() == "constraint")
                    .filter_map(|constraint| {
                        let parameter = constraint.children.first().map(text)?;
                        let protocol = constraint.children.get(1).map(path_name)?;
                        if !type_parameters.contains(&parameter) {
                            diagnostics.push(Diagnostic::error(
                                "E2012",
                                constraint.span,
                                format!(
                                    "constraint references unknown type parameter `{parameter}`"
                                ),
                            ));
                            return None;
                        }
                        let local_protocol = protocol
                            .strip_prefix(&format!("{module_name}."))
                            .unwrap_or(&protocol);
                        if !is_core_protocol(local_protocol)
                            && !protocol_names.contains_key(local_protocol)
                        {
                            diagnostics.push(Diagnostic::error(
                                "E2013",
                                constraint.span,
                                format!("unknown protocol `{protocol}`"),
                            ));
                            return None;
                        }
                        Some(Constraint {
                            parameter,
                            protocol: local_protocol.to_owned(),
                            span: constraint.span,
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let visibility = node
            .children
            .iter()
            .find(|node| node.kind.as_str() == "visibility")
            .and_then(|node| node.value.as_ref())
            .map_or(Visibility::Public, |value| match value {
                Value::Text(value) if value == "private" => Visibility::Private,
                _ => Visibility::Public,
            });
        let body = child(node, "block")
            .expect("function declaration has a body")
            .clone();
        functions.push(Function {
            id: declaration_ids[&node.span.start()],
            name,
            name_span: name_node.span,
            span: node.span,
            visibility,
            module_name: module_name.clone(),
            parameters,
            return_type,
            type_parameters,
            constraints,
            body,
        });
    }

    let mut implementations = Vec::new();
    let protocol_metadata = protocols
        .iter()
        .map(|protocol| (protocol.name.clone(), protocol))
        .collect::<BTreeMap<_, _>>();
    let mut impl_heads = Vec::<(String, TypeSyntax, Span)>::new();
    for (index, node) in module
        .children
        .iter()
        .filter(|node| node.kind.as_str() == "protocol_impl")
        .enumerate()
    {
        let Some(protocol_path) = node
            .children
            .iter()
            .find(|child| child.kind.as_str() == "type_path")
        else {
            continue;
        };
        let written_protocol = path_name(protocol_path);
        let protocol_name = written_protocol
            .strip_prefix(&format!("{module_name}."))
            .unwrap_or(&written_protocol)
            .to_owned();
        if !is_core_protocol(&protocol_name) && !protocol_metadata.contains_key(&protocol_name) {
            diagnostics.push(Diagnostic::error(
                "E2015",
                protocol_path.span,
                format!("unknown protocol `{written_protocol}`"),
            ));
            continue;
        }
        let Some(target_node) = node
            .children
            .iter()
            .skip_while(|child| child.span != protocol_path.span)
            .skip(1)
            .find(|child| is_type_syntax_node(child))
        else {
            continue;
        };
        let Some(target) = parse_type(target_node, &type_names, &module_name, &mut diagnostics)
        else {
            continue;
        };
        let protocol_is_local = protocol_metadata.contains_key(&protocol_name);
        let target_is_local = matches!(&target, TypeSyntax::Named { declaration, .. }
            if structs.iter().any(|structure| structure.id == *declaration));
        if !protocol_is_local && !target_is_local {
            diagnostics.push(Diagnostic::error(
                "E2032",
                node.span,
                "orphan implementation: this package owns neither the protocol nor the target type",
            ));
            continue;
        }
        if matches!(target, TypeSyntax::Union { .. })
            || matches!(&target, TypeSyntax::Named { declaration, .. } if aliases.iter().any(|alias| alias.id == *declaration))
        {
            diagnostics.push(Diagnostic::error(
                "E2030",
                target.span(),
                "transparent aliases and structural unions cannot be implementation targets",
            ));
            continue;
        }
        if let Some((_, _, previous)) = impl_heads.iter().find(|(protocol, candidate, _)| {
            protocol == &protocol_name && implementation_heads_overlap(candidate, &target)
        }) {
            diagnostics.push(
                Diagnostic::error("E2016", node.span, "overlapping implementation head")
                    .with_label(*previous, "first implemented here")
                    .with_note(
                        "positive protocol constraints do not disambiguate implementation heads",
                    ),
            );
            continue;
        }
        impl_heads.push((protocol_name.clone(), target.clone(), node.span));
        let mut target_parameters = Vec::new();
        collect_type_parameter(&target, &mut target_parameters);
        let constraints: Vec<Constraint> = node
            .children
            .iter()
            .find(|child| child.kind.as_str() == "when_clause")
            .map(|when| {
                when.children
                    .iter()
                    .filter(|child| child.kind.as_str() == "constraint")
                    .filter_map(|constraint| {
                        let parameter = constraint.children.first().map(text)?;
                        let written = constraint.children.get(1).map(path_name)?;
                        let protocol = written
                            .strip_prefix(&format!("{module_name}."))
                            .unwrap_or(&written)
                            .to_owned();
                        if !target_parameters.contains(&parameter) {
                            diagnostics.push(Diagnostic::error(
                                "E2031",
                                constraint.span,
                                format!("implementation constraint references unknown type parameter `{parameter}`"),
                            ));
                            return None;
                        }
                        if !is_core_protocol(&protocol) && !protocol_names.contains_key(&protocol) {
                            diagnostics.push(Diagnostic::error(
                                "E2013",
                                constraint.span,
                                format!("unknown protocol `{written}`"),
                            ));
                            return None;
                        }
                        Some(Constraint { parameter, protocol, span: constraint.span })
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut associated_types = Vec::new();
        let mut associated_names = BTreeMap::new();
        let mut methods = Vec::new();
        let mut method_declarations = Vec::new();
        let mut method_names = BTreeMap::new();
        for item in &node.children {
            match item.kind.as_str() {
                "assoc_type_def" => {
                    let name_node = child(item, "type_name").expect("associated type name");
                    let name = text(name_node);
                    if associated_names
                        .insert(name.clone(), name_node.span)
                        .is_some()
                    {
                        diagnostics.push(Diagnostic::error(
                            "E2017",
                            name_node.span,
                            format!("duplicate associated type `{name}`"),
                        ));
                    } else if let Some(value) = item.children.last().and_then(|value| {
                        parse_type(value, &type_names, &module_name, &mut diagnostics)
                    }) {
                        associated_types.push((name, value));
                    }
                }
                "function_decl" => {
                    let name_node = child(item, "identifier").expect("implementation method name");
                    let name = text(name_node);
                    if method_names.insert(name.clone(), name_node.span).is_some() {
                        diagnostics.push(Diagnostic::error(
                            "E2018",
                            name_node.span,
                            format!("duplicate implementation method `{name}`"),
                        ));
                    } else {
                        methods.push(name);
                    }
                }
                _ => {}
            }
        }
        for method in node
            .children
            .iter()
            .filter(|item| item.kind.as_str() == "function_decl")
        {
            let name_node = child(method, "identifier").expect("implementation method name");
            let protocol_method_name = text(name_node);
            let catalog_name = format!(
                "{module_name}.<impl@{}>.{protocol_method_name}",
                node.span.start()
            );
            let id = catalog.get(&catalog_name).map_or_else(
                || {
                    let id = DeclId(next_declaration);
                    next_declaration += 1;
                    id
                },
                |entry| entry.0,
            );
            let mut parameters = Vec::new();
            let mut parameter_names = BTreeMap::<String, Span>::new();
            for parameter in method
                .children
                .iter()
                .filter(|candidate| candidate.kind.as_str() == "parameter")
            {
                let parameter_name_node =
                    child(parameter, "identifier").expect("implementation parameter name");
                let parameter_name = text(parameter_name_node);
                if let Some(previous) =
                    parameter_names.insert(parameter_name.clone(), parameter_name_node.span)
                {
                    diagnostics.push(
                        Diagnostic::error(
                            "E2002",
                            parameter_name_node.span,
                            format!("duplicate parameter `{parameter_name}`"),
                        )
                        .with_label(previous, "first declared here"),
                    );
                    continue;
                }
                let Some(type_node) = parameter.children.get(1) else {
                    continue;
                };
                if let Some(ty) = parse_type(type_node, &type_names, &module_name, &mut diagnostics)
                {
                    parameters.push(Parameter {
                        symbol: SymbolId(next_symbol),
                        name: parameter_name,
                        name_span: parameter_name_node.span,
                        ty,
                    });
                    next_symbol += 1;
                }
            }
            let return_type = method
                .children
                .iter()
                .find(|candidate| candidate.kind.as_str() == "return_type")
                .and_then(|result| result.children.first())
                .and_then(|result| parse_type(result, &type_names, &module_name, &mut diagnostics));
            let body = child(method, "block")
                .expect("implementation method has a body")
                .clone();
            functions.push(Function {
                id,
                name: format!("<impl{index}>.{protocol_method_name}"),
                name_span: name_node.span,
                span: method.span,
                visibility: Visibility::Private,
                module_name: module_name.clone(),
                parameters,
                return_type,
                type_parameters: target_parameters.clone(),
                constraints: constraints.clone(),
                body,
            });
            method_declarations.push((protocol_method_name, id));
        }
        if let Some(protocol) = protocol_metadata.get(&protocol_name) {
            for required in &protocol.associated_types {
                if !associated_names.contains_key(required) {
                    diagnostics.push(Diagnostic::error(
                        "E2019",
                        node.span,
                        format!("implementation is missing associated type `{required}`"),
                    ));
                }
            }
            for required in &protocol.methods {
                if !method_names.contains_key(required) {
                    diagnostics.push(Diagnostic::error(
                        "E2020",
                        node.span,
                        format!("implementation is missing method `{required}`"),
                    ));
                }
            }
            for provided in associated_names.keys() {
                if !protocol.associated_types.contains(provided) {
                    diagnostics.push(Diagnostic::error(
                        "E2021",
                        node.span,
                        format!("unknown associated type `{provided}`"),
                    ));
                }
            }
            for provided in method_names.keys() {
                if !protocol.methods.contains(provided) {
                    diagnostics.push(Diagnostic::error(
                        "E2022",
                        node.span,
                        format!("unknown implementation method `{provided}`"),
                    ));
                }
            }
            let mut replacements = associated_types
                .iter()
                .map(|(name, ty)| (name.clone(), render_type_syntax(ty)))
                .collect::<BTreeMap<_, _>>();
            replacements.insert("Self".to_owned(), render_type_syntax(&target));
            for required in &protocol.method_signatures {
                let Some(name_node) = child(required, "identifier") else {
                    continue;
                };
                let name = text(name_node);
                let implementation_method = node.children.iter().find(|candidate| {
                    candidate.kind.as_str() == "function_decl"
                        && child(candidate, "identifier")
                            .is_some_and(|candidate_name| text(candidate_name) == name)
                });
                if let Some(implementation_method) = implementation_method
                    && method_signature_key(required, &replacements)
                        != method_signature_key(implementation_method, &replacements)
                {
                    diagnostics.push(Diagnostic::error(
                        "E2036",
                        implementation_method.span,
                        format!("implementation method `{name}` does not exactly match the substituted protocol signature"),
                    ));
                }
            }
        }
        if !protocol_metadata.contains_key(&protocol_name)
            && let Some((required_associated, required_methods)) =
                core_protocol_shape(&protocol_name)
        {
            for required in required_associated {
                if !associated_names.contains_key(*required) {
                    diagnostics.push(Diagnostic::error(
                        "E2019",
                        node.span,
                        format!("implementation is missing associated type `{required}`"),
                    ));
                }
            }
            for required in required_methods {
                if !method_names.contains_key(*required) {
                    diagnostics.push(Diagnostic::error(
                        "E2020",
                        node.span,
                        format!("implementation is missing method `{required}`"),
                    ));
                }
            }
            for provided in associated_names.keys() {
                if !required_associated.contains(&provided.as_str()) {
                    diagnostics.push(Diagnostic::error(
                        "E2021",
                        node.span,
                        format!("unknown associated type `{provided}`"),
                    ));
                }
            }
            for provided in method_names.keys() {
                if !required_methods.contains(&provided.as_str()) {
                    diagnostics.push(Diagnostic::error(
                        "E2022",
                        node.span,
                        format!("unknown implementation method `{provided}`"),
                    ));
                }
            }
        }
        implementations.push(Implementation {
            id: ImplId(index as u32),
            protocol: protocol_name,
            target,
            associated_types,
            methods,
            method_declarations,
            constraints,
            span: node.span,
        });
    }

    for structure in &structs {
        for protocol in &structure.derives {
            if let Some(implementation) = implementations.iter().find(|implementation| {
                implementation.protocol == *protocol
                    && matches!(&implementation.target, TypeSyntax::Named { declaration, .. } if *declaration == structure.id)
            }) {
                diagnostics.push(
                    Diagnostic::error(
                        "E2033",
                        structure.span,
                        format!("derived `{protocol}` conflicts with an explicit implementation"),
                    )
                    .with_label(implementation.span, "explicit implementation declared here"),
                );
            }
        }
    }

    if diagnostics.is_empty() {
        Ok(ResolvedProgram {
            module_id,
            module_name,
            module_span: module.span,
            aliases,
            structs,
            protocols,
            implementations,
            functions,
        })
    } else {
        Err(diagnostics)
    }
}

fn is_core_protocol(name: &str) -> bool {
    matches!(
        name,
        "Eq" | "Ord" | "Show" | "Hash" | "Iterable" | "Reader" | "Writer" | "Concat"
    )
}

fn core_protocol_shape(name: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    Some(match name {
        "Eq" => (&[], &["eq"]),
        "Ord" => (&[], &["compare"]),
        "Show" => (&[], &["show"]),
        "Hash" => (&[], &["hash"]),
        "Iterable" => (&["Item", "Cursor"], &["iter", "next"]),
        "Reader" => (&["Error"], &["read"]),
        "Writer" => (&["Error"], &["write", "flush"]),
        "Concat" => (&[], &["concat"]),
        _ => return None,
    })
}

fn is_type_syntax_node(node: &Node) -> bool {
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

fn parse_type(
    node: &Node,
    aliases: &BTreeMap<String, (DeclId, Span, usize)>,
    module_name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<TypeSyntax> {
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
        "list_or_array_type" => {
            let item = Box::new(parse_type(
                &node.children[0],
                aliases,
                module_name,
                diagnostics,
            )?);
            if let Some(length) = node.children.get(1) {
                let length = match length.value.as_ref() {
                    Some(Value::Text(text)) => text.replace('_', "").parse().ok(),
                    _ => None,
                }?;
                Some(TypeSyntax::Array {
                    item,
                    length,
                    span: node.span,
                })
            } else {
                Some(TypeSyntax::List {
                    item,
                    span: node.span,
                })
            }
        }
        "tuple_type" => Some(TypeSyntax::Tuple {
            elements: node
                .children
                .iter()
                .filter_map(|node| parse_type(node, aliases, module_name, diagnostics))
                .collect(),
            span: node.span,
        }),
        "function_type" => {
            let (result, parameters) = node.children.split_last()?;
            Some(TypeSyntax::Function {
                parameters: parameters
                    .iter()
                    .filter_map(|node| parse_type(node, aliases, module_name, diagnostics))
                    .collect(),
                result: Box::new(parse_type(result, aliases, module_name, diagnostics)?),
                span: node.span,
            })
        }
        "named_type" => {
            let path = node.children.first()?;
            let written = path_name(path);
            if written == "Self" && node.children.len() == 1 {
                return Some(TypeSyntax::SelfType { span: node.span });
            }
            if written == "Slice" {
                if node.children.len() != 2 {
                    diagnostics.push(Diagnostic::error(
                        "E2007",
                        node.span,
                        format!(
                            "type `Slice` expects 1 argument but received {}",
                            node.children.len() - 1
                        ),
                    ));
                    return None;
                }
                return Some(TypeSyntax::Slice {
                    item: Box::new(parse_type(
                        &node.children[1],
                        aliases,
                        module_name,
                        diagnostics,
                    )?),
                    span: node.span,
                });
            }
            if written == "Map" {
                if node.children.len() != 3 {
                    diagnostics.push(Diagnostic::error(
                        "E2007",
                        node.span,
                        format!(
                            "type `Map` expects 2 arguments but received {}",
                            node.children.len() - 1
                        ),
                    ));
                    return None;
                }
                return Some(TypeSyntax::Map {
                    key: Box::new(parse_type(
                        &node.children[1],
                        aliases,
                        module_name,
                        diagnostics,
                    )?),
                    value: Box::new(parse_type(
                        &node.children[2],
                        aliases,
                        module_name,
                        diagnostics,
                    )?),
                    span: node.span,
                });
            }
            if matches!(
                written.as_str(),
                "Buffer" | "String.Utf8Error" | "String.CodepointView" | "String.GraphemeView"
            ) {
                if node.children.len() != 1 {
                    diagnostics.push(Diagnostic::error(
                        "E2007",
                        node.span,
                        format!("type `{written}` expects 0 arguments"),
                    ));
                    return None;
                }
                return Some(TypeSyntax::Primitive {
                    name: written,
                    span: node.span,
                });
            }
            let local_name = written
                .strip_prefix(&format!("{module_name}."))
                .unwrap_or(&written);
            let Some((declaration, _, arity)) = aliases.get(local_name) else {
                if let Some((protocol, associated)) = written.rsplit_once('.')
                    && node.children.len() == 2
                {
                    return Some(TypeSyntax::Projection {
                        protocol: protocol.to_owned(),
                        associated: associated.to_owned(),
                        argument: Box::new(parse_type(
                            &node.children[1],
                            aliases,
                            module_name,
                            diagnostics,
                        )?),
                        span: node.span,
                    });
                }
                diagnostics.push(Diagnostic::error(
                    "E2006",
                    path.span,
                    format!("unknown type `{written}`"),
                ));
                return None;
            };
            let arguments = node.children[1..]
                .iter()
                .filter_map(|node| parse_type(node, aliases, module_name, diagnostics))
                .collect::<Vec<_>>();
            if arguments.len() != *arity {
                diagnostics.push(Diagnostic::error(
                    "E2007",
                    node.span,
                    format!(
                        "type `{local_name}` expects {arity} arguments but received {}",
                        arguments.len()
                    ),
                ));
                return None;
            }
            Some(TypeSyntax::Named {
                declaration: *declaration,
                name: local_name.to_owned(),
                arguments,
                span: node.span,
            })
        }
        "union_type" => Some(TypeSyntax::Union {
            members: node
                .children
                .iter()
                .filter_map(|node| parse_type(node, aliases, module_name, diagnostics))
                .collect(),
            span: node.span,
        }),
        _ => {
            diagnostics.push(Diagnostic::error(
                "E2003",
                node.span,
                "this type form is not implemented in the Milestone 2 initial slice",
            ));
            None
        }
    }
}

fn collect_type_parameter(ty: &TypeSyntax, parameters: &mut Vec<String>) {
    match ty {
        TypeSyntax::Variable { name, .. } if !parameters.contains(name) => {
            parameters.push(name.clone());
        }
        TypeSyntax::Projection { argument, .. } => {
            collect_type_parameter(argument, parameters);
        }
        TypeSyntax::Named { arguments, .. } => {
            for argument in arguments {
                collect_type_parameter(argument, parameters);
            }
        }
        TypeSyntax::Union { members, .. } => {
            for member in members {
                collect_type_parameter(member, parameters);
            }
        }
        TypeSyntax::List { item, .. } | TypeSyntax::Slice { item, .. } => {
            collect_type_parameter(item, parameters)
        }
        TypeSyntax::Tuple { elements, .. } => {
            for element in elements {
                collect_type_parameter(element, parameters);
            }
        }
        TypeSyntax::Function {
            parameters: inputs,
            result,
            ..
        } => {
            for input in inputs {
                collect_type_parameter(input, parameters);
            }
            collect_type_parameter(result, parameters);
        }
        _ => {}
    }
}

fn reject_alias_cycles(aliases: &[TypeAlias], diagnostics: &mut Vec<Diagnostic>) {
    let by_id = aliases
        .iter()
        .map(|alias| (alias.id, alias))
        .collect::<BTreeMap<_, _>>();
    let mut complete = BTreeMap::new();
    for alias in aliases {
        let mut path = Vec::new();
        if visit_alias(alias.id, &by_id, &mut complete, &mut path) {
            let names = path
                .iter()
                .filter_map(|id| by_id.get(id))
                .map(|alias| alias.name.as_str())
                .collect::<Vec<_>>();
            diagnostics.push(
                Diagnostic::error(
                    "E2008",
                    alias.name_span,
                    format!("transparent alias cycle: {}", names.join(" -> ")),
                )
                .with_note("transparent aliases must be acyclic even beneath type constructors"),
            );
            return;
        }
    }
}

fn visit_alias(
    id: DeclId,
    aliases: &BTreeMap<DeclId, &TypeAlias>,
    complete: &mut BTreeMap<DeclId, bool>,
    path: &mut Vec<DeclId>,
) -> bool {
    if complete.contains_key(&id) {
        return false;
    }
    if let Some(position) = path.iter().position(|candidate| *candidate == id) {
        path.drain(..position);
        path.push(id);
        return true;
    }
    path.push(id);
    let cyclic = aliases.get(&id).is_some_and(|alias| {
        let mut references = Vec::new();
        collect_alias_references(&alias.value, &mut references);
        references
            .into_iter()
            .any(|target| visit_alias(target, aliases, complete, path))
    });
    if cyclic {
        return true;
    }
    path.pop();
    complete.insert(id, true);
    false
}

fn collect_alias_references(ty: &TypeSyntax, output: &mut Vec<DeclId>) {
    match ty {
        TypeSyntax::Named {
            declaration,
            arguments,
            ..
        } => {
            output.push(*declaration);
            for argument in arguments {
                collect_alias_references(argument, output);
            }
        }
        TypeSyntax::Union { members, .. } => {
            for member in members {
                collect_alias_references(member, output);
            }
        }
        TypeSyntax::List { item, .. } | TypeSyntax::Slice { item, .. } => {
            collect_alias_references(item, output)
        }
        TypeSyntax::Tuple { elements, .. } => {
            for element in elements {
                collect_alias_references(element, output);
            }
        }
        TypeSyntax::Function {
            parameters, result, ..
        } => {
            for parameter in parameters {
                collect_alias_references(parameter, output);
            }
            collect_alias_references(result, output);
        }
        _ => {}
    }
}

fn reject_infinite_struct_layouts(
    structs: &[Struct],
    aliases: &[TypeAlias],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let struct_ids = structs.iter().map(|item| item.id).collect::<Vec<_>>();
    let alias_by_id = aliases
        .iter()
        .map(|alias| (alias.id, alias))
        .collect::<BTreeMap<_, _>>();
    let mut edges = BTreeMap::<DeclId, Vec<DeclId>>::new();
    for structure in structs {
        let mut targets = Vec::new();
        for field in &structure.fields {
            collect_inline_structs(&field.ty, &struct_ids, &alias_by_id, &mut targets);
        }
        targets.sort_unstable();
        targets.dedup();
        edges.insert(structure.id, targets);
    }
    for structure in structs {
        let mut path = Vec::new();
        if find_struct_cycle(structure.id, structure.id, &edges, &mut path) {
            let names = path
                .iter()
                .filter_map(|id| structs.iter().find(|item| item.id == *id))
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>();
            diagnostics.push(
                Diagnostic::error(
                    "E2010",
                    structure.name_span,
                    format!("infinite inline struct layout: {}", names.join(" -> ")),
                )
                .with_note("every recursive containment cycle must cross a built-in managed-indirection boundary"),
            );
            return;
        }
    }
}

fn collect_inline_structs(
    ty: &TypeSyntax,
    structs: &[DeclId],
    aliases: &BTreeMap<DeclId, &TypeAlias>,
    output: &mut Vec<DeclId>,
) {
    match ty {
        TypeSyntax::Named {
            declaration,
            arguments,
            ..
        } if structs.contains(declaration) => {
            output.push(*declaration);
            for argument in arguments {
                collect_inline_structs(argument, structs, aliases, output);
            }
        }
        TypeSyntax::Named { declaration, .. } if aliases.contains_key(declaration) => {
            collect_inline_structs(&aliases[declaration].value, structs, aliases, output);
        }
        TypeSyntax::Tuple { elements, .. }
        | TypeSyntax::Union {
            members: elements, ..
        } => {
            for element in elements {
                collect_inline_structs(element, structs, aliases, output);
            }
        }
        TypeSyntax::List { .. } | TypeSyntax::Slice { .. } | TypeSyntax::Function { .. } => {}
        _ => {}
    }
}

fn find_struct_cycle(
    start: DeclId,
    current: DeclId,
    edges: &BTreeMap<DeclId, Vec<DeclId>>,
    path: &mut Vec<DeclId>,
) -> bool {
    path.push(current);
    for target in edges.get(&current).into_iter().flatten() {
        if *target == start {
            path.push(start);
            return true;
        }
        if !path.contains(target) && find_struct_cycle(start, *target, edges, path) {
            return true;
        }
    }
    path.pop();
    false
}

fn child<'a>(node: &'a Node, kind: &str) -> Option<&'a Node> {
    node.children.iter().find(|node| node.kind.as_str() == kind)
}

fn path_name(node: &Node) -> String {
    node.children.iter().map(text).collect::<Vec<_>>().join(".")
}

fn text(node: &Node) -> String {
    match node.value.as_ref() {
        Some(Value::Text(value)) => value.clone(),
        _ => String::new(),
    }
}

fn type_name(ty: &TypeSyntax) -> &str {
    match ty {
        TypeSyntax::Primitive { name, .. }
        | TypeSyntax::Variable { name, .. }
        | TypeSyntax::Named { name, .. } => name,
        TypeSyntax::SelfType { .. } => "Self",
        TypeSyntax::Projection { associated, .. } => associated,
        TypeSyntax::Union { .. } => "union",
        TypeSyntax::Atom { name, .. } => name,
        TypeSyntax::List { .. } => "list",
        TypeSyntax::Array { .. } => "array",
        TypeSyntax::Slice { .. } => "slice",
        TypeSyntax::Map { .. } => "map",
        TypeSyntax::Tuple { .. } => "tuple",
        TypeSyntax::Function { .. } => "function",
    }
}

fn method_signature_key(node: &Node, replacements: &BTreeMap<String, String>) -> String {
    let parameters = node
        .children
        .iter()
        .filter(|child| child.kind.as_str() == "parameter")
        .filter_map(|parameter| parameter.children.get(1))
        .map(|ty| render_type_node(ty, replacements))
        .collect::<Vec<_>>();
    let result = node
        .children
        .iter()
        .find(|child| child.kind.as_str() == "return_type")
        .and_then(|result| result.children.first())
        .map_or_else(
            || "unit".to_owned(),
            |ty| render_type_node(ty, replacements),
        );
    format!("({})->{result}", parameters.join(","))
}

fn render_type_node(node: &Node, replacements: &BTreeMap<String, String>) -> String {
    match node.kind.as_str() {
        "primitive_type" | "type_variable" => text(node),
        "atom" => match node.value.as_ref() {
            Some(Value::Atom { name, .. }) => format!(":{name}"),
            _ => String::new(),
        },
        "named_type" => {
            let name = node.children.first().map(path_name).unwrap_or_default();
            if node.children.len() == 1
                && let Some(replacement) = replacements.get(&name)
            {
                return replacement.clone();
            }
            let arguments = node.children[1..]
                .iter()
                .map(|argument| render_type_node(argument, replacements))
                .collect::<Vec<_>>();
            if arguments.is_empty() {
                name
            } else {
                format!("{name}({})", arguments.join(","))
            }
        }
        "list_or_array_type" if node.children.len() == 1 => {
            format!("[{}]", render_type_node(&node.children[0], replacements))
        }
        "list_or_array_type" => format!(
            "[{};{}]",
            render_type_node(&node.children[0], replacements),
            text(&node.children[1]).replace('_', "")
        ),
        "tuple_type" => format!(
            "{{{}}}",
            node.children
                .iter()
                .map(|child| render_type_node(child, replacements))
                .collect::<Vec<_>>()
                .join(",")
        ),
        "function_type" => {
            let Some((result, parameters)) = node.children.split_last() else {
                return String::new();
            };
            format!(
                "({})->{}",
                parameters
                    .iter()
                    .map(|child| render_type_node(child, replacements))
                    .collect::<Vec<_>>()
                    .join(","),
                render_type_node(result, replacements)
            )
        }
        "union_type" => node
            .children
            .iter()
            .map(|child| render_type_node(child, replacements))
            .collect::<Vec<_>>()
            .join("|"),
        _ => String::new(),
    }
}

fn render_type_syntax(ty: &TypeSyntax) -> String {
    match ty {
        TypeSyntax::Primitive { name, .. } | TypeSyntax::Variable { name, .. } => name.clone(),
        TypeSyntax::SelfType { .. } => "Self".to_owned(),
        TypeSyntax::Projection {
            protocol,
            associated,
            argument,
            ..
        } => format!("{protocol}.{associated}({})", render_type_syntax(argument)),
        TypeSyntax::Named {
            name, arguments, ..
        } if arguments.is_empty() => name.clone(),
        TypeSyntax::Named {
            name, arguments, ..
        } => format!(
            "{name}({})",
            arguments
                .iter()
                .map(render_type_syntax)
                .collect::<Vec<_>>()
                .join(",")
        ),
        TypeSyntax::Union { members, .. } => members
            .iter()
            .map(render_type_syntax)
            .collect::<Vec<_>>()
            .join("|"),
        TypeSyntax::Atom { name, .. } => format!(":{name}"),
        TypeSyntax::List { item, .. } => format!("[{}]", render_type_syntax(item)),
        TypeSyntax::Array { item, length, .. } => {
            format!("[{};{length}]", render_type_syntax(item))
        }
        TypeSyntax::Slice { item, .. } => format!("Slice({})", render_type_syntax(item)),
        TypeSyntax::Map { key, value, .. } => format!(
            "Map({},{})",
            render_type_syntax(key),
            render_type_syntax(value)
        ),
        TypeSyntax::Tuple { elements, .. } => format!(
            "{{{}}}",
            elements
                .iter()
                .map(render_type_syntax)
                .collect::<Vec<_>>()
                .join(",")
        ),
        TypeSyntax::Function {
            parameters, result, ..
        } => format!(
            "({})->{}",
            parameters
                .iter()
                .map(render_type_syntax)
                .collect::<Vec<_>>()
                .join(","),
            render_type_syntax(result)
        ),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ImplementationHead {
    Variable(String),
    Constructor(String, Vec<ImplementationHead>),
}

fn implementation_heads_overlap(left: &TypeSyntax, right: &TypeSyntax) -> bool {
    let left = implementation_head(left, "left");
    let right = implementation_head(right, "right");
    unify_implementation_heads(left, right, &mut BTreeMap::new())
}

fn implementation_head(ty: &TypeSyntax, side: &str) -> ImplementationHead {
    let constructor = |name: String, arguments: Vec<ImplementationHead>| {
        ImplementationHead::Constructor(name, arguments)
    };
    match ty {
        TypeSyntax::Variable { name, .. } => ImplementationHead::Variable(format!("{side}:{name}")),
        TypeSyntax::Primitive { name, .. } => constructor(format!("primitive:{name}"), vec![]),
        TypeSyntax::SelfType { .. } => constructor("self".to_owned(), vec![]),
        TypeSyntax::Projection {
            protocol,
            associated,
            argument,
            ..
        } => constructor(
            format!("projection:{protocol}.{associated}"),
            vec![implementation_head(argument, side)],
        ),
        TypeSyntax::Named {
            declaration,
            arguments,
            ..
        } => constructor(
            format!("named:{}", declaration.0),
            arguments
                .iter()
                .map(|argument| implementation_head(argument, side))
                .collect(),
        ),
        TypeSyntax::Union { members, .. } => constructor(
            "union".to_owned(),
            members
                .iter()
                .map(|member| implementation_head(member, side))
                .collect(),
        ),
        TypeSyntax::Atom { name, .. } => constructor(format!("atom:{name}"), vec![]),
        TypeSyntax::List { item, .. } => {
            constructor("list".to_owned(), vec![implementation_head(item, side)])
        }
        TypeSyntax::Array { item, length, .. } => constructor(
            format!("array:{length}"),
            vec![implementation_head(item, side)],
        ),
        TypeSyntax::Slice { item, .. } => {
            constructor("slice".to_owned(), vec![implementation_head(item, side)])
        }
        TypeSyntax::Map { key, value, .. } => constructor(
            "map".to_owned(),
            vec![
                implementation_head(key, side),
                implementation_head(value, side),
            ],
        ),
        TypeSyntax::Tuple { elements, .. } => constructor(
            format!("tuple:{}", elements.len()),
            elements
                .iter()
                .map(|element| implementation_head(element, side))
                .collect(),
        ),
        TypeSyntax::Function {
            parameters, result, ..
        } => {
            let mut arguments = parameters
                .iter()
                .map(|parameter| implementation_head(parameter, side))
                .collect::<Vec<_>>();
            arguments.push(implementation_head(result, side));
            constructor(format!("function:{}", parameters.len()), arguments)
        }
    }
}

fn unify_implementation_heads(
    left: ImplementationHead,
    right: ImplementationHead,
    substitutions: &mut BTreeMap<String, ImplementationHead>,
) -> bool {
    let left = substitute_implementation_head(left, substitutions);
    let right = substitute_implementation_head(right, substitutions);
    match (left, right) {
        (ImplementationHead::Variable(left), ImplementationHead::Variable(right))
            if left == right =>
        {
            true
        }
        (ImplementationHead::Variable(variable), value)
        | (value, ImplementationHead::Variable(variable)) => {
            if occurs_in_implementation_head(&variable, &value, substitutions) {
                false
            } else {
                substitutions.insert(variable, value);
                true
            }
        }
        (
            ImplementationHead::Constructor(left_name, left_arguments),
            ImplementationHead::Constructor(right_name, right_arguments),
        ) => {
            left_name == right_name
                && left_arguments.len() == right_arguments.len()
                && left_arguments
                    .into_iter()
                    .zip(right_arguments)
                    .all(|(left, right)| unify_implementation_heads(left, right, substitutions))
        }
    }
}

fn substitute_implementation_head(
    mut head: ImplementationHead,
    substitutions: &BTreeMap<String, ImplementationHead>,
) -> ImplementationHead {
    while let ImplementationHead::Variable(variable) = &head {
        let Some(replacement) = substitutions.get(variable) else {
            break;
        };
        head = replacement.clone();
    }
    head
}

fn occurs_in_implementation_head(
    variable: &str,
    head: &ImplementationHead,
    substitutions: &BTreeMap<String, ImplementationHead>,
) -> bool {
    match substitute_implementation_head(head.clone(), substitutions) {
        ImplementationHead::Variable(candidate) => candidate == variable,
        ImplementationHead::Constructor(_, arguments) => arguments
            .iter()
            .any(|argument| occurs_in_implementation_head(variable, argument, substitutions)),
    }
}
