//! Private target layout and native code-generation support.

use el_ir::{ConcreteModule, Type, TypeId};
use std::fmt;

mod linker;
#[cfg(feature = "managed-runtime")]
pub use linker::link_host_managed_executable;
pub use linker::{IoError, LinkerError, link_host_executable, link_host_objects};
mod metadata;
pub use metadata::{InvalidTargetMetadata, MetadataWriteError, TargetMetadata};

/// Native code-generation profile selected by the driver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodegenProfile {
    Development,
    Release,
}

#[cfg(any(feature = "llvm", feature = "llvm-api-check", test))]
mod integer_checks;

#[cfg(any(feature = "llvm", feature = "llvm-api-check"))]
mod llvm;
#[cfg(any(feature = "llvm", feature = "llvm-api-check"))]
pub use llvm::{
    BackendError, VerifiedLlvmIr, emit_host_object, emit_host_object_with_profile,
    host_target_metadata, lower_to_llvm_ir,
};

/// The allocation size and ABI alignment of a concrete value, in bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeLayout {
    size: u64,
    alignment: u64,
}

impl TypeLayout {
    /// Constructs a target layout after checking the invariants later aggregate
    /// layout computations rely on.
    pub fn new(size: u64, alignment: u64) -> Result<Self, InvalidTypeLayout> {
        if alignment == 0 || !alignment.is_power_of_two() {
            return Err(InvalidTypeLayout { size, alignment });
        }
        if size != 0 && !size.is_multiple_of(alignment) {
            return Err(InvalidTypeLayout { size, alignment });
        }
        Ok(Self { size, alignment })
    }

    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }

    #[must_use]
    pub const fn alignment(self) -> u64 {
        self.alignment
    }
}

/// A malformed size/alignment pair supplied by a target description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidTypeLayout {
    pub size: u64,
    pub alignment: u64,
}

impl fmt::Display for InvalidTypeLayout {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid target layout: size {} and alignment {} bytes",
            self.size, self.alignment
        )
    }
}

impl std::error::Error for InvalidTypeLayout {}

/// Target ABI facts needed for the first native primitive slice.
///
/// LLVM-derived target data will replace the host constructor at the LLVM
/// initialization boundary. Keeping these facts explicit prevents aggregate
/// layout from accidentally depending on Rust struct layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrimitiveAbi {
    pub u8: TypeLayout,
    pub i32: TypeLayout,
    pub i64: TypeLayout,
    pub usize: TypeLayout,
    pub boolean: TypeLayout,
    pub unit: TypeLayout,
}

impl PrimitiveAbi {
    /// Describes the compiler host, which is the only Milestone 3 target.
    #[must_use]
    pub fn host() -> Self {
        Self {
            u8: rust_layout::<u8>(),
            i32: rust_layout::<i32>(),
            i64: rust_layout::<i64>(),
            usize: rust_layout::<usize>(),
            boolean: rust_layout::<bool>(),
            unit: rust_layout::<()>(),
        }
    }
}

fn rust_layout<T>() -> TypeLayout {
    TypeLayout::new(
        std::mem::size_of::<T>() as u64,
        std::mem::align_of::<T>() as u64,
    )
    .expect("Rust primitive layout satisfies target layout invariants")
}

/// Target layouts indexed by the Concrete Core module's stable `TypeId`s.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetLayouts {
    layouts: Vec<Option<TypeLayout>>,
}

impl TargetLayouts {
    /// Returns a computed primitive layout. Composite types remain unavailable
    /// until their corresponding Milestone deliverable is implemented.
    pub fn get(&self, ty: TypeId) -> Result<TypeLayout, LayoutError> {
        self.layouts.get(ty.0 as usize).copied().flatten().ok_or({
            if ty.0 as usize >= self.layouts.len() {
                LayoutError::UnknownType(ty)
            } else {
                LayoutError::UnsupportedType(ty)
            }
        })
    }
}

/// A requested type cannot be laid out by the current backend slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LayoutError {
    UnknownType(TypeId),
    UnsupportedType(TypeId),
}

impl fmt::Display for LayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownType(ty) => write!(formatter, "unknown Concrete Core type {ty:?}"),
            Self::UnsupportedType(ty) => {
                write!(formatter, "target layout is not implemented for {ty:?}")
            }
        }
    }
}

