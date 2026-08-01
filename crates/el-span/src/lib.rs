//! Dependency-light source locations and structured diagnostic records.

use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

/// Stable identity for one source file within a compilation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FileId(u32);

impl FileId {
    fn from_index(index: usize) -> Self {
        let raw = u32::try_from(index).expect("source map exhausted FileId space");
        Self(raw)
    }

    fn index(self) -> usize {
        self.0 as usize
    }

    /// Returns the stable compilation-local numeric identity used by private
    /// compiler metadata and runtime source tables.
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0
    }
}

/// A half-open byte range in one source file.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Span {
    file: FileId,
    start: usize,
    end: usize,
}

impl Span {
    /// Creates a span when `start <= end`.
    #[must_use]
    pub const fn new(file: FileId, start: usize, end: usize) -> Option<Self> {
        if start <= end {
            Some(Self { file, start, end })
        } else {
            None
        }
    }

    #[must_use]
    pub const fn file(self) -> FileId {
        self.file
    }

    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    #[must_use]
    pub const fn end(self) -> usize {
        self.end
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }
}

/// A one-based display position derived from UTF-8 source text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Location {
    pub line: usize,
    pub column: usize,
}

/// Immutable source text and its package-relative display path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFile {
    path: PathBuf,
    text: String,
}

impl SourceFile {
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Source storage for one compilation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds immutable UTF-8 source in deterministic insertion order.
    pub fn add_file(&mut self, path: impl Into<PathBuf>, text: impl Into<String>) -> FileId {
        let id = FileId::from_index(self.files.len());
        self.files.push(SourceFile {
            path: path.into(),
            text: text.into(),
        });
        id
    }

    pub fn file(&self, id: FileId) -> Result<&SourceFile, SourceMapError> {
        self.files
            .get(id.index())
            .ok_or(SourceMapError::UnknownFile(id))
    }

    /// Derives a one-based line and Unicode-scalar column from a byte offset.
    pub fn location(&self, id: FileId, offset: usize) -> Result<Location, SourceMapError> {
        let text = self.file(id)?.text();
        if offset > text.len() {
            return Err(SourceMapError::OffsetOutOfBounds {
                file: id,
                offset,
                length: text.len(),
            });
        }
        if !text.is_char_boundary(offset) {
            return Err(SourceMapError::NotCharBoundary { file: id, offset });
        }

        let prefix = &text[..offset];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let line_prefix = prefix.rsplit_once('\n').map_or(prefix, |(_, tail)| tail);
        let column = line_prefix.chars().count() + 1;
        Ok(Location { line, column })
    }

    /// Verifies that a span belongs to this map and lands on UTF-8 boundaries.
    pub fn verify_span(&self, span: Span) -> Result<(), SourceMapError> {
        let text = self.file(span.file)?.text();
        if span.end > text.len() {
            return Err(SourceMapError::InvalidSpan(span));
        }
        if !text.is_char_boundary(span.start) || !text.is_char_boundary(span.end) {
            return Err(SourceMapError::InvalidSpan(span));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceMapError {
    UnknownFile(FileId),
    OffsetOutOfBounds {
        file: FileId,
        offset: usize,
        length: usize,
    },
    NotCharBoundary {
        file: FileId,
        offset: usize,
    },
    InvalidSpan(Span),
}

impl fmt::Display for SourceMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFile(file) => write!(formatter, "unknown source file {file:?}"),
            Self::OffsetOutOfBounds {
                file,
                offset,
                length,
            } => write!(
                formatter,
                "byte offset {offset} is outside {file:?}, whose length is {length}"
            ),
            Self::NotCharBoundary { file, offset } => {
                write!(
                    formatter,
                    "byte offset {offset} is not a UTF-8 boundary in {file:?}"
                )
            }
            Self::InvalidSpan(span) => write!(formatter, "invalid source span {span:?}"),
        }
    }
}

impl Error for SourceMapError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Label {
    pub span: Span,
    pub message: String,
}

/// Renderer-independent diagnostic data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub primary: Span,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn error(code: impl Into<String>, primary: Span, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity: Severity::Error,
            primary,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            help: None,
        }
    }

    #[must_use]
    pub fn with_label(mut self, span: Span, message: impl Into<String>) -> Self {
        self.labels.push(Label {
            span,
            message: message.into(),
        });
        self
    }

    #[must_use]
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    #[must_use]
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Checks every stored span before the diagnostic reaches a renderer.
    pub fn verify(&self, sources: &SourceMap) -> Result<(), SourceMapError> {
        sources.verify_span(self.primary)?;
        for label in &self.labels {
            sources.verify_span(label.span)?;
        }
        Ok(())
    }
}

