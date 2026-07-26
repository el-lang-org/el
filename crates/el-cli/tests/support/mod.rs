use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("el-cli-test-{}-{sequence}", std::process::id()));
        fs::create_dir(&path).expect("create temporary test directory");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove temporary test directory");
    }
}

pub fn assert_golden(name: &str, actual: &[u8]) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    let expected = fs::read(&path)
        .unwrap_or_else(|error| panic!("could not read golden file {}: {error}", path.display()));
    assert_eq!(
        actual,
        expected,
        "golden output differed: {}",
        path.display()
    );
}
