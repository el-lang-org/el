//! `pest` parsing, grammar validation, and parser-independent AST construction.

use el_ast::{Node, Program, SyntaxKind, Value};
use el_span::{FileId, Span};
use pest::Parser;
use pest::error::InputLocation;
use pest::iterators::Pair;
use pest_derive::Parser;
use std::error::Error;
use std::fmt;

#[derive(Parser)]
#[grammar = "el.pest"]
struct ElParser;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseError {
    pub span: Span,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} at byte {}", self.message, self.span.start())
    }
}

impl Error for ParseError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParseOutcome {
    /// Recovery or validation errors prevent construction of a conforming AST.
    pub program: Option<Program>,
    pub diagnostics: Vec<ParseError>,
}

/// Decodes and parses one complete EL source file.
pub fn parse_bytes(file: FileId, source: &[u8]) -> Result<Program, ParseError> {
    let source = std::str::from_utf8(source).map_err(|error| {
        let start = error.valid_up_to();
        let end = error.error_len().map_or(start, |length| start + length);
        error_at(file, start, end, "EL source must be valid UTF-8")
    })?;
    parse(file, source)
}

/// Parses one complete UTF-8 EL source file and returns only a conforming AST.
pub fn parse(file: FileId, source: &str) -> Result<Program, ParseError> {
    let outcome = parse_recovering(file, source);
    match (outcome.program, outcome.diagnostics.into_iter().next()) {
        (Some(program), None) => Ok(program),
        (_, Some(error)) => Err(error),
        (None, None) => unreachable!("a parse outcome has a program or a diagnostic"),
    }
}

/// Parses with boundary recovery so callers can report independent syntax errors.
pub fn parse_recovering(file: FileId, source: &str) -> ParseOutcome {
    if source.starts_with('\u{feff}') {
        return failed(error_at(file, 0, 3, "UTF-8 BOMs are not accepted"));
    }
    if let Some(offset) = bare_carriage_return(source) {
        return failed(error_at(
            file,
            offset,
            offset + 1,
            "a carriage return must be followed by a line feed",
        ));
    }
    let normalized = normalize_delimited_layout(source);
    let mut pairs = match ElParser::parse(Rule::program, &normalized) {
        Ok(pairs) => pairs,
        Err(error) => return failed(pest_error(file, error)),
    };
    let pair = pairs.next().expect("program rule produces one pair");
    let mut diagnostics = Vec::new();
    validate_pair(file, pair.clone(), &mut diagnostics);
    if diagnostics.is_empty() {
        match build_node(file, pair) {
            Ok(root) => ParseOutcome {
                program: Some(Program { root }),
                diagnostics,
            },
            Err(error) => failed(error),
        }
    } else {
        ParseOutcome {
            program: None,
            diagnostics,
        }
    }
}

fn failed(error: ParseError) -> ParseOutcome {
    ParseOutcome {
        program: None,
        diagnostics: vec![error],
    }
}

fn validate_pair(file: FileId, pair: Pair<'_, Rule>, diagnostics: &mut Vec<ParseError>) {
    match pair.as_rule() {
        Rule::recovery_module_item
        | Rule::recovery_block_item
        | Rule::recovery_protocol_item
        | Rule::recovery_implementation_item => {
            let trimmed = pair.as_str().trim_start();
            let message = if pair.as_str().contains(';') {
                "semicolons are not accepted as statement separators"
            } else if starts_with_operator(trimmed) {
                "an operator cannot continue a complete expression from the preceding line"
            } else {
                "could not recover this malformed construct"
            };
            diagnostics.push(error_pair(file, &pair, message));
        }
        Rule::equality_expr => validate_non_associative(
            file,
            &pair,
            Rule::equality_operator,
            "equality operators cannot be chained",
            diagnostics,
        ),
        Rule::comparison_expr => validate_non_associative(
            file,
            &pair,
            Rule::comparison_operator,
            "comparison operators cannot be chained",
            diagnostics,
        ),
        Rule::ascription_expr => validate_non_associative(
            file,
            &pair,
            Rule::ascription_operator,
            "type ascriptions cannot be chained",
            diagnostics,
        ),
        Rule::pipeline_expr | Rule::segment_expression => {
            validate_pipeline(file, &pair, diagnostics);
        }
        Rule::assignment => validate_assignment(file, &pair, diagnostics),
        Rule::bitstring_expr | Rule::bitstring_pattern => {
            validate_bitstring(file, &pair, diagnostics);
        }
        Rule::array_length => validate_array_length(file, &pair, diagnostics),
        Rule::implementation_body => validate_implementation_body(file, &pair, diagnostics),
        _ => {}
    }
    for child in pair.into_inner() {
        validate_pair(file, child, diagnostics);
    }
}

