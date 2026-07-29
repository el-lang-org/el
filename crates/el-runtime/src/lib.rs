//! Private runtime boundary shared by a matching EL compiler distribution.
//!
//! The Rust surface intentionally contains metadata only. Native implementation
//! details, including Boehm types, stay in the private C wrapper built by the
//! `boehm` feature.

#[cfg(feature = "boehm")]
use std::path::{Path, PathBuf};

/// Current private compiler/runtime ABI revision.
pub const PRIVATE_ABI_VERSION: u32 = 4;
/// Unicode data version fixed by the EL v1 language contract.
pub const UNICODE_VERSION: &str = "17.0.0";

/// Symbol called by generated code for source-mandated unrecoverable failures.
pub const FAILURE_SYMBOL: &str = "__el_runtime_fail";

/// Must run before compiler-generated managed global initialization.
pub const INITIALIZE_SYMBOL: &str = "__el_runtime_init";
/// Allocates collector-scanned storage whose scan class never changes.
pub const ALLOCATE_SCANNED_SYMBOL: &str = "__el_runtime_alloc_scanned";
/// Allocates pointer-free storage whose scan class never changes.
pub const ALLOCATE_ATOMIC_SYMBOL: &str = "__el_runtime_alloc_atomic";
/// Registers compiler-emitted scanned global storage with the collector.
pub const REGISTER_GLOBALS_SYMBOL: &str = "__el_runtime_register_managed_globals";
/// Returns the opaque process-local seed used by generated map hashers.
pub const HASH_SEED_SYMBOL: &str = "__el_runtime_hash_seed";
/// Validates UTF-8 and returns the input length or the first invalid sequence offset.
pub const UTF8_VALIDATE_SYMBOL: &str = "__el_runtime_utf8_validate";
/// Finds the next Unicode extended-grapheme boundary at or after an input boundary.
pub const GRAPHEME_NEXT_SYMBOL: &str = "__el_runtime_grapheme_next";
/// Counts Unicode extended grapheme clusters in valid UTF-8.
pub const GRAPHEME_COUNT_SYMBOL: &str = "__el_runtime_grapheme_count";

/// Static archives required when linking a managed EL executable.
#[cfg(feature = "boehm")]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeRuntimeArchives {
    wrapper: PathBuf,
    collector: PathBuf,
}

#[cfg(feature = "boehm")]
impl NativeRuntimeArchives {
    #[must_use]
    pub fn wrapper(&self) -> &Path {
        &self.wrapper
    }

    #[must_use]
    pub fn collector(&self) -> &Path {
        &self.collector
    }
}

/// Returns the Cargo-built private runtime artifacts without exposing Boehm
/// headers or types to compiler stages.
#[cfg(feature = "boehm")]
#[must_use]
pub fn native_runtime_archives() -> NativeRuntimeArchives {
    NativeRuntimeArchives {
        wrapper: PathBuf::from(env!("EL_RUNTIME_NATIVE_DIR")).join("libel_runtime.a"),
        collector: PathBuf::from(env!("EL_RUNTIME_BOEHM_LIB_DIR")).join("libgc.a"),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeCallEffect {
    Allocating,
    NonAllocating,
}

#[must_use]
pub fn runtime_call_effect(symbol: &str) -> Option<RuntimeCallEffect> {
    match symbol {
        INITIALIZE_SYMBOL
        | REGISTER_GLOBALS_SYMBOL
        | HASH_SEED_SYMBOL
        | UTF8_VALIDATE_SYMBOL
        | GRAPHEME_NEXT_SYMBOL
        | GRAPHEME_COUNT_SYMBOL => Some(RuntimeCallEffect::NonAllocating),
        ALLOCATE_SCANNED_SYMBOL | ALLOCATE_ATOMIC_SYMBOL => Some(RuntimeCallEffect::Allocating),
        FAILURE_SYMBOL => Some(RuntimeCallEffect::NonAllocating),
        _ => None,
    }
}

/// Stable categories currently emitted by the first integer backend slice.
/// Numeric values are private to a matching compiler/runtime distribution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FailureCategory {
    IntegerOverflow = 1,
    DivisionByZero = 2,
    InvalidShift = 3,
    InvalidConversion = 4,
    IndexOutOfBounds = 5,
    AllocationExhausted = 6,
    ConsoleOutputFailed = 7,
    BitstringSizeMismatch = 8,
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
            Self::InvalidShift => "invalid_shift",
            Self::InvalidConversion => "invalid_conversion",
            Self::IndexOutOfBounds => "index_out_of_bounds",
            Self::AllocationExhausted => "allocation_exhausted",
            Self::ConsoleOutputFailed => "console_output_failed",
            Self::BitstringSizeMismatch => "bitstring_size_mismatch",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_abi_tracks_the_unicode_segmentation_boundary() {
        assert_eq!(PRIVATE_ABI_VERSION, 4);
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

    #[test]
    fn managed_runtime_calls_have_explicit_collection_effects() {
        assert_eq!(
            runtime_call_effect(ALLOCATE_SCANNED_SYMBOL),
            Some(RuntimeCallEffect::Allocating)
        );
        assert_eq!(
            runtime_call_effect(ALLOCATE_ATOMIC_SYMBOL),
            Some(RuntimeCallEffect::Allocating)
        );
        assert_eq!(
            runtime_call_effect(INITIALIZE_SYMBOL),
            Some(RuntimeCallEffect::NonAllocating)
        );
        assert_eq!(
            runtime_call_effect(UTF8_VALIDATE_SYMBOL),
            Some(RuntimeCallEffect::NonAllocating)
        );
        assert_eq!(
            runtime_call_effect(GRAPHEME_NEXT_SYMBOL),
            Some(RuntimeCallEffect::NonAllocating)
        );
        assert_eq!(
            runtime_call_effect(GRAPHEME_COUNT_SYMBOL),
            Some(RuntimeCallEffect::NonAllocating)
        );
        assert_eq!(
            runtime_call_effect("GC_malloc"),
            None,
            "Boehm symbols are not ABI calls"
        );
    }

    #[test]
    fn all_v1_failure_categories_have_stable_identifiers() {
        assert_eq!(FailureCategory::AllocationExhausted.code(), 6);
        assert_eq!(
            FailureCategory::AllocationExhausted.identifier(),
            "allocation_exhausted"
        );
        assert_eq!(FailureCategory::BitstringSizeMismatch.code(), 8);
    }
}
