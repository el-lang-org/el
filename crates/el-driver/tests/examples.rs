use el_driver::{ProjectCommand, run_project_command};
use std::path::Path;

#[test]
fn unicode_report_is_a_locked_buildable_v1_project() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/unicode_report");
    run_project_command(ProjectCommand::Check { locked: true }, &project)
        .expect("unicode report example passes the complete frontend");
}

#[test]
fn registration_validation_uses_with_and_is_a_locked_buildable_project() {
    let project =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/registration_validation");
    run_project_command(ProjectCommand::Check { locked: true }, &project)
        .expect("registration validation example passes the complete frontend");
}