fn starts_with_operator(source: &str) -> bool {
    [
        "|>", "::", "==", "!=", "<=", ">=", "++", "<<", ">>", "+", "-", "*", "/", "%", "|", "^",
        "&", "<", ">",
    ]
    .iter()
    .any(|operator| source.starts_with(operator))
}

fn validate_array_length(file: FileId, pair: &Pair<'_, Rule>, diagnostics: &mut Vec<ParseError>) {
    let digits = pair.as_str().replace('_', "");
    let representable = digits
        .parse::<u128>()
        .is_ok_and(|length| length <= usize::MAX as u128);
    if !representable {
        diagnostics.push(error_pair(
            file,
            pair,
            "a fixed-array length must be representable as `usize`",
        ));
    }
}

fn validate_implementation_body(
    file: FileId,
    pair: &Pair<'_, Rule>,
    diagnostics: &mut Vec<ParseError>,
) {
    for function in descendants(pair.clone()).filter(|node| node.as_rule() == Rule::function_decl) {
        if descendants(function.clone()).any(|node| node.as_rule() == Rule::kw_defp) {
            diagnostics.push(error_pair(
                file,
                &function,
                "only public `def` methods are permitted in an implementation body",
            ));
        }
    }
}

fn validate_non_associative(
    file: FileId,
    pair: &Pair<'_, Rule>,
    operator: Rule,
    message: &str,
    diagnostics: &mut Vec<ParseError>,
) {
    let mut operators = pair
        .clone()
        .into_inner()
        .filter(|child| child.as_rule() == operator);
    let _ = operators.next();
    if let Some(second) = operators.next() {
        diagnostics.push(error_pair(file, &second, message));
    }
}

fn validate_pipeline(file: FileId, pair: &Pair<'_, Rule>, diagnostics: &mut Vec<ParseError>) {
    let mut expect_target = false;
    for child in pair.clone().into_inner() {
        if child.as_rule() == Rule::pipeline_operator {
            expect_target = true;
        } else if expect_target {
            if !is_statically_resolvable_call(&child) {
                diagnostics.push(error_pair(
                    file,
                    &child,
                    "the right side of `|>` must be a statically resolvable call",
                ));
            }
            expect_target = false;
        }
    }
}

fn is_statically_resolvable_call(pair: &Pair<'_, Rule>) -> bool {
    let Some(postfix) = direct_postfix(pair.clone()) else {
        return false;
    };
    let children = postfix.into_inner().collect::<Vec<_>>();
    let qualified = children.first().is_some_and(|primary| {
        descendants(primary.clone()).any(|node| node.as_rule() == Rule::qualified_value)
    });
    let calls = children
        .iter()
        .filter(|child| {
            descendants((*child).clone()).any(|node| node.as_rule() == Rule::call_arguments)
        })
        .count();
    let non_calls = children
        .iter()
        .filter(|child| {
            descendants((*child).clone())
                .any(|node| matches!(node.as_rule(), Rule::field_access | Rule::index_access))
        })
        .count();
    qualified && calls == 1 && non_calls == 0
}