/// Renders one diagnostic using only package-relative source paths.
///
/// The format is part of the EL v1 tool contract. It deliberately avoids
/// terminal color, host paths, and locale-sensitive formatting so snapshots
/// and diagnostics are identical on every supported host.
pub fn render_diagnostic(
    sources: &SourceMap,
    diagnostic: &Diagnostic,
) -> Result<String, SourceMapError> {
    diagnostic.verify(sources)?;
    let file = sources.file(diagnostic.primary.file())?;
    let location = sources.location(diagnostic.primary.file(), diagnostic.primary.start())?;
    let (line_text, line_start, line_end) = source_line(file.text(), diagnostic.primary.start());
    let marker_start = file.text()[line_start..diagnostic.primary.start()]
        .chars()
        .count();
    let marker_end = diagnostic.primary.end().min(line_end);
    let marker_width = file.text()[diagnostic.primary.start()..marker_end]
        .chars()
        .count()
        .max(1);
    let gutter_width = location.line.to_string().len();

    let mut output = format!(
        "error[{}]: {}\n --> {}:{}:{}\n{:width$} |\n{} | {}\n{:width$} | {}{}",
        diagnostic.code,
        diagnostic.message,
        file.path().display(),
        location.line,
        location.column,
        "",
        location.line,
        expand_tabs(line_text),
        "",
        " ".repeat(display_indent(line_text, marker_start)),
        "^".repeat(marker_width),
        width = gutter_width,
    );

    for label in &diagnostic.labels {
        let label_file = sources.file(label.span.file())?;
        let label_location = sources.location(label.span.file(), label.span.start())?;
        output.push_str(&format!(
            "\n{:width$} = label: {}:{}:{}: {}",
            "",
            label_file.path().display(),
            label_location.line,
            label_location.column,
            label.message,
            width = gutter_width,
        ));
    }
    for note in &diagnostic.notes {
        output.push_str(&format!(
            "\n{:width$} = note: {note}",
            "",
            width = gutter_width
        ));
    }
    if let Some(help) = &diagnostic.help {
        output.push_str(&format!(
            "\n{:width$} = help: {help}",
            "",
            width = gutter_width
        ));
    }
    output.push('\n');
    Ok(output)
}

fn source_line(source: &str, offset: usize) -> (&str, usize, usize) {
    let start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let mut end = source[offset..]
        .find('\n')
        .map_or(source.len(), |index| offset + index);
    if end > start && source.as_bytes()[end - 1] == b'\r' {
        end -= 1;
    }
    (&source[start..end], start, end)
}

fn expand_tabs(text: &str) -> String {
    let mut expanded = String::with_capacity(text.len());
    for character in text.chars() {
        if character == '\t' {
            expanded.push_str("    ");
        } else {
            expanded.push(character);
        }
    }
    expanded
}

fn display_indent(line: &str, scalar_count: usize) -> usize {
    line.chars()
        .take(scalar_count)
        .map(|character| if character == '\t' { 4 } else { 1 })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_locations_from_utf8_lf_crlf_and_eof() {
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/Main.ell", "α\r\nbeta\n");

        assert_eq!(
            sources.location(file, 0),
            Ok(Location { line: 1, column: 1 })
        );
        assert_eq!(
            sources.location(file, 2),
            Ok(Location { line: 1, column: 2 })
        );
        assert_eq!(
            sources.location(file, 4),
            Ok(Location { line: 2, column: 1 })
        );
        assert_eq!(
            sources.location(file, 9),
            Ok(Location { line: 3, column: 1 })
        );
    }

    #[test]
    fn rejects_offsets_inside_multibyte_text() {
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/Main.ell", "α");

        assert_eq!(
            sources.location(file, 1),
            Err(SourceMapError::NotCharBoundary { file, offset: 1 })
        );
    }

    #[test]
    fn verifies_zero_width_and_label_spans() {
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/Main.ell", "value");
        let eof = Span::new(file, 5, 5).expect("ordered span");
        let value = Span::new(file, 0, 5).expect("ordered span");
        let diagnostic = Diagnostic::error("E0001", eof, "expected expression")
            .with_label(value, "previous expression")
            .with_note("notes are structured")
            .with_help("add an expression");

        assert_eq!(diagnostic.verify(&sources), Ok(()));
        assert_eq!(
            sources.file(file).expect("known file").path(),
            Path::new("src/Main.ell")
        );
    }

    #[test]
    fn renders_stable_package_relative_unicode_source_presentations() {
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/Main.ell", "first\r\n\tvalue = λ\n");
        let lambda = Span::new(file, 16, 18).unwrap();
        let diagnostic = Diagnostic::error("E2106", lambda, "unknown value `λ`")
            .with_label(Span::new(file, 8, 13).unwrap(), "binding starts here")
            .with_note("names are resolved before type checking")
            .with_help("declare `λ` before this expression");

        assert_eq!(
            render_diagnostic(&sources, &diagnostic).unwrap(),
            "error[E2106]: unknown value `λ`\n --> src/Main.ell:2:10\n  |\n2 |     value = λ\n  |             ^\n  = label: src/Main.ell:2:2: binding starts here\n  = note: names are resolved before type checking\n  = help: declare `λ` before this expression\n"
        );
    }
}
