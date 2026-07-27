use el_ir::ArithmeticOperator;
use el_runtime::FailureCategory;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct IntegerCheckCategories {
    pub zero: Option<FailureCategory>,
    pub overflow: FailureCategory,
}

pub(crate) fn failure_categories(operator: ArithmeticOperator) -> IntegerCheckCategories {
    match operator {
        ArithmeticOperator::Add | ArithmeticOperator::Subtract | ArithmeticOperator::Multiply => {
            IntegerCheckCategories {
                zero: None,
                overflow: FailureCategory::IntegerOverflow,
            }
        }
        ArithmeticOperator::Divide | ArithmeticOperator::Remainder => IntegerCheckCategories {
            zero: Some(FailureCategory::DivisionByZero),
            overflow: FailureCategory::IntegerOverflow,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use el_span::SourceMap;

    #[test]
    fn assigns_mandated_categories_independently_of_build_profile() {
        for operator in [
            ArithmeticOperator::Add,
            ArithmeticOperator::Subtract,
            ArithmeticOperator::Multiply,
        ] {
            assert_eq!(
                failure_categories(operator),
                IntegerCheckCategories {
                    zero: None,
                    overflow: FailureCategory::IntegerOverflow,
                }
            );
        }
        for operator in [ArithmeticOperator::Divide, ArithmeticOperator::Remainder] {
            assert_eq!(
                failure_categories(operator),
                IntegerCheckCategories {
                    zero: Some(FailureCategory::DivisionByZero),
                    overflow: FailureCategory::IntegerOverflow,
                }
            );
        }
    }

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
