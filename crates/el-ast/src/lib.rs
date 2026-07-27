//! Parser-independent, source-oriented syntax trees.

use el_span::Span;
use std::fmt::Write;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    pub root: Node,
}

impl Program {
    #[must_use]
    pub const fn span(&self) -> Span {
        self.root.span
    }

    /// A stable tree representation used by checked-in snapshots.
    #[must_use]
    pub fn debug_tree(&self) -> String {
        let mut output = String::new();
        write_node(&mut output, &self.root, 0);
        output
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Node {
    pub kind: SyntaxKind,
    pub span: Span,
    pub value: Option<Value>,
    pub children: Vec<Node>,
}

impl Node {
    #[must_use]
    pub fn descendants(&self) -> Descendants<'_> {
        Descendants { stack: vec![self] }
    }

    #[must_use]
    pub fn debug_tree(&self) -> String {
        let mut output = String::new();
        write_node(&mut output, self, 0);
        output
    }
}

pub struct Descendants<'a> {
    stack: Vec<&'a Node>,
}

impl<'a> Iterator for Descendants<'a> {
    type Item = &'a Node;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.stack.pop()?;
        self.stack.extend(node.children.iter().rev());
        Some(node)
    }
}

/// Stable source-syntax identity. Names match the normative grammar productions.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SyntaxKind(String);

impl SyntaxKind {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Value {
    Text(String),
    Integer {
        spelling: String,
        radix: u32,
        digits: String,
    },
    Float {
        spelling: String,
        normalized: String,
    },
    String {
        spelling: String,
        decoded: String,
    },
    Rune {
        spelling: String,
        decoded: char,
    },
    Atom {
        spelling: String,
        name: String,
    },
    Boolean(bool),
    Unit,
}

fn write_node(output: &mut String, node: &Node, depth: usize) {
    let _ = write!(
        output,
        "{}{} @{}..{}",
        "  ".repeat(depth),
        node.kind.as_str(),
        node.span.start(),
        node.span.end()
    );
    if let Some(value) = &node.value {
        let _ = write!(output, " {}", display_value(value));
    }
    output.push('\n');
    for child in &node.children {
        write_node(output, child, depth + 1);
    }
}

fn display_value(value: &Value) -> String {
    match value {
        Value::Text(value) => format!("{value:?}"),
        Value::Integer {
            spelling,
            radix,
            digits,
        } => format!("spelling={spelling:?} radix={radix} digits={digits:?}"),
        Value::Float {
            spelling,
            normalized,
        } => format!("spelling={spelling:?} normalized={normalized:?}"),
        Value::String { spelling, decoded } => {
            format!("spelling={spelling:?} decoded={decoded:?}")
        }
        Value::Rune { spelling, decoded } => {
            format!("spelling={spelling:?} decoded={decoded:?}")
        }
        Value::Atom { spelling, name } => format!("spelling={spelling:?} name={name:?}"),
        Value::Boolean(value) => value.to_string(),
        Value::Unit => "unit".to_owned(),
    }
}
