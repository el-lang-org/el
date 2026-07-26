//! Private runtime boundary shared by a matching EL compiler distribution.
//!
//! Milestone 0 defines only the ABI identity. Native allocation and the
//! vendored collector are introduced behind this crate in later milestones.

/// Current private compiler/runtime ABI revision.
pub const PRIVATE_ABI_VERSION: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_abi_starts_at_one() {
        assert_eq!(PRIVATE_ABI_VERSION, 1);
    }
}
