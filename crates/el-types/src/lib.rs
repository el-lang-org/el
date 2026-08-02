//! Canonical types, bidirectional checking, and the verified Typed AST.

use el_ast::{Node, Value};
use el_resolve::{
    DeclId, Function, ImplId, ModuleId, ResolvedProgram, Struct, SymbolId, TypeAlias, TypeSyntax,
    Visibility,
};
use el_span::{Diagnostic, Span};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TypeId(pub u32);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum OpaqueType {
    FileReader,
    FileWriter,
    FileError,
    IoStdin,
    IoStdout,
    IoStderr,
    IoError,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Type {
    I8,
    I16,
    I32,
    I64,
    Isize,
    Usize,
    Bool,
    Unit,
    String,
    Bytes,
    Bits,
    Buffer,
    Rune,
    Utf8Error,
    Opaque(OpaqueType),
    CodepointView,
    GraphemeView,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Atom(String),
    List(TypeId),
    Array {
        item: TypeId,
        length: u64,
    },
    Slice(TypeId),
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
    Projection {
        protocol: String,
        associated: String,
        argument: TypeId,
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
    pub method_declarations: Vec<(String, DeclId)>,
    pub constraints: Vec<(TypeId, String)>,
    pub span: Span,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedStruct {
    pub id: DeclId,
    pub name: String,
    pub span: Span,
    pub parameters: Vec<TypeId>,
    pub derives: Vec<String>,
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
    pub module_name: String,
    pub name: String,
    pub visibility: Visibility,
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
    Float(u64),
    Atom(String),
    UnionMember {
        member: TypeId,
        symbol: SymbolId,
        name: String,
    },
    StructuralUnionMember {
        member: TypeId,
        pattern: Box<TypedPattern>,
    },
    Tuple(Vec<TypedPattern>),
    ListEmpty,
    ListCons {
        head: Box<TypedPattern>,
        tail: Box<TypedPattern>,
    },
    Struct {
        declaration: DeclId,
        field_count: usize,
        fields: Vec<(usize, TypedPattern)>,
    },
    Bitstring(Vec<TypedBitstringPatternSegment>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedBitstringPatternSegment {
    pub pattern: TypedPattern,
    pub kind: TypedBitstringPatternSegmentKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedBitstringPatternSegmentKind {
    Integer {
        signed: bool,
        byte_order: BitstringByteOrder,
        width: u8,
    },
    Bytes {
        size: Option<TypedExpr>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedMatchArm {
    pub pattern: TypedPattern,
    pub body: TypedBlock,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedCapture {
    pub source: SymbolId,
    pub symbol: SymbolId,
    pub name: String,
    pub ty: TypeId,
    pub span: Span,
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
    StructFieldAssign {
        symbol: SymbolId,
        declaration: DeclId,
        field: usize,
        field_types: Vec<TypeId>,
        value: TypedExpr,
        span: Span,
    },
    Expr(TypedExpr),
    Return(TypedExpr),
    While {
        condition: TypedExpr,
        body: TypedBlock,
        span: Span,
    },
    For {
        pattern: TypedPattern,
        iterable: TypedExpr,
        index_ty: TypeId,
        option_ty: TypeId,
        some_ty: TypeId,
        body: TypedBlock,
        span: Span,
    },
    DeferCall {
        function: DeclId,
        substitutions: Vec<(TypeId, TypeId)>,
        arguments: Vec<TypedExpr>,
        span: Span,
    },
    DeferBlock {
        captures: Vec<TypedCapture>,
        body: TypedBlock,
        span: Span,
    },
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
    Float(u64),
    Boolean(bool),
    Unit,
    String(String),
    Rune(char),
    Atom(String),
    StandardCall {
        operation: StandardOperation,
        arguments: Vec<TypedExpr>,
    },
    List {
        elements: Vec<TypedExpr>,
        tail: Option<Box<TypedExpr>>,
    },
    ListReverse(Box<TypedExpr>),
    Array(Vec<TypedExpr>),
    Map(Vec<(TypedExpr, TypedExpr)>),
    MapPut {
        map: Box<TypedExpr>,
        key: Box<TypedExpr>,
        value: Box<TypedExpr>,
    },
    MapRemove {
        map: Box<TypedExpr>,
        key: Box<TypedExpr>,
    },
    MapFetch {
        map: Box<TypedExpr>,
        key: Box<TypedExpr>,
    },
    MapToList(Box<TypedExpr>),
    Tuple(Vec<TypedExpr>),
    Struct {
        declaration: DeclId,
        field_count: usize,
        fields: Vec<(usize, TypedExpr)>,
    },
    StructProject {
        value: Box<TypedExpr>,
        declaration: DeclId,
        field: usize,
    },
    Index {
        value: Box<TypedExpr>,
        index: Box<TypedExpr>,
        length: Option<u64>,
    },
    SliceFromArray {
        value: Box<TypedExpr>,
        length: u64,
    },
    SliceSubslice {
        value: Box<TypedExpr>,
        start: Box<TypedExpr>,
        length: Box<TypedExpr>,
    },
    SliceCopy(Box<TypedExpr>),
    StringBytes(Box<TypedExpr>),
    StringCodepoints(Box<TypedExpr>),
    StringCodepointView(Box<TypedExpr>),
    StringGraphemeView(Box<TypedExpr>),
    StringLength(Box<TypedExpr>),
    StringEmpty(Box<TypedExpr>),
    StringContains {
        string: Box<TypedExpr>,
        pattern: Box<TypedExpr>,
    },
    StringSplit {
        string: Box<TypedExpr>,
        separator: Box<TypedExpr>,
    },
    Bitstring(Vec<TypedBitstringSegment>),
    StringFromBytes(Box<TypedExpr>),
    Utf8ErrorOffset(Box<TypedExpr>),
    RuneToString(Box<TypedExpr>),
    IntegerToString(Box<TypedExpr>),
    BooleanToString(Box<TypedExpr>),
    ShowConstant {
        value: Box<TypedExpr>,
        rendered: String,
    },
    BufferNew,
    BufferAppend {
        buffer: Box<TypedExpr>,
        value: Box<TypedExpr>,
        kind: BufferAppendKind,
    },
    BufferToBytes(Box<TypedExpr>),
    BufferToString(Box<TypedExpr>),
    BytesToBits(Box<TypedExpr>),
    BitsToBytes(Box<TypedExpr>),
    BitsSlice {
        value: Box<TypedExpr>,
        start: Box<TypedExpr>,
        length: Box<TypedExpr>,
    },
    BytesFromList(Box<TypedExpr>),
    BytesToList(Box<TypedExpr>),
    BytesSlice {
        value: Box<TypedExpr>,
        start: Box<TypedExpr>,
        length: Box<TypedExpr>,
    },
    CollectionLength {
        value: Box<TypedExpr>,
        known_length: Option<u64>,
    },
    EnumAt {
        value: Box<TypedExpr>,
        index: Box<TypedExpr>,
    },
    EnumToList(Box<TypedExpr>),
    EnumVisit {
        value: Box<TypedExpr>,
        initial: Option<Box<TypedExpr>>,
        function: Box<TypedExpr>,
        kind: EnumVisitKind,
    },
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
    FunctionRef {
        function: DeclId,
        substitutions: Vec<(TypeId, TypeId)>,
    },
    Binary {
        operator: ArithmeticOperator,
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
    },
    IntegerUnary {
        operator: IntegerUnaryOperator,
        operand: Box<TypedExpr>,
    },
    FloatNegate(Box<TypedExpr>),
    IntegerBinary {
        operator: IntegerBinaryOperator,
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
    },
    IntegerConvert(Box<TypedExpr>),
    NumericConvert(Box<TypedExpr>),
    Concat {
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
    },
    WrappingInteger {
        operator: WrappingIntegerOperator,
        left: Box<TypedExpr>,
        right: Option<Box<TypedExpr>>,
    },
    Comparison {
        operator: ComparisonOperator,
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
    },
    Logical {
        operator: LogicalOperator,
        left: Box<TypedExpr>,
        right: Box<TypedExpr>,
    },
    Call {
        function: DeclId,
        substitutions: Vec<(TypeId, TypeId)>,
        arguments: Vec<TypedExpr>,
    },
    IndirectCall {
        callee: Box<TypedExpr>,
        arguments: Vec<TypedExpr>,
    },
    UnionInject {
        member: TypeId,
        value: Box<TypedExpr>,
    },
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StandardOperation {
    FileOpenRead,
    FileCreate,
    FileAppend,
    FileClose,
    ReaderRead,
    WriterWrite,
    WriterFlush,
    IoStdin,
    IoStdout,
    IoStderr,
    ErrorKind,
    ErrorOperation,
    ErrorCode,
    ProcessArguments,
    ProcessGetEnv,
    IoPrint,
    IoPrintln,
    IoReport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitstringByteOrder {
    Big,
    Little,
    Native,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnumVisitKind {
    Each,
    Any,
    All,
    Reduce,
    Filter,
    Map,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedBitstringSegment {
    pub value: TypedExpr,
    pub kind: TypedBitstringSegmentKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TypedBitstringSegmentKind {
    Integer {
        signed: bool,
        byte_order: BitstringByteOrder,
        width: u8,
    },
    Bytes {
        size: Option<TypedExpr>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BufferAppendKind {
    Byte,
    Bytes,
    String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArithmeticOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegerUnaryOperator {
    Negate,
    BitwiseNot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IntegerBinaryOperator {
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    ShiftLeft,
    ShiftRight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WrappingIntegerOperator {
    Add,
    Subtract,
    Multiply,
    Negate,
    ShiftLeft,
    ShiftRight,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonOperator {
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogicalOperator {
    And,
    Or,
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
    Float(u64),
    Atom(String),
    Union(TypeId, Box<(TypeId, PatternShape)>),
    Tuple(Vec<(TypeId, PatternShape)>),
    ListEmpty,
    ListCons(Box<(TypeId, PatternShape)>, Box<(TypeId, PatternShape)>),
    Struct(DeclId, Vec<(TypeId, PatternShape)>),
    Bitstring(String),
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
    defer_depth: usize,
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
            defer_depth: 0,
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
                let constraints = implementation
                    .constraints
                    .iter()
                    .filter_map(|constraint| {
                        parameters
                            .get(&constraint.parameter)
                            .map(|parameter| (*parameter, constraint.protocol.clone()))
                    })
                    .collect();
                Some(TypedImplementation {
                    id: implementation.id,
                    protocol: implementation.protocol.clone(),
                    target,
                    associated_types,
                    methods: implementation.methods.clone(),
                    method_declarations: implementation.method_declarations.clone(),
                    constraints,
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
            derives: structure.derives.clone(),
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
                "i8" => Some(self.intern(Type::I8)),
                "i16" => Some(self.intern(Type::I16)),
                "isize" => Some(self.intern(Type::Isize)),
                "usize" => Some(self.intern(Type::Usize)),
                "bool" => Some(TypeId(2)),
                "unit" => Some(TypeId(3)),
                "string" => Some(self.intern(Type::String)),
                "bytes" => Some(self.intern(Type::Bytes)),
                "bits" => Some(self.intern(Type::Bits)),
                "Buffer" => Some(self.intern(Type::Buffer)),
                "rune" => Some(self.intern(Type::Rune)),
                "String.Utf8Error" => Some(self.intern(Type::Utf8Error)),
                "File.Reader" => Some(self.intern(Type::Opaque(OpaqueType::FileReader))),
                "File.Writer" => Some(self.intern(Type::Opaque(OpaqueType::FileWriter))),
                "File.Error" => Some(self.intern(Type::Opaque(OpaqueType::FileError))),
                "IO.Stdin" => Some(self.intern(Type::Opaque(OpaqueType::IoStdin))),
                "IO.Stdout" => Some(self.intern(Type::Opaque(OpaqueType::IoStdout))),
                "IO.Stderr" => Some(self.intern(Type::Opaque(OpaqueType::IoStderr))),
                "IO.Error" => Some(self.intern(Type::Opaque(OpaqueType::IoError))),
                "IO.ErrorKind" => self.closed_atom_union(
                    &[
                        "not_found",
                        "permission_denied",
                        "already_exists",
                        "invalid_input",
                        "is_directory",
                        "not_directory",
                        "closed",
                        "broken_pipe",
                        "out_of_space",
                        "other",
                    ],
                    *span,
                ),
                "IO.Operation" => self.closed_atom_union(
                    &[
                        "open_read",
                        "create",
                        "append",
                        "read",
                        "write",
                        "flush",
                        "close",
                    ],
                    *span,
                ),
                "String.CodepointView" => Some(self.intern(Type::CodepointView)),
                "String.GraphemeView" => Some(self.intern(Type::GraphemeView)),
                "u8" => Some(self.intern(Type::U8)),
                "u16" => Some(self.intern(Type::U16)),
                "u32" => Some(self.intern(Type::U32)),
                "u64" => Some(self.intern(Type::U64)),
                "f32" => Some(self.intern(Type::F32)),
                "f64" => Some(self.intern(Type::F64)),
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
            TypeSyntax::SelfType { span } => {
                self.diagnostics.push(Diagnostic::error(
                    "E2163",
                    *span,
                    "`Self` is only valid in a protocol or implementation signature",
                ));
                None
            }
            TypeSyntax::Projection {
                protocol,
                associated,
                argument,
                ..
            } => {
                let argument = self.resolve_type(argument, parameters)?;
                self.normalize_standard_projection(protocol, associated, argument)
                    .or_else(|| {
                        Some(self.intern(Type::Projection {
                            protocol: protocol.clone(),
                            associated: associated.clone(),
                            argument,
                        }))
                    })
            }
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
            TypeSyntax::Slice { item, .. } => {
                let item = self.resolve_type(item, parameters)?;
                Some(self.intern(Type::Slice(item)))
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
            module_name: function.module_name.clone(),
            name: function.name.clone(),
            visibility: function.visibility,
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
                    if self.defer_depth > 0 {
                        self.diagnostics.push(Diagnostic::error(
                            "E2142",
                            item.span,
                            "a deferred block cannot return",
                        ));
                        continue;
                    }
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
                "while_expr" => {
                    let condition =
                        self.check_expr(item.children.first()?, Some(TypeId(2)), owner, scopes);
                    scopes.push(BTreeMap::new());
                    let body =
                        self.check_block(item.children.get(1)?, Some(TypeId(3)), owner, scopes);
                    scopes.pop();
                    if let (Some(condition), Some(body)) = (condition, body) {
                        items.push(TypedItem::While {
                            condition,
                            body,
                            span: item.span,
                        });
                    }
                }
                "for_expr" => {
                    let Some(iterable) = self.check_expr(&item.children[1], None, owner, scopes)
                    else {
                        continue;
                    };
                    let Some(item_ty) = self.iterable_item_type(iterable.ty, owner) else {
                        self.diagnostics.push(Diagnostic::error(
                            "E2161",
                            item.children[1].span,
                            format!(
                                "type `{}` does not implement `Iterable`",
                                self.type_name(iterable.ty)
                            ),
                        ));
                        continue;
                    };
                    let mut bindings = BTreeMap::new();
                    let Some((pattern, _)) = self.check_pattern(
                        &item.children[0],
                        item_ty,
                        owner,
                        scopes,
                        &mut bindings,
                    ) else {
                        continue;
                    };
                    if !pattern.facts.irrefutable {
                        self.diagnostics.push(Diagnostic::error(
                            "E2162",
                            pattern.span,
                            "a `for` pattern must be irrefutable for the iterable item type",
                        ));
                        continue;
                    }
                    let some_atom = self.intern(Type::Atom("some".to_owned()));
                    let none_atom = self.intern(Type::Atom("none".to_owned()));
                    let index_ty = self.intern(Type::Usize);
                    let some_ty = self.intern(Type::Tuple(vec![some_atom, item_ty]));
                    let Some(option_ty) = self.normalize_union(vec![some_ty, none_atom], item.span)
                    else {
                        continue;
                    };
                    scopes.push(bindings);
                    let body = self.check_block(&item.children[2], Some(TypeId(3)), owner, scopes);
                    scopes.pop();
                    if let Some(body) = body {
                        items.push(TypedItem::For {
                            pattern,
                            iterable,
                            index_ty,
                            option_ty,
                            some_ty,
                            body,
                            span: item.span,
                        });
                    }
                }
                "defer_expr" => {
                    if self.defer_depth > 0 {
                        self.diagnostics.push(Diagnostic::error(
                            "E2141",
                            item.span,
                            "a deferred block cannot register another `defer`",
                        ));
                        continue;
                    }
                    if let Some(typed) = self.check_defer(item, owner, scopes) {
                        items.push(typed);
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

    fn check_defer(
        &mut self,
        node: &Node,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedItem> {
        let action = node.children.first()?;
        if action.kind.as_str() != "defer_block" {
            let call = self.check_call(action, Some(TypeId(3)), owner, scopes)?;
            if call.ty != TypeId(3) {
                self.type_mismatch(action.span, TypeId(3), call.ty);
                return None;
            }
            let TypedExprKind::Call {
                function,
                substitutions,
                arguments,
            } = call.kind
            else {
                self.diagnostics.push(Diagnostic::error(
                    "E2143",
                    action.span,
                    "a deferred call must have a statically resolved target",
                ));
                return None;
            };
            return Some(TypedItem::DeferCall {
                function,
                substitutions,
                arguments,
                span: node.span,
            });
        }

        let block = action.children.first()?;
        let mut visible = BTreeMap::<String, Local>::new();
        for scope in scopes.iter() {
            visible.extend(scope.clone());
        }
        let mut visible = visible.into_iter().collect::<Vec<_>>();
        visible.sort_by_key(|(_, local)| local.symbol);
        let mut capture_scope = BTreeMap::new();
        let mut captures = Vec::new();
        for (name, local) in visible {
            let symbol = SymbolId(self.next_symbol);
            self.next_symbol += 1;
            capture_scope.insert(
                name.clone(),
                Local {
                    symbol,
                    ty: local.ty,
                    mutable: false,
                    span: local.span,
                },
            );
            captures.push(TypedCapture {
                source: local.symbol,
                symbol,
                name,
                ty: local.ty,
                span: local.span,
            });
        }

        let mut action_scopes = vec![capture_scope];
        self.defer_depth += 1;
        let body = self.check_block(block, Some(TypeId(3)), owner, &mut action_scopes);
        self.defer_depth -= 1;
        let body = body?;
        let mut referenced = BTreeSet::new();
        collect_block_locals(&body, &mut referenced);
        captures.retain(|capture| referenced.contains(&capture.symbol));
        Some(TypedItem::DeferBlock {
            captures,
            body,
            span: node.span,
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
                if written == "Slice" {
                    if node.children.len() != 2 {
                        self.diagnostics.push(Diagnostic::error(
                            "E2118",
                            node.span,
                            format!(
                                "type `Slice` expects 1 argument but received {}",
                                node.children.len() - 1
                            ),
                        ));
                        return None;
                    }
                    return Some(TypeSyntax::Slice {
                        item: Box::new(self.annotation_syntax(&node.children[1], module)?),
                        span: node.span,
                    });
                }
                if matches!(
                    written.as_str(),
                    "String.Utf8Error"
                        | "File.Reader"
                        | "File.Writer"
                        | "File.Error"
                        | "IO.Stdin"
                        | "IO.Stdout"
                        | "IO.Stderr"
                        | "IO.Error"
                        | "IO.ErrorKind"
                        | "IO.Operation"
                ) && node.children.len() == 1
                {
                    return Some(TypeSyntax::Primitive {
                        name: written,
                        span: node.span,
                    });
                }
                if let Some((protocol, associated)) = written.rsplit_once('.') {
                    let protocol_known = matches!(
                        protocol,
                        "Eq" | "Ord"
                            | "Show"
                            | "Hash"
                            | "Iterable"
                            | "Reader"
                            | "Writer"
                            | "Concat"
                    ) || self
                        .program
                        .protocols
                        .iter()
                        .any(|candidate| candidate.name == protocol);
                    if node.children.len() == 2 && protocol_known {
                        return Some(TypeSyntax::Projection {
                            protocol: protocol.to_owned(),
                            associated: associated.to_owned(),
                            argument: Box::new(self.annotation_syntax(&node.children[1], module)?),
                            span: node.span,
                        });
                    }
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
        let (root, field) = if target.kind.as_str() == "postfix_expr" {
            let root = target.children.first().and_then(unqualified_name)?;
            let field = target
                .children
                .get(1)
                .and_then(|access| access.children.first())
                .map(text)?;
            (root, Some(field))
        } else {
            (unqualified_name(target)?, None)
        };
        let Some(local) = lookup(scopes, &root).cloned() else {
            self.diagnostics.push(Diagnostic::error(
                "E2103",
                target.span,
                format!("unknown local `{root}`"),
            ));
            return None;
        };
        if !local.mutable {
            self.diagnostics.push(Diagnostic::error(
                "E2104",
                target.span,
                format!("cannot assign to immutable local `{root}`"),
            ));
            return None;
        }
        if let Some(field_name) = field {
            let Type::Struct {
                declaration,
                arguments,
            } = self.types[local.ty.0 as usize].clone()
            else {
                self.diagnostics.push(Diagnostic::error(
                    "E2143",
                    target.span,
                    "direct field update requires a struct local",
                ));
                return None;
            };
            let structure = self.structs.get(&declaration)?.clone();
            let Some(field) = structure
                .fields
                .iter()
                .position(|candidate| candidate.name == field_name)
            else {
                self.diagnostics.push(Diagnostic::error(
                    "E2144",
                    target.span,
                    format!("unknown field `{field_name}` on `{}`", structure.name),
                ));
                return None;
            };
            let substitutions = structure
                .parameters
                .iter()
                .cloned()
                .zip(arguments)
                .collect::<BTreeMap<_, _>>();
            let field_ty = self.resolve_type(&structure.fields[field].ty, &substitutions)?;
            let field_types = structure
                .fields
                .iter()
                .filter_map(|candidate| self.resolve_type(&candidate.ty, &substitutions))
                .collect::<Vec<_>>();
            if field_types.len() != structure.fields.len() {
                return None;
            }
            let value = self.check_expr(node.children.last()?, Some(field_ty), owner, scopes)?;
            Some(TypedItem::StructFieldAssign {
                symbol: local.symbol,
                declaration,
                field,
                field_types,
                value,
                span: node.span,
            })
        } else {
            let value = self.check_expr(node.children.last()?, Some(local.ty), owner, scopes)?;
            Some(TypedItem::Assign {
                symbol: local.symbol,
                value,
                span: node.span,
            })
        }
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
            && node.kind.as_str() != "pipeline_expr"
        {
            if node.kind.as_str() == "if_expr" {
                return self.check_if(node, Some(expected), owner, scopes);
            }
            if node.kind.as_str() == "match_expr" {
                return self.check_match(node, Some(expected), owner, scopes);
            }
            return self.check_union_injection(node, expected, owner, scopes);
        }
        let expression = match node.kind.as_str() {
            "integer" => self.check_integer(node, expected),
            "float" => self.check_float(node, expected),
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
            "string" => self.check_string(node, owner, scopes),
            "rune" => self.check_rune(node),
            "bitstring_expr" => self.check_bitstring(node, owner, scopes),
            "atom" => self.check_atom(node),
            "list_literal" => self.check_list(node, expected, owner, scopes),
            "array_literal" => self.check_array(node, expected, owner, scopes),
            "map_literal" => self.check_map(node, expected, owner, scopes),
            "if_expr" => self.check_if(node, expected, owner, scopes),
            "match_expr" => self.check_match(node, expected, owner, scopes),
            "tuple_literal" => self.check_tuple(node, expected, owner, scopes),
            "struct_literal" => self.check_struct_literal(node, expected, owner, scopes),
            "ascription_expr" => self.check_ascription(node, owner, scopes),
            "qualified_value" | "identifier" => self.check_name(node, expected, owner, scopes),
            "additive_expr" | "multiplicative_expr" => {
                self.check_binary(node, expected, owner, scopes)
            }
            "unary_expr" => self.check_integer_unary(node, expected, owner, scopes),
            "bit_or_expr" | "bit_xor_expr" | "bit_and_expr" | "shift_expr" => {
                self.check_integer_binary(node, expected, owner, scopes)
            }
            "equality_expr" | "comparison_expr" => self.check_comparison(node, owner, scopes),
            "concat_expr" => self.check_concat(node, expected, owner, scopes),
            "logical_and_expr" | "logical_or_expr" => self.check_logical(node, owner, scopes),
            "pipeline_expr" => self.check_pipeline(node, expected, owner, scopes),
            "postfix_expr" => self.check_postfix(node, expected, owner, scopes),
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
            "integer"
            | "additive_expr"
            | "multiplicative_expr"
            | "unary_expr"
            | "bit_or_expr"
            | "bit_xor_expr"
            | "bit_and_expr"
            | "shift_expr" => members
                .iter()
                .copied()
                .filter(|member| is_integer_type(&self.types[member.0 as usize]))
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
            "string" => members
                .iter()
                .copied()
                .filter(|member| matches!(self.types[member.0 as usize], Type::String))
                .collect(),
            "rune" => members
                .iter()
                .copied()
                .filter(|member| matches!(self.types[member.0 as usize], Type::Rune))
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
        let mut matrix_spans = Vec::<Span>::new();
        let mut arms = Vec::new();
        let mut result_type = expected;
        for arm_node in &node.children[1..] {
            let pattern_node = &arm_node.children[0];
            let mut arm_scope = BTreeMap::new();
            let (mut pattern, shape) =
                self.check_pattern(pattern_node, subject.ty, owner, scopes, &mut arm_scope)?;
            let reachable =
                pattern_is_useful(&matrix, vec![shape.clone()], vec![subject.ty], &self.types);
            if !reachable {
                let mut diagnostic =
                    Diagnostic::error("E2125", pattern.span, "unreachable match arm");
                if let Some((_, span)) = matrix.iter().zip(&matrix_spans).find(|(row, _)| {
                    !pattern_is_useful(
                        std::slice::from_ref(*row),
                        vec![shape.clone()],
                        vec![subject.ty],
                        &self.types,
                    )
                }) {
                    diagnostic = diagnostic.with_label(*span, "earlier arm subsumes this pattern");
                } else {
                    diagnostic =
                        diagnostic.with_note("the preceding arms collectively cover this pattern");
                }
                self.diagnostics.push(diagnostic);
                continue;
            }
            pattern.facts.reachable = true;
            matrix.push(vec![shape]);
            matrix_spans.push(pattern.span);
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
        );
        if !exhaustive {
            self.diagnostics.push(
                Diagnostic::error("E2126", node.span, "non-exhaustive match")
                    .with_help("add the missing cases or a wildcard/binding catch-all arm"),
            );
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
        scopes: &[BTreeMap<String, Local>],
        bindings: &mut BTreeMap<String, Local>,
    ) -> Option<(TypedPattern, PatternShape)> {
        if node.kind.as_str() == "tuple_pattern"
            && let Type::Union(members) = self.types[subject.0 as usize].clone()
        {
            let candidates = members
                .into_iter()
                .filter(|member| self.pattern_may_match_type(node, *member))
                .collect::<Vec<_>>();
            let [member] = candidates.as_slice() else {
                self.diagnostics.push(Diagnostic::error(
                    "E2131",
                    node.span,
                    "tuple pattern must select exactly one tuple member of the subject union",
                ));
                return None;
            };
            let member = *member;
            let (nested, nested_shape) =
                self.check_pattern(node, member, owner, scopes, bindings)?;
            return Some((
                TypedPattern {
                    kind: TypedPatternKind::StructuralUnionMember {
                        member,
                        pattern: Box::new(nested),
                    },
                    ty: subject,
                    span: node.span,
                    facts: PatternFacts {
                        reachable: true,
                        irrefutable: false,
                    },
                },
                PatternShape::Union(member, Box::new((member, nested_shape))),
            ));
        }
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
                if matches!(
                    self.types.get(subject.0 as usize),
                    Some(Type::Union(members)) if members.contains(&expected)
                ) {
                    let symbol = SymbolId(self.next_symbol);
                    self.next_symbol += 1;
                    (
                        TypedPatternKind::UnionMember {
                            member: expected,
                            symbol,
                            name: String::new(),
                        },
                        false,
                        PatternShape::Union(expected, Box::new((expected, PatternShape::Wildcard))),
                    )
                } else {
                    if expected != subject {
                        self.type_mismatch(node.span, subject, expected);
                        return None;
                    }
                    (
                        TypedPatternKind::Atom(name.clone()),
                        true,
                        PatternShape::Atom(name.clone()),
                    )
                }
            }
            "integer" if is_integer_type(&self.types[subject.0 as usize]) => {
                let value = integer_value(node)?;
                let (minimum, maximum) = integer_bounds(&self.types[subject.0 as usize])
                    .expect("guard accepts only integer pattern types");
                let in_range = (minimum..=maximum).contains(&value);
                if !in_range {
                    self.diagnostics.push(Diagnostic::error(
                        "E2106",
                        node.span,
                        format!(
                            "integer literal is out of range for `{}`",
                            self.type_name(subject)
                        ),
                    ));
                    return None;
                }
                (
                    TypedPatternKind::Integer(value),
                    false,
                    PatternShape::Integer(value),
                )
            }
            "float"
                if matches!(
                    self.types.get(subject.0 as usize),
                    Some(Type::F32 | Type::F64)
                ) =>
            {
                let Some(bits) = float_bits(node, &self.types[subject.0 as usize]) else {
                    self.diagnostics.push(Diagnostic::error(
                        "E2106",
                        node.span,
                        format!(
                            "floating literal is out of range for `{}`",
                            self.type_name(subject)
                        ),
                    ));
                    return None;
                };
                let shape_bits = canonical_float_pattern_bits(bits);
                (
                    TypedPatternKind::Float(bits),
                    false,
                    PatternShape::Float(shape_bits),
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
                    PatternShape::Union(member, Box::new((member, PatternShape::Wildcard))),
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
                    let (pattern, shape) =
                        self.check_pattern(child, ty, owner, scopes, bindings)?;
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
                        self.check_pattern(&node.children[0], item, owner, scopes, bindings)?;
                    let (tail, tail_shape) =
                        self.check_pattern(&node.children[1], subject, owner, scopes, bindings)?;
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
            "bitstring_pattern" => {
                if !matches!(self.types[subject.0 as usize], Type::Bytes) {
                    self.diagnostics.push(Diagnostic::error(
                        "E2157",
                        node.span,
                        "bitstring pattern requires a bytes subject",
                    ));
                    return None;
                }
                let mut segments = Vec::with_capacity(node.children.len());
                for segment in &node.children {
                    let pattern_node = segment.children.first()?;
                    let modifiers = segment.children.get(1)?;
                    let is_bytes = modifiers
                        .children
                        .iter()
                        .any(|modifier| modifier.kind.as_str() == "modifier_bytes");
                    let size_node = modifiers
                        .children
                        .iter()
                        .find(|modifier| modifier.kind.as_str() == "size_modifier")
                        .and_then(|modifier| modifier.children.first());
                    let (pattern, kind) = if is_bytes {
                        let mut size_scopes = scopes.to_vec();
                        size_scopes.push(bindings.clone());
                        let usize_ty = self.intern(Type::Usize);
                        let size = if let Some(size) = size_node {
                            Some(self.check_expr(size, Some(usize_ty), owner, &mut size_scopes)?)
                        } else {
                            None
                        };
                        let pattern =
                            self.check_pattern(pattern_node, subject, owner, scopes, bindings)?;
                        (pattern.0, TypedBitstringPatternSegmentKind::Bytes { size })
                    } else {
                        let signed = modifiers
                            .children
                            .iter()
                            .any(|modifier| modifier.kind.as_str() == "modifier_signed");
                        let byte_order = if modifiers
                            .children
                            .iter()
                            .any(|modifier| modifier.kind.as_str() == "modifier_little")
                        {
                            BitstringByteOrder::Little
                        } else if modifiers
                            .children
                            .iter()
                            .any(|modifier| modifier.kind.as_str() == "modifier_native")
                        {
                            BitstringByteOrder::Native
                        } else {
                            BitstringByteOrder::Big
                        };
                        let width = size_node
                            .and_then(integer_value)
                            .and_then(|width| u8::try_from(width).ok())?;
                        let integer_ty = if signed {
                            TypeId(1)
                        } else {
                            self.intern(Type::U64)
                        };
                        let (pattern, _) =
                            self.check_pattern(pattern_node, integer_ty, owner, scopes, bindings)?;
                        if let TypedPatternKind::Integer(value) = &pattern.kind {
                            let minimum = if signed { -(1_i128 << (width - 1)) } else { 0 };
                            let maximum = if signed {
                                (1_i128 << (width - 1)) - 1
                            } else if width == 64 {
                                u64::MAX as i128
                            } else {
                                (1_i128 << width) - 1
                            };
                            if *value < minimum || *value > maximum {
                                self.diagnostics.push(Diagnostic::error(
                                    "E2158",
                                    pattern.span,
                                    "integer pattern does not fit its bitstring segment",
                                ));
                                return None;
                            }
                        }
                        (
                            pattern,
                            TypedBitstringPatternSegmentKind::Integer {
                                signed,
                                byte_order,
                                width,
                            },
                        )
                    };
                    segments.push(TypedBitstringPatternSegment { pattern, kind });
                }
                let irrefutable = matches!(segments.as_slice(), [TypedBitstringPatternSegment {
                    pattern,
                    kind: TypedBitstringPatternSegmentKind::Bytes { size: None },
                }] if pattern.facts.irrefutable);
                let shape = if irrefutable {
                    PatternShape::Wildcard
                } else {
                    PatternShape::Bitstring(bitstring_pattern_shape(&segments))
                };
                (TypedPatternKind::Bitstring(segments), irrefutable, shape)
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
                    let (pattern, field_shape) = self.check_pattern(
                        &field_node.children[1],
                        field_ty,
                        owner,
                        scopes,
                        bindings,
                    )?;
                    shapes[index].1 = field_shape;
                    fields.push((index, pattern));
                }
                fields.sort_by_key(|(index, _)| *index);
                let irrefutable = fields.iter().all(|(_, pattern)| pattern.facts.irrefutable);
                (
                    TypedPatternKind::Struct {
                        declaration,
                        field_count: structure.fields.len(),
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

    fn pattern_may_match_type(&self, node: &Node, ty: TypeId) -> bool {
        match node.kind.as_str() {
            "wildcard_pattern" | "identifier" => true,
            "kw_true" | "kw_false" => matches!(self.types[ty.0 as usize], Type::Bool),
            "atom" => {
                let Some(Value::Atom { name, .. }) = node.value.as_ref() else {
                    return false;
                };
                matches!(&self.types[ty.0 as usize], Type::Atom(expected) if expected == name)
            }
            "integer" => is_integer_type(&self.types[ty.0 as usize]),
            "float" => matches!(self.types[ty.0 as usize], Type::F32 | Type::F64),
            "tuple_pattern" => {
                let Type::Tuple(elements) = &self.types[ty.0 as usize] else {
                    return false;
                };
                node.children.len() == elements.len()
                    && node
                        .children
                        .iter()
                        .zip(elements)
                        .all(|(child, element)| self.pattern_may_match_type(child, *element))
            }
            "list_pattern" => matches!(self.types[ty.0 as usize], Type::List(_)),
            "bitstring_pattern" => matches!(self.types[ty.0 as usize], Type::Bytes),
            "struct_pattern" => matches!(self.types[ty.0 as usize], Type::Struct { .. }),
            "typed_pattern" => matches!(self.types[ty.0 as usize], Type::Union(_)),
            _ => false,
        }
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
            .filter(|ty| is_integer_type(&self.types[ty.0 as usize]))
            .unwrap_or(TypeId(1));
        let Some(Value::Integer { radix, digits, .. }) = &node.value else {
            return None;
        };
        let value = u128::from_str_radix(digits, *radix).ok();
        let limit = integer_bounds(&self.types[ty.0 as usize])
            .map(|(_, maximum)| maximum as u128)
            .expect("expected type is an integer");
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

    fn check_float(&mut self, node: &Node, expected: Option<TypeId>) -> Option<TypedExpr> {
        let ty = expected
            .filter(|ty| matches!(self.types.get(ty.0 as usize), Some(Type::F32 | Type::F64)))
            .unwrap_or_else(|| self.intern(Type::F64));
        let Some(Value::Float { normalized, .. }) = &node.value else {
            return None;
        };
        let bits = match self.types.get(ty.0 as usize) {
            Some(Type::F32) => normalized
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| u64::from(value.to_bits())),
            Some(Type::F64) => normalized
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(f64::to_bits),
            _ => None,
        };
        let Some(bits) = bits else {
            self.diagnostics.push(Diagnostic::error(
                "E2106",
                node.span,
                format!(
                    "floating literal is out of range for `{}`",
                    self.type_name(ty)
                ),
            ));
            return None;
        };
        Some(TypedExpr {
            kind: TypedExprKind::Float(bits),
            ty,
            span: node.span,
        })
    }

    fn check_string(
        &mut self,
        node: &Node,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let string_ty = self.intern(Type::String);
        if let Some(Value::String { decoded, .. }) = &node.value {
            return Some(TypedExpr {
                kind: TypedExprKind::String(decoded.clone()),
                ty: string_ty,
                span: node.span,
            });
        }
        let mut parts = Vec::new();
        for child in &node.children {
            let part = match child.kind.as_str() {
                "string_text" | "escape" | "escaped_interpolation" => {
                    let Some(Value::String { decoded, .. }) = &child.value else {
                        return None;
                    };
                    TypedExpr {
                        kind: TypedExprKind::String(decoded.clone()),
                        ty: string_ty,
                        span: child.span,
                    }
                }
                "interpolation" => {
                    let value = self.check_expr(child.children.first()?, None, owner, scopes)?;
                    if !self.type_satisfies(value.ty, "Show", owner) {
                        self.diagnostics.push(Diagnostic::error(
                            "E2120",
                            value.span,
                            format!(
                                "type `{}` does not satisfy `Show`",
                                self.type_name(value.ty)
                            ),
                        ));
                        return None;
                    }
                    self.show_value(value, owner)?
                }
                _ => return None,
            };
            parts.push(part);
        }
        let mut parts = parts.into_iter();
        let mut result = parts.next().unwrap_or(TypedExpr {
            kind: TypedExprKind::String(String::new()),
            ty: string_ty,
            span: node.span,
        });
        for part in parts {
            result = TypedExpr {
                kind: TypedExprKind::Concat {
                    left: Box::new(result),
                    right: Box::new(part),
                },
                ty: string_ty,
                span: node.span,
            };
        }
        Some(result)
    }

    fn show_value(&mut self, value: TypedExpr, owner: DeclId) -> Option<TypedExpr> {
        let string_ty = self.intern(Type::String);
        let span = value.span;
        if let Some(call) =
            self.explicit_protocol_call(value.ty, "Show", "show", vec![value.clone()], span, owner)
        {
            return Some(call);
        }
        match self.types.get(value.ty.0 as usize) {
            Some(Type::String) => Some(value),
            Some(Type::Rune) => Some(TypedExpr {
                kind: TypedExprKind::RuneToString(Box::new(value)),
                ty: string_ty,
                span,
            }),
            Some(ty) if is_integer_type(ty) => Some(TypedExpr {
                kind: TypedExprKind::IntegerToString(Box::new(value)),
                ty: string_ty,
                span,
            }),
            Some(Type::Bool) => Some(TypedExpr {
                kind: TypedExprKind::BooleanToString(Box::new(value)),
                ty: string_ty,
                span,
            }),
            Some(Type::Unit) => Some(TypedExpr {
                kind: TypedExprKind::ShowConstant {
                    value: Box::new(value),
                    rendered: "unit".to_owned(),
                },
                ty: string_ty,
                span,
            }),
            Some(Type::Atom(name)) => Some(TypedExpr {
                kind: TypedExprKind::ShowConstant {
                    value: Box::new(value),
                    rendered: format!(":{name}"),
                },
                ty: string_ty,
                span,
            }),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E2105",
                    span,
                    format!(
                        "standard `Show` formatting for `{}` is not implemented",
                        self.type_name(value.ty)
                    ),
                ));
                None
            }
        }
    }

    fn check_bitstring(
        &mut self,
        node: &Node,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let bytes_ty = self.intern(Type::Bytes);
        let usize_ty = self.intern(Type::Usize);
        let mut segments = Vec::with_capacity(node.children.len());
        for segment in &node.children {
            let value_node = segment.children.first()?;
            let modifiers = segment.children.get(1)?;
            let is_bytes = modifiers
                .children
                .iter()
                .any(|modifier| modifier.kind.as_str() == "modifier_bytes");
            let size_node = modifiers
                .children
                .iter()
                .find(|modifier| modifier.kind.as_str() == "size_modifier")
                .and_then(|modifier| modifier.children.first());
            let (value, kind) = if is_bytes {
                let value = self.check_expr(value_node, Some(bytes_ty), owner, scopes)?;
                let size = if let Some(size) = size_node {
                    Some(self.check_expr(size, Some(usize_ty), owner, scopes)?)
                } else {
                    None
                };
                if let (
                    Some(actual),
                    Some(TypedExpr {
                        kind: TypedExprKind::Integer(expected),
                        ..
                    }),
                ) = (known_bytes_length(&value), size.as_ref())
                    && actual != *expected as u128
                {
                    self.diagnostics.push(Diagnostic::error(
                        "E2156",
                        value.span,
                        "statically known bytes value does not match its bitstring segment size",
                    ));
                    return None;
                }
                (value, TypedBitstringSegmentKind::Bytes { size })
            } else {
                let value = self.check_expr(value_node, None, owner, scopes)?;
                if !is_integer_type(&self.types[value.ty.0 as usize]) {
                    self.diagnostics.push(Diagnostic::error(
                        "E2154",
                        value.span,
                        "integer bitstring segments require an integer operand",
                    ));
                    return None;
                }
                let width = size_node
                    .and_then(integer_value)
                    .and_then(|width| u8::try_from(width).ok())?;
                let signed = modifiers
                    .children
                    .iter()
                    .any(|modifier| modifier.kind.as_str() == "modifier_signed");
                let byte_order = if modifiers
                    .children
                    .iter()
                    .any(|modifier| modifier.kind.as_str() == "modifier_little")
                {
                    BitstringByteOrder::Little
                } else if modifiers
                    .children
                    .iter()
                    .any(|modifier| modifier.kind.as_str() == "modifier_native")
                {
                    BitstringByteOrder::Native
                } else {
                    BitstringByteOrder::Big
                };
                if let TypedExprKind::Integer(literal) = &value.kind {
                    let maximum = if signed {
                        (1_i128 << (width - 1)) - 1
                    } else if width == 64 {
                        u64::MAX as i128
                    } else {
                        (1_i128 << width) - 1
                    };
                    if *literal > maximum {
                        self.diagnostics.push(Diagnostic::error(
                            "E2155",
                            value.span,
                            "integer literal does not fit its bitstring segment",
                        ));
                        return None;
                    }
                }
                (
                    value,
                    TypedBitstringSegmentKind::Integer {
                        signed,
                        byte_order,
                        width,
                    },
                )
            };
            segments.push(TypedBitstringSegment { value, kind });
        }
        Some(TypedExpr {
            kind: TypedExprKind::Bitstring(segments),
            ty: bytes_ty,
            span: node.span,
        })
    }

    fn check_rune(&mut self, node: &Node) -> Option<TypedExpr> {
        let Some(Value::Rune { decoded, .. }) = &node.value else {
            return None;
        };
        Some(TypedExpr {
            kind: TypedExprKind::Rune(*decoded),
            ty: self.intern(Type::Rune),
            span: node.span,
        })
    }

    fn check_name(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &[BTreeMap<String, Local>],
    ) -> Option<TypedExpr> {
        let local_name = unqualified_name(node);
        if let Some(local) = local_name.as_ref().and_then(|name| lookup(scopes, name)) {
            return Some(TypedExpr {
                kind: TypedExprKind::Local(local.symbol),
                ty: local.ty,
                span: node.span,
            });
        }
        if let Some(reference) = self.check_function_reference(node, expected, owner) {
            return Some(reference);
        }
        let name = node.children.iter().map(text).collect::<Vec<_>>().join(".");
        self.diagnostics.push(Diagnostic::error(
            "E2107",
            node.span,
            format!("unknown value `{name}`"),
        ));
        None
    }

    fn check_function_reference(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
    ) -> Option<TypedExpr> {
        let name = self.call_name(node, owner)?;
        let function = *self.functions_by_name.get(&name)?;
        let called = self
            .program
            .functions
            .iter()
            .find(|candidate| candidate.id == function)?;
        if called.visibility == Visibility::Private
            && called.module_name != self.owner_module(owner)?
        {
            self.diagnostics.push(Diagnostic::error(
                "E2138",
                node.span,
                format!("function `{name}` is private"),
            ));
            return None;
        }
        let signature = self.signatures[&function].clone();
        let mut substitutions = BTreeMap::new();
        if let Some(expected) = expected {
            let Type::Function { parameters, result } = self.types[expected.0 as usize].clone()
            else {
                let found = self.intern(Type::Function {
                    parameters: signature.parameters.clone(),
                    result: signature.result,
                });
                self.type_mismatch(node.span, expected, found);
                return None;
            };
            let declared = self.intern(Type::Function {
                parameters: signature.parameters.clone(),
                result: signature.result,
            });
            let expected_function = self.intern(Type::Function { parameters, result });
            if !unify_types(&self.types, declared, expected_function, &mut substitutions) {
                self.type_mismatch(node.span, expected_function, declared);
                return None;
            }
        }
        if let Some(missing) = signature
            .type_parameters
            .iter()
            .find(|parameter| !substitutions.contains_key(parameter))
        {
            self.diagnostics.push(Diagnostic::error(
                "E2112",
                node.span,
                format!(
                    "cannot infer generic type parameter `{}` for function value",
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
        let parameters = signature
            .parameters
            .iter()
            .map(|parameter| self.apply_substitutions(*parameter, &substitutions))
            .collect();
        let result = self.apply_substitutions(signature.result, &substitutions);
        let ty = self.intern(Type::Function { parameters, result });
        Some(TypedExpr {
            kind: TypedExprKind::FunctionRef {
                function,
                substitutions: substitutions.into_iter().collect(),
            },
            ty,
            span: node.span,
        })
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
        let numeric = is_integer_type(&self.types[left.ty.0 as usize])
            || matches!(self.types[left.ty.0 as usize], Type::F32 | Type::F64);
        if !numeric {
            self.diagnostics.push(Diagnostic::error(
                "E2108",
                node.span,
                "arithmetic requires matching integer or floating operands",
            ));
            return None;
        }
        let right = self.check_expr(&node.children[1], Some(left.ty), owner, scopes)?;
        let ty = right.ty;
        if is_integer_type(&self.types[ty.0 as usize])
            && self.reject_known_arithmetic_failure(operator, &left, &right, ty, node.span)
        {
            return None;
        }
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

    fn check_concat(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let left = self.check_expr(&node.children[0], expected, owner, scopes)?;
        if !self.type_satisfies(left.ty, "Concat", owner) {
            self.diagnostics.push(Diagnostic::error(
                "E2160",
                node.span,
                format!(
                    "type `{}` does not implement `Concat`",
                    self.type_name(left.ty)
                ),
            ));
            return None;
        }
        let right = self.check_expr(&node.children[1], Some(left.ty), owner, scopes)?;
        let concat_ty = left.ty;
        if let Some(call) = self.explicit_protocol_call(
            concat_ty,
            "Concat",
            "concat",
            vec![left.clone(), right.clone()],
            node.span,
            owner,
        ) {
            if call.ty != concat_ty {
                self.type_mismatch(node.span, concat_ty, call.ty);
                return None;
            }
            return Some(call);
        }
        let buffer_kind = match self.types[concat_ty.0 as usize] {
            Type::Bytes => Some(BufferAppendKind::Bytes),
            _ => None,
        };
        if let Some(kind) = buffer_kind {
            let buffer_ty = self.intern(Type::Buffer);
            let buffer = TypedExpr {
                kind: TypedExprKind::BufferNew,
                ty: buffer_ty,
                span: node.span,
            };
            let buffer = TypedExpr {
                kind: TypedExprKind::BufferAppend {
                    buffer: Box::new(buffer),
                    value: Box::new(left),
                    kind,
                },
                ty: buffer_ty,
                span: node.span,
            };
            let buffer = TypedExpr {
                kind: TypedExprKind::BufferAppend {
                    buffer: Box::new(buffer),
                    value: Box::new(right),
                    kind,
                },
                ty: buffer_ty,
                span: node.span,
            };
            return Some(TypedExpr {
                kind: TypedExprKind::BufferToBytes(Box::new(buffer)),
                ty: concat_ty,
                span: node.span,
            });
        }
        Some(TypedExpr {
            ty: concat_ty,
            span: node.span,
            kind: TypedExprKind::Concat {
                left: Box::new(left),
                right: Box::new(right),
            },
        })
    }

    fn check_integer_unary(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let operator = match node.value.as_ref() {
            Some(Value::Text(value)) if value == "-" => IntegerUnaryOperator::Negate,
            Some(Value::Text(value)) if value == "~" => IntegerUnaryOperator::BitwiseNot,
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E2105",
                    node.span,
                    "this unary operator is not implemented in the current slice",
                ));
                return None;
            }
        };
        if matches!(operator, IntegerUnaryOperator::Negate)
            && node.children[0].kind.as_str() == "integer"
        {
            let ty = expected.unwrap_or(TypeId(1));
            let signed = matches!(
                self.types.get(ty.0 as usize),
                Some(Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Isize)
            );
            if !signed {
                self.diagnostics.push(Diagnostic::error(
                    "E2108",
                    node.span,
                    "unary negation requires a signed integer operand",
                ));
                return None;
            }
            let Some(Value::Integer { radix, digits, .. }) = &node.children[0].value else {
                return None;
            };
            let magnitude = u128::from_str_radix(digits, *radix).ok();
            let minimum = integer_bounds(&self.types[ty.0 as usize])
                .map(|(minimum, _)| minimum)
                .expect("signed integer type has bounds");
            let limit = minimum.unsigned_abs();
            if magnitude.is_none_or(|magnitude| magnitude > limit) {
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
            return Some(TypedExpr {
                kind: TypedExprKind::Integer(-(magnitude.expect("range checked") as i128)),
                ty,
                span: node.span,
            });
        }
        let operand = self.check_expr(&node.children[0], expected, owner, scopes)?;
        let operand_ty = self.types.get(operand.ty.0 as usize)?;
        if matches!(operator, IntegerUnaryOperator::Negate)
            && matches!(operand_ty, Type::F32 | Type::F64)
        {
            let ty = operand.ty;
            return Some(TypedExpr {
                kind: TypedExprKind::FloatNegate(Box::new(operand)),
                ty,
                span: node.span,
            });
        }
        if !is_integer_type(operand_ty) {
            self.diagnostics.push(Diagnostic::error(
                "E2108",
                node.span,
                "integer unary operators require an integer operand",
            ));
            return None;
        }
        if matches!(operator, IntegerUnaryOperator::Negate)
            && !matches!(
                operand_ty,
                Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Isize
            )
        {
            self.diagnostics.push(Diagnostic::error(
                "E2108",
                node.span,
                "unary negation requires a signed integer operand",
            ));
            return None;
        }
        let ty = operand.ty;
        if matches!(operator, IntegerUnaryOperator::Negate)
            && self.constant_integer_value(&operand).is_some_and(|value| {
                integer_bounds(&self.types[ty.0 as usize])
                    .is_some_and(|(minimum, _)| value == minimum)
            })
        {
            self.diagnostics.push(Diagnostic::error(
                "E2106",
                node.span,
                format!("compile-time integer overflow for `{}`", self.type_name(ty)),
            ));
            return None;
        }
        Some(TypedExpr {
            kind: TypedExprKind::IntegerUnary {
                operator,
                operand: Box::new(operand),
            },
            ty,
            span: node.span,
        })
    }

    fn check_integer_binary(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let operator = match node.value.as_ref() {
            Some(Value::Text(value)) if value == "&" => IntegerBinaryOperator::BitwiseAnd,
            Some(Value::Text(value)) if value == "|" => IntegerBinaryOperator::BitwiseOr,
            Some(Value::Text(value)) if value == "^" => IntegerBinaryOperator::BitwiseXor,
            Some(Value::Text(value)) if value == "<<" => IntegerBinaryOperator::ShiftLeft,
            Some(Value::Text(value)) if value == ">>" => IntegerBinaryOperator::ShiftRight,
            _ => return None,
        };
        let left = self.check_expr(&node.children[0], expected, owner, scopes)?;
        if !is_integer_type(&self.types[left.ty.0 as usize]) {
            self.diagnostics.push(Diagnostic::error(
                "E2108",
                node.span,
                "bitwise and shift operators require an integer left operand",
            ));
            return None;
        }
        let right_expected = if matches!(
            operator,
            IntegerBinaryOperator::ShiftLeft | IntegerBinaryOperator::ShiftRight
        ) {
            self.intern(Type::Usize)
        } else {
            left.ty
        };
        let right = self.check_expr(&node.children[1], Some(right_expected), owner, scopes)?;
        let ty = left.ty;
        if self.reject_known_shift_failure(operator, &left, &right, ty, node.span) {
            return None;
        }
        Some(TypedExpr {
            kind: TypedExprKind::IntegerBinary {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty,
            span: node.span,
        })
    }

    fn reject_known_arithmetic_failure(
        &mut self,
        operator: ArithmeticOperator,
        left: &TypedExpr,
        right: &TypedExpr,
        ty: TypeId,
        span: Span,
    ) -> bool {
        let right = self.constant_integer_value(right);
        if matches!(
            operator,
            ArithmeticOperator::Divide | ArithmeticOperator::Remainder
        ) && right == Some(0)
        {
            self.diagnostics.push(Diagnostic::error(
                "E2106",
                span,
                "compile-time division or remainder by zero",
            ));
            return true;
        }
        let (Some(left), Some(right)) = (self.constant_integer_value(left), right) else {
            return false;
        };
        if matches!(
            operator,
            ArithmeticOperator::Divide | ArithmeticOperator::Remainder
        ) && right == -1
            && integer_bounds(&self.types[ty.0 as usize])
                .is_some_and(|(minimum, _)| left == minimum)
        {
            self.diagnostics.push(Diagnostic::error(
                "E2106",
                span,
                format!("compile-time integer overflow for `{}`", self.type_name(ty)),
            ));
            return true;
        }
        let result = match operator {
            ArithmeticOperator::Add => left.checked_add(right),
            ArithmeticOperator::Subtract => left.checked_sub(right),
            ArithmeticOperator::Multiply => left.checked_mul(right),
            ArithmeticOperator::Divide => left.checked_div(right),
            ArithmeticOperator::Remainder => left.checked_rem(right),
        };
        let in_range = result.is_some_and(|result| {
            integer_bounds(&self.types[ty.0 as usize])
                .is_some_and(|(minimum, maximum)| (minimum..=maximum).contains(&result))
        });
        if !in_range {
            self.diagnostics.push(Diagnostic::error(
                "E2106",
                span,
                format!("compile-time integer overflow for `{}`", self.type_name(ty)),
            ));
        }
        !in_range
    }

    fn reject_known_shift_failure(
        &mut self,
        operator: IntegerBinaryOperator,
        left: &TypedExpr,
        right: &TypedExpr,
        ty: TypeId,
        span: Span,
    ) -> bool {
        if !matches!(
            operator,
            IntegerBinaryOperator::ShiftLeft | IntegerBinaryOperator::ShiftRight
        ) {
            return false;
        }
        let width = integer_bit_width(&self.types[ty.0 as usize])
            .expect("shift operand has an integer type");
        let right = self.constant_integer_value(right);
        if right.is_some_and(|right| right >= i128::from(width)) {
            self.diagnostics.push(Diagnostic::error(
                "E2106",
                span,
                format!("compile-time shift count must be less than {width}"),
            ));
            return true;
        }
        if matches!(operator, IntegerBinaryOperator::ShiftLeft)
            && let (Some(left), Some(right)) = (self.constant_integer_value(left), right)
        {
            let result = left.checked_shl(right as u32);
            let in_range = result.is_some_and(|result| {
                integer_bounds(&self.types[ty.0 as usize])
                    .is_some_and(|(minimum, maximum)| (minimum..=maximum).contains(&result))
            });
            if !in_range {
                self.diagnostics.push(Diagnostic::error(
                    "E2106",
                    span,
                    format!("compile-time integer overflow for `{}`", self.type_name(ty)),
                ));
                return true;
            }
        }
        false
    }

    fn constant_integer_value(&self, expression: &TypedExpr) -> Option<i128> {
        match &expression.kind {
            TypedExprKind::Integer(value) => Some(*value),
            TypedExprKind::Ascription(value) => self.constant_integer_value(value),
            TypedExprKind::IntegerConvert(value) => self.constant_integer_value(value),
            TypedExprKind::IntegerUnary { operator, operand } => {
                let value = self.constant_integer_value(operand)?;
                match operator {
                    IntegerUnaryOperator::Negate => value.checked_neg(),
                    IntegerUnaryOperator::BitwiseNot => {
                        if matches!(
                            self.types.get(expression.ty.0 as usize),
                            Some(Type::U8 | Type::U16 | Type::U32 | Type::U64 | Type::Usize)
                        ) {
                            let width = integer_bit_width(&self.types[expression.ty.0 as usize])?;
                            let mask = (1_i128 << width) - 1;
                            Some(!value & mask)
                        } else {
                            Some(!value)
                        }
                    }
                }
            }
            TypedExprKind::Binary {
                operator,
                left,
                right,
            } => {
                let left = self.constant_integer_value(left)?;
                let right = self.constant_integer_value(right)?;
                match operator {
                    ArithmeticOperator::Add => left.checked_add(right),
                    ArithmeticOperator::Subtract => left.checked_sub(right),
                    ArithmeticOperator::Multiply => left.checked_mul(right),
                    ArithmeticOperator::Divide => left.checked_div(right),
                    ArithmeticOperator::Remainder => left.checked_rem(right),
                }
            }
            TypedExprKind::IntegerBinary {
                operator,
                left,
                right,
            } => {
                let left = self.constant_integer_value(left)?;
                let right = self.constant_integer_value(right)?;
                match operator {
                    IntegerBinaryOperator::BitwiseAnd => Some(left & right),
                    IntegerBinaryOperator::BitwiseOr => Some(left | right),
                    IntegerBinaryOperator::BitwiseXor => Some(left ^ right),
                    IntegerBinaryOperator::ShiftLeft => left.checked_shl(right.try_into().ok()?),
                    IntegerBinaryOperator::ShiftRight => left.checked_shr(right.try_into().ok()?),
                }
            }
            _ => None,
        }
    }

    fn constant_float_value(&self, expression: &TypedExpr) -> Option<f64> {
        let round = |value: f64, ty: TypeId| match self.types.get(ty.0 as usize) {
            Some(Type::F32) => f64::from(value as f32),
            Some(Type::F64) => value,
            _ => value,
        };
        match &expression.kind {
            TypedExprKind::Float(bits) => match self.types.get(expression.ty.0 as usize) {
                Some(Type::F32) => Some(f64::from(f32::from_bits(*bits as u32))),
                Some(Type::F64) => Some(f64::from_bits(*bits)),
                _ => None,
            },
            TypedExprKind::Ascription(value) => self.constant_float_value(value),
            TypedExprKind::FloatNegate(value) => {
                Some(round(-self.constant_float_value(value)?, expression.ty))
            }
            TypedExprKind::Binary {
                operator,
                left,
                right,
            } if is_float_type(&self.types[expression.ty.0 as usize]) => {
                let left = self.constant_float_value(left)?;
                let right = self.constant_float_value(right)?;
                let value = match operator {
                    ArithmeticOperator::Add => left + right,
                    ArithmeticOperator::Subtract => left - right,
                    ArithmeticOperator::Multiply => left * right,
                    ArithmeticOperator::Divide => left / right,
                    ArithmeticOperator::Remainder => left % right,
                };
                Some(round(value, expression.ty))
            }
            TypedExprKind::NumericConvert(value)
                if is_float_type(&self.types[expression.ty.0 as usize]) =>
            {
                let value = if is_integer_type(&self.types[value.ty.0 as usize]) {
                    self.constant_integer_value(value)? as f64
                } else {
                    self.constant_float_value(value)?
                };
                Some(round(value, expression.ty))
            }
            _ => None,
        }
    }

    fn check_comparison(
        &mut self,
        node: &Node,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let operator = match node.value.as_ref() {
            Some(Value::Text(value)) if value == "==" => ComparisonOperator::Equal,
            Some(Value::Text(value)) if value == "!=" => ComparisonOperator::NotEqual,
            Some(Value::Text(value)) if value == "<" => ComparisonOperator::Less,
            Some(Value::Text(value)) if value == "<=" => ComparisonOperator::LessEqual,
            Some(Value::Text(value)) if value == ">" => ComparisonOperator::Greater,
            Some(Value::Text(value)) if value == ">=" => ComparisonOperator::GreaterEqual,
            _ => return None,
        };
        let left = self.check_expr(&node.children[0], None, owner, scopes)?;
        let ordered = !matches!(
            operator,
            ComparisonOperator::Equal | ComparisonOperator::NotEqual
        );
        let supported = is_integer_type(&self.types[left.ty.0 as usize])
            || matches!(self.types[left.ty.0 as usize], Type::F32 | Type::F64)
            || matches!(self.types[left.ty.0 as usize], Type::Rune)
            || if ordered {
                self.type_satisfies(left.ty, "Ord", owner)
            } else {
                self.type_satisfies(left.ty, "Eq", owner)
            };
        if !supported {
            self.diagnostics.push(Diagnostic::error(
                "E2139",
                node.span,
                if ordered {
                    "ordered comparison requires matching numeric or rune operands"
                } else {
                    "equality requires matching comparable operands"
                },
            ));
            return None;
        }
        let right = self.check_expr(&node.children[1], Some(left.ty), owner, scopes)?;
        let protocol = if ordered { "Ord" } else { "Eq" };
        let method = if ordered { "compare" } else { "eq" };
        if let Some(call) = self.explicit_protocol_call(
            left.ty,
            protocol,
            method,
            vec![left.clone(), right.clone()],
            node.span,
            owner,
        ) {
            return self.adapt_explicit_comparison(operator, call, node.span);
        }
        Some(TypedExpr {
            kind: TypedExprKind::Comparison {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty: TypeId(2),
            span: node.span,
        })
    }

    fn explicit_protocol_call(
        &mut self,
        target: TypeId,
        protocol: &str,
        method: &str,
        arguments: Vec<TypedExpr>,
        span: Span,
        owner: DeclId,
    ) -> Option<TypedExpr> {
        let selected = self
            .program
            .implementations
            .iter()
            .find_map(|implementation| {
                if implementation.protocol != protocol {
                    return None;
                }
                let mut concrete_by_name = BTreeMap::new();
                if !implementation_target_matches(
                    &self.types,
                    target,
                    &implementation.target,
                    &mut concrete_by_name,
                ) {
                    return None;
                }
                Some((
                    implementation.clone(),
                    concrete_by_name,
                    implementation
                        .method_declarations
                        .iter()
                        .find(|(name, _)| name == method)
                        .map(|(_, declaration)| *declaration),
                ))
            })?;
        let (implementation, concrete_by_name, declaration) = selected;
        if !implementation.constraints.iter().all(|constraint| {
            concrete_by_name
                .get(&constraint.parameter)
                .is_some_and(|argument| self.type_satisfies(*argument, &constraint.protocol, owner))
        }) {
            return None;
        }
        let declaration = declaration?;
        let signature = self.signatures.get(&declaration)?.clone();
        let substitutions = signature
            .type_parameters
            .iter()
            .filter_map(|parameter| match &self.types[parameter.0 as usize] {
                Type::Parameter { name, .. } => concrete_by_name
                    .get(name)
                    .map(|concrete| (*parameter, *concrete)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        if signature.type_parameters.len() != substitutions.len()
            || signature.parameters.len() != arguments.len()
        {
            return None;
        }
        for (parameter, argument) in signature.parameters.iter().zip(&arguments) {
            let expected = self.apply_substitutions(*parameter, &substitutions);
            if expected != argument.ty {
                self.type_mismatch(argument.span, expected, argument.ty);
                return None;
            }
        }
        let result = self.apply_substitutions(signature.result, &substitutions);
        Some(TypedExpr {
            kind: TypedExprKind::Call {
                function: declaration,
                substitutions: substitutions.into_iter().collect(),
                arguments,
            },
            ty: result,
            span,
        })
    }

    fn adapt_explicit_comparison(
        &mut self,
        operator: ComparisonOperator,
        call: TypedExpr,
        span: Span,
    ) -> Option<TypedExpr> {
        if matches!(
            operator,
            ComparisonOperator::Equal | ComparisonOperator::NotEqual
        ) {
            if call.ty != TypeId(2) {
                self.type_mismatch(span, TypeId(2), call.ty);
                return None;
            }
            if matches!(operator, ComparisonOperator::Equal) {
                return Some(call);
            }
            return Some(TypedExpr {
                kind: TypedExprKind::Comparison {
                    operator: ComparisonOperator::Equal,
                    left: Box::new(call),
                    right: Box::new(TypedExpr {
                        kind: TypedExprKind::Boolean(false),
                        ty: TypeId(2),
                        span,
                    }),
                },
                ty: TypeId(2),
                span,
            });
        }

        let (atom, comparison) = match operator {
            ComparisonOperator::Less => ("less", ComparisonOperator::Equal),
            ComparisonOperator::LessEqual => ("greater", ComparisonOperator::NotEqual),
            ComparisonOperator::Greater => ("greater", ComparisonOperator::Equal),
            ComparisonOperator::GreaterEqual => ("less", ComparisonOperator::NotEqual),
            ComparisonOperator::Equal | ComparisonOperator::NotEqual => unreachable!(),
        };
        let atom_ty = self.intern(Type::Atom(atom.to_owned()));
        let Type::Union(members) = &self.types[call.ty.0 as usize] else {
            self.diagnostics.push(Diagnostic::error(
                "E2113",
                span,
                "`Ord.compare` must return `:less | :equal | :greater`",
            ));
            return None;
        };
        if !members.contains(&atom_ty) {
            self.diagnostics.push(Diagnostic::error(
                "E2113",
                span,
                "`Ord.compare` must return `:less | :equal | :greater`",
            ));
            return None;
        }
        let expected = TypedExpr {
            kind: TypedExprKind::UnionInject {
                member: atom_ty,
                value: Box::new(TypedExpr {
                    kind: TypedExprKind::Atom(atom.to_owned()),
                    ty: atom_ty,
                    span,
                }),
            },
            ty: call.ty,
            span,
        };
        Some(TypedExpr {
            kind: TypedExprKind::Comparison {
                operator: comparison,
                left: Box::new(call),
                right: Box::new(expected),
            },
            ty: TypeId(2),
            span,
        })
    }

    fn check_logical(
        &mut self,
        node: &Node,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let operator = match node.value.as_ref() {
            Some(Value::Text(value)) if value == "and" => LogicalOperator::And,
            Some(Value::Text(value)) if value == "or" => LogicalOperator::Or,
            _ => return None,
        };
        let left = self.check_expr(&node.children[0], Some(TypeId(2)), owner, scopes)?;
        let right = self.check_expr(&node.children[1], Some(TypeId(2)), owner, scopes)?;
        Some(TypedExpr {
            kind: TypedExprKind::Logical {
                operator,
                left: Box::new(left),
                right: Box::new(right),
            },
            ty: TypeId(2),
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
        self.check_call_with_input(node, None, node.span, expected, owner, scopes)
    }

    fn check_postfix(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let call = node
            .children
            .iter()
            .skip(1)
            .position(|part| part.kind.as_str() == "call_arguments")
            .map(|index| index + 1);
        let (mut value, suffix_start) = if let Some(call) = call {
            let final_expected = (call + 1 == node.children.len())
                .then_some(expected)
                .flatten();
            let callee = node.children.first()?;
            let qualified = callee.kind.as_str() == "qualified_value" && callee.children.len() > 1;
            let shadowed = !qualified
                && unqualified_name(callee)
                    .as_ref()
                    .is_some_and(|name| lookup(scopes, name).is_some());
            if shadowed {
                let callee = self.check_expr(callee, None, owner, scopes)?;
                (
                    self.check_indirect_call(
                        callee,
                        &node.children[call].children,
                        node.span,
                        final_expected,
                        owner,
                        scopes,
                    )?,
                    call + 1,
                )
            } else {
                (
                    self.check_call(node, final_expected, owner, scopes)?,
                    call + 1,
                )
            }
        } else {
            let primary_expected = (node.children.len() == 1).then_some(expected).flatten();
            (
                self.check_expr(node.children.first()?, primary_expected, owner, scopes)?,
                1,
            )
        };
        for access in &node.children[suffix_start..] {
            if access.kind.as_str() == "call_arguments" {
                let final_expected = std::ptr::eq(access, node.children.last()?)
                    .then_some(expected)
                    .flatten();
                value = self.check_indirect_call(
                    value,
                    &access.children,
                    access.span,
                    final_expected,
                    owner,
                    scopes,
                )?;
                continue;
            }
            if access.kind.as_str() == "index_access" {
                let (item, length) = match self.types[value.ty.0 as usize] {
                    Type::Array { item, length } => (item, Some(length)),
                    Type::Slice(item) => (item, None),
                    Type::Bytes => (self.intern(Type::U8), None),
                    Type::Bits => (self.intern(Type::Bool), None),
                    Type::String => {
                        self.diagnostics.push(Diagnostic::error(
                            "E2150",
                            access.span,
                            "`string` does not support integer indexing",
                        ));
                        return None;
                    }
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E2150",
                            access.span,
                            format!(
                                "`{}` does not support integer indexing",
                                self.type_name(value.ty)
                            ),
                        ));
                        return None;
                    }
                };
                let usize_ty = self.intern(Type::Usize);
                let index =
                    self.check_expr(access.children.first()?, Some(usize_ty), owner, scopes)?;
                value = TypedExpr {
                    kind: TypedExprKind::Index {
                        value: Box::new(value),
                        index: Box::new(index),
                        length,
                    },
                    ty: item,
                    span: access.span,
                };
                continue;
            }
            if access.kind.as_str() != "field_access" {
                return None;
            }
            let Type::Struct {
                declaration,
                arguments,
            } = self.types[value.ty.0 as usize].clone()
            else {
                self.diagnostics.push(Diagnostic::error(
                    "E2143",
                    access.span,
                    "field access requires a struct value",
                ));
                return None;
            };
            let structure = self.structs.get(&declaration)?.clone();
            let name = access.children.first().map(text)?;
            let Some(field) = structure
                .fields
                .iter()
                .position(|candidate| candidate.name == name)
            else {
                self.diagnostics.push(Diagnostic::error(
                    "E2144",
                    access.span,
                    format!("unknown field `{name}` on `{}`", structure.name),
                ));
                return None;
            };
            let substitutions = structure
                .parameters
                .iter()
                .cloned()
                .zip(arguments)
                .collect::<BTreeMap<_, _>>();
            let ty = self.resolve_type(&structure.fields[field].ty, &substitutions)?;
            value = TypedExpr {
                kind: TypedExprKind::StructProject {
                    value: Box::new(value),
                    declaration,
                    field,
                },
                ty,
                span: access.span,
            };
        }
        if let Some(expected) = expected
            && value.ty != expected
        {
            self.type_mismatch(node.span, expected, value.ty);
            return None;
        }
        Some(value)
    }

    fn check_indirect_call(
        &mut self,
        callee: TypedExpr,
        argument_nodes: &[Node],
        span: Span,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let Type::Function { parameters, result } = self.types[callee.ty.0 as usize].clone() else {
            self.diagnostics.push(Diagnostic::error(
                "E2109",
                callee.span,
                format!("`{}` is not callable", self.type_name(callee.ty)),
            ));
            return None;
        };
        if argument_nodes.len() != parameters.len() {
            self.diagnostics.push(Diagnostic::error(
                "E2111",
                span,
                format!(
                    "function value expects {} arguments but received {}",
                    parameters.len(),
                    argument_nodes.len()
                ),
            ));
            return None;
        }
        let arguments = argument_nodes
            .iter()
            .zip(parameters)
            .map(|(argument, parameter)| self.check_expr(argument, Some(parameter), owner, scopes))
            .collect::<Option<Vec<_>>>()?;
        if let Some(expected) = expected
            && expected != result
        {
            self.type_mismatch(span, expected, result);
            return None;
        }
        Some(TypedExpr {
            kind: TypedExprKind::IndirectCall {
                callee: Box::new(callee),
                arguments,
            },
            ty: result,
            span,
        })
    }

    fn check_struct_literal(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let path = node.children.first()?;
        let written = path.children.iter().map(text).collect::<Vec<_>>().join(".");
        let owner_module = self
            .program
            .functions
            .iter()
            .find(|function| function.id == owner)
            .map(|function| function.module_name.as_str())?;
        let structure = self
            .structs
            .values()
            .find(|candidate| {
                written == format!("{}.{}", candidate.module_name, candidate.name)
                    || (written == candidate.name && candidate.module_name == owner_module)
            })
            .cloned();
        let Some(structure) = structure else {
            self.diagnostics.push(Diagnostic::error(
                "E2146",
                path.span,
                format!("unknown struct `{written}`"),
            ));
            return None;
        };
        let parameters = structure
            .parameters
            .iter()
            .map(|name| {
                let ty = self.intern(Type::Parameter {
                    owner: structure.id,
                    name: name.clone(),
                });
                (name.clone(), ty)
            })
            .collect::<BTreeMap<_, _>>();
        let mut substitutions = BTreeMap::new();
        if let Some(expected) = expected
            && let Type::Struct {
                declaration,
                arguments,
            } = self.types[expected.0 as usize].clone()
            && declaration == structure.id
        {
            for (parameter, argument) in parameters.values().zip(arguments) {
                substitutions.insert(*parameter, argument);
            }
        }
        let mut seen = BTreeMap::<String, Span>::new();
        let mut fields = Vec::new();
        for field_node in &node.children[1..] {
            let name_node = field_node.children.first()?;
            let name = text(name_node);
            if let Some(previous) = seen.insert(name.clone(), name_node.span) {
                self.diagnostics.push(
                    Diagnostic::error(
                        "E2147",
                        name_node.span,
                        format!("duplicate struct field `{name}`"),
                    )
                    .with_label(previous, "first field here"),
                );
                return None;
            }
            let Some(index) = structure.fields.iter().position(|field| field.name == name) else {
                self.diagnostics.push(Diagnostic::error(
                    "E2144",
                    name_node.span,
                    format!("unknown field `{name}` on `{}`", structure.name),
                ));
                return None;
            };
            let declared = self.resolve_type(&structure.fields[index].ty, &parameters)?;
            let field_expected = self.apply_substitutions(declared, &substitutions);
            let expected_is_parameter = matches!(
                self.types[field_expected.0 as usize],
                Type::Parameter { .. }
            );
            let value = self.check_expr(
                field_node.children.get(1)?,
                (!expected_is_parameter).then_some(field_expected),
                owner,
                scopes,
            )?;
            if !unify_types(&self.types, declared, value.ty, &mut substitutions) {
                self.type_mismatch(value.span, field_expected, value.ty);
                return None;
            }
            fields.push((index, value));
        }
        let missing = structure
            .fields
            .iter()
            .filter(|field| !seen.contains_key(&field.name))
            .map(|field| field.name.clone())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            self.diagnostics.push(Diagnostic::error(
                "E2148",
                node.span,
                format!("missing struct fields: {}", missing.join(", ")),
            ));
            return None;
        }
        let mut arguments = Vec::new();
        for parameter in parameters.values() {
            let argument = self.apply_substitutions(*parameter, &substitutions);
            if matches!(self.types[argument.0 as usize], Type::Parameter { .. }) {
                self.diagnostics.push(Diagnostic::error(
                    "E2149",
                    node.span,
                    format!("cannot infer all type arguments for `{}`", structure.name),
                ));
                return None;
            }
            arguments.push(argument);
        }
        let ty = self.intern(Type::Struct {
            declaration: structure.id,
            arguments,
        });
        Some(TypedExpr {
            kind: TypedExprKind::Struct {
                declaration: structure.id,
                field_count: structure.fields.len(),
                fields,
            },
            ty,
            span: node.span,
        })
    }

    fn check_pipeline(
        &mut self,
        node: &Node,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let [input, call] = node.children.as_slice() else {
            return None;
        };
        if call.kind.as_str() != "postfix_expr" {
            self.diagnostics.push(Diagnostic::error(
                "E2140",
                call.span,
                "the right side of `|>` must be a statically resolvable call",
            ));
            return None;
        }
        self.check_call_with_input(call, Some(input), node.span, expected, owner, scopes)
    }

    fn check_call_with_input<'b>(
        &mut self,
        node: &'b Node,
        input: Option<&'b Node>,
        span: Span,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let callee = node.children.first()?;
        let written = (callee.kind.as_str() == "qualified_value").then(|| {
            callee
                .children
                .iter()
                .map(text)
                .collect::<Vec<_>>()
                .join(".")
        });
        let conversion_name = unqualified_name(callee);
        if conversion_name.as_deref() == Some("rune") {
            let arguments_node = node
                .children
                .iter()
                .find(|node| node.kind.as_str() == "call_arguments")?;
            let arguments = input
                .into_iter()
                .chain(arguments_node.children.iter())
                .collect::<Vec<_>>();
            if arguments.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E2114",
                    span,
                    "rune conversion expects exactly one argument",
                ));
                return None;
            }
            let value = self.check_expr(arguments[0], None, owner, scopes)?;
            if !is_integer_type(&self.types[value.ty.0 as usize]) {
                self.diagnostics.push(Diagnostic::error(
                    "E2108",
                    value.span,
                    "rune conversion requires an integer operand",
                ));
                return None;
            }
            if self
                .constant_integer_value(&value)
                .is_some_and(|value| !is_unicode_scalar(value))
            {
                self.diagnostics.push(Diagnostic::error(
                    "E2106",
                    span,
                    "compile-time invalid conversion to `rune`",
                ));
                return None;
            }
            return Some(TypedExpr {
                kind: TypedExprKind::IntegerConvert(Box::new(value)),
                ty: self.intern(Type::Rune),
                span,
            });
        }
        if let Some(target_type) = conversion_name.as_deref().and_then(integer_type_from_name) {
            let target_name = conversion_name.as_deref().expect("conversion name exists");
            let arguments_node = node
                .children
                .iter()
                .find(|node| node.kind.as_str() == "call_arguments")?;
            let arguments = input
                .into_iter()
                .chain(arguments_node.children.iter())
                .collect::<Vec<_>>();
            if arguments.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E2114",
                    span,
                    format!("integer conversion `{target_name}` expects exactly one argument"),
                ));
                return None;
            }
            let target = self.intern(target_type);
            let value = self.check_expr(arguments[0], None, owner, scopes)?;
            let source_is_integer = is_integer_type(&self.types[value.ty.0 as usize]);
            let source_is_float = is_float_type(&self.types[value.ty.0 as usize]);
            if !source_is_integer && !source_is_float {
                self.diagnostics.push(Diagnostic::error(
                    "E2108",
                    value.span,
                    "integer conversion requires an integer or floating operand",
                ));
                return None;
            }
            let statically_invalid = if source_is_integer {
                self.constant_integer_value(&value).is_some_and(|value| {
                    integer_bounds(&self.types[target.0 as usize])
                        .is_some_and(|(minimum, maximum)| !(minimum..=maximum).contains(&value))
                })
            } else {
                self.constant_float_value(&value)
                    .is_some_and(|value| !float_fits_integer(value, &self.types[target.0 as usize]))
            };
            if statically_invalid {
                self.diagnostics.push(Diagnostic::error(
                    "E2106",
                    span,
                    format!(
                        "compile-time invalid conversion to `{}`",
                        self.type_name(target)
                    ),
                ));
                return None;
            }
            return Some(TypedExpr {
                kind: if source_is_integer {
                    TypedExprKind::IntegerConvert(Box::new(value))
                } else {
                    TypedExprKind::NumericConvert(Box::new(value))
                },
                ty: target,
                span,
            });
        }
        if let Some(target_type) = conversion_name.as_deref().and_then(float_type_from_name) {
            let target_name = conversion_name.as_deref().expect("conversion name exists");
            let arguments_node = node
                .children
                .iter()
                .find(|node| node.kind.as_str() == "call_arguments")?;
            let arguments = input
                .into_iter()
                .chain(arguments_node.children.iter())
                .collect::<Vec<_>>();
            if arguments.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E2114",
                    span,
                    format!("float conversion `{target_name}` expects exactly one argument"),
                ));
                return None;
            }
            let value = self.check_expr(arguments[0], None, owner, scopes)?;
            if !is_integer_type(&self.types[value.ty.0 as usize])
                && !is_float_type(&self.types[value.ty.0 as usize])
            {
                self.diagnostics.push(Diagnostic::error(
                    "E2108",
                    value.span,
                    "float conversion requires an integer or floating operand",
                ));
                return None;
            }
            return Some(TypedExpr {
                kind: TypedExprKind::NumericConvert(Box::new(value)),
                ty: self.intern(target_type),
                span,
            });
        }
        if let Some((target_type, operator)) = written.as_deref().and_then(wrapping_intrinsic) {
            let arguments_node = node
                .children
                .iter()
                .find(|node| node.kind.as_str() == "call_arguments")?;
            let argument_nodes = input
                .into_iter()
                .chain(arguments_node.children.iter())
                .collect::<Vec<_>>();
            let required = if matches!(operator, WrappingIntegerOperator::Negate) {
                1
            } else {
                2
            };
            if argument_nodes.len() != required {
                self.diagnostics.push(Diagnostic::error(
                    "E2111",
                    span,
                    format!(
                        "function `{}` expects {required} arguments but received {}",
                        written.as_deref().expect("wrapping intrinsic is qualified"),
                        argument_nodes.len()
                    ),
                ));
                return None;
            }
            let target = self.intern(target_type);
            let left = self.check_expr(argument_nodes[0], Some(target), owner, scopes)?;
            if left.ty != target {
                self.type_mismatch(left.span, target, left.ty);
                return None;
            }
            let right = if required == 2 {
                let expected = if matches!(
                    operator,
                    WrappingIntegerOperator::ShiftLeft | WrappingIntegerOperator::ShiftRight
                ) {
                    self.intern(Type::Usize)
                } else {
                    target
                };
                let value = self.check_expr(argument_nodes[1], Some(expected), owner, scopes)?;
                if value.ty != expected {
                    self.type_mismatch(value.span, expected, value.ty);
                    return None;
                }
                Some(Box::new(value))
            } else {
                None
            };
            return Some(TypedExpr {
                kind: TypedExprKind::WrappingInteger {
                    operator,
                    left: Box::new(left),
                    right,
                },
                ty: target,
                span,
            });
        }
        if written.as_deref() == Some("Show.show") {
            let arguments_node = node
                .children
                .iter()
                .find(|node| node.kind.as_str() == "call_arguments")?;
            let argument_nodes = input
                .into_iter()
                .chain(arguments_node.children.iter())
                .collect::<Vec<_>>();
            if argument_nodes.len() != 1 {
                self.diagnostics.push(Diagnostic::error(
                    "E2111",
                    span,
                    format!(
                        "function `Show.show` expects 1 argument but received {}",
                        argument_nodes.len()
                    ),
                ));
                return None;
            }
            let value = self.check_expr(argument_nodes[0], None, owner, scopes)?;
            if !self.type_satisfies(value.ty, "Show", owner) {
                self.diagnostics.push(Diagnostic::error(
                    "E2120",
                    value.span,
                    format!(
                        "type `{}` does not satisfy `Show`",
                        self.type_name(value.ty)
                    ),
                ));
                return None;
            }
            return self.show_value(value, owner);
        }
        if written.as_deref().is_some_and(|name| {
            matches!(
                name,
                "File.open_read"
                    | "File.create"
                    | "File.append"
                    | "File.close"
                    | "Reader.read"
                    | "Writer.write"
                    | "Writer.flush"
                    | "IO.stdin"
                    | "IO.stdout"
                    | "IO.stderr"
                    | "IO.error_kind"
                    | "IO.error_operation"
                    | "IO.error_code"
                    | "File.error_kind"
                    | "File.error_operation"
                    | "File.error_code"
                    | "Process.arguments"
                    | "Process.get_env"
                    | "IO.print"
                    | "IO.println"
                    | "IO.report"
            )
        }) {
            let arguments_node = node
                .children
                .iter()
                .find(|node| node.kind.as_str() == "call_arguments")?;
            let arguments = input
                .into_iter()
                .chain(arguments_node.children.iter())
                .collect::<Vec<_>>();
            return self.check_standard_intrinsic(
                written.as_deref()?,
                &arguments,
                span,
                owner,
                scopes,
            );
        }
        if written.as_deref().is_some_and(|name| {
            matches!(
                name,
                "Array.length"
                    | "List.reverse"
                    | "Map.put"
                    | "Map.remove"
                    | "Map.fetch"
                    | "Map.new"
                    | "Map.size"
                    | "Enum.count"
                    | "Enum.at"
                    | "Enum.to_list"
                    | "Enum.each"
                    | "Enum.any"
                    | "Enum.all"
                    | "Enum.reduce"
                    | "Enum.filter"
                    | "Enum.map"
                    | "Slice.from_array"
                    | "Slice.subslice"
                    | "Slice.copy"
                    | "Slice.length"
                    | "String.byte_size"
                    | "String.length"
                    | "String.empty"
                    | "String.contains"
                    | "String.split"
                    | "String.bytes"
                    | "String.codepoints"
                    | "String.graphemes"
                    | "String.codepoint_view"
                    | "String.grapheme_view"
                    | "String.from_bytes"
                    | "String.utf8_error_offset"
                    | "Rune.to_string"
                    | "Buffer.new"
                    | "Buffer.byte_size"
                    | "Buffer.append_byte"
                    | "Buffer.append_bytes"
                    | "Buffer.append_string"
                    | "Buffer.to_bytes"
                    | "Buffer.to_string"
                    | "Bits.bit_size"
                    | "Bits.slice"
                    | "Bits.to_bytes"
                    | "Bytes.byte_size"
                    | "Bytes.slice"
                    | "Bytes.from_list"
                    | "Bytes.to_list"
                    | "Bytes.to_bits"
            )
        }) {
            let arguments_node = node
                .children
                .iter()
                .find(|node| node.kind.as_str() == "call_arguments")?;
            let arguments = input
                .into_iter()
                .chain(arguments_node.children.iter())
                .collect::<Vec<_>>();
            return self.check_collection_intrinsic(
                written.as_deref()?,
                &arguments,
                span,
                expected,
                owner,
                scopes,
            );
        }
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
        let argument_nodes = input
            .into_iter()
            .chain(arguments_node.children.iter())
            .collect::<Vec<_>>();
        if argument_nodes.len() != signature.parameters.len() {
            self.diagnostics.push(Diagnostic::error(
                "E2111",
                span,
                format!(
                    "function `{name}` expects {} arguments but received {}",
                    signature.parameters.len(),
                    argument_nodes.len()
                ),
            ));
            return None;
        }
        let mut substitutions = BTreeMap::new();
        if let Some(expected) = expected {
            let _ = unify_types(&self.types, signature.result, expected, &mut substitutions);
        }
        let mut arguments = Vec::new();
        for (argument_node, parameter) in argument_nodes.into_iter().zip(&signature.parameters) {
            let parameter_expected = self.apply_substitutions(*parameter, &substitutions);
            let argument = self.check_expr(
                argument_node,
                (!is_parameter(&self.types, parameter_expected)).then_some(parameter_expected),
                owner,
                scopes,
            )?;
            if parameter_expected != argument.ty
                && !unify_types(&self.types, *parameter, argument.ty, &mut substitutions)
            {
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
                span,
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
                    span,
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
            span,
        })
    }

    fn check_standard_intrinsic(
        &mut self,
        name: &str,
        argument_nodes: &[&Node],
        span: Span,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        let (operation, expected_arguments) = match name {
            "File.open_read" => (StandardOperation::FileOpenRead, 1),
            "File.create" => (StandardOperation::FileCreate, 1),
            "File.append" => (StandardOperation::FileAppend, 1),
            "File.close" => (StandardOperation::FileClose, 1),
            "Reader.read" => (StandardOperation::ReaderRead, 2),
            "Writer.write" => (StandardOperation::WriterWrite, 2),
            "Writer.flush" => (StandardOperation::WriterFlush, 1),
            "IO.stdin" => (StandardOperation::IoStdin, 0),
            "IO.stdout" => (StandardOperation::IoStdout, 0),
            "IO.stderr" => (StandardOperation::IoStderr, 0),
            "IO.error_kind" | "File.error_kind" => (StandardOperation::ErrorKind, 1),
            "IO.error_operation" | "File.error_operation" => (StandardOperation::ErrorOperation, 1),
            "IO.error_code" | "File.error_code" => (StandardOperation::ErrorCode, 1),
            "Process.arguments" => (StandardOperation::ProcessArguments, 0),
            "Process.get_env" => (StandardOperation::ProcessGetEnv, 1),
            "IO.print" => (StandardOperation::IoPrint, 1),
            "IO.println" => (StandardOperation::IoPrintln, 1),
            "IO.report" => (StandardOperation::IoReport, 1),
            _ => return None,
        };
        if argument_nodes.len() != expected_arguments {
            self.diagnostics.push(Diagnostic::error(
                "E2111",
                span,
                format!(
                    "function `{name}` expects {expected_arguments} arguments but received {}",
                    argument_nodes.len()
                ),
            ));
            return None;
        }
        let string_ty = self.intern(Type::String);
        let usize_ty = self.intern(Type::Usize);
        let bytes_ty = self.intern(Type::Bytes);
        let unit_ty = self.intern(Type::Unit);
        let i64_ty = self.intern(Type::I64);
        let reader_ty = self.intern(Type::Opaque(OpaqueType::FileReader));
        let writer_ty = self.intern(Type::Opaque(OpaqueType::FileWriter));
        let file_error_ty = self.intern(Type::Opaque(OpaqueType::FileError));
        let stdin_ty = self.intern(Type::Opaque(OpaqueType::IoStdin));
        let stdout_ty = self.intern(Type::Opaque(OpaqueType::IoStdout));
        let stderr_ty = self.intern(Type::Opaque(OpaqueType::IoStderr));
        let io_error_ty = self.intern(Type::Opaque(OpaqueType::IoError));
        let mut arguments = Vec::new();
        for (index, node) in argument_nodes.iter().enumerate() {
            let expected = match operation {
                StandardOperation::FileOpenRead
                | StandardOperation::FileCreate
                | StandardOperation::FileAppend => Some(string_ty),
                StandardOperation::ReaderRead if index == 1 => Some(usize_ty),
                StandardOperation::WriterWrite if index == 1 => Some(bytes_ty),
                StandardOperation::ProcessGetEnv => Some(string_ty),
                _ => None,
            };
            arguments.push(self.check_expr(node, expected, owner, scopes)?);
        }
        let result = match operation {
            StandardOperation::FileOpenRead
            | StandardOperation::FileCreate
            | StandardOperation::FileAppend => {
                let handle = if matches!(operation, StandardOperation::FileOpenRead) {
                    reader_ty
                } else {
                    writer_ty
                };
                self.tagged_result_type(handle, file_error_ty, span)?
            }
            StandardOperation::FileClose => {
                let argument = arguments[0].ty;
                if !matches!(
                    self.types.get(argument.0 as usize),
                    Some(Type::Opaque(
                        OpaqueType::FileReader | OpaqueType::FileWriter
                    ))
                ) {
                    self.diagnostics.push(Diagnostic::error(
                        "E2108",
                        arguments[0].span,
                        "File.close requires File.Reader or File.Writer",
                    ));
                    return None;
                }
                self.tagged_result_type(unit_ty, file_error_ty, span)?
            }
            StandardOperation::ReaderRead => {
                let error = match self.types.get(arguments[0].ty.0 as usize) {
                    Some(Type::Opaque(OpaqueType::FileReader)) => file_error_ty,
                    Some(Type::Opaque(OpaqueType::IoStdin)) => io_error_ty,
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E2108",
                            arguments[0].span,
                            "Reader.read requires a standard Reader handle",
                        ));
                        return None;
                    }
                };
                let ok = self.tagged_tuple_type("ok", bytes_ty);
                let eof = self.intern(Type::Atom("eof".to_owned()));
                let error = self.tagged_tuple_type("error", error);
                self.normalize_union(vec![ok, eof, error], span)?
            }
            StandardOperation::WriterWrite | StandardOperation::WriterFlush => {
                let error = match self.types.get(arguments[0].ty.0 as usize) {
                    Some(Type::Opaque(OpaqueType::FileWriter)) => file_error_ty,
                    Some(Type::Opaque(OpaqueType::IoStdout | OpaqueType::IoStderr)) => io_error_ty,
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E2108",
                            arguments[0].span,
                            "Writer operation requires a standard Writer handle",
                        ));
                        return None;
                    }
                };
                self.tagged_result_type(unit_ty, error, span)?
            }
            StandardOperation::IoStdin => stdin_ty,
            StandardOperation::IoStdout => stdout_ty,
            StandardOperation::IoStderr => stderr_ty,
            StandardOperation::ErrorKind
            | StandardOperation::ErrorOperation
            | StandardOperation::ErrorCode => {
                if !matches!(
                    self.types.get(arguments[0].ty.0 as usize),
                    Some(Type::Opaque(OpaqueType::FileError | OpaqueType::IoError))
                ) {
                    self.diagnostics.push(Diagnostic::error(
                        "E2108",
                        arguments[0].span,
                        "error inspection requires File.Error or IO.Error",
                    ));
                    return None;
                }
                match operation {
                    StandardOperation::ErrorKind => self.closed_atom_union(
                        &[
                            "not_found",
                            "permission_denied",
                            "already_exists",
                            "invalid_input",
                            "is_directory",
                            "not_directory",
                            "closed",
                            "broken_pipe",
                            "out_of_space",
                            "other",
                        ],
                        span,
                    )?,
                    StandardOperation::ErrorOperation => self.closed_atom_union(
                        &[
                            "open_read",
                            "create",
                            "append",
                            "read",
                            "write",
                            "flush",
                            "close",
                        ],
                        span,
                    )?,
                    StandardOperation::ErrorCode => {
                        let some = self.tagged_tuple_type("some", i64_ty);
                        let none = self.intern(Type::Atom("none".to_owned()));
                        self.normalize_union(vec![some, none], span)?
                    }
                    _ => unreachable!(),
                }
            }
            StandardOperation::ProcessArguments => {
                let arguments = self.intern(Type::List(string_ty));
                let ok = self.tagged_tuple_type("ok", arguments);
                let invalid = self.tagged_tuple_type("invalid_text", usize_ty);
                let error = self.tagged_tuple_type("error", invalid);
                self.normalize_union(vec![ok, error], span)?
            }
            StandardOperation::ProcessGetEnv => {
                let ok = self.tagged_tuple_type("ok", string_ty);
                let not_found = self.intern(Type::Atom("not_found".to_owned()));
                let invalid_name = self.intern(Type::Atom("invalid_name".to_owned()));
                let invalid_text = self.intern(Type::Atom("invalid_text".to_owned()));
                let reason = self.normalize_union(vec![invalid_name, invalid_text], span)?;
                let error = self.tagged_tuple_type("error", reason);
                self.normalize_union(vec![ok, not_found, error], span)?
            }
            StandardOperation::IoPrint
            | StandardOperation::IoPrintln
            | StandardOperation::IoReport => {
                if !self.type_satisfies(arguments[0].ty, "Show", owner) {
                    self.diagnostics.push(Diagnostic::error(
                        "E2120",
                        arguments[0].span,
                        format!(
                            "type `{}` does not satisfy `Show`",
                            self.type_name(arguments[0].ty)
                        ),
                    ));
                    return None;
                }
                if !matches!(
                    self.types.get(arguments[0].ty.0 as usize),
                    Some(Type::String | Type::Opaque(OpaqueType::FileError | OpaqueType::IoError))
                ) {
                    arguments[0] = self.show_value(arguments[0].clone(), owner)?;
                }
                unit_ty
            }
        };
        Some(TypedExpr {
            kind: TypedExprKind::StandardCall {
                operation,
                arguments,
            },
            ty: result,
            span,
        })
    }

    fn tagged_tuple_type(&mut self, tag: &str, value: TypeId) -> TypeId {
        let tag = self.intern(Type::Atom(tag.to_owned()));
        self.intern(Type::Tuple(vec![tag, value]))
    }

    fn closed_atom_union(&mut self, names: &[&str], span: Span) -> Option<TypeId> {
        let members = names
            .iter()
            .map(|name| self.intern(Type::Atom((*name).to_owned())))
            .collect();
        self.normalize_union(members, span)
    }

    fn tagged_result_type(&mut self, ok: TypeId, error: TypeId, span: Span) -> Option<TypeId> {
        let ok = self.tagged_tuple_type("ok", ok);
        let error = self.tagged_tuple_type("error", error);
        self.normalize_union(vec![ok, error], span)
    }

    fn check_collection_intrinsic(
        &mut self,
        name: &str,
        arguments: &[&Node],
        span: Span,
        expected: Option<TypeId>,
        owner: DeclId,
        scopes: &mut Vec<BTreeMap<String, Local>>,
    ) -> Option<TypedExpr> {
        if name == "Map.new" {
            if !arguments.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    "E2111",
                    span,
                    format!(
                        "function `{name}` expects 0 arguments but received {}",
                        arguments.len()
                    ),
                ));
                return None;
            }
            let Some(ty) = expected
                .filter(|ty| matches!(self.types.get(ty.0 as usize), Some(Type::Map { .. })))
            else {
                self.diagnostics.push(Diagnostic::error(
                    "E2107",
                    span,
                    "cannot infer the key and value types of an empty map",
                ));
                return None;
            };
            let Some(Type::Map { key, .. }) = self.types.get(ty.0 as usize) else {
                unreachable!("expected map type was checked above")
            };
            let key = *key;
            if !self.type_satisfies(key, "Eq", owner) || !self.type_satisfies(key, "Hash", owner) {
                self.diagnostics.push(Diagnostic::error(
                    "E2108",
                    span,
                    format!(
                        "map key type `{}` must implement `Eq` and `Hash`",
                        self.type_name(key)
                    ),
                ));
                return None;
            }
            return Some(TypedExpr {
                kind: TypedExprKind::Map(Vec::new()),
                ty,
                span,
            });
        }
        if name == "Buffer.new" {
            if !arguments.is_empty() {
                self.diagnostics.push(Diagnostic::error(
                    "E2111",
                    span,
                    format!(
                        "function `{name}` expects 0 arguments but received {}",
                        arguments.len()
                    ),
                ));
                return None;
            }
            let ty = self.intern(Type::Buffer);
            return Some(TypedExpr {
                kind: TypedExprKind::BufferNew,
                ty,
                span,
            });
        }
        let required = match name {
            "Slice.subslice" | "Bytes.slice" | "Bits.slice" | "Map.put" | "Enum.reduce" => 3,
            "Map.remove"
            | "Map.fetch"
            | "Enum.at"
            | "Enum.each"
            | "Enum.any"
            | "Enum.all"
            | "Enum.filter"
            | "Enum.map"
            | "Buffer.append_byte"
            | "Buffer.append_bytes"
            | "Buffer.append_string" => 2,
            "String.contains" | "String.split" => 2,
            _ => 1,
        };
        if arguments.len() != required {
            self.diagnostics.push(Diagnostic::error(
                "E2111",
                span,
                format!(
                    "function `{name}` expects {required} arguments but received {}",
                    arguments.len()
                ),
            ));
            return None;
        }
        let usize_ty = self.intern(Type::Usize);
        let first_expected = if matches!(name, "Map.put" | "Map.remove")
            && let Some(expected) = expected
            && matches!(self.types[expected.0 as usize], Type::Map { .. })
        {
            Some(expected)
        } else if name == "List.reverse"
            && let Some(expected) = expected
            && matches!(self.types[expected.0 as usize], Type::List(_))
        {
            Some(expected)
        } else if name == "Slice.from_array"
            && arguments[0].kind.as_str() == "array_literal"
            && let Some(expected) = expected
            && let Type::Slice(item) = self.types[expected.0 as usize]
        {
            Some(self.intern(Type::Array {
                item,
                length: arguments[0].children.len() as u64,
            }))
        } else if name == "Bytes.from_list" {
            let u8_ty = self.intern(Type::U8);
            Some(self.intern(Type::List(u8_ty)))
        } else {
            None
        };
        let first = self.check_expr(arguments[0], first_expected, owner, scopes)?;
        let (kind, ty) = match (name, self.types[first.ty.0 as usize].clone()) {
            ("List.reverse", Type::List(_)) => {
                let ty = first.ty;
                (TypedExprKind::ListReverse(Box::new(first)), ty)
            }
            ("Array.length", Type::Array { length, .. }) => (
                TypedExprKind::CollectionLength {
                    value: Box::new(first),
                    known_length: Some(length),
                },
                usize_ty,
            ),
            ("Slice.from_array", Type::Array { item, length }) => {
                let ty = self.intern(Type::Slice(item));
                (
                    TypedExprKind::SliceFromArray {
                        value: Box::new(first),
                        length,
                    },
                    ty,
                )
            }
            ("Slice.subslice", Type::Slice(_)) => {
                let start = self.check_expr(arguments[1], Some(usize_ty), owner, scopes)?;
                let length = self.check_expr(arguments[2], Some(usize_ty), owner, scopes)?;
                let ty = first.ty;
                (
                    TypedExprKind::SliceSubslice {
                        value: Box::new(first),
                        start: Box::new(start),
                        length: Box::new(length),
                    },
                    ty,
                )
            }
            ("Slice.copy", Type::Slice(_)) => {
                let ty = first.ty;
                (TypedExprKind::SliceCopy(Box::new(first)), ty)
            }
            ("Slice.length", Type::Slice(_)) => (
                TypedExprKind::CollectionLength {
                    value: Box::new(first),
                    known_length: None,
                },
                usize_ty,
            ),
            ("String.byte_size", Type::String) => (
                TypedExprKind::CollectionLength {
                    value: Box::new(first),
                    known_length: None,
                },
                usize_ty,
            ),
            ("String.length", Type::String) => {
                (TypedExprKind::StringLength(Box::new(first)), usize_ty)
            }
            ("String.empty", Type::String) => {
                let ty = self.intern(Type::Bool);
                (TypedExprKind::StringEmpty(Box::new(first)), ty)
            }
            ("String.contains", Type::String) => {
                let string_ty = self.intern(Type::String);
                let pattern = self.check_expr(arguments[1], Some(string_ty), owner, scopes)?;
                let ty = self.intern(Type::Bool);
                (
                    TypedExprKind::StringContains {
                        string: Box::new(first),
                        pattern: Box::new(pattern),
                    },
                    ty,
                )
            }
            ("String.split", Type::String) => {
                let string_ty = self.intern(Type::String);
                let separator = self.check_expr(arguments[1], Some(string_ty), owner, scopes)?;
                let ty = self.intern(Type::List(string_ty));
                (
                    TypedExprKind::StringSplit {
                        string: Box::new(first),
                        separator: Box::new(separator),
                    },
                    ty,
                )
            }
            ("String.bytes", Type::String) => {
                let ty = self.intern(Type::Bytes);
                (TypedExprKind::StringBytes(Box::new(first)), ty)
            }
            ("String.codepoints", Type::String) => {
                let rune_ty = self.intern(Type::Rune);
                let ty = self.intern(Type::List(rune_ty));
                (TypedExprKind::StringCodepoints(Box::new(first)), ty)
            }
            ("String.graphemes", Type::String) => {
                let string_ty = self.intern(Type::String);
                let view_ty = self.intern(Type::GraphemeView);
                let view = TypedExpr {
                    kind: TypedExprKind::StringGraphemeView(Box::new(first)),
                    ty: view_ty,
                    span,
                };
                let ty = self.intern(Type::List(string_ty));
                (TypedExprKind::EnumToList(Box::new(view)), ty)
            }
            ("String.codepoint_view", Type::String) => {
                let ty = self.intern(Type::CodepointView);
                (TypedExprKind::StringCodepointView(Box::new(first)), ty)
            }
            ("String.grapheme_view", Type::String) => {
                let ty = self.intern(Type::GraphemeView);
                (TypedExprKind::StringGraphemeView(Box::new(first)), ty)
            }
            ("String.from_bytes", Type::Bytes) => {
                let ok_atom = self.intern(Type::Atom("ok".to_owned()));
                let error_atom = self.intern(Type::Atom("error".to_owned()));
                let string_ty = self.intern(Type::String);
                let error_ty = self.intern(Type::Utf8Error);
                let ok = self.intern(Type::Tuple(vec![ok_atom, string_ty]));
                let error = self.intern(Type::Tuple(vec![error_atom, error_ty]));
                let ty = self.normalize_union(vec![ok, error], span)?;
                (TypedExprKind::StringFromBytes(Box::new(first)), ty)
            }
            ("String.utf8_error_offset", Type::Utf8Error) => {
                (TypedExprKind::Utf8ErrorOffset(Box::new(first)), usize_ty)
            }
            ("Rune.to_string", Type::Rune) => {
                let ty = self.intern(Type::String);
                (TypedExprKind::RuneToString(Box::new(first)), ty)
            }
            ("Buffer.byte_size", Type::Buffer) => (
                TypedExprKind::CollectionLength {
                    value: Box::new(first),
                    known_length: None,
                },
                usize_ty,
            ),
            ("Buffer.append_byte", Type::Buffer) => {
                let expected = self.intern(Type::U8);
                let value = self.check_expr(arguments[1], Some(expected), owner, scopes)?;
                let ty = first.ty;
                (
                    TypedExprKind::BufferAppend {
                        buffer: Box::new(first),
                        value: Box::new(value),
                        kind: BufferAppendKind::Byte,
                    },
                    ty,
                )
            }
            ("Buffer.append_bytes", Type::Buffer) => {
                let expected = self.intern(Type::Bytes);
                let value = self.check_expr(arguments[1], Some(expected), owner, scopes)?;
                let ty = first.ty;
                (
                    TypedExprKind::BufferAppend {
                        buffer: Box::new(first),
                        value: Box::new(value),
                        kind: BufferAppendKind::Bytes,
                    },
                    ty,
                )
            }
            ("Buffer.append_string", Type::Buffer) => {
                let expected = self.intern(Type::String);
                let value = self.check_expr(arguments[1], Some(expected), owner, scopes)?;
                let ty = first.ty;
                (
                    TypedExprKind::BufferAppend {
                        buffer: Box::new(first),
                        value: Box::new(value),
                        kind: BufferAppendKind::String,
                    },
                    ty,
                )
            }
            ("Buffer.to_bytes", Type::Buffer) => {
                let ty = self.intern(Type::Bytes);
                (TypedExprKind::BufferToBytes(Box::new(first)), ty)
            }
            ("Buffer.to_string", Type::Buffer) => {
                let ok_atom = self.intern(Type::Atom("ok".to_owned()));
                let error_atom = self.intern(Type::Atom("error".to_owned()));
                let string_ty = self.intern(Type::String);
                let error_ty = self.intern(Type::Utf8Error);
                let ok = self.intern(Type::Tuple(vec![ok_atom, string_ty]));
                let error = self.intern(Type::Tuple(vec![error_atom, error_ty]));
                let ty = self.normalize_union(vec![ok, error], span)?;
                (TypedExprKind::BufferToString(Box::new(first)), ty)
            }
            ("Bits.bit_size", Type::Bits) => (
                TypedExprKind::CollectionLength {
                    value: Box::new(first),
                    known_length: None,
                },
                usize_ty,
            ),
            ("Bits.slice", Type::Bits) => {
                let start = self.check_expr(arguments[1], Some(usize_ty), owner, scopes)?;
                let length = self.check_expr(arguments[2], Some(usize_ty), owner, scopes)?;
                let ty = first.ty;
                (
                    TypedExprKind::BitsSlice {
                        value: Box::new(first),
                        start: Box::new(start),
                        length: Box::new(length),
                    },
                    ty,
                )
            }
            ("Bits.to_bytes", Type::Bits) => {
                let some_atom = self.intern(Type::Atom("some".to_owned()));
                let none_atom = self.intern(Type::Atom("none".to_owned()));
                let bytes_ty = self.intern(Type::Bytes);
                let some = self.intern(Type::Tuple(vec![some_atom, bytes_ty]));
                let ty = self.normalize_union(vec![some, none_atom], span)?;
                (TypedExprKind::BitsToBytes(Box::new(first)), ty)
            }
            ("Bytes.byte_size", Type::Bytes) => (
                TypedExprKind::CollectionLength {
                    value: Box::new(first),
                    known_length: None,
                },
                usize_ty,
            ),
            ("Bytes.from_list", Type::List(item))
                if matches!(self.types[item.0 as usize], Type::U8) =>
            {
                let ty = self.intern(Type::Bytes);
                (TypedExprKind::BytesFromList(Box::new(first)), ty)
            }
            ("Bytes.to_list", Type::Bytes) => {
                let u8_ty = self.intern(Type::U8);
                let ty = self.intern(Type::List(u8_ty));
                (TypedExprKind::BytesToList(Box::new(first)), ty)
            }
            ("Bytes.to_bits", Type::Bytes) => {
                let ty = self.intern(Type::Bits);
                (TypedExprKind::BytesToBits(Box::new(first)), ty)
            }
            ("Bytes.slice", Type::Bytes) => {
                let start = self.check_expr(arguments[1], Some(usize_ty), owner, scopes)?;
                let length = self.check_expr(arguments[2], Some(usize_ty), owner, scopes)?;
                let ty = first.ty;
                (
                    TypedExprKind::BytesSlice {
                        value: Box::new(first),
                        start: Box::new(start),
                        length: Box::new(length),
                    },
                    ty,
                )
            }
            ("Map.size", Type::Map { .. }) => (
                TypedExprKind::CollectionLength {
                    value: Box::new(first),
                    known_length: None,
                },
                usize_ty,
            ),
            (
                "Enum.count",
                Type::List(_)
                | Type::Slice(_)
                | Type::Bytes
                | Type::Map { .. }
                | Type::CodepointView
                | Type::GraphemeView,
            ) => (
                TypedExprKind::CollectionLength {
                    value: Box::new(first),
                    known_length: None,
                },
                usize_ty,
            ),
            ("Enum.count", Type::Array { length, .. }) => (
                TypedExprKind::CollectionLength {
                    value: Box::new(first),
                    known_length: Some(length),
                },
                usize_ty,
            ),
            ("Enum.at", source) => {
                let item = match source {
                    Type::List(item) | Type::Array { item, .. } | Type::Slice(item) => item,
                    Type::Bytes => self.intern(Type::U8),
                    Type::Map { key, value } => self.intern(Type::Tuple(vec![key, value])),
                    Type::CodepointView => self.intern(Type::Rune),
                    Type::GraphemeView => self.intern(Type::String),
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E2151",
                            arguments[0].span,
                            "`Enum.at` requires a standard iterable value",
                        ));
                        return None;
                    }
                };
                let index = self.check_expr(arguments[1], Some(usize_ty), owner, scopes)?;
                let some_atom = self.intern(Type::Atom("some".to_owned()));
                let none_atom = self.intern(Type::Atom("none".to_owned()));
                let some = self.intern(Type::Tuple(vec![some_atom, item]));
                let ty = self.normalize_union(vec![some, none_atom], span)?;
                (
                    TypedExprKind::EnumAt {
                        value: Box::new(first),
                        index: Box::new(index),
                    },
                    ty,
                )
            }
            ("Enum.to_list", Type::List(_)) => {
                let ty = first.ty;
                let inner = TypedExpr {
                    kind: TypedExprKind::ListReverse(Box::new(first)),
                    ty,
                    span,
                };
                (TypedExprKind::ListReverse(Box::new(inner)), ty)
            }
            ("Enum.to_list", Type::Array { item, .. } | Type::Slice(item)) => {
                let ty = self.intern(Type::List(item));
                (TypedExprKind::EnumToList(Box::new(first)), ty)
            }
            ("Enum.to_list", Type::Bytes) => {
                let u8_ty = self.intern(Type::U8);
                let ty = self.intern(Type::List(u8_ty));
                (TypedExprKind::BytesToList(Box::new(first)), ty)
            }
            ("Enum.to_list", Type::Map { key, value }) => {
                let pair = self.intern(Type::Tuple(vec![key, value]));
                let ty = self.intern(Type::List(pair));
                (TypedExprKind::MapToList(Box::new(first)), ty)
            }
            ("Enum.to_list", Type::CodepointView) => {
                let item = self.intern(Type::Rune);
                let ty = self.intern(Type::List(item));
                (TypedExprKind::EnumToList(Box::new(first)), ty)
            }
            ("Enum.to_list", Type::GraphemeView) => {
                let item = self.intern(Type::String);
                let ty = self.intern(Type::List(item));
                (TypedExprKind::EnumToList(Box::new(first)), ty)
            }
            (
                "Enum.each" | "Enum.any" | "Enum.all" | "Enum.reduce" | "Enum.filter" | "Enum.map",
                source,
            ) => {
                let item = match source {
                    Type::List(item) | Type::Array { item, .. } | Type::Slice(item) => item,
                    Type::Bytes => self.intern(Type::U8),
                    Type::Map { key, value } => self.intern(Type::Tuple(vec![key, value])),
                    Type::CodepointView => self.intern(Type::Rune),
                    Type::GraphemeView => self.intern(Type::String),
                    _ => {
                        self.diagnostics.push(Diagnostic::error(
                            "E2151",
                            arguments[0].span,
                            format!("`{name}` requires a standard iterable value"),
                        ));
                        return None;
                    }
                };
                let kind = match name {
                    "Enum.each" => EnumVisitKind::Each,
                    "Enum.any" => EnumVisitKind::Any,
                    "Enum.all" => EnumVisitKind::All,
                    "Enum.reduce" => EnumVisitKind::Reduce,
                    "Enum.filter" => EnumVisitKind::Filter,
                    "Enum.map" => EnumVisitKind::Map,
                    _ => unreachable!("matched Enum visit intrinsic"),
                };
                let initial = if kind == EnumVisitKind::Reduce {
                    Some(self.check_expr(arguments[1], expected, owner, scopes)?)
                } else {
                    None
                };
                let expected_map_item = (kind == EnumVisitKind::Map)
                    .then_some(expected)
                    .flatten()
                    .and_then(|expected| match self.types.get(expected.0 as usize) {
                        Some(Type::List(item)) => Some(*item),
                        _ => None,
                    });
                let mut prechecked_function = None;
                let callback_result = match kind {
                    EnumVisitKind::Each => self.intern(Type::Unit),
                    EnumVisitKind::Any | EnumVisitKind::All => self.intern(Type::Bool),
                    EnumVisitKind::Reduce => initial.as_ref()?.ty,
                    EnumVisitKind::Filter => self.intern(Type::Bool),
                    EnumVisitKind::Map if expected_map_item.is_some() => expected_map_item?,
                    EnumVisitKind::Map => {
                        let function_node = arguments[1];
                        let shadowed = unqualified_name(function_node)
                            .as_ref()
                            .is_some_and(|name| lookup(scopes, name).is_some());
                        let inferred_function_ty = if shadowed {
                            None
                        } else {
                            self.call_name(function_node, owner)
                                .and_then(|name| self.functions_by_name.get(&name).copied())
                                .and_then(|function| self.signatures.get(&function).cloned())
                                .and_then(|signature| {
                                    let [parameter] = signature.parameters.as_slice() else {
                                        return None;
                                    };
                                    let mut substitutions = BTreeMap::new();
                                    if !unify_types(
                                        &self.types,
                                        *parameter,
                                        item,
                                        &mut substitutions,
                                    ) || signature
                                        .type_parameters
                                        .iter()
                                        .any(|parameter| !substitutions.contains_key(parameter))
                                    {
                                        return None;
                                    }
                                    let result =
                                        self.apply_substitutions(signature.result, &substitutions);
                                    Some(self.intern(Type::Function {
                                        parameters: vec![item],
                                        result,
                                    }))
                                })
                        };
                        let function =
                            self.check_expr(function_node, inferred_function_ty, owner, scopes)?;
                        let Some(Type::Function { parameters, result }) =
                            self.types.get(function.ty.0 as usize)
                        else {
                            self.diagnostics.push(Diagnostic::error(
                                "E2109",
                                arguments[1].span,
                                "`Enum.map` requires a named function value",
                            ));
                            return None;
                        };
                        if parameters.as_slice() != [item] {
                            let expected = self.intern(Type::Function {
                                parameters: vec![item],
                                result: *result,
                            });
                            self.type_mismatch(function.span, expected, function.ty);
                            return None;
                        }
                        let result = *result;
                        prechecked_function = Some(function);
                        result
                    }
                };
                let function_ty = self.intern(Type::Function {
                    parameters: if kind == EnumVisitKind::Reduce {
                        vec![callback_result, item]
                    } else {
                        vec![item]
                    },
                    result: callback_result,
                });
                let function = if let Some(function) = prechecked_function {
                    function
                } else {
                    self.check_expr(
                        arguments[usize::from(kind == EnumVisitKind::Reduce) + 1],
                        Some(function_ty),
                        owner,
                        scopes,
                    )?
                };
                (
                    TypedExprKind::EnumVisit {
                        value: Box::new(first),
                        initial: initial.map(Box::new),
                        function: Box::new(function),
                        kind,
                    },
                    match kind {
                        EnumVisitKind::Filter => self.intern(Type::List(item)),
                        EnumVisitKind::Map => self.intern(Type::List(callback_result)),
                        _ => callback_result,
                    },
                )
            }
            ("Map.put", Type::Map { key, value }) => {
                if !self.type_satisfies(key, "Eq", owner)
                    || !self.type_satisfies(key, "Hash", owner)
                {
                    self.diagnostics.push(Diagnostic::error(
                        "E2108",
                        arguments[0].span,
                        format!(
                            "map key type `{}` must implement `Eq` and `Hash`",
                            self.type_name(key)
                        ),
                    ));
                    return None;
                }
                let key_expr = self.check_expr(arguments[1], Some(key), owner, scopes)?;
                let value_expr = self.check_expr(arguments[2], Some(value), owner, scopes)?;
                let ty = first.ty;
                (
                    TypedExprKind::MapPut {
                        map: Box::new(first),
                        key: Box::new(key_expr),
                        value: Box::new(value_expr),
                    },
                    ty,
                )
            }
            ("Map.remove", Type::Map { key, .. }) => {
                if !self.type_satisfies(key, "Eq", owner)
                    || !self.type_satisfies(key, "Hash", owner)
                {
                    self.diagnostics.push(Diagnostic::error(
                        "E2108",
                        arguments[0].span,
                        format!(
                            "map key type `{}` must implement `Eq` and `Hash`",
                            self.type_name(key)
                        ),
                    ));
                    return None;
                }
                let key_expr = self.check_expr(arguments[1], Some(key), owner, scopes)?;
                let ty = first.ty;
                (
                    TypedExprKind::MapRemove {
                        map: Box::new(first),
                        key: Box::new(key_expr),
                    },
                    ty,
                )
            }
            ("Map.fetch", Type::Map { key, value }) => {
                if !self.type_satisfies(key, "Eq", owner)
                    || !self.type_satisfies(key, "Hash", owner)
                {
                    self.diagnostics.push(Diagnostic::error(
                        "E2108",
                        arguments[0].span,
                        format!(
                            "map key type `{}` must implement `Eq` and `Hash`",
                            self.type_name(key)
                        ),
                    ));
                    return None;
                }
                let key_expr = self.check_expr(arguments[1], Some(key), owner, scopes)?;
                let none = self.intern(Type::Atom("none".to_owned()));
                let some_atom = self.intern(Type::Atom("some".to_owned()));
                let some = self.intern(Type::Tuple(vec![some_atom, value]));
                let ty = self.normalize_union(vec![some, none], span)?;
                (
                    TypedExprKind::MapFetch {
                        map: Box::new(first),
                        key: Box::new(key_expr),
                    },
                    ty,
                )
            }
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    "E2151",
                    arguments[0].span,
                    format!("invalid argument type for `{name}`"),
                ));
                return None;
            }
        };
        if let Some(expected) = expected
            && expected != ty
        {
            self.type_mismatch(span, expected, ty);
            return None;
        }
        Some(TypedExpr { kind, ty, span })
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
            Type::Slice(item) => {
                let item = self.apply_substitutions(item, substitutions);
                self.intern(Type::Slice(item))
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
            Type::Projection {
                protocol,
                associated,
                argument,
            } => {
                let argument = self.apply_substitutions(argument, substitutions);
                self.normalize_standard_projection(&protocol, &associated, argument)
                    .or_else(|| {
                        self.normalize_implementation_projection(&protocol, &associated, argument)
                    })
                    .unwrap_or_else(|| {
                        self.intern(Type::Projection {
                            protocol,
                            associated,
                            argument,
                        })
                    })
            }
            _ => ty,
        }
    }

    fn normalize_implementation_projection(
        &self,
        protocol: &str,
        associated: &str,
        argument: TypeId,
    ) -> Option<TypeId> {
        self.program
            .implementations
            .iter()
            .find_map(|implementation| {
                if implementation.protocol != protocol {
                    return None;
                }
                let mut substitutions = BTreeMap::new();
                if !implementation_target_matches(
                    &self.types,
                    argument,
                    &implementation.target,
                    &mut substitutions,
                ) {
                    return None;
                }
                let (_, projected) = implementation
                    .associated_types
                    .iter()
                    .find(|(name, _)| name == associated)?;
                resolved_type_id_for_conformance(
                    &self.types,
                    projected,
                    &substitutions,
                    &self.aliases,
                )
            })
    }

    fn type_satisfies(&self, ty: TypeId, protocol: &str, owner: DeclId) -> bool {
        if self.explicit_implementation_satisfies(ty, protocol, owner) {
            return true;
        }
        match &self.types[ty.0 as usize] {
            Type::Parameter { .. } => self.signatures.get(&owner).is_some_and(|signature| {
                signature
                    .constraints
                    .iter()
                    .any(|(parameter, required)| *parameter == ty && required == protocol)
            }),
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Isize
            | Type::Usize
            | Type::Bool
            | Type::Unit
            | Type::String
            | Type::Bytes
            | Type::Bits
            | Type::Rune
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Atom(_) => {
                matches!(protocol, "Eq" | "Ord" | "Show" | "Hash")
                    || protocol == "Concat"
                        && matches!(
                            self.types[ty.0 as usize],
                            Type::String | Type::Bytes | Type::Bits
                        )
            }
            Type::Buffer => false,
            Type::F32 | Type::F64 => false,
            Type::Utf8Error => matches!(protocol, "Eq" | "Show" | "Hash"),
            Type::Opaque(kind) => match kind {
                OpaqueType::FileReader | OpaqueType::IoStdin => protocol == "Reader",
                OpaqueType::FileWriter | OpaqueType::IoStdout | OpaqueType::IoStderr => {
                    protocol == "Writer"
                }
                OpaqueType::FileError | OpaqueType::IoError => {
                    matches!(protocol, "Eq" | "Show" | "Hash")
                }
            },
            Type::CodepointView | Type::GraphemeView => protocol == "Iterable",
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
            Type::Slice(item) => match protocol {
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
            Type::Struct {
                declaration,
                arguments,
            } => self.structs.get(declaration).is_some_and(|structure| {
                structure.derives.contains(&protocol.to_owned())
                    && matches!(protocol, "Eq" | "Ord" | "Show" | "Hash")
                    && structure.fields.iter().all(|field| {
                        let substitutions = structure
                            .parameters
                            .iter()
                            .cloned()
                            .zip(arguments.iter().copied())
                            .collect::<BTreeMap<_, _>>();
                        resolved_type_id_for_conformance(
                            &self.types,
                            &field.ty,
                            &substitutions,
                            &self.aliases,
                        )
                        .is_some_and(|field| self.type_satisfies(field, protocol, owner))
                    })
            }),
            Type::Projection { .. } | Type::Function { .. } | Type::Union(_) => false,
        }
    }

    fn iterable_item_type(&mut self, ty: TypeId, owner: DeclId) -> Option<TypeId> {
        match self.types[ty.0 as usize].clone() {
            Type::List(item) | Type::Array { item, .. } | Type::Slice(item) => Some(item),
            Type::Bytes => Some(self.intern(Type::U8)),
            Type::Map { key, value } => Some(self.intern(Type::Tuple(vec![key, value]))),
            Type::CodepointView => Some(self.intern(Type::Rune)),
            Type::GraphemeView => Some(self.intern(Type::String)),
            Type::Parameter { .. } if self.type_satisfies(ty, "Iterable", owner) => {
                Some(self.intern(Type::Projection {
                    protocol: "Iterable".to_owned(),
                    associated: "Item".to_owned(),
                    argument: ty,
                }))
            }
            _ => {
                let implementations = self.program.implementations.clone();
                implementations.into_iter().find_map(|implementation| {
                    if implementation.protocol != "Iterable" {
                        return None;
                    }
                    let mut substitutions = BTreeMap::new();
                    if !implementation_target_matches(
                        &self.types,
                        ty,
                        &implementation.target,
                        &mut substitutions,
                    ) {
                        return None;
                    }
                    let (_, item) = implementation
                        .associated_types
                        .iter()
                        .find(|(name, _)| name == "Item")?;
                    resolved_type_id_for_conformance(
                        &self.types,
                        item,
                        &substitutions,
                        &self.aliases,
                    )
                })
            }
        }
    }

    fn normalize_standard_projection(
        &mut self,
        protocol: &str,
        associated: &str,
        argument: TypeId,
    ) -> Option<TypeId> {
        if protocol == "Reader" && associated == "Error" {
            return match self.types[argument.0 as usize] {
                Type::Opaque(OpaqueType::FileReader) => {
                    Some(self.intern(Type::Opaque(OpaqueType::FileError)))
                }
                Type::Opaque(OpaqueType::IoStdin) => {
                    Some(self.intern(Type::Opaque(OpaqueType::IoError)))
                }
                _ => None,
            };
        }
        if protocol == "Writer" && associated == "Error" {
            return match self.types[argument.0 as usize] {
                Type::Opaque(OpaqueType::FileWriter) => {
                    Some(self.intern(Type::Opaque(OpaqueType::FileError)))
                }
                Type::Opaque(OpaqueType::IoStdout | OpaqueType::IoStderr) => {
                    Some(self.intern(Type::Opaque(OpaqueType::IoError)))
                }
                _ => None,
            };
        }
        if protocol != "Iterable" {
            return None;
        }
        match associated {
            "Item" => match self.types[argument.0 as usize].clone() {
                Type::List(item) | Type::Slice(item) | Type::Array { item, .. } => Some(item),
                Type::Bytes => Some(self.intern(Type::U8)),
                Type::Map { key, value } => Some(self.intern(Type::Tuple(vec![key, value]))),
                Type::CodepointView => Some(self.intern(Type::Rune)),
                Type::GraphemeView => Some(self.intern(Type::String)),
                _ => None,
            },
            "Cursor" => match self.types[argument.0 as usize].clone() {
                Type::List(_) => Some(argument),
                Type::Array { .. }
                | Type::Slice(_)
                | Type::Bytes
                | Type::Map { .. }
                | Type::CodepointView
                | Type::GraphemeView => {
                    let usize_ty = self.intern(Type::Usize);
                    Some(self.intern(Type::Tuple(vec![argument, usize_ty])))
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn explicit_implementation_satisfies(&self, ty: TypeId, protocol: &str, owner: DeclId) -> bool {
        self.program.implementations.iter().any(|implementation| {
            if implementation.protocol != protocol {
                return false;
            }
            let mut substitutions = BTreeMap::new();
            if !implementation_target_matches(
                &self.types,
                ty,
                &implementation.target,
                &mut substitutions,
            ) {
                return false;
            }
            implementation.constraints.iter().all(|constraint| {
                substitutions
                    .get(&constraint.parameter)
                    .is_some_and(|argument| {
                        self.type_satisfies(*argument, &constraint.protocol, owner)
                    })
            })
        })
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
            Type::I8 => "i8".to_owned(),
            Type::I16 => "i16".to_owned(),
            Type::I32 => "i32".to_owned(),
            Type::I64 => "i64".to_owned(),
            Type::Isize => "isize".to_owned(),
            Type::Usize => "usize".to_owned(),
            Type::Bool => "bool".to_owned(),
            Type::Unit => "unit".to_owned(),
            Type::String => "string".to_owned(),
            Type::Bytes => "bytes".to_owned(),
            Type::Bits => "bits".to_owned(),
            Type::Buffer => "Buffer".to_owned(),
            Type::Rune => "rune".to_owned(),
            Type::Utf8Error => "String.Utf8Error".to_owned(),
            Type::Opaque(kind) => opaque_type_name(*kind).to_owned(),
            Type::CodepointView => "String.CodepointView".to_owned(),
            Type::GraphemeView => "String.GraphemeView".to_owned(),
            Type::U8 => "u8".to_owned(),
            Type::U16 => "u16".to_owned(),
            Type::U32 => "u32".to_owned(),
            Type::U64 => "u64".to_owned(),
            Type::F32 => "f32".to_owned(),
            Type::F64 => "f64".to_owned(),
            Type::Atom(name) => format!(":{name}"),
            Type::List(item) => format!("[{}]", self.type_name(*item)),
            Type::Array { item, length } => format!("[{}; {length}]", self.type_name(*item)),
            Type::Slice(item) => format!("Slice({})", self.type_name(*item)),
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
            Type::Projection {
                protocol,
                associated,
                argument,
            } => format!("{protocol}.{associated}({})", self.type_name(*argument)),
            Type::Union(members) => members
                .iter()
                .map(|member| self.type_name(*member))
                .collect::<Vec<_>>()
                .join(" | "),
        }
    }

    fn type_key(&self, id: TypeId) -> String {
        match &self.types[id.0 as usize] {
            Type::I8 => "00:i8".to_owned(),
            Type::I16 => "00:i16".to_owned(),
            Type::I32 => "00:i32".to_owned(),
            Type::I64 => "00:i64".to_owned(),
            Type::Isize => "00:isize".to_owned(),
            Type::Usize => "00:usize".to_owned(),
            Type::Bool => "00:bool".to_owned(),
            Type::Unit => "00:unit".to_owned(),
            Type::String => "00:string".to_owned(),
            Type::Bytes => "00:bytes".to_owned(),
            Type::Bits => "00:bits".to_owned(),
            Type::Buffer => "00:Buffer".to_owned(),
            Type::Rune => "00:rune".to_owned(),
            Type::Utf8Error => "00:String.Utf8Error".to_owned(),
            Type::Opaque(kind) => format!("00:{}", opaque_type_name(*kind)),
            Type::CodepointView => "00:String.CodepointView".to_owned(),
            Type::GraphemeView => "00:String.GraphemeView".to_owned(),
            Type::U8 => "00:u8".to_owned(),
            Type::U16 => "00:u16".to_owned(),
            Type::U32 => "00:u32".to_owned(),
            Type::U64 => "00:u64".to_owned(),
            Type::F32 => "00:f32".to_owned(),
            Type::F64 => "00:f64".to_owned(),
            Type::Atom(name) => format!("01:{name}"),
            Type::List(item) => format!("02:[{}]", self.type_key(*item)),
            Type::Array { item, length } => format!("02a:[{};{length}]", self.type_key(*item)),
            Type::Slice(item) => format!("02s:Slice({})", self.type_key(*item)),
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
            Type::Projection {
                protocol,
                associated,
                argument,
            } => format!("11:{protocol}.{associated}({})", self.type_key(*argument)),
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

fn resolved_type_id_for_conformance(
    types: &[Type],
    syntax: &TypeSyntax,
    substitutions: &BTreeMap<String, TypeId>,
    aliases: &BTreeMap<DeclId, TypeAlias>,
) -> Option<TypeId> {
    let find = |expected: Type| {
        types
            .iter()
            .position(|candidate| *candidate == expected)
            .map(|index| TypeId(index as u32))
    };
    match syntax {
        TypeSyntax::Primitive { name, .. } => types
            .iter()
            .position(|candidate| primitive_type_name(candidate) == Some(name.as_str()))
            .map(|index| TypeId(index as u32)),
        TypeSyntax::Variable { name, .. } => substitutions.get(name).copied(),
        TypeSyntax::SelfType { .. } => None,
        TypeSyntax::Projection {
            protocol,
            associated,
            argument,
            ..
        } => find(Type::Projection {
            protocol: protocol.clone(),
            associated: associated.clone(),
            argument: resolved_type_id_for_conformance(types, argument, substitutions, aliases)?,
        }),
        TypeSyntax::Named {
            declaration,
            arguments,
            ..
        } => {
            if let Some(alias) = aliases.get(declaration) {
                let resolved_arguments = arguments
                    .iter()
                    .map(|argument| {
                        resolved_type_id_for_conformance(types, argument, substitutions, aliases)
                    })
                    .collect::<Option<Vec<_>>>()?;
                let alias_substitutions = alias
                    .parameters
                    .iter()
                    .cloned()
                    .zip(resolved_arguments)
                    .collect::<BTreeMap<_, _>>();
                return resolved_type_id_for_conformance(
                    types,
                    &alias.value,
                    &alias_substitutions,
                    aliases,
                );
            }
            let arguments = arguments
                .iter()
                .map(|argument| {
                    resolved_type_id_for_conformance(types, argument, substitutions, aliases)
                })
                .collect::<Option<Vec<_>>>()?;
            find(Type::Struct {
                declaration: *declaration,
                arguments,
            })
        }
        TypeSyntax::Atom { name, .. } => find(Type::Atom(name.clone())),
        TypeSyntax::List { item, .. } => find(Type::List(resolved_type_id_for_conformance(
            types,
            item,
            substitutions,
            aliases,
        )?)),
        TypeSyntax::Array { item, length, .. } => find(Type::Array {
            item: resolved_type_id_for_conformance(types, item, substitutions, aliases)?,
            length: *length,
        }),
        TypeSyntax::Slice { item, .. } => find(Type::Slice(resolved_type_id_for_conformance(
            types,
            item,
            substitutions,
            aliases,
        )?)),
        TypeSyntax::Map { key, value, .. } => find(Type::Map {
            key: resolved_type_id_for_conformance(types, key, substitutions, aliases)?,
            value: resolved_type_id_for_conformance(types, value, substitutions, aliases)?,
        }),
        TypeSyntax::Tuple { elements, .. } => find(Type::Tuple(
            elements
                .iter()
                .map(|element| {
                    resolved_type_id_for_conformance(types, element, substitutions, aliases)
                })
                .collect::<Option<Vec<_>>>()?,
        )),
        TypeSyntax::Function {
            parameters, result, ..
        } => find(Type::Function {
            parameters: parameters
                .iter()
                .map(|parameter| {
                    resolved_type_id_for_conformance(types, parameter, substitutions, aliases)
                })
                .collect::<Option<Vec<_>>>()?,
            result: resolved_type_id_for_conformance(types, result, substitutions, aliases)?,
        }),
        TypeSyntax::Union { members, .. } => {
            let mut members = members
                .iter()
                .map(|member| {
                    resolved_type_id_for_conformance(types, member, substitutions, aliases)
                })
                .collect::<Option<Vec<_>>>()?;
            members.sort_unstable();
            members.dedup();
            find(Type::Union(members))
        }
    }
}

fn opaque_type_name(ty: OpaqueType) -> &'static str {
    match ty {
        OpaqueType::FileReader => "File.Reader",
        OpaqueType::FileWriter => "File.Writer",
        OpaqueType::FileError => "File.Error",
        OpaqueType::IoStdin => "IO.Stdin",
        OpaqueType::IoStdout => "IO.Stdout",
        OpaqueType::IoStderr => "IO.Stderr",
        OpaqueType::IoError => "IO.Error",
    }
}

fn primitive_type_name(ty: &Type) -> Option<&'static str> {
    Some(match ty {
        Type::I8 => "i8",
        Type::I16 => "i16",
        Type::I32 => "i32",
        Type::I64 => "i64",
        Type::Isize => "isize",
        Type::Usize => "usize",
        Type::Bool => "bool",
        Type::Unit => "unit",
        Type::String => "string",
        Type::Bytes => "bytes",
        Type::Bits => "bits",
        Type::Buffer => "Buffer",
        Type::Rune => "rune",
        Type::Utf8Error => "String.Utf8Error",
        Type::Opaque(kind) => opaque_type_name(*kind),
        Type::CodepointView => "String.CodepointView",
        Type::GraphemeView => "String.GraphemeView",
        Type::U8 => "u8",
        Type::U16 => "u16",
        Type::U32 => "u32",
        Type::U64 => "u64",
        Type::F32 => "f32",
        Type::F64 => "f64",
        Type::Atom(_)
        | Type::List(_)
        | Type::Array { .. }
        | Type::Slice(_)
        | Type::Map { .. }
        | Type::Tuple(_)
        | Type::Function { .. }
        | Type::Struct { .. }
        | Type::Parameter { .. }
        | Type::Projection { .. }
        | Type::Union(_) => return None,
    })
}

fn implementation_target_matches(
    types: &[Type],
    ty: TypeId,
    target: &TypeSyntax,
    substitutions: &mut BTreeMap<String, TypeId>,
) -> bool {
    match target {
        TypeSyntax::Variable { name, .. } => {
            if let Some(existing) = substitutions.get(name) {
                *existing == ty
            } else {
                substitutions.insert(name.clone(), ty);
                true
            }
        }
        TypeSyntax::Primitive { name, .. } => {
            primitive_type_name(&types[ty.0 as usize]) == Some(name.as_str())
        }
        TypeSyntax::Named {
            declaration,
            arguments,
            ..
        } => {
            matches!(&types[ty.0 as usize], Type::Struct { declaration: actual, arguments: actual_arguments }
            if actual == declaration
                && actual_arguments.len() == arguments.len()
                && arguments.iter().zip(actual_arguments).all(|(target, actual)| implementation_target_matches(types, *actual, target, substitutions)))
        }
        TypeSyntax::Atom { name, .. } => {
            matches!(&types[ty.0 as usize], Type::Atom(actual) if actual == name)
        }
        TypeSyntax::List { item, .. } => {
            matches!(types[ty.0 as usize], Type::List(actual) if implementation_target_matches(types, actual, item, substitutions))
        }
        TypeSyntax::Array { item, length, .. } => {
            matches!(types[ty.0 as usize], Type::Array { item: actual, length: actual_length } if actual_length == *length && implementation_target_matches(types, actual, item, substitutions))
        }
        TypeSyntax::Slice { item, .. } => {
            matches!(types[ty.0 as usize], Type::Slice(actual) if implementation_target_matches(types, actual, item, substitutions))
        }
        TypeSyntax::Map { key, value, .. } => {
            matches!(types[ty.0 as usize], Type::Map { key: actual_key, value: actual_value } if implementation_target_matches(types, actual_key, key, substitutions) && implementation_target_matches(types, actual_value, value, substitutions))
        }
        TypeSyntax::Tuple { elements, .. } => {
            matches!(&types[ty.0 as usize], Type::Tuple(actual) if actual.len() == elements.len() && elements.iter().zip(actual).all(|(target, actual)| implementation_target_matches(types, *actual, target, substitutions)))
        }
        TypeSyntax::Function {
            parameters, result, ..
        } => {
            matches!(&types[ty.0 as usize], Type::Function { parameters: actual, result: actual_result } if actual.len() == parameters.len() && parameters.iter().zip(actual).all(|(target, actual)| implementation_target_matches(types, *actual, target, substitutions)) && implementation_target_matches(types, *actual_result, result, substitutions))
        }
        TypeSyntax::SelfType { .. } | TypeSyntax::Projection { .. } | TypeSyntax::Union { .. } => {
            false
        }
    }
}

fn known_bytes_length(expression: &TypedExpr) -> Option<u128> {
    match &expression.kind {
        TypedExprKind::StringBytes(value) => match &value.kind {
            TypedExprKind::String(value) => Some(value.len() as u128),
            _ => None,
        },
        TypedExprKind::BytesFromList(value) => match &value.kind {
            TypedExprKind::List {
                elements,
                tail: None,
            } => Some(elements.len() as u128),
            _ => None,
        },
        TypedExprKind::Bitstring(segments) => segments.iter().try_fold(0_u128, |total, segment| {
            let length = match &segment.kind {
                TypedBitstringSegmentKind::Integer { width, .. } => u128::from(*width / 8),
                TypedBitstringSegmentKind::Bytes { size: Some(size) } => match size.kind {
                    TypedExprKind::Integer(size) => size as u128,
                    _ => return None,
                },
                TypedBitstringSegmentKind::Bytes { size: None } => {
                    known_bytes_length(&segment.value)?
                }
            };
            total.checked_add(length)
        }),
        _ => None,
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
        (Type::String, Type::String) => true,
        (Type::Bytes, Type::Bytes) => true,
        (Type::Bits, Type::Bits) => true,
        (Type::Buffer, Type::Buffer) => true,
        (Type::Rune, Type::Rune) => true,
        (Type::Utf8Error, Type::Utf8Error) => true,
        (Type::U8, Type::U8) => true,
        (Type::U64, Type::U64) => true,
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
        (Type::Slice(left), Type::Slice(right)) => unify_types(types, *left, *right, substitutions),
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

fn float_bits(node: &Node, ty: &Type) -> Option<u64> {
    let Some(Value::Float { normalized, .. }) = &node.value else {
        return None;
    };
    match ty {
        Type::F32 => normalized
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| u64::from(value.to_bits())),
        Type::F64 => normalized
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(f64::to_bits),
        _ => None,
    }
}

fn canonical_float_pattern_bits(bits: u64) -> u64 {
    if matches!(bits, 0x8000_0000 | 0x8000_0000_0000_0000) {
        0
    } else {
        bits
    }
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
        TypeSyntax::List { item, .. }
        | TypeSyntax::Array { item, .. }
        | TypeSyntax::Slice { item, .. } => collect_type_variables(item, output),
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
        TypeSyntax::Projection { argument, .. } => collect_type_variables(argument, output),
        TypeSyntax::SelfType { .. } | TypeSyntax::Primitive { .. } | TypeSyntax::Atom { .. } => {}
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
    Float(u64),
    Atom(String),
    Union(TypeId),
    Tuple(usize),
    ListEmpty,
    ListCons,
    Struct(DeclId),
    Bitstring(String),
}

fn pattern_is_useful(
    matrix: &[Vec<PatternShape>],
    query: Vec<PatternShape>,
    types_to_match: Vec<TypeId>,
    types: &[Type],
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
                && !pattern_is_useful(&defaults, query[1..].to_vec(), tail_types.to_vec(), types)
            {
                return false;
            }
            if let Some(constructors) = complete_constructors(head_ty, matrix, types) {
                constructors.into_iter().any(|constructor| {
                    let component_types =
                        constructor_component_types(&constructor, head_ty, matrix, types);
                    let specialized = specialize_matrix(matrix, &constructor);
                    let mut specialized_query = vec![PatternShape::Wildcard; component_types.len()];
                    specialized_query.extend_from_slice(&query[1..]);
                    let mut specialized_types = component_types;
                    specialized_types.extend_from_slice(tail_types);
                    pattern_is_useful(&specialized, specialized_query, specialized_types, types)
                })
            } else {
                pattern_is_useful(&defaults, query[1..].to_vec(), tail_types.to_vec(), types)
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
            pattern_is_useful(&specialized, specialized_query, specialized_types, types)
        }
    }
}

fn complete_constructors(
    ty: TypeId,
    matrix: &[Vec<PatternShape>],
    types: &[Type],
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
        Type::Struct { declaration, .. } => Some(vec![PatternConstructor::Struct(*declaration)]),
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
        PatternConstructor::Union(member) => vec![*member],
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
        PatternConstructor::Union(_) => 1,
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
        PatternShape::Float(value) => Some(PatternConstructor::Float(*value)),
        PatternShape::Atom(value) => Some(PatternConstructor::Atom(value.clone())),
        PatternShape::Union(member, _) => Some(PatternConstructor::Union(*member)),
        PatternShape::Tuple(elements) => Some(PatternConstructor::Tuple(elements.len())),
        PatternShape::ListEmpty => Some(PatternConstructor::ListEmpty),
        PatternShape::ListCons(_, _) => Some(PatternConstructor::ListCons),
        PatternShape::Struct(declaration, _) => Some(PatternConstructor::Struct(*declaration)),
        PatternShape::Bitstring(shape) => Some(PatternConstructor::Bitstring(shape.clone())),
    }
}

fn shape_components(shape: &PatternShape) -> Vec<(TypeId, PatternShape)> {
    match shape {
        PatternShape::Union(_, payload) => vec![(**payload).clone()],
        PatternShape::Tuple(elements) | PatternShape::Struct(_, elements) => elements.clone(),
        PatternShape::ListCons(head, tail) => vec![(**head).clone(), (**tail).clone()],
        _ => Vec::new(),
    }
}

fn bitstring_pattern_shape(segments: &[TypedBitstringPatternSegment]) -> String {
    segments
        .iter()
        .map(|segment| {
            let (pattern, _) = verified_pattern_shape(&segment.pattern);
            match &segment.kind {
                TypedBitstringPatternSegmentKind::Integer {
                    signed,
                    byte_order,
                    width,
                } => format!("i:{signed}:{byte_order:?}:{width}:{pattern:?}"),
                TypedBitstringPatternSegmentKind::Bytes { size } => {
                    let size = match size {
                        None => "rest".to_owned(),
                        Some(size) => match &size.kind {
                            TypedExprKind::Integer(value) => format!("literal:{value}"),
                            TypedExprKind::Local(symbol) => format!("local:{}", symbol.0),
                            _ => format!(
                                "expr:{}:{}:{}",
                                size.span.file().as_u32(),
                                size.span.start(),
                                size.span.end()
                            ),
                        },
                    };
                    format!("b:{size}:{pattern:?}")
                }
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn verified_pattern_shape(pattern: &TypedPattern) -> (PatternShape, bool) {
    match &pattern.kind {
        TypedPatternKind::Wildcard | TypedPatternKind::Binding { .. } => {
            (PatternShape::Wildcard, true)
        }
        TypedPatternKind::Boolean(value) => (PatternShape::Bool(*value), false),
        TypedPatternKind::Integer(value) => (PatternShape::Integer(*value), false),
        TypedPatternKind::Float(bits) => (
            PatternShape::Float(canonical_float_pattern_bits(*bits)),
            false,
        ),
        TypedPatternKind::Atom(value) => (PatternShape::Atom(value.clone()), true),
        TypedPatternKind::UnionMember { member, .. } => (
            PatternShape::Union(*member, Box::new((*member, PatternShape::Wildcard))),
            false,
        ),
        TypedPatternKind::StructuralUnionMember { member, pattern } => {
            let (shape, _) = verified_pattern_shape(pattern);
            (
                PatternShape::Union(*member, Box::new((*member, shape))),
                false,
            )
        }
        TypedPatternKind::Tuple(elements) => {
            let mut irrefutable = true;
            let children = elements
                .iter()
                .map(|child| {
                    let (shape, child_irrefutable) = verified_pattern_shape(child);
                    irrefutable &= child_irrefutable;
                    (child.ty, shape)
                })
                .collect();
            (PatternShape::Tuple(children), irrefutable)
        }
        TypedPatternKind::ListEmpty => (PatternShape::ListEmpty, false),
        TypedPatternKind::ListCons { head, tail } => {
            let (head_shape, _) = verified_pattern_shape(head);
            let (tail_shape, _) = verified_pattern_shape(tail);
            (
                PatternShape::ListCons(
                    Box::new((head.ty, head_shape)),
                    Box::new((tail.ty, tail_shape)),
                ),
                false,
            )
        }
        TypedPatternKind::Struct {
            declaration,
            field_count,
            fields,
        } => {
            let mut shapes = vec![(TypeId(3), PatternShape::Wildcard); *field_count];
            let mut irrefutable = true;
            for (index, child) in fields {
                if let Some(slot) = shapes.get_mut(*index) {
                    let (shape, child_irrefutable) = verified_pattern_shape(child);
                    irrefutable &= child_irrefutable;
                    *slot = (child.ty, shape);
                }
            }
            (PatternShape::Struct(*declaration, shapes), irrefutable)
        }
        TypedPatternKind::Bitstring(segments) => {
            let irrefutable = matches!(segments.as_slice(), [TypedBitstringPatternSegment {
                pattern,
                kind: TypedBitstringPatternSegmentKind::Bytes { size: None },
            }] if verified_pattern_shape(pattern).1);
            if irrefutable {
                (PatternShape::Wildcard, true)
            } else {
                (
                    PatternShape::Bitstring(bitstring_pattern_shape(segments)),
                    false,
                )
            }
        }
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
    for implementation in &program.implementations {
        let mut method_names = BTreeSet::new();
        let mut method_declarations = BTreeSet::new();
        for (name, declaration) in &implementation.method_declarations {
            if !implementation.methods.contains(name) {
                errors.push(format!(
                    "implementation {:?} references unknown method `{name}`",
                    implementation.id
                ));
            }
            if !method_names.insert(name) {
                errors.push(format!(
                    "implementation {:?} references method `{name}` twice",
                    implementation.id
                ));
            }
            if !method_declarations.insert(declaration) {
                errors.push(format!(
                    "implementation {:?} references declaration {declaration:?} twice",
                    implementation.id
                ));
            }
            if !declarations.contains(declaration) {
                errors.push(format!(
                    "implementation {:?} references missing declaration {declaration:?}",
                    implementation.id
                ));
            }
        }
    }
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
                mutable_symbols,
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
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
        }
        TypedItem::StructFieldAssign {
            symbol,
            declaration,
            value,
            ..
        } => {
            match symbols.get(symbol).and_then(|ty| types.get(ty.0 as usize)) {
                Some(Type::Struct {
                    declaration: found, ..
                }) if found == declaration => {}
                Some(_) => errors.push(format!(
                    "field assignment to {symbol:?} has an incorrect struct type"
                )),
                None => errors.push(format!(
                    "field assignment references unknown symbol {symbol:?}"
                )),
            }
            if !mutable_symbols.contains(symbol) {
                errors.push(format!(
                    "field assignment targets immutable symbol {symbol:?}"
                ));
            }
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
        }
        TypedItem::Expr(expression) | TypedItem::Return(expression) => {
            verify_expr(
                expression,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
        }
        TypedItem::While {
            condition, body, ..
        } => {
            verify_expr(
                condition,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if condition.ty != TypeId(2) {
                errors.push("while condition has a non-bool type".to_owned());
            }
            let mut nested_symbols = symbols.clone();
            let mut nested_mutable = mutable_symbols.clone();
            for item in &body.items {
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
            if body.ty != TypeId(3) {
                errors.push("while body has a non-unit type".to_owned());
            }
        }
        TypedItem::For {
            pattern,
            iterable,
            body,
            ..
        } => {
            verify_expr(
                iterable,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let mut nested_symbols = symbols.clone();
            verify_pattern(
                pattern,
                types,
                type_count,
                declarations,
                &mut nested_symbols,
                mutable_symbols,
                errors,
            );
            if !pattern.facts.irrefutable {
                errors.push("for pattern is refutable".to_owned());
            }
            let mut nested_mutable = mutable_symbols.clone();
            for item in &body.items {
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
            if body.ty != TypeId(3) {
                errors.push("for body has a non-unit type".to_owned());
            }
        }
        TypedItem::DeferCall {
            function,
            arguments,
            ..
        } => {
            if !declarations.contains(function) {
                errors.push(format!(
                    "deferred call references unknown function {function:?}"
                ));
            }
            for argument in arguments {
                verify_expr(
                    argument,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
        }
        TypedItem::DeferBlock { captures, body, .. } => {
            let mut nested_symbols = BTreeMap::new();
            let mut seen_sources = BTreeSet::new();
            for capture in captures {
                match symbols.get(&capture.source) {
                    Some(ty) if *ty != capture.ty => {
                        errors.push("deferred capture source has an incorrect type".to_owned())
                    }
                    None => errors.push("deferred capture references an unknown source".to_owned()),
                    _ => {}
                }
                if capture.source == capture.symbol
                    || !seen_sources.insert(capture.source)
                    || nested_symbols.insert(capture.symbol, capture.ty).is_some()
                {
                    errors.push("deferred capture identities are not unique".to_owned());
                }
            }
            let mut nested_mutable = BTreeSet::new();
            for item in &body.items {
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
            if body.ty != TypeId(3) {
                errors.push("deferred block has a non-unit type".to_owned());
            }
            if deferred_block_has_forbidden_item(body) {
                errors.push("deferred block contains return or nested defer".to_owned());
            }
        }
    }
}

fn deferred_block_has_forbidden_item(block: &TypedBlock) -> bool {
    block.items.iter().any(|item| match item {
        TypedItem::Return(_) | TypedItem::DeferCall { .. } | TypedItem::DeferBlock { .. } => true,
        TypedItem::While { body, .. } => deferred_block_has_forbidden_item(body),
        TypedItem::Expr(TypedExpr {
            kind:
                TypedExprKind::If {
                    then_block,
                    else_block,
                    ..
                },
            ..
        }) => {
            deferred_block_has_forbidden_item(then_block)
                || else_block
                    .as_ref()
                    .is_some_and(deferred_block_has_forbidden_item)
        }
        TypedItem::Expr(TypedExpr {
            kind: TypedExprKind::Match { arms, .. },
            ..
        }) => arms
            .iter()
            .any(|arm| deferred_block_has_forbidden_item(&arm.body)),
        TypedItem::For { body, .. } => deferred_block_has_forbidden_item(body),
        _ => false,
    })
}

fn collect_block_locals(block: &TypedBlock, output: &mut BTreeSet<SymbolId>) {
    for item in &block.items {
        match item {
            TypedItem::Let { initializer, .. } => collect_expr_locals(initializer, output),
            TypedItem::Assign { symbol, value, .. } => {
                output.insert(*symbol);
                collect_expr_locals(value, output);
            }
            TypedItem::StructFieldAssign { symbol, value, .. } => {
                output.insert(*symbol);
                collect_expr_locals(value, output);
            }
            TypedItem::Expr(expression) | TypedItem::Return(expression) => {
                collect_expr_locals(expression, output);
            }
            TypedItem::While {
                condition, body, ..
            } => {
                collect_expr_locals(condition, output);
                collect_block_locals(body, output);
            }
            TypedItem::For { iterable, body, .. } => {
                collect_expr_locals(iterable, output);
                collect_block_locals(body, output);
            }
            TypedItem::DeferCall { arguments, .. } => {
                for argument in arguments {
                    collect_expr_locals(argument, output);
                }
            }
            TypedItem::DeferBlock { captures, .. } => {
                output.extend(captures.iter().map(|capture| capture.source));
            }
        }
    }
}

fn collect_expr_locals(expression: &TypedExpr, output: &mut BTreeSet<SymbolId>) {
    match &expression.kind {
        TypedExprKind::StandardCall { arguments, .. } => {
            for argument in arguments {
                collect_expr_locals(argument, output);
            }
        }
        TypedExprKind::Local(symbol) => {
            output.insert(*symbol);
        }
        TypedExprKind::List { elements, tail } => {
            for element in elements {
                collect_expr_locals(element, output);
            }
            if let Some(tail) = tail {
                collect_expr_locals(tail, output);
            }
        }
        TypedExprKind::ListReverse(value) | TypedExprKind::MapToList(value) => {
            collect_expr_locals(value, output)
        }
        TypedExprKind::Array(elements) | TypedExprKind::Tuple(elements) => {
            for element in elements {
                collect_expr_locals(element, output);
            }
        }
        TypedExprKind::Bitstring(segments) => {
            for segment in segments {
                collect_expr_locals(&segment.value, output);
                if let TypedBitstringSegmentKind::Bytes { size: Some(size) } = &segment.kind {
                    collect_expr_locals(size, output);
                }
            }
        }
        TypedExprKind::Map(entries) => {
            for (key, value) in entries {
                collect_expr_locals(key, output);
                collect_expr_locals(value, output);
            }
        }
        TypedExprKind::MapPut { map, key, value } => {
            for child in [map.as_ref(), key.as_ref(), value.as_ref()] {
                collect_expr_locals(child, output);
            }
        }
        TypedExprKind::MapRemove { map, key } => {
            collect_expr_locals(map, output);
            collect_expr_locals(key, output);
        }
        TypedExprKind::MapFetch { map, key } => {
            collect_expr_locals(map, output);
            collect_expr_locals(key, output);
        }
        TypedExprKind::Struct { fields, .. } => {
            for (_, value) in fields {
                collect_expr_locals(value, output);
            }
        }
        TypedExprKind::StructProject { value, .. } => collect_expr_locals(value, output),
        TypedExprKind::Index { value, index, .. } => {
            collect_expr_locals(value, output);
            collect_expr_locals(index, output);
        }
        TypedExprKind::SliceFromArray { value, .. }
        | TypedExprKind::SliceCopy(value)
        | TypedExprKind::StringBytes(value)
        | TypedExprKind::StringCodepoints(value)
        | TypedExprKind::StringCodepointView(value)
        | TypedExprKind::StringGraphemeView(value)
        | TypedExprKind::StringLength(value)
        | TypedExprKind::StringEmpty(value)
        | TypedExprKind::StringFromBytes(value)
        | TypedExprKind::Utf8ErrorOffset(value)
        | TypedExprKind::RuneToString(value)
        | TypedExprKind::IntegerToString(value)
        | TypedExprKind::BooleanToString(value)
        | TypedExprKind::BufferToBytes(value)
        | TypedExprKind::BufferToString(value)
        | TypedExprKind::BytesToBits(value)
        | TypedExprKind::BitsToBytes(value)
        | TypedExprKind::BytesFromList(value)
        | TypedExprKind::BytesToList(value)
        | TypedExprKind::EnumToList(value)
        | TypedExprKind::CollectionLength { value, .. } => collect_expr_locals(value, output),
        TypedExprKind::StringContains { string, pattern } => {
            collect_expr_locals(string, output);
            collect_expr_locals(pattern, output);
        }
        TypedExprKind::StringSplit { string, separator } => {
            collect_expr_locals(string, output);
            collect_expr_locals(separator, output);
        }
        TypedExprKind::EnumAt { value, index } => {
            collect_expr_locals(value, output);
            collect_expr_locals(index, output);
        }
        TypedExprKind::EnumVisit {
            value,
            initial,
            function,
            ..
        } => {
            collect_expr_locals(value, output);
            if let Some(initial) = initial {
                collect_expr_locals(initial, output);
            }
            collect_expr_locals(function, output);
        }
        TypedExprKind::BytesSlice {
            value,
            start,
            length,
        } => {
            collect_expr_locals(value, output);
            collect_expr_locals(start, output);
            collect_expr_locals(length, output);
        }
        TypedExprKind::BitsSlice {
            value,
            start,
            length,
        } => {
            collect_expr_locals(value, output);
            collect_expr_locals(start, output);
            collect_expr_locals(length, output);
        }
        TypedExprKind::BufferAppend { buffer, value, .. } => {
            collect_expr_locals(buffer, output);
            collect_expr_locals(value, output);
        }
        TypedExprKind::ShowConstant { value, .. } => collect_expr_locals(value, output),
        TypedExprKind::SliceSubslice {
            value,
            start,
            length,
        } => {
            collect_expr_locals(value, output);
            collect_expr_locals(start, output);
            collect_expr_locals(length, output);
        }
        TypedExprKind::If {
            condition,
            then_block,
            else_block,
        } => {
            collect_expr_locals(condition, output);
            collect_block_locals(then_block, output);
            if let Some(block) = else_block {
                collect_block_locals(block, output);
            }
        }
        TypedExprKind::Match { subject, arms, .. } => {
            collect_expr_locals(subject, output);
            for arm in arms {
                collect_pattern_expr_locals(&arm.pattern, output);
                collect_block_locals(&arm.body, output);
            }
        }
        TypedExprKind::Ascription(value) | TypedExprKind::UnionInject { value, .. } => {
            collect_expr_locals(value, output);
        }
        TypedExprKind::IntegerUnary { operand, .. } => collect_expr_locals(operand, output),
        TypedExprKind::FloatNegate(operand) => collect_expr_locals(operand, output),
        TypedExprKind::IntegerConvert(value) => collect_expr_locals(value, output),
        TypedExprKind::NumericConvert(value) => collect_expr_locals(value, output),
        TypedExprKind::WrappingInteger { left, right, .. } => {
            collect_expr_locals(left, output);
            if let Some(right) = right {
                collect_expr_locals(right, output);
            }
        }
        TypedExprKind::Binary { left, right, .. }
        | TypedExprKind::IntegerBinary { left, right, .. }
        | TypedExprKind::Concat { left, right }
        | TypedExprKind::Comparison { left, right, .. }
        | TypedExprKind::Logical { left, right, .. } => {
            collect_expr_locals(left, output);
            collect_expr_locals(right, output);
        }
        TypedExprKind::Call { arguments, .. } => {
            for argument in arguments {
                collect_expr_locals(argument, output);
            }
        }
        TypedExprKind::IndirectCall { callee, arguments } => {
            collect_expr_locals(callee, output);
            for argument in arguments {
                collect_expr_locals(argument, output);
            }
        }
        TypedExprKind::Integer(_)
        | TypedExprKind::Float(_)
        | TypedExprKind::Boolean(_)
        | TypedExprKind::Unit
        | TypedExprKind::String(_)
        | TypedExprKind::Rune(_)
        | TypedExprKind::FunctionRef { .. }
        | TypedExprKind::BufferNew
        | TypedExprKind::Atom(_) => {}
    }
}

fn collect_pattern_expr_locals(pattern: &TypedPattern, output: &mut BTreeSet<SymbolId>) {
    match &pattern.kind {
        TypedPatternKind::Tuple(elements) => {
            for element in elements {
                collect_pattern_expr_locals(element, output);
            }
        }
        TypedPatternKind::ListCons { head, tail } => {
            collect_pattern_expr_locals(head, output);
            collect_pattern_expr_locals(tail, output);
        }
        TypedPatternKind::Struct { fields, .. } => {
            for (_, field) in fields {
                collect_pattern_expr_locals(field, output);
            }
        }
        TypedPatternKind::Bitstring(segments) => {
            for segment in segments {
                if let TypedBitstringPatternSegmentKind::Bytes { size: Some(size) } = &segment.kind
                {
                    collect_expr_locals(size, output);
                }
                collect_pattern_expr_locals(&segment.pattern, output);
            }
        }
        TypedPatternKind::StructuralUnionMember { pattern, .. } => {
            collect_pattern_expr_locals(pattern, output);
        }
        TypedPatternKind::Wildcard
        | TypedPatternKind::Binding { .. }
        | TypedPatternKind::Boolean(_)
        | TypedPatternKind::Integer(_)
        | TypedPatternKind::Float(_)
        | TypedPatternKind::Atom(_)
        | TypedPatternKind::UnionMember { .. }
        | TypedPatternKind::ListEmpty => {}
    }
}

fn verify_expr(
    expression: &TypedExpr,
    types: &[Type],
    type_count: u32,
    declarations: &BTreeSet<DeclId>,
    symbols: &BTreeMap<SymbolId, TypeId>,
    mutable_symbols: &BTreeSet<SymbolId>,
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
            if !types
                .get(expression.ty.0 as usize)
                .is_some_and(is_integer_type) =>
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
        TypedExprKind::String(_)
            if !matches!(types.get(expression.ty.0 as usize), Some(Type::String)) =>
        {
            errors.push("string expression has a non-string type".to_owned());
        }
        TypedExprKind::Rune(_)
            if !matches!(types.get(expression.ty.0 as usize), Some(Type::Rune)) =>
        {
            errors.push("rune expression has a non-rune type".to_owned());
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
                verify_expr(
                    element,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
                if item_type.is_some_and(|item| element.ty != item) {
                    errors.push("list element has an incorrect type".to_owned());
                }
            }
            if let Some(tail) = tail {
                verify_expr(
                    tail,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
                if tail.ty != expression.ty {
                    errors.push("list tail has an incorrect type".to_owned());
                }
            }
        }
        TypedExprKind::ListReverse(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if value.ty != expression.ty
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::List(_)))
            {
                errors.push("list reverse has incorrect types".to_owned());
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
                verify_expr(
                    element,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
                if element_types.is_some_and(|types| element.ty != types[index]) {
                    errors.push("tuple element has an incorrect type".to_owned());
                }
            }
        }
        TypedExprKind::Struct {
            declaration,
            field_count,
            fields,
        } => {
            if !matches!(types.get(expression.ty.0 as usize), Some(Type::Struct { declaration: found, .. }) if found == declaration)
            {
                errors.push("struct expression has an incorrect nominal type".to_owned());
            }
            let mut seen = BTreeSet::new();
            for (index, value) in fields {
                if *index >= *field_count || !seen.insert(*index) {
                    errors.push("struct expression has an invalid field index".to_owned());
                }
                verify_expr(
                    value,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
            if seen.len() != *field_count {
                errors.push("struct expression does not initialize every field".to_owned());
            }
        }
        TypedExprKind::StructProject {
            value, declaration, ..
        } => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::Struct { declaration: found, .. }) if found == declaration)
            {
                errors.push("struct projection has an incorrect source type".to_owned());
            }
        }
        TypedExprKind::Index {
            value,
            index,
            length,
        } => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            verify_expr(
                index,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(index.ty.0 as usize), Some(Type::Usize)) {
                errors.push("index expression has a non-usize index".to_owned());
            }
            match (types.get(value.ty.0 as usize), length) {
                (
                    Some(Type::Array {
                        item,
                        length: source_length,
                    }),
                    Some(length),
                ) if *item == expression.ty && source_length == length => {}
                (Some(Type::Slice(item)), None) if *item == expression.ty => {}
                (Some(Type::Bytes), None)
                    if matches!(types.get(expression.ty.0 as usize), Some(Type::U8)) => {}
                (Some(Type::Bits), None)
                    if matches!(types.get(expression.ty.0 as usize), Some(Type::Bool)) => {}
                _ => {
                    errors.push("index expression has an invalid source or result type".to_owned())
                }
            }
        }
        TypedExprKind::SliceFromArray { value, length } => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!((types.get(value.ty.0 as usize), types.get(expression.ty.0 as usize)),
                (Some(Type::Array { item: source, length: source_length }), Some(Type::Slice(item)))
                    if source == item && source_length == length)
            {
                errors.push("slice construction has incorrect types".to_owned());
            }
        }
        TypedExprKind::SliceSubslice {
            value,
            start,
            length,
        } => {
            for child in [value.as_ref(), start.as_ref(), length.as_ref()] {
                verify_expr(
                    child,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
            if value.ty != expression.ty
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::Slice(_)))
                || !matches!(types.get(start.ty.0 as usize), Some(Type::Usize))
                || !matches!(types.get(length.ty.0 as usize), Some(Type::Usize))
            {
                errors.push("subslice expression has incorrect types".to_owned());
            }
        }
        TypedExprKind::SliceCopy(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if value.ty != expression.ty
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::Slice(_)))
            {
                errors.push("slice copy has incorrect types".to_owned());
            }
        }
        TypedExprKind::StringBytes(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::String))
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::Bytes))
            {
                errors.push("string bytes conversion has incorrect types".to_owned());
            }
        }
        TypedExprKind::StringCodepoints(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::String))
                || !matches!(
                    types.get(expression.ty.0 as usize),
                    Some(Type::List(item)) if matches!(types.get(item.0 as usize), Some(Type::Rune))
                )
            {
                errors.push("string codepoints conversion has incorrect types".to_owned());
            }
        }
        TypedExprKind::StringCodepointView(value) | TypedExprKind::StringGraphemeView(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let valid = matches!(types.get(value.ty.0 as usize), Some(Type::String))
                && matches!(
                    (&expression.kind, types.get(expression.ty.0 as usize)),
                    (
                        TypedExprKind::StringCodepointView(_),
                        Some(Type::CodepointView)
                    ) | (
                        TypedExprKind::StringGraphemeView(_),
                        Some(Type::GraphemeView)
                    )
                );
            if !valid {
                errors.push("string lazy view has incorrect types".to_owned());
            }
        }
        TypedExprKind::StringLength(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::String))
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::Usize))
            {
                errors.push("string grapheme length has incorrect types".to_owned());
            }
        }
        TypedExprKind::Bitstring(segments) => {
            if !matches!(types.get(expression.ty.0 as usize), Some(Type::Bytes)) {
                errors.push("bitstring construction has a non-bytes type".to_owned());
            }
            for segment in segments {
                verify_expr(
                    &segment.value,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
                match &segment.kind {
                    TypedBitstringSegmentKind::Integer { width, .. } => {
                        if !types
                            .get(segment.value.ty.0 as usize)
                            .is_some_and(is_integer_type)
                            || !matches!(*width, 8 | 16 | 24 | 32 | 40 | 48 | 56 | 64)
                        {
                            errors.push("integer bitstring segment is invalid".to_owned());
                        }
                    }
                    TypedBitstringSegmentKind::Bytes { size } => {
                        if !matches!(types.get(segment.value.ty.0 as usize), Some(Type::Bytes)) {
                            errors.push("bytes bitstring segment has a non-bytes value".to_owned());
                        }
                        if let Some(size) = size {
                            verify_expr(
                                size,
                                types,
                                type_count,
                                declarations,
                                symbols,
                                mutable_symbols,
                                errors,
                            );
                            if !matches!(types.get(size.ty.0 as usize), Some(Type::Usize)) {
                                errors.push(
                                    "bytes bitstring segment has a non-usize size".to_owned(),
                                );
                            }
                        }
                    }
                }
            }
        }
        TypedExprKind::StringFromBytes(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let valid_result = matches!(types.get(expression.ty.0 as usize), Some(Type::Union(members)) if {
                let has = |tag: &str, payload: fn(&Type) -> bool| members.iter().any(|member| {
                    matches!(types.get(member.0 as usize), Some(Type::Tuple(fields)) if fields.len() == 2
                        && matches!(types.get(fields[0].0 as usize), Some(Type::Atom(found)) if found == tag)
                        && types.get(fields[1].0 as usize).is_some_and(payload))
                });
                has("ok", |ty| matches!(ty, Type::String))
                    && has("error", |ty| matches!(ty, Type::Utf8Error))
            });
            if !matches!(types.get(value.ty.0 as usize), Some(Type::Bytes)) || !valid_result {
                errors.push("string from-bytes conversion has incorrect types".to_owned());
            }
        }
        TypedExprKind::Utf8ErrorOffset(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::Utf8Error))
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::Usize))
            {
                errors.push("UTF-8 error offset access has incorrect types".to_owned());
            }
        }
        TypedExprKind::RuneToString(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::Rune))
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::String))
            {
                errors.push("rune to-string conversion has incorrect types".to_owned());
            }
        }
        TypedExprKind::IntegerToString(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !types.get(value.ty.0 as usize).is_some_and(is_integer_type)
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::String))
            {
                errors.push("integer to-string has invalid types".to_owned());
            }
        }
        TypedExprKind::BooleanToString(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::Bool))
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::String))
            {
                errors.push("boolean to-string has invalid types".to_owned());
            }
        }
        TypedExprKind::ShowConstant { value, .. } => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(expression.ty.0 as usize), Some(Type::String)) {
                errors.push("constant Show conversion must produce string".to_owned());
            }
        }
        TypedExprKind::BufferNew => {
            if !matches!(types.get(expression.ty.0 as usize), Some(Type::Buffer)) {
                errors.push("buffer construction has an incorrect type".to_owned());
            }
        }
        TypedExprKind::BufferAppend {
            buffer,
            value,
            kind,
        } => {
            verify_expr(
                buffer,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let valid_value = match kind {
                BufferAppendKind::Byte => matches!(types.get(value.ty.0 as usize), Some(Type::U8)),
                BufferAppendKind::Bytes => {
                    matches!(types.get(value.ty.0 as usize), Some(Type::Bytes))
                }
                BufferAppendKind::String => {
                    matches!(types.get(value.ty.0 as usize), Some(Type::String))
                }
            };
            if buffer.ty != expression.ty
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::Buffer))
                || !valid_value
            {
                errors.push("buffer append has incorrect types".to_owned());
            }
        }
        TypedExprKind::BufferToBytes(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::Buffer))
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::Bytes))
            {
                errors.push("buffer to-bytes conversion has incorrect types".to_owned());
            }
        }
        TypedExprKind::BufferToString(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::Buffer))
                || !is_utf8_result_type(types, expression.ty)
            {
                errors.push("buffer to-string conversion has incorrect types".to_owned());
            }
        }
        TypedExprKind::BytesToBits(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::Bytes))
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::Bits))
            {
                errors.push("bytes to-bits conversion has incorrect types".to_owned());
            }
        }
        TypedExprKind::BitsToBytes(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::Bits))
                || !is_option_bytes_type(types, expression.ty)
            {
                errors.push("bits to-bytes conversion has incorrect types".to_owned());
            }
        }
        TypedExprKind::BitsSlice {
            value,
            start,
            length,
        } => {
            for child in [value.as_ref(), start.as_ref(), length.as_ref()] {
                verify_expr(
                    child,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
            if value.ty != expression.ty
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::Bits))
                || !matches!(types.get(start.ty.0 as usize), Some(Type::Usize))
                || !matches!(types.get(length.ty.0 as usize), Some(Type::Usize))
            {
                errors.push("bits slice has incorrect types".to_owned());
            }
        }
        TypedExprKind::BytesFromList(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(
                types.get(value.ty.0 as usize),
                Some(Type::List(item)) if matches!(types.get(item.0 as usize), Some(Type::U8))
            ) || !matches!(types.get(expression.ty.0 as usize), Some(Type::Bytes))
            {
                errors.push("bytes from-list conversion has incorrect types".to_owned());
            }
        }
        TypedExprKind::BytesToList(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(value.ty.0 as usize), Some(Type::Bytes))
                || !matches!(
                    types.get(expression.ty.0 as usize),
                    Some(Type::List(item)) if matches!(types.get(item.0 as usize), Some(Type::U8))
                )
            {
                errors.push("bytes to-list conversion has incorrect types".to_owned());
            }
        }
        TypedExprKind::BytesSlice {
            value,
            start,
            length,
        } => {
            for child in [value.as_ref(), start.as_ref(), length.as_ref()] {
                verify_expr(
                    child,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
            if value.ty != expression.ty
                || !matches!(types.get(expression.ty.0 as usize), Some(Type::Bytes))
                || !matches!(types.get(start.ty.0 as usize), Some(Type::Usize))
                || !matches!(types.get(length.ty.0 as usize), Some(Type::Usize))
            {
                errors.push("bytes slice has incorrect types".to_owned());
            }
        }
        TypedExprKind::CollectionLength {
            value,
            known_length,
        } => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let valid_source = match (types.get(value.ty.0 as usize), known_length) {
                (Some(Type::Array { length, .. }), Some(known)) => length == known,
                (
                    Some(
                        Type::String
                        | Type::Bytes
                        | Type::Bits
                        | Type::Buffer
                        | Type::List(_)
                        | Type::Slice(_)
                        | Type::Map { .. }
                        | Type::CodepointView
                        | Type::GraphemeView,
                    ),
                    None,
                ) => true,
                _ => false,
            };
            if !matches!(types.get(expression.ty.0 as usize), Some(Type::Usize)) || !valid_source {
                errors.push("collection length has incorrect types".to_owned());
            }
        }
        TypedExprKind::EnumAt { value, index } => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            verify_expr(
                index,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let payload = option_payload(types, expression.ty);
            let valid_item = match (types.get(value.ty.0 as usize), payload) {
                (
                    Some(Type::List(item) | Type::Array { item, .. } | Type::Slice(item)),
                    Some(payload),
                ) => *item == payload,
                (Some(Type::Bytes), Some(payload)) => {
                    matches!(types.get(payload.0 as usize), Some(Type::U8))
                }
                (Some(Type::Map { key, value }), Some(payload)) => matches!(
                    types.get(payload.0 as usize),
                    Some(Type::Tuple(fields)) if fields.as_slice() == [*key, *value]
                ),
                (Some(Type::CodepointView), Some(payload)) => {
                    matches!(types.get(payload.0 as usize), Some(Type::Rune))
                }
                (Some(Type::GraphemeView), Some(payload)) => {
                    matches!(types.get(payload.0 as usize), Some(Type::String))
                }
                _ => false,
            };
            if !matches!(types.get(index.ty.0 as usize), Some(Type::Usize)) || !valid_item {
                errors.push("Enum.at has incorrect types".to_owned());
            }
        }
        TypedExprKind::EnumToList(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let source_item = match types.get(value.ty.0 as usize) {
                Some(Type::Array { item, .. } | Type::Slice(item)) => Some(*item),
                Some(Type::CodepointView) => types
                    .iter()
                    .position(|ty| matches!(ty, Type::Rune))
                    .map(|index| TypeId(index as u32)),
                Some(Type::GraphemeView) => types
                    .iter()
                    .position(|ty| matches!(ty, Type::String))
                    .map(|index| TypeId(index as u32)),
                _ => None,
            };
            if !matches!(
                (source_item, types.get(expression.ty.0 as usize)),
                (Some(source), Some(Type::List(item))) if source == *item
            ) {
                errors.push("Enum.to_list has incorrect types".to_owned());
            }
        }
        TypedExprKind::EnumVisit {
            value,
            initial,
            function,
            kind,
        } => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if let Some(initial) = initial {
                verify_expr(
                    initial,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
            verify_expr(
                function,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let item = match types.get(value.ty.0 as usize) {
                Some(Type::List(item) | Type::Array { item, .. } | Type::Slice(item)) => {
                    Some(*item)
                }
                Some(Type::Bytes) => types
                    .iter()
                    .position(|ty| matches!(ty, Type::U8))
                    .map(|index| TypeId(index as u32)),
                Some(Type::Map { key, value }) => types
                    .iter()
                    .position(|ty| matches!(ty, Type::Tuple(fields) if fields.as_slice() == [*key, *value]))
                    .map(|index| TypeId(index as u32)),
                Some(Type::CodepointView) => types.iter().position(|ty| matches!(ty, Type::Rune)).map(|index| TypeId(index as u32)),
                Some(Type::GraphemeView) => types.iter().position(|ty| matches!(ty, Type::String)).map(|index| TypeId(index as u32)),
                _ => None,
            };
            let valid_result = match kind {
                EnumVisitKind::Each => {
                    initial.is_none()
                        && matches!(types.get(expression.ty.0 as usize), Some(Type::Unit))
                }
                EnumVisitKind::Any | EnumVisitKind::All => {
                    initial.is_none()
                        && matches!(types.get(expression.ty.0 as usize), Some(Type::Bool))
                }
                EnumVisitKind::Reduce => initial
                    .as_ref()
                    .is_some_and(|initial| initial.ty == expression.ty),
                EnumVisitKind::Filter => {
                    initial.is_none()
                        && item.is_some_and(|item| {
                            matches!(types.get(expression.ty.0 as usize), Some(Type::List(result)) if *result == item)
                        })
                }
                EnumVisitKind::Map => {
                    initial.is_none()
                        && matches!(types.get(expression.ty.0 as usize), Some(Type::List(_)))
                }
            };
            let valid_function = item.is_some_and(|item| {
                matches!(
                    types.get(function.ty.0 as usize),
                    Some(Type::Function { parameters, result })
                        if (match kind {
                                EnumVisitKind::Filter => matches!(types.get(result.0 as usize), Some(Type::Bool)),
                                EnumVisitKind::Map => matches!(types.get(expression.ty.0 as usize), Some(Type::List(item)) if item == result),
                                _ => *result == expression.ty,
                            })
                            && if *kind == EnumVisitKind::Reduce {
                                parameters.as_slice() == [expression.ty, item]
                            } else {
                                parameters.as_slice() == [item]
                            }
                )
            });
            if !valid_function || !valid_result {
                errors.push("Enum visit has incorrect types".to_owned());
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
                verify_expr(
                    element,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
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
                verify_expr(
                    key,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
                verify_expr(
                    value,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
                if components.is_some_and(|pair| pair != (key.ty, value.ty)) {
                    errors.push("map entry has incorrect types".to_owned());
                }
            }
        }
        TypedExprKind::MapPut { map, key, value } => {
            for child in [map.as_ref(), key.as_ref(), value.as_ref()] {
                verify_expr(
                    child,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
            match types.get(expression.ty.0 as usize) {
                Some(Type::Map {
                    key: expected_key,
                    value: expected_value,
                }) if map.ty == expression.ty
                    && key.ty == *expected_key
                    && value.ty == *expected_value => {}
                _ => errors.push("map put expression has incorrect types".to_owned()),
            }
        }
        TypedExprKind::MapRemove { map, key } => {
            for child in [map.as_ref(), key.as_ref()] {
                verify_expr(
                    child,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
            match types.get(expression.ty.0 as usize) {
                Some(Type::Map {
                    key: expected_key, ..
                }) if map.ty == expression.ty && key.ty == *expected_key => {}
                _ => errors.push("map remove expression has incorrect types".to_owned()),
            }
        }
        TypedExprKind::MapFetch { map, key } => {
            for child in [map.as_ref(), key.as_ref()] {
                verify_expr(
                    child,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
            let valid = match types.get(map.ty.0 as usize) {
                Some(Type::Map {
                    key: expected_key,
                    value,
                }) if key.ty == *expected_key => {
                    matches!(
                        types.get(expression.ty.0 as usize),
                        Some(Type::Union(members)) if members.iter().any(|member| {
                            matches!(types.get(member.0 as usize), Some(Type::Atom(name)) if name == "none")
                        }) && members.iter().any(|member| {
                            matches!(types.get(member.0 as usize), Some(Type::Tuple(items)) if items.len() == 2
                                && items[1] == *value
                                && matches!(types.get(items[0].0 as usize), Some(Type::Atom(name)) if name == "some"))
                        })
                    )
                }
                _ => false,
            };
            if !valid {
                errors.push("map fetch expression has incorrect types".to_owned());
            }
        }
        TypedExprKind::MapToList(map) => {
            verify_expr(
                map,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let valid = match (
                types.get(map.ty.0 as usize),
                types.get(expression.ty.0 as usize),
            ) {
                (Some(Type::Map { key, value }), Some(Type::List(pair))) => {
                    matches!(types.get(pair.0 as usize), Some(Type::Tuple(items)) if items.as_slice() == [*key, *value])
                }
                _ => false,
            };
            if !valid {
                errors.push("map to-list expression has incorrect types".to_owned());
            }
        }
        TypedExprKind::If {
            condition,
            then_block,
            else_block,
        } => {
            verify_expr(
                condition,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !matches!(types.get(condition.ty.0 as usize), Some(Type::Bool)) {
                errors.push("if condition has a non-bool type".to_owned());
            }
            let mut verify_nested = |block: &TypedBlock| {
                let mut nested_symbols = symbols.clone();
                let mut nested_mutable = mutable_symbols.clone();
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
            verify_expr(
                subject,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let mut matrix = Vec::<Vec<PatternShape>>::new();
            for arm in arms {
                let (shape, irrefutable) = verified_pattern_shape(&arm.pattern);
                let useful =
                    pattern_is_useful(&matrix, vec![shape.clone()], vec![subject.ty], types);
                if arm.pattern.ty != subject.ty
                    || arm.pattern.facts.reachable != useful
                    || arm.pattern.facts.irrefutable != irrefutable
                {
                    errors.push("match pattern facts are inconsistent".to_owned());
                }
                matrix.push(vec![shape]);
                let mut nested_symbols = symbols.clone();
                verify_pattern(
                    &arm.pattern,
                    types,
                    type_count,
                    declarations,
                    &mut nested_symbols,
                    mutable_symbols,
                    errors,
                );
                let mut nested_mutable = mutable_symbols.clone();
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
            let proven_exhaustive = !pattern_is_useful(
                &matrix,
                vec![PatternShape::Wildcard],
                vec![subject.ty],
                types,
            );
            if *exhaustive != proven_exhaustive {
                errors.push("typed match exhaustiveness fact is inconsistent".to_owned());
            }
        }
        TypedExprKind::Ascription(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if value.ty != expression.ty {
                errors.push("ascription changed the expression type".to_owned());
            }
        }
        TypedExprKind::Binary { left, right, .. } => {
            verify_expr(
                left,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            verify_expr(
                right,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if left.ty != right.ty
                || left.ty != expression.ty
                || !types
                    .get(left.ty.0 as usize)
                    .is_some_and(|ty| is_integer_type(ty) || matches!(ty, Type::F32 | Type::F64))
            {
                errors.push("binary expression has invalid numeric types".to_owned());
            }
        }
        TypedExprKind::FloatNegate(operand) => {
            verify_expr(
                operand,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if operand.ty != expression.ty
                || !matches!(
                    types.get(operand.ty.0 as usize),
                    Some(Type::F32 | Type::F64)
                )
            {
                errors.push("float negation has invalid types".to_owned());
            }
        }
        TypedExprKind::IntegerUnary { operator, operand } => {
            verify_expr(
                operand,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let signed = matches!(
                types.get(operand.ty.0 as usize),
                Some(Type::I8 | Type::I16 | Type::I32 | Type::I64 | Type::Isize)
            );
            if operand.ty != expression.ty
                || !types
                    .get(operand.ty.0 as usize)
                    .is_some_and(is_integer_type)
                || (matches!(operator, IntegerUnaryOperator::Negate) && !signed)
            {
                errors.push("integer unary expression has invalid types".to_owned());
            }
        }
        TypedExprKind::IntegerBinary {
            operator,
            left,
            right,
        } => {
            verify_expr(
                left,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            verify_expr(
                right,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let shift = matches!(
                operator,
                IntegerBinaryOperator::ShiftLeft | IntegerBinaryOperator::ShiftRight
            );
            if left.ty != expression.ty
                || !types.get(left.ty.0 as usize).is_some_and(is_integer_type)
                || if shift {
                    !matches!(types.get(right.ty.0 as usize), Some(Type::Usize))
                } else {
                    right.ty != left.ty
                }
            {
                errors.push("integer binary expression has invalid types".to_owned());
            }
        }
        TypedExprKind::IntegerConvert(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if !types.get(value.ty.0 as usize).is_some_and(is_integer_type)
                || !types
                    .get(expression.ty.0 as usize)
                    .is_some_and(|ty| is_integer_type(ty) || matches!(ty, Type::Rune))
            {
                errors.push("integer conversion has invalid types".to_owned());
            }
        }
        TypedExprKind::NumericConvert(value) => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let source = types.get(value.ty.0 as usize);
            let target = types.get(expression.ty.0 as usize);
            let source_numeric = source.is_some_and(|ty| is_integer_type(ty) || is_float_type(ty));
            let target_numeric = target.is_some_and(|ty| is_integer_type(ty) || is_float_type(ty));
            if !source_numeric
                || !target_numeric
                || !source.is_some_and(is_float_type) && !target.is_some_and(is_float_type)
            {
                errors.push("numeric conversion has invalid types".to_owned());
            }
        }
        TypedExprKind::Concat { left, right } => {
            verify_expr(
                left,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            verify_expr(
                right,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if left.ty != right.ty
                || left.ty != expression.ty
                || !matches!(
                    types.get(expression.ty.0 as usize),
                    Some(
                        Type::String
                            | Type::Bytes
                            | Type::Bits
                            | Type::List(_)
                            | Type::Parameter { .. }
                    )
                )
            {
                errors.push("concat expression has invalid types".to_owned());
            }
        }
        TypedExprKind::WrappingInteger {
            operator,
            left,
            right,
        } => {
            verify_expr(
                left,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if let Some(right) = right {
                verify_expr(
                    right,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
            let unary = matches!(operator, WrappingIntegerOperator::Negate);
            let shift = matches!(
                operator,
                WrappingIntegerOperator::ShiftLeft | WrappingIntegerOperator::ShiftRight
            );
            let valid_right = match (unary, right) {
                (true, None) => true,
                (false, Some(right)) if shift => {
                    matches!(types.get(right.ty.0 as usize), Some(Type::Usize))
                }
                (false, Some(right)) => right.ty == left.ty,
                _ => false,
            };
            if left.ty != expression.ty
                || !types.get(left.ty.0 as usize).is_some_and(is_integer_type)
                || !valid_right
            {
                errors.push("wrapping integer expression has invalid types".to_owned());
            }
        }
        TypedExprKind::Comparison {
            operator,
            left,
            right,
        } => {
            verify_expr(
                left,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            verify_expr(
                right,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let ordered = !matches!(
                operator,
                ComparisonOperator::Equal | ComparisonOperator::NotEqual
            );
            let supported = types.get(left.ty.0 as usize).is_some_and(|ty| {
                is_integer_type(ty) || matches!(ty, Type::Rune | Type::F32 | Type::F64)
            }) || if ordered {
                standard_ord_type(types, left.ty)
                    || matches!(types.get(left.ty.0 as usize), Some(Type::Parameter { .. }))
            } else {
                standard_eq_type(types, left.ty)
                    || matches!(types.get(left.ty.0 as usize), Some(Type::Union(members))
                        if members.iter().all(|member| matches!(types.get(member.0 as usize), Some(Type::Atom(_)))))
                    || matches!(
                        types.get(left.ty.0 as usize),
                        Some(Type::Struct { .. } | Type::Parameter { .. })
                    )
            };
            if left.ty != right.ty || expression.ty != TypeId(2) || !supported {
                errors.push("comparison has invalid operand or result types".to_owned());
            }
        }
        TypedExprKind::Logical { left, right, .. } => {
            verify_expr(
                left,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            verify_expr(
                right,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            if left.ty != TypeId(2) || right.ty != TypeId(2) || expression.ty != TypeId(2) {
                errors.push("logical expression has a non-bool type".to_owned());
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
                verify_expr(
                    argument,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
        }
        TypedExprKind::FunctionRef { function, .. } => {
            if !declarations.contains(function) {
                errors.push(format!(
                    "function value references unknown function {function:?}"
                ));
            }
            if !matches!(
                types.get(expression.ty.0 as usize),
                Some(Type::Function { .. })
            ) {
                errors.push("function value has a non-function type".to_owned());
            }
        }
        TypedExprKind::IndirectCall { callee, arguments } => {
            verify_expr(
                callee,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            let signature = match types.get(callee.ty.0 as usize) {
                Some(Type::Function { parameters, result }) => Some((parameters, *result)),
                _ => None,
            };
            for argument in arguments {
                verify_expr(
                    argument,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
            if !matches!(signature, Some((parameters, result))
                if parameters.len() == arguments.len()
                    && parameters.iter().zip(arguments).all(|(expected, argument)| *expected == argument.ty)
                    && result == expression.ty)
            {
                errors.push("indirect call does not match its exact function type".to_owned());
            }
        }
        TypedExprKind::UnionInject { member, value } => {
            verify_expr(
                value,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
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

fn option_payload(types: &[Type], ty: TypeId) -> Option<TypeId> {
    let Type::Union(members) = types.get(ty.0 as usize)? else {
        return None;
    };
    let has_none = members.iter().any(
        |member| matches!(types.get(member.0 as usize), Some(Type::Atom(name)) if name == "none"),
    );
    let payload = members.iter().find_map(|member| match types.get(member.0 as usize) {
        Some(Type::Tuple(fields))
            if fields.len() == 2
                && matches!(types.get(fields[0].0 as usize), Some(Type::Atom(name)) if name == "some") =>
        {
            Some(fields[1])
        }
        _ => None,
    });
    has_none.then_some(payload).flatten()
}

fn integer_bounds(ty: &Type) -> Option<(i128, i128)> {
    Some(match ty {
        Type::I8 => (i8::MIN.into(), i8::MAX.into()),
        Type::I16 => (i16::MIN.into(), i16::MAX.into()),
        Type::I32 => (i32::MIN.into(), i32::MAX.into()),
        Type::I64 => (i64::MIN.into(), i64::MAX.into()),
        Type::Isize => (isize::MIN as i128, isize::MAX as i128),
        Type::U8 => (0, u8::MAX.into()),
        Type::U16 => (0, u16::MAX.into()),
        Type::U32 => (0, u32::MAX.into()),
        Type::U64 => (0, u64::MAX.into()),
        Type::Usize => (0, usize::MAX as i128),
        _ => return None,
    })
}

fn integer_type_from_name(name: &str) -> Option<Type> {
    Some(match name {
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "isize" => Type::Isize,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        "usize" => Type::Usize,
        _ => return None,
    })
}

fn float_type_from_name(name: &str) -> Option<Type> {
    match name {
        "f32" => Some(Type::F32),
        "f64" => Some(Type::F64),
        _ => None,
    }
}

fn is_float_type(ty: &Type) -> bool {
    matches!(ty, Type::F32 | Type::F64)
}

fn float_fits_integer(value: f64, target: &Type) -> bool {
    let Some((minimum, maximum)) = integer_bounds(target) else {
        return false;
    };
    if !value.is_finite() {
        return false;
    }
    let minimum_float = minimum as f64;
    let lower_exclusive = if minimum == 0 {
        -1.0
    } else {
        minimum_float - 1.0
    };
    let upper_exclusive = maximum as f64 + 1.0;
    let above_lower = if lower_exclusive == minimum_float {
        value >= minimum_float
    } else {
        value > lower_exclusive
    };
    above_lower && value < upper_exclusive
}

fn wrapping_intrinsic(name: &str) -> Option<(Type, WrappingIntegerOperator)> {
    let (module, function) = name.split_once('.')?;
    let ty = match module {
        "I8" => Type::I8,
        "I16" => Type::I16,
        "I32" => Type::I32,
        "I64" => Type::I64,
        "Isize" => Type::Isize,
        "U8" => Type::U8,
        "U16" => Type::U16,
        "U32" => Type::U32,
        "U64" => Type::U64,
        "Usize" => Type::Usize,
        _ => return None,
    };
    let operator = match function {
        "wrapping_add" => WrappingIntegerOperator::Add,
        "wrapping_sub" => WrappingIntegerOperator::Subtract,
        "wrapping_mul" => WrappingIntegerOperator::Multiply,
        "wrapping_neg" => WrappingIntegerOperator::Negate,
        "wrapping_shl" => WrappingIntegerOperator::ShiftLeft,
        "wrapping_shr" => WrappingIntegerOperator::ShiftRight,
        _ => return None,
    };
    Some((ty, operator))
}

fn is_unicode_scalar(value: i128) -> bool {
    (0..=0x10_ffff).contains(&value) && !(0xd800..=0xdfff).contains(&value)
}

fn integer_bit_width(ty: &Type) -> Option<u32> {
    Some(match ty {
        Type::I8 | Type::U8 => 8,
        Type::I16 | Type::U16 => 16,
        Type::I32 | Type::U32 => 32,
        Type::I64 | Type::U64 => 64,
        Type::Isize | Type::Usize => usize::BITS,
        _ => return None,
    })
}

fn is_integer_type(ty: &Type) -> bool {
    integer_bounds(ty).is_some()
}

fn standard_eq_type(types: &[Type], ty: TypeId) -> bool {
    match types.get(ty.0 as usize) {
        Some(
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Isize
            | Type::Usize
            | Type::Bool
            | Type::Unit
            | Type::String
            | Type::Bytes
            | Type::Bits
            | Type::Rune
            | Type::Utf8Error
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
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

fn standard_ord_type(types: &[Type], ty: TypeId) -> bool {
    match types.get(ty.0 as usize) {
        Some(
            Type::I8
            | Type::I16
            | Type::I32
            | Type::I64
            | Type::Isize
            | Type::Usize
            | Type::Bool
            | Type::Unit
            | Type::String
            | Type::Bytes
            | Type::Bits
            | Type::Rune
            | Type::U8
            | Type::U16
            | Type::U32
            | Type::U64
            | Type::Atom(_)
            | Type::Struct { .. },
        ) => true,
        Some(Type::List(item) | Type::Slice(item)) => standard_ord_type(types, *item),
        Some(Type::Array { item, .. }) => standard_ord_type(types, *item),
        Some(Type::Tuple(items)) => items.iter().all(|item| standard_ord_type(types, *item)),
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
        has("ok", |member| matches!(member, Type::String))
            && has("error", |member| matches!(member, Type::Utf8Error))
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

fn verify_pattern(
    pattern: &TypedPattern,
    types: &[Type],
    type_count: u32,
    declarations: &BTreeSet<DeclId>,
    symbols: &mut BTreeMap<SymbolId, TypeId>,
    mutable_symbols: &BTreeSet<SymbolId>,
    errors: &mut Vec<String>,
) {
    if pattern.ty.0 >= type_count {
        errors.push("pattern has an unknown type".to_owned());
        return;
    }
    let (_, irrefutable) = verified_pattern_shape(pattern);
    if !pattern.facts.reachable || pattern.facts.irrefutable != irrefutable {
        errors.push("pattern facts are inconsistent".to_owned());
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
        TypedPatternKind::StructuralUnionMember {
            member,
            pattern: nested,
        } => {
            if !matches!(&types[pattern.ty.0 as usize], Type::Union(members) if members.contains(member))
                || nested.ty != *member
            {
                errors.push("structural union pattern names an invalid member".to_owned());
            }
            verify_pattern(
                nested,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
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
                verify_pattern(
                    child,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
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
            verify_pattern(
                head,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
            verify_pattern(
                tail,
                types,
                type_count,
                declarations,
                symbols,
                mutable_symbols,
                errors,
            );
        }
        TypedPatternKind::Struct {
            declaration,
            field_count,
            fields,
        } => {
            if !matches!(types[pattern.ty.0 as usize], Type::Struct { declaration: found, .. } if found == *declaration)
            {
                errors.push("struct pattern has an incorrect nominal type".to_owned());
            }
            let mut seen = BTreeSet::new();
            for (index, child) in fields {
                if *index >= *field_count || !seen.insert(*index) {
                    errors.push("struct pattern has an invalid field index".to_owned());
                }
                verify_pattern(
                    child,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
        }
        TypedPatternKind::Bitstring(segments) => {
            if !matches!(types.get(pattern.ty.0 as usize), Some(Type::Bytes)) {
                errors.push("bitstring pattern has a non-bytes type".to_owned());
            }
            for segment in segments {
                match &segment.kind {
                    TypedBitstringPatternSegmentKind::Integer { signed, width, .. } => {
                        let valid_ty = if *signed {
                            matches!(types.get(segment.pattern.ty.0 as usize), Some(Type::I64))
                        } else {
                            matches!(types.get(segment.pattern.ty.0 as usize), Some(Type::U64))
                        };
                        if !valid_ty || !matches!(*width, 8 | 16 | 24 | 32 | 40 | 48 | 56 | 64) {
                            errors.push("integer bitstring pattern segment is invalid".to_owned());
                        }
                    }
                    TypedBitstringPatternSegmentKind::Bytes { size } => {
                        if !matches!(types.get(segment.pattern.ty.0 as usize), Some(Type::Bytes)) {
                            errors.push("bytes bitstring pattern segment is invalid".to_owned());
                        }
                        if let Some(size) = size {
                            verify_expr(
                                size,
                                types,
                                type_count,
                                declarations,
                                symbols,
                                mutable_symbols,
                                errors,
                            );
                            if !matches!(types.get(size.ty.0 as usize), Some(Type::Usize)) {
                                errors.push("bitstring pattern size is not usize".to_owned());
                            }
                        }
                    }
                }
                verify_pattern(
                    &segment.pattern,
                    types,
                    type_count,
                    declarations,
                    symbols,
                    mutable_symbols,
                    errors,
                );
            }
        }
        TypedPatternKind::Float(bits) => {
            let valid = match types.get(pattern.ty.0 as usize) {
                Some(Type::F32) => *bits <= u64::from(u32::MAX),
                Some(Type::F64) => true,
                _ => false,
            };
            if !valid {
                errors.push("float pattern has an invalid type or bit pattern".to_owned());
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
            Type::I8 => "i8".to_owned(),
            Type::I16 => "i16".to_owned(),
            Type::I32 => "i32".to_owned(),
            Type::I64 => "i64".to_owned(),
            Type::Isize => "isize".to_owned(),
            Type::Usize => "usize".to_owned(),
            Type::Bool => "bool".to_owned(),
            Type::Unit => "unit".to_owned(),
            Type::String => "string".to_owned(),
            Type::Bytes => "bytes".to_owned(),
            Type::Bits => "bits".to_owned(),
            Type::Buffer => "Buffer".to_owned(),
            Type::Rune => "rune".to_owned(),
            Type::Utf8Error => "String.Utf8Error".to_owned(),
            Type::Opaque(kind) => opaque_type_name(*kind).to_owned(),
            Type::CodepointView => "String.CodepointView".to_owned(),
            Type::GraphemeView => "String.GraphemeView".to_owned(),
            Type::U8 => "u8".to_owned(),
            Type::U16 => "u16".to_owned(),
            Type::U32 => "u32".to_owned(),
            Type::U64 => "u64".to_owned(),
            Type::F32 => "f32".to_owned(),
            Type::F64 => "f64".to_owned(),
            Type::Atom(name) => format!(":{name}"),
            Type::List(item) => format!("[{}]", self.display_type(*item)),
            Type::Array { item, length } => format!("[{}; {length}]", self.display_type(*item)),
            Type::Slice(item) => format!("Slice({})", self.display_type(*item)),
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
            Type::Projection {
                protocol,
                associated,
                argument,
            } => format!("{protocol}.{associated}({})", self.display_type(*argument)),
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
            TypedItem::StructFieldAssign {
                symbol,
                field,
                value,
                ..
            } => {
                output.push_str(&format!("{indent}field assign s{} .{}\n", symbol.0, field));
                write_expr(program, output, value, depth + 1);
            }
            TypedItem::Expr(expression) => write_expr(program, output, expression, depth),
            TypedItem::Return(expression) => {
                output.push_str(&format!("{indent}return\n"));
                write_expr(program, output, expression, depth + 1);
            }
            TypedItem::While {
                condition, body, ..
            } => {
                output.push_str(&format!("{indent}while\n"));
                write_expr(program, output, condition, depth + 1);
                write_items(program, output, &body.items, depth + 1);
            }
            TypedItem::For {
                pattern,
                iterable,
                body,
                ..
            } => {
                output.push_str(&format!("{indent}for {:?}\n", pattern.kind));
                write_expr(program, output, iterable, depth + 1);
                write_items(program, output, &body.items, depth + 1);
            }
            TypedItem::DeferCall {
                function,
                arguments,
                ..
            } => {
                output.push_str(&format!("{indent}defer call d{}\n", function.0));
                for argument in arguments {
                    write_expr(program, output, argument, depth + 1);
                }
            }
            TypedItem::DeferBlock { captures, body, .. } => {
                output.push_str(&format!("{indent}defer block\n"));
                for capture in captures {
                    output.push_str(&format!(
                        "{indent}  capture s{} as s{} {}: {}\n",
                        capture.source.0,
                        capture.symbol.0,
                        capture.name,
                        program.display_type(capture.ty)
                    ));
                }
                write_items(program, output, &body.items, depth + 1);
            }
        }
    }
}

fn write_expr(program: &TypedProgram, output: &mut String, expression: &TypedExpr, depth: usize) {
    let indent = "  ".repeat(depth);
    let label = match &expression.kind {
        TypedExprKind::Integer(value) => format!("integer {value}"),
        TypedExprKind::Float(bits) => format!("float 0x{bits:016x}"),
        TypedExprKind::Boolean(value) => format!("boolean {value}"),
        TypedExprKind::Unit => "unit".to_owned(),
        TypedExprKind::String(value) => format!("string {value:?}"),
        TypedExprKind::Rune(value) => format!("rune {value:?}"),
        TypedExprKind::Atom(name) => format!("atom :{name}"),
        TypedExprKind::StandardCall { operation, .. } => format!("standard {operation:?}"),
        TypedExprKind::List { .. } => "list".to_owned(),
        TypedExprKind::ListReverse(_) => "list reverse".to_owned(),
        TypedExprKind::Array(_) => "array".to_owned(),
        TypedExprKind::Map(_) => "map".to_owned(),
        TypedExprKind::MapPut { .. } => "map put".to_owned(),
        TypedExprKind::MapRemove { .. } => "map remove".to_owned(),
        TypedExprKind::MapFetch { .. } => "map fetch".to_owned(),
        TypedExprKind::MapToList(_) => "map to list".to_owned(),
        TypedExprKind::Tuple(_) => "tuple".to_owned(),
        TypedExprKind::Struct { declaration, .. } => format!("struct d{}", declaration.0),
        TypedExprKind::StructProject {
            declaration, field, ..
        } => format!("struct project d{} .{}", declaration.0, field),
        TypedExprKind::Index { .. } => "index".to_owned(),
        TypedExprKind::SliceFromArray { .. } => "slice from array".to_owned(),
        TypedExprKind::SliceSubslice { .. } => "subslice".to_owned(),
        TypedExprKind::SliceCopy(_) => "slice copy".to_owned(),
        TypedExprKind::StringBytes(_) => "string bytes".to_owned(),
        TypedExprKind::StringCodepoints(_) => "string codepoints".to_owned(),
        TypedExprKind::StringCodepointView(_) => "string codepoint view".to_owned(),
        TypedExprKind::StringGraphemeView(_) => "string grapheme view".to_owned(),
        TypedExprKind::StringLength(_) => "string grapheme length".to_owned(),
        TypedExprKind::StringEmpty(_) => "string empty".to_owned(),
        TypedExprKind::StringContains { .. } => "string contains".to_owned(),
        TypedExprKind::StringSplit { .. } => "string split".to_owned(),
        TypedExprKind::Bitstring(_) => "bitstring".to_owned(),
        TypedExprKind::StringFromBytes(_) => "string from bytes".to_owned(),
        TypedExprKind::Utf8ErrorOffset(_) => "UTF-8 error offset".to_owned(),
        TypedExprKind::RuneToString(_) => "rune to string".to_owned(),
        TypedExprKind::IntegerToString(_) => "integer to string".to_owned(),
        TypedExprKind::BooleanToString(_) => "boolean to string".to_owned(),
        TypedExprKind::ShowConstant { rendered, .. } => format!("show constant {rendered:?}"),
        TypedExprKind::BufferNew => "buffer new".to_owned(),
        TypedExprKind::BufferAppend { kind, .. } => format!("buffer append {kind:?}"),
        TypedExprKind::BufferToBytes(_) => "buffer to bytes".to_owned(),
        TypedExprKind::BufferToString(_) => "buffer to string".to_owned(),
        TypedExprKind::BytesToBits(_) => "bytes to bits".to_owned(),
        TypedExprKind::BitsToBytes(_) => "bits to bytes".to_owned(),
        TypedExprKind::BitsSlice { .. } => "bits slice".to_owned(),
        TypedExprKind::BytesFromList(_) => "bytes from list".to_owned(),
        TypedExprKind::BytesToList(_) => "bytes to list".to_owned(),
        TypedExprKind::BytesSlice { .. } => "bytes slice".to_owned(),
        TypedExprKind::CollectionLength { .. } => "collection length".to_owned(),
        TypedExprKind::EnumAt { .. } => "enum at".to_owned(),
        TypedExprKind::EnumToList(_) => "enum to list".to_owned(),
        TypedExprKind::EnumVisit { kind, .. } => format!("enum {kind:?}"),
        TypedExprKind::If { .. } => "if".to_owned(),
        TypedExprKind::Match { exhaustive, .. } => format!("match exhaustive={exhaustive}"),
        TypedExprKind::Ascription(_) => "ascription".to_owned(),
        TypedExprKind::Local(symbol) => format!("local s{}", symbol.0),
        TypedExprKind::FunctionRef {
            function,
            substitutions,
        } => format!(
            "function d{} [{}]",
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
        TypedExprKind::Binary { operator, .. } => format!("binary {operator:?}"),
        TypedExprKind::IntegerUnary { operator, .. } => format!("integer unary {operator:?}"),
        TypedExprKind::FloatNegate(_) => "float negate".to_owned(),
        TypedExprKind::IntegerBinary { operator, .. } => format!("integer binary {operator:?}"),
        TypedExprKind::IntegerConvert(_) => "integer convert".to_owned(),
        TypedExprKind::NumericConvert(_) => "numeric convert".to_owned(),
        TypedExprKind::Concat { .. } => "concat".to_owned(),
        TypedExprKind::WrappingInteger { operator, .. } => {
            format!("wrapping integer {operator:?}")
        }
        TypedExprKind::Comparison { operator, .. } => format!("comparison {operator:?}"),
        TypedExprKind::Logical { operator, .. } => format!("logical {operator:?}"),
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
        TypedExprKind::IndirectCall { .. } => "indirect call".to_owned(),
        TypedExprKind::UnionInject { member, .. } => {
            format!("inject {}", program.display_type(*member))
        }
    };
    output.push_str(&format!(
        "{indent}{label}: {}\n",
        program.display_type(expression.ty)
    ));
    match &expression.kind {
        TypedExprKind::IntegerUnary { operand, .. } => {
            write_expr(program, output, operand, depth + 1);
        }
        TypedExprKind::FloatNegate(operand) => write_expr(program, output, operand, depth + 1),
        TypedExprKind::IntegerConvert(value) => write_expr(program, output, value, depth + 1),
        TypedExprKind::NumericConvert(value) => write_expr(program, output, value, depth + 1),
        TypedExprKind::WrappingInteger { left, right, .. } => {
            write_expr(program, output, left, depth + 1);
            if let Some(right) = right {
                write_expr(program, output, right, depth + 1);
            }
        }
        TypedExprKind::Binary { left, right, .. }
        | TypedExprKind::IntegerBinary { left, right, .. }
        | TypedExprKind::Concat { left, right }
        | TypedExprKind::Comparison { left, right, .. }
        | TypedExprKind::Logical { left, right, .. } => {
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
        TypedExprKind::ListReverse(value) | TypedExprKind::MapToList(value) => {
            write_expr(program, output, value, depth + 1);
        }
        TypedExprKind::Tuple(elements) => {
            for element in elements {
                write_expr(program, output, element, depth + 1);
            }
        }
        TypedExprKind::Struct { fields, .. } => {
            for (_, value) in fields {
                write_expr(program, output, value, depth + 1);
            }
        }
        TypedExprKind::Bitstring(segments) => {
            for segment in segments {
                write_expr(program, output, &segment.value, depth + 1);
                if let TypedBitstringSegmentKind::Bytes { size: Some(size) } = &segment.kind {
                    write_expr(program, output, size, depth + 1);
                }
            }
        }
        TypedExprKind::StructProject { value, .. } => {
            write_expr(program, output, value, depth + 1);
        }
        TypedExprKind::Index { value, index, .. } => {
            write_expr(program, output, value, depth + 1);
            write_expr(program, output, index, depth + 1);
        }
        TypedExprKind::SliceFromArray { value, .. }
        | TypedExprKind::SliceCopy(value)
        | TypedExprKind::StringBytes(value)
        | TypedExprKind::StringCodepoints(value)
        | TypedExprKind::StringLength(value)
        | TypedExprKind::StringEmpty(value)
        | TypedExprKind::StringFromBytes(value)
        | TypedExprKind::Utf8ErrorOffset(value)
        | TypedExprKind::RuneToString(value)
        | TypedExprKind::IntegerToString(value)
        | TypedExprKind::BooleanToString(value)
        | TypedExprKind::BufferToBytes(value)
        | TypedExprKind::BufferToString(value)
        | TypedExprKind::BytesToBits(value)
        | TypedExprKind::BitsToBytes(value)
        | TypedExprKind::BytesFromList(value)
        | TypedExprKind::BytesToList(value)
        | TypedExprKind::EnumToList(value)
        | TypedExprKind::CollectionLength { value, .. } => {
            write_expr(program, output, value, depth + 1);
        }
        TypedExprKind::StringContains { string, pattern } => {
            write_expr(program, output, string, depth + 1);
            write_expr(program, output, pattern, depth + 1);
        }
        TypedExprKind::StringSplit { string, separator } => {
            write_expr(program, output, string, depth + 1);
            write_expr(program, output, separator, depth + 1);
        }
        TypedExprKind::ShowConstant { value, .. } => {
            write_expr(program, output, value, depth + 1);
        }
        TypedExprKind::EnumAt { value, index } => {
            write_expr(program, output, value, depth + 1);
            write_expr(program, output, index, depth + 1);
        }
        TypedExprKind::EnumVisit {
            value,
            initial,
            function,
            ..
        } => {
            write_expr(program, output, value, depth + 1);
            if let Some(initial) = initial {
                write_expr(program, output, initial, depth + 1);
            }
            write_expr(program, output, function, depth + 1);
        }
        TypedExprKind::BufferAppend { buffer, value, .. } => {
            write_expr(program, output, buffer, depth + 1);
            write_expr(program, output, value, depth + 1);
        }
        TypedExprKind::SliceSubslice {
            value,
            start,
            length,
        } => {
            write_expr(program, output, value, depth + 1);
            write_expr(program, output, start, depth + 1);
            write_expr(program, output, length, depth + 1);
        }
        TypedExprKind::BytesSlice {
            value,
            start,
            length,
        } => {
            write_expr(program, output, value, depth + 1);
            write_expr(program, output, start, depth + 1);
            write_expr(program, output, length, depth + 1);
        }
        TypedExprKind::BitsSlice {
            value,
            start,
            length,
        } => {
            write_expr(program, output, value, depth + 1);
            write_expr(program, output, start, depth + 1);
            write_expr(program, output, length, depth + 1);
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
        TypedExprKind::MapPut { map, key, value } => {
            write_expr(program, output, map, depth + 1);
            write_expr(program, output, key, depth + 1);
            write_expr(program, output, value, depth + 1);
        }
        TypedExprKind::MapRemove { map, key } => {
            write_expr(program, output, map, depth + 1);
            write_expr(program, output, key, depth + 1);
        }
        TypedExprKind::MapFetch { map, key } => {
            write_expr(program, output, map, depth + 1);
            write_expr(program, output, key, depth + 1);
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
        TypedExprKind::IndirectCall { callee, arguments } => {
            write_expr(program, output, callee, depth + 1);
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
