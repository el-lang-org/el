use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

/// Target-dependent facts recorded with native build output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TargetMetadata {
    llvm_target_triple: String,
    pointer_width: u32,
}

impl TargetMetadata {
    pub fn new(
        llvm_target_triple: impl Into<String>,
        pointer_width: u32,
    ) -> Result<Self, InvalidTargetMetadata> {
        let llvm_target_triple = llvm_target_triple.into();
        if llvm_target_triple.is_empty() {
            return Err(InvalidTargetMetadata::EmptyTriple);
        }
        if !llvm_target_triple
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(InvalidTargetMetadata::InvalidTripleCharacter);
        }
        if !matches!(pointer_width, 32 | 64) {
            return Err(InvalidTargetMetadata::UnsupportedPointerWidth(
                pointer_width,
            ));
        }
        Ok(Self {
            llvm_target_triple,
            pointer_width,
        })
    }

    #[must_use]
    pub fn llvm_target_triple(&self) -> &str {
        &self.llvm_target_triple
    }

    #[must_use]
    pub const fn pointer_width(&self) -> u32 {
        self.pointer_width
    }

    /// Deterministic build metadata text. Additional pinned build facts can be
    /// appended without relying on debug formatting or unordered maps.
    #[must_use]
    pub fn reproducibility_text(&self) -> String {
        format!(
            "llvm_target_triple = \"{}\"\npointer_width = {}\nunicode_version = \"{}\"\n",
            self.llvm_target_triple,
            self.pointer_width,
            el_runtime::UNICODE_VERSION
        )
    }

    pub fn write_reproducibility_file(&self, path: &Path) -> Result<(), MetadataWriteError> {
        fs::write(path, self.reproducibility_text()).map_err(|source| MetadataWriteError {
            path: path.to_path_buf(),
            kind: source.kind(),
            message: source.to_string(),
        })
    }
}

impl fmt::Display for TargetMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "LLVM target {} ({}-bit pointers)",
            self.llvm_target_triple, self.pointer_width
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidTargetMetadata {
    EmptyTriple,
    InvalidTripleCharacter,
    UnsupportedPointerWidth(u32),
}

impl fmt::Display for InvalidTargetMetadata {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTriple => formatter.write_str("LLVM target triple cannot be empty"),
            Self::InvalidTripleCharacter => formatter.write_str(
                "LLVM target triple contains a character that cannot be recorded safely",
            ),
            Self::UnsupportedPointerWidth(width) => {
                write!(formatter, "unsupported target pointer width {width}")
            }
        }
    }
}

impl std::error::Error for InvalidTargetMetadata {}

/// A reproducibility metadata file could not be written.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataWriteError {
    pub path: std::path::PathBuf,
    pub kind: io::ErrorKind,
    pub message: String,
}

impl fmt::Display for MetadataWriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "could not write target metadata `{}`: {}",
            self.path.display(),
            self.message
        )
    }
}

impl std::error::Error for MetadataWriteError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FILE: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn renders_and_records_deterministic_target_metadata() {
        let metadata = TargetMetadata::new("aarch64-unknown-test", 64).unwrap();
        assert_eq!(metadata.llvm_target_triple(), "aarch64-unknown-test");
        assert_eq!(metadata.pointer_width(), 64);
        assert_eq!(
            metadata.reproducibility_text(),
            "llvm_target_triple = \"aarch64-unknown-test\"\npointer_width = 64\nunicode_version = \"17.0.0\"\n"
        );
        assert_eq!(
            metadata.to_string(),
            "LLVM target aarch64-unknown-test (64-bit pointers)"
        );

        let sequence = NEXT_FILE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "el-target-metadata-test-{}-{sequence}",
            std::process::id()
        ));
        metadata
            .write_reproducibility_file(&path)
            .expect("write metadata");
        assert_eq!(
            fs::read_to_string(&path).expect("read metadata"),
            metadata.reproducibility_text()
        );
        fs::remove_file(path).expect("remove metadata");
    }

    #[test]
    fn accepts_only_v1_target_pointer_widths() {
        assert_eq!(
            TargetMetadata::new("", 64),
            Err(InvalidTargetMetadata::EmptyTriple)
        );
        assert_eq!(
            TargetMetadata::new("target", 16),
            Err(InvalidTargetMetadata::UnsupportedPointerWidth(16))
        );
        assert_eq!(
            TargetMetadata::new("target\nother", 64),
            Err(InvalidTargetMetadata::InvalidTripleCharacter)
        );
        assert!(TargetMetadata::new("target", 32).is_ok());
        assert!(TargetMetadata::new("target", 64).is_ok());
    }
}
