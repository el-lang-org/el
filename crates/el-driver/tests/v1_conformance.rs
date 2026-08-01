use el_driver::analyze_source;
use el_parser::parse;
use el_span::SourceMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
struct Case {
    disposition: String,
    phase: Option<String>,
    code: Option<String>,
    spec: String,
}

#[test]
fn reference_programs_and_negative_examples_are_traceable_conformance_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/conformance/v1");
    let mut fixtures = fixture_paths(&root);
    fixtures.sort();
    assert!(!fixtures.is_empty());

    for path in fixtures {
        let source = fs::read_to_string(&path).expect("read conformance fixture");
        let case = parse_metadata(&source);
        assert!(
            case.spec.contains(".md §"),
            "{} is not traceable to a specification section",
            path.display()
        );
        let relative = path.strip_prefix(&root).unwrap();
        let mut sources = SourceMap::new();
        let file = sources.add_file(relative, &source);

        match case.disposition.as_str() {
            "accept" => {
                analyze_source(file, &source).unwrap_or_else(|diagnostics| {
                    panic!("{} was rejected: {diagnostics:#?}", path.display())
                });
                assert!(case.phase.is_none() && case.code.is_none());
            }
            "reject" if case.phase.as_deref() == Some("syntax") => {
                let error = parse(file, &source)
                    .expect_err("negative syntax fixture unexpectedly produced an AST");
                assert!(!error.message.is_empty());
                assert_eq!(case.code.as_deref(), Some("E1000"));
            }
            "reject" => {
                let diagnostics = analyze_source(file, &source)
                    .expect_err("negative semantic fixture unexpectedly reached Core IR");
                let expected = case.code.as_deref().expect("negative case diagnostic code");
                assert!(
                    diagnostics
                        .iter()
                        .any(|diagnostic| diagnostic.code == expected),
                    "{} expected {expected}, got {diagnostics:#?}",
                    path.display()
                );
            }
            disposition => panic!("unknown fixture disposition {disposition:?}"),
        }
    }
}

fn fixture_paths(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for directory in [root.join("accept"), root.join("reject")] {
        for entry in fs::read_dir(directory).expect("read fixture directory") {
            let path = entry.expect("read fixture entry").path();
            if path.extension().is_some_and(|extension| extension == "el") {
                paths.push(path);
            }
        }
    }
    paths
}

fn parse_metadata(source: &str) -> Case {
    let header = source
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("# conformance: "))
        .expect("fixture metadata header");
    let mut fields = header.split(';').map(str::trim);
    let disposition = fields.next().expect("fixture disposition").to_owned();
    let mut phase = None;
    let mut code = None;
    let mut spec = None;
    for field in fields {
        let (key, value) = field.split_once('=').expect("metadata key/value");
        match key {
            "phase" => phase = Some(value.to_owned()),
            "code" => code = Some(value.to_owned()),
            "spec" => spec = Some(value.to_owned()),
            _ => panic!("unknown fixture metadata key {key:?}"),
        }
    }
    Case {
        disposition,
        phase,
        code,
        spec: spec.expect("fixture specification reference"),
    }
}
