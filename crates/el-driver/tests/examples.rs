use el_driver::{ProjectCommand, run_project_command};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "el-examples-test-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create examples test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove examples test directory");
    }
}

#[test]
fn unicode_report_is_a_locked_buildable_v1_project() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/unicode_report");
    run_project_command(ProjectCommand::Check { locked: true }, &project)
        .expect("unicode report example passes the complete frontend");
}

#[test]
fn registration_validation_uses_with_and_is_a_locked_buildable_project() {
    let temp = TempDir::new();
    let example =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/registration_validation");
    fs::create_dir(temp.path().join("src")).unwrap();
    fs::copy(example.join("main.ell"), temp.path().join("src/main.ell")).unwrap();
    fs::write(
        temp.path().join("el.toml"),
        "[package]\nname = \"registration_validation\"\nnamespace = \"RegistrationValidation\"\nversion = \"1.0.0\"\n\n[deps]\n\n[target]\nmain = \"Main\"\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("el.lock"),
        "version = 1\n\n[[package]]\nname = \"registration_validation\"\nnamespace = \"RegistrationValidation\"\nversion = \"1.0.0\"\nsource = \"root\"\ndependencies = []\n",
    )
    .unwrap();

    run_project_command(ProjectCommand::Check { locked: true }, temp.path())
        .expect("registration validation example passes the complete frontend");
}
