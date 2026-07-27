use el_span::Span;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct FailureOrigin {
    pub file: u32,
    pub start: u64,
    pub end: u64,
}

impl FailureOrigin {
    pub fn from_span(span: Span) -> Result<Self, ()> {
        Ok(Self {
            file: span.file().as_u32(),
            start: u64::try_from(span.start()).map_err(|_| ())?,
            end: u64::try_from(span.end()).map_err(|_| ())?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use el_span::SourceMap;

    #[test]
    fn preserves_the_complete_source_span_for_runtime_metadata() {
        let mut sources = SourceMap::new();
        let file = sources.add_file("src/main.el", "123 + 456");
        let span = Span::new(file, 4, 9).unwrap();

        assert_eq!(
            FailureOrigin::from_span(span),
            Ok(FailureOrigin {
                file: file.as_u32(),
                start: 4,
                end: 9,
            })
        );
    }
}
