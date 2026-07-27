//! Private runtime boundary shared by a matching EL compiler distribution.
//!
//! Milestone 3 adds the failure categories and symbol declarations required by
//! checked integer lowering. Native failure reporting, allocation, and the
//! vendored collector are introduced behind this crate in later deliverables.

/// Current private compiler/runtime ABI revision.
pub const PRIVATE_ABI_VERSION: u32 = 1;

/// Symbol called by generated code for source-mandated unrecoverable failures.
pub const FAILURE_SYMBOL: &str = "__el_runtime_fail";

/// Stable categories currently emitted by the first integer backend slice.
/// Numeric values are private to a matching compiler/runtime distribution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FailureCategory {
    IntegerOverflow = 1,
    DivisionByZero = 2,
}

impl FailureCategory {
    #[must_use]
    pub const fn code(self) -> u32 {
        self as u32
    }

    #[must_use]
    pub const fn identifier(self) -> &'static str {
        match self {
            Self::IntegerOverflow => "integer_overflow",
            Self::DivisionByZero => "division_by_zero",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_abi_starts_at_one() {
        assert_eq!(PRIVATE_ABI_VERSION, 1);
    }

    #[test]
    fn integer_failure_categories_have_stable_private_codes_and_public_identifiers() {
        assert_eq!(FailureCategory::IntegerOverflow.code(), 1);
        assert_eq!(
            FailureCategory::IntegerOverflow.identifier(),
            "integer_overflow"
        );
        assert_eq!(FailureCategory::DivisionByZero.code(), 2);
        assert_eq!(
            FailureCategory::DivisionByZero.identifier(),
            "division_by_zero"
        );
    }
}