fn direct_postfix(pair: Pair<'_, Rule>) -> Option<Pair<'_, Rule>> {
    if pair.as_rule() == Rule::postfix_expr {
        return Some(pair);
    }
    if !matches!(
        pair.as_rule(),
        Rule::ascription_expr
            | Rule::logical_or_expr
            | Rule::logical_and_expr
            | Rule::equality_expr
            | Rule::comparison_expr
            | Rule::concat_expr
            | Rule::bit_or_expr
            | Rule::bit_xor_expr
            | Rule::bit_and_expr
            | Rule::shift_expr
            | Rule::additive_expr
            | Rule::multiplicative_expr
            | Rule::unary_expr
    ) {
        return None;
    }
    let mut children = pair.into_inner();
    let child = children.next()?;
    if children.next().is_some() {
        return None;
    }
    direct_postfix(child)
}

fn validate_assignment(file: FileId, pair: &Pair<'_, Rule>, diagnostics: &mut Vec<ParseError>) {
    let Some(target) = pair.clone().into_inner().next() else {
        return;
    };
    let fields = descendants(target.clone())
        .filter(|node| node.as_rule() == Rule::field_access)
        .count();
    let indexes = descendants(target.clone())
        .any(|node| node.as_rule() == Rule::index_access || node.as_rule() == Rule::call_arguments);
    let primary_text = descendants(target)
        .find(|node| node.as_rule() == Rule::qualified_value)
        .map(|node| node.as_str());
    let bare_root = primary_text.is_some_and(|text| !text.contains('.'));
    if !bare_root || fields > 1 || indexes {
        diagnostics.push(error_pair(
            file,
            pair,
            "an assignment target must be an identifier or one direct field",
        ));
    }
}

fn validate_bitstring(file: FileId, pair: &Pair<'_, Rule>, diagnostics: &mut Vec<ParseError>) {
    let pattern = pair.as_rule() == Rule::bitstring_pattern;
    let segment_rule = if pattern {
        Rule::bit_pattern_segment
    } else {
        Rule::bit_expr_segment
    };
    let segments = pair
        .clone()
        .into_inner()
        .filter(|child| child.as_rule() == segment_rule)
        .collect::<Vec<_>>();
    for (index, segment) in segments.iter().enumerate() {
        let modifiers = descendants((*segment).clone())
            .filter(|node| {
                matches!(
                    node.as_rule(),
                    Rule::modifier_integer
                        | Rule::modifier_signed
                        | Rule::modifier_unsigned
                        | Rule::modifier_big
                        | Rule::modifier_little
                        | Rule::modifier_native
                        | Rule::modifier_bytes
                        | Rule::size_modifier
                )
            })
            .collect::<Vec<_>>();
        let count = |rule| {
            modifiers
                .iter()
                .filter(|item| item.as_rule() == rule)
                .count()
        };
        let bytes = count(Rule::modifier_bytes);
        let sizes = count(Rule::size_modifier);
        let signs = count(Rule::modifier_signed) + count(Rule::modifier_unsigned);
        let orders =
            count(Rule::modifier_big) + count(Rule::modifier_little) + count(Rule::modifier_native);
        let duplicate = modifiers.iter().any(|item| count(item.as_rule()) > 1);
        let invalid = if bytes == 1 {
            duplicate
                || sizes > 1
                || count(Rule::modifier_integer) > 0
                || signs > 0
                || orders > 0
                || (pattern && sizes == 0 && index + 1 != segments.len())
        } else {
            duplicate || bytes > 1 || sizes != 1 || signs > 1 || orders > 1
        };
        if invalid {
            diagnostics.push(error_pair(
                file,
                segment,
                "invalid or conflicting bitstring modifiers",
            ));
            continue;
        }
        if bytes == 0 {
            let size = modifiers
                .iter()
                .find(|item| item.as_rule() == Rule::size_modifier)
                .and_then(|item| bitstring_integer_size(item.as_str()));
            if !matches!(size, Some(8 | 16 | 24 | 32 | 40 | 48 | 56 | 64)) {
                diagnostics.push(error_pair(
                    file,
                    segment,
                    "integer bitstring sizes must be literal multiples of eight from 8 through 64",
                ));
            }
        }
    }
}

fn bitstring_integer_size(modifier: &str) -> Option<u128> {
    let compact = modifier
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    let compact = std::str::from_utf8(&compact).expect("layout removal preserves UTF-8");
    let token = compact.strip_prefix("size(")?.strip_suffix(')')?;
    parse_integer_value(token)
}

fn parse_integer_value(token: &str) -> Option<u128> {
    let token = token.replace('_', "");
    if let Some(digits) = token.strip_prefix("0b") {
        u128::from_str_radix(digits, 2).ok()
    } else if let Some(digits) = token.strip_prefix("0o") {
        u128::from_str_radix(digits, 8).ok()
    } else if let Some(digits) = token.strip_prefix("0x") {
        u128::from_str_radix(digits, 16).ok()
    } else {
        token.parse().ok()
    }
}

fn descendants(pair: Pair<'_, Rule>) -> impl Iterator<Item = Pair<'_, Rule>> {
    let mut stack = vec![pair];
    std::iter::from_fn(move || {
        let pair = stack.pop()?;
        stack.extend(pair.clone().into_inner().rev());
        Some(pair)
    })
}

fn build_node(file: FileId, pair: Pair<'_, Rule>) -> Result<Node, ParseError> {
    let mut span = pair_span(file, &pair);
    let rule = pair.as_rule();
    if is_left_associative(rule) && pair.clone().into_inner().count() > 1 {
        return build_left_associative(file, pair);
    }
    if matches!(
        rule,
        Rule::equality_expr | Rule::comparison_expr | Rule::concat_expr | Rule::ascription_expr
    ) && pair.clone().into_inner().count() > 1
    {
        return build_binary_like(file, pair);
    }
    if rule == Rule::unary_expr && pair.clone().into_inner().count() > 1 {
        return build_unary(file, pair);
    }
    let value = build_value(file, &pair)?;
    let mut children = Vec::new();
    for child in pair.clone().into_inner() {
        if is_ast_punctuation(child.as_rule()) {
            continue;
        }
        let flatten = is_parser_wrapper(child.as_rule());
        let node = build_node(file, child)?;
        if flatten {
            children.extend(node.children);
        } else {
            children.push(node);
        }
    }
    if children.len() == 1 && is_transparent(rule) {
        return Ok(children.into_iter().next().expect("one transparent child"));
    }
    if matches!(rule, Rule::type_path | Rule::qualified_value)
        && let (Some(first), Some(last)) = (children.first(), children.last())
    {
        span = Span::new(file, first.span.start(), last.span.end())
            .expect("path component spans are ordered");
    }
    Ok(Node {
        kind: SyntaxKind::new(format!("{rule:?}")),
        span,
        value,
        children,
    })
}

fn is_left_associative(rule: Rule) -> bool {
    matches!(
        rule,
        Rule::pipeline_expr
            | Rule::logical_or_expr
            | Rule::logical_and_expr
            | Rule::segment_expression
            | Rule::bit_or_expr
            | Rule::bit_xor_expr
            | Rule::bit_and_expr
            | Rule::shift_expr
            | Rule::additive_expr
            | Rule::multiplicative_expr
    )
}

fn is_parser_wrapper(rule: Rule) -> bool {
    matches!(
        rule,
        Rule::module_body
            | Rule::derived_struct
            | Rule::field_body
            | Rule::function_head
            | Rule::parameters
            | Rule::protocol_body
            | Rule::implementation_body
            | Rule::postfix_part
            | Rule::non_call_postfix
            | Rule::pattern
            | Rule::pattern_literal
            | Rule::bit_modifier
    )
}

fn is_ast_punctuation(rule: Rule) -> bool {
    matches!(
        rule,
        Rule::EOI
            | Rule::attr_derive
            | Rule::attr_type
            | Rule::kw_defmodule
            | Rule::kw_defprotocol
            | Rule::kw_defstruct
            | Rule::kw_defimpl
            | Rule::kw_defp
            | Rule::kw_def
            | Rule::kw_defer
            | Rule::kw_do
            | Rule::kw_else
            | Rule::kw_end
            | Rule::kw_for
            | Rule::kw_if
            | Rule::kw_in
            | Rule::kw_match
            | Rule::kw_mut
            | Rule::kw_return
            | Rule::kw_type
            | Rule::kw_when
            | Rule::kw_while
            | Rule::kw_and
            | Rule::kw_or
            | Rule::kw_integer
            | Rule::kw_signed
            | Rule::kw_unsigned
            | Rule::kw_big
            | Rule::kw_little
            | Rule::kw_native
            | Rule::kw_bytes
            | Rule::kw_size
            | Rule::arrow
            | Rule::fat_arrow
            | Rule::pipeline_operator
            | Rule::ascription_operator
            | Rule::logical_or_operator
            | Rule::logical_and_operator
            | Rule::equality_operator
            | Rule::comparison_operator
            | Rule::concat_operator
            | Rule::bit_or_operator
            | Rule::bit_xor_operator
            | Rule::bit_and_operator
            | Rule::shift_operator
            | Rule::additive_operator
            | Rule::multiplicative_operator
            | Rule::unary_operator
            | Rule::unary_minus
    )
}

fn build_left_associative(file: FileId, pair: Pair<'_, Rule>) -> Result<Node, ParseError> {
    let kind = format!("{:?}", pair.as_rule());
    let mut parts = pair.into_inner();
    let mut left = build_node(
        file,
        parts.next().expect("operator level has a left operand"),
    )?;
    while let Some(operator) = parts.next() {
        let operator = operator.as_str().to_owned();
        let right = build_node(file, parts.next().expect("operator has a right operand"))?;
        let span = Span::new(file, left.span.start(), right.span.end())
            .expect("left-to-right expression spans are ordered");
        left = Node {
            kind: SyntaxKind::new(kind.clone()),
            span,
            value: Some(Value::Text(operator)),
            children: vec![left, right],
        };
    }
    Ok(left)
}

fn build_binary_like(file: FileId, pair: Pair<'_, Rule>) -> Result<Node, ParseError> {
    let kind = SyntaxKind::new(format!("{:?}", pair.as_rule()));
    let mut parts = pair.into_inner();
    let left = build_node(file, parts.next().expect("binary form has a left operand"))?;
    let operator = parts.next().expect("binary form has an operator");
    let right = build_node(file, parts.next().expect("binary form has a right operand"))?;
    Ok(Node {
        kind,
        span: Span::new(file, left.span.start(), right.span.end())
            .expect("binary expression spans are ordered"),
        value: Some(Value::Text(operator.as_str().to_owned())),
        children: vec![left, right],
    })
}

fn build_unary(file: FileId, pair: Pair<'_, Rule>) -> Result<Node, ParseError> {
    let span = pair_span(file, &pair);
    let mut parts = pair.into_inner();
    let operator = parts.next().expect("unary form has an operator");
    let operand = build_node(file, parts.next().expect("unary form has an operand"))?;
    Ok(Node {
        kind: SyntaxKind::new("unary_expr"),
        span,
        value: Some(Value::Text(operator.as_str().to_owned())),
        children: vec![operand],
    })
}

fn is_transparent(rule: Rule) -> bool {
    matches!(
        rule,
        Rule::module_name
            | Rule::module_item
            | Rule::protocol_item
            | Rule::implementation_item
            | Rule::type_expr
            | Rule::union_type
            | Rule::primary_type
            | Rule::block_item
            | Rule::expression
            | Rule::pipeline_expr
            | Rule::ascription_expr
            | Rule::logical_or_expr
            | Rule::logical_and_expr
            | Rule::equality_expr
            | Rule::comparison_expr
            | Rule::concat_expr
            | Rule::bit_or_expr
            | Rule::bit_xor_expr
            | Rule::bit_and_expr
            | Rule::shift_expr
            | Rule::additive_expr
            | Rule::multiplicative_expr
            | Rule::unary_expr
            | Rule::postfix_expr
            | Rule::primary_expr
            | Rule::literal
    )
}

fn build_value(file: FileId, pair: &Pair<'_, Rule>) -> Result<Option<Value>, ParseError> {
    let spelling = pair.as_str();
    let value = match pair.as_rule() {
        Rule::visibility => Some(Value::Text(match spelling {
            "def" => "public".to_owned(),
            "defp" => "private".to_owned(),
            _ => unreachable!("grammar limits function visibility"),
        })),
        Rule::binding => Some(Value::Text(
            if spelling.trim_start().starts_with("mut ") {
                "mutable"
            } else {
                "immutable"
            }
            .to_owned(),
        )),
        Rule::integer => {
            let (radix, digits) = if let Some(digits) = spelling.strip_prefix("0b") {
                (2, digits)
            } else if let Some(digits) = spelling.strip_prefix("0o") {
                (8, digits)
            } else if let Some(digits) = spelling.strip_prefix("0x") {
                (16, digits)
            } else {
                (10, spelling)
            };
            Some(Value::Integer {
                spelling: spelling.to_owned(),
                radix,
                digits: digits.replace('_', ""),
            })
        }
        Rule::float => Some(Value::Float {
            spelling: spelling.to_owned(),
            normalized: spelling.replace('_', ""),
        }),
        Rule::string => Some(Value::String {
            spelling: spelling.to_owned(),
            decoded: decode_quoted(file, pair, '"')?,
        }),
        Rule::rune => {
            let decoded = decode_quoted(file, pair, '\'')?;
            let mut chars = decoded.chars();
            let character = chars.next().ok_or_else(|| {
                error_pair(file, pair, "a rune must contain one Unicode scalar value")
            })?;
            if chars.next().is_some() {
                return Err(error_pair(
                    file,
                    pair,
                    "a rune must contain one Unicode scalar value",
                ));
            }
            Some(Value::Rune {
                spelling: spelling.to_owned(),
                decoded: character,
            })
        }
        Rule::atom => Some(Value::Atom {
            spelling: spelling.to_owned(),
            name: spelling[1..].to_owned(),
        }),
        Rule::kw_true => Some(Value::Boolean(true)),
        Rule::kw_false => Some(Value::Boolean(false)),
        Rule::kw_unit => Some(Value::Unit),
        _ if pair.clone().into_inner().next().is_none() => Some(Value::Text(spelling.to_owned())),
        _ => None,
    };
    Ok(value)
}

fn decode_quoted(file: FileId, pair: &Pair<'_, Rule>, quote: char) -> Result<String, ParseError> {
    let spelling = pair.as_str();
    let content = &spelling[quote.len_utf8()..spelling.len() - quote.len_utf8()];
    let mut output = String::new();
    let mut chars = content.char_indices();
    while let Some((offset, character)) = chars.next() {
        if character != '\\' {
            output.push(character);
            continue;
        }
        let (_, escape) = chars.next().expect("grammar requires an escape body");
        match escape {
            '\\' => output.push('\\'),
            '"' => output.push('"'),
            '\'' => output.push('\''),
            'n' => output.push('\n'),
            'r' => output.push('\r'),
            't' => output.push('\t'),
            '0' => output.push('\0'),
            'x' => {
                let (_, high) = chars.next().expect("grammar requires two hex digits");
                let (_, low) = chars.next().expect("grammar requires two hex digits");
                let byte = high.to_digit(16).expect("hex digit") * 16
                    + low.to_digit(16).expect("hex digit");
                let scalar = char::from_u32(byte).expect("one-byte value is a scalar");
                output.push(scalar);
            }
            'u' => {
                let _ = chars.next().expect("grammar requires `{`");
                let mut digits = String::new();
                for (_, character) in chars.by_ref() {
                    if character == '}' {
                        break;
                    }
                    digits.push(character);
                }
                let scalar = u32::from_str_radix(&digits, 16).expect("grammar bounds Unicode hex");
                let Some(character) = char::from_u32(scalar) else {
                    return Err(error_at(
                        file,
                        pair.as_span().start() + 1 + offset,
                        pair.as_span().start() + 1 + offset + digits.len() + 4,
                        "Unicode escapes must denote scalar values",
                    ));
                };
                output.push(character);
            }
            _ => unreachable!("grammar limits escapes"),
        }
    }
    Ok(output)
}

fn pest_error(file: FileId, error: pest::error::Error<Rule>) -> ParseError {
    let message = format!("invalid EL syntax: {}", error.variant.message());
    let (start, end) = match error.location {
        InputLocation::Pos(position) => (position, position),
        InputLocation::Span((start, end)) => (start, end),
    };
    error_at(file, start, end, &message)
}

fn pair_span(file: FileId, pair: &Pair<'_, Rule>) -> Span {
    Span::new(file, pair.as_span().start(), pair.as_span().end()).expect("pest spans are ordered")
}

fn error_pair(file: FileId, pair: &Pair<'_, Rule>, message: &str) -> ParseError {
    error_at(file, pair.as_span().start(), pair.as_span().end(), message)
}

fn error_at(file: FileId, start: usize, end: usize, message: &str) -> ParseError {
    ParseError {
        span: Span::new(file, start, end).expect("error spans are ordered"),
        message: message.to_owned(),
    }
}

fn bare_carriage_return(source: &str) -> Option<usize> {
    source
        .as_bytes()
        .iter()
        .enumerate()
        .find_map(|(index, byte)| {
            (*byte == b'\r' && source.as_bytes().get(index + 1) != Some(&b'\n')).then_some(index)
        })
}

/// `pest` is scannerless, but significant newlines depend on delimiter depth.
/// Masking only layout bytes keeps every byte offset stable while allowing the
/// checked-in PEG to keep top-level newlines explicit.
fn normalize_delimited_layout(source: &str) -> String {
    let mut bytes = source.as_bytes().to_vec();
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }
        if byte == b'"' || byte == b'\'' {
            quote = Some(byte);
            index += 1;
            continue;
        }
        if byte == b'#' && bytes.get(index + 1) != Some(&b'[') {
            while index < bytes.len() && !matches!(bytes[index], b'\r' | b'\n') {
                bytes[index] = b' ';
                index += 1;
            }
            continue;
        }
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'\r' | b'\n' if depth > 0 => bytes[index] = b' ',
            _ => {}
        }
        index += 1;
    }
    String::from_utf8(bytes).expect("masking layout bytes preserves UTF-8")
}