impl std::error::Error for LayoutError {}

/// Computes the first target-layout table without depending on source type
/// spellings or fixed primitive `TypeId` positions.
#[must_use]
pub fn compute_primitive_layouts(module: &ConcreteModule, abi: PrimitiveAbi) -> TargetLayouts {
    let layouts = module
        .types
        .iter()
        .map(|ty| match ty {
            Type::U8 => Some(abi.u8),
            Type::I32 => Some(abi.i32),
            Type::Rune => Some(abi.i32),
            Type::Utf8Error => Some(abi.usize),
            Type::I64 => Some(abi.i64),
            Type::Usize => Some(abi.usize),
            Type::Bool => Some(abi.boolean),
            Type::Unit => Some(abi.unit),
            Type::String
            | Type::Bytes
            | Type::Bits
            | Type::Buffer
            | Type::Atom(_)
            | Type::List(_)
            | Type::Array { .. }
            | Type::Slice(_)
            | Type::Map { .. }
            | Type::Tuple(_)
            | Type::Function { .. }
            | Type::Struct { .. }
            | Type::Parameter { .. }
            | Type::Union(_) => None,
        })
        .collect();
    TargetLayouts { layouts }
}

#[cfg(test)]
mod tests {
    use super::*;
    use el_ir::{ConcreteModule, ReachabilityRoots};

    fn module(types: Vec<Type>) -> ConcreteModule {
        ConcreteModule {
            types,
            structs: Vec::new(),
            functions: Vec::new(),
            roots: ReachabilityRoots {
                functions: Vec::new(),
            },
        }
    }

    #[test]
    fn computes_all_first_slice_primitive_layouts_by_type_id() {
        let module = module(vec![Type::Unit, Type::Bool, Type::I64, Type::I32, Type::U8]);
        let layouts = compute_primitive_layouts(&module, PrimitiveAbi::host());

        assert_eq!(layouts.get(TypeId(0)).unwrap(), rust_layout::<()>());
        assert_eq!(layouts.get(TypeId(1)).unwrap(), rust_layout::<bool>());
        assert_eq!(layouts.get(TypeId(2)).unwrap(), rust_layout::<i64>());
        assert_eq!(layouts.get(TypeId(3)).unwrap(), rust_layout::<i32>());
        assert_eq!(layouts.get(TypeId(4)).unwrap(), rust_layout::<u8>());
        assert_eq!(layouts.get(TypeId(0)).unwrap().size(), 0);
        assert_eq!(layouts.get(TypeId(0)).unwrap().alignment(), 1);
    }

    #[test]
    fn uses_explicit_target_abi_instead_of_assuming_host_alignment() {
        let i64 = TypeLayout::new(8, 4).unwrap();
        let abi = PrimitiveAbi {
            u8: TypeLayout::new(1, 1).unwrap(),
            i32: TypeLayout::new(4, 4).unwrap(),
            i64,
            usize: TypeLayout::new(8, 8).unwrap(),
            boolean: TypeLayout::new(1, 1).unwrap(),
            unit: TypeLayout::new(0, 1).unwrap(),
        };
        let layouts = compute_primitive_layouts(&module(vec![Type::I64]), abi);

        assert_eq!(layouts.get(TypeId(0)), Ok(i64));
    }

    #[test]
    fn rejects_invalid_layout_facts_and_unavailable_types() {
        assert_eq!(
            TypeLayout::new(8, 3),
            Err(InvalidTypeLayout {
                size: 8,
                alignment: 3,
            })
        );
        assert_eq!(
            TypeLayout::new(6, 4),
            Err(InvalidTypeLayout {
                size: 6,
                alignment: 4,
            })
        );

        let layouts = compute_primitive_layouts(
            &module(vec![Type::Tuple(vec![TypeId(1)])]),
            PrimitiveAbi::host(),
        );
        assert_eq!(
            layouts.get(TypeId(0)),
            Err(LayoutError::UnsupportedType(TypeId(0)))
        );
        assert_eq!(
            layouts.get(TypeId(9)),
            Err(LayoutError::UnknownType(TypeId(9)))
        );
    }
}
