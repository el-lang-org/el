use el_parser::parse;
use el_resolve::{resolve, resolve_package, resolve_package_function};
use el_span::SourceMap;

fn parsed(source: &str) -> el_ast::Program {
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.ell", source);
    parse(file, source).expect("fixture parses")
}

#[test]
fn assigns_declaration_and_parameter_ids_in_source_order() {
    let program = parsed(
        "defmodule Main do\n  defp identity(value: a) -> a do\n    value\n  end\n  def main() -> i32 do\n    identity(42)\n  end\nend\n",
    );

    let resolved = resolve(&program).expect("names resolve");

    assert_eq!(
        resolved.debug_tree(),
        "module m0 Main\n  function d0 identity Private <a>\n    parameter s0 value: a\n  function d1 main Public\n"
    );
}

#[test]
fn reports_duplicate_functions_and_parameters_with_source_spans() {
    let program = parsed(
        "defmodule Main do\n  def same(x: i64, x: i64) -> i64 do\n    x\n  end\n  def same() -> unit do\n    unit\n  end\nend\n",
    );

    let diagnostics = resolve(&program).expect_err("duplicates are rejected");

    assert_eq!(
        diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>(),
        ["E2002", "E2001"]
    );
    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| !diagnostic.primary.is_empty())
    );
}

#[test]
fn collects_a_separate_alias_namespace_in_source_order() {
    let program = parsed(
        "defmodule Main do\n  @type Scalar = bool | i64\n  def main() -> Scalar do\n    true\n  end\nend\n",
    );

    let resolved = resolve(&program).expect("alias resolves before signature use");

    assert_eq!(resolved.aliases[0].id.0, 0);
    assert_eq!(resolved.functions[0].id.0, 1);
    assert!(resolved.debug_tree().contains("alias d0 Scalar"));
}

#[test]
fn rejects_direct_mutual_and_generic_alias_cycles() {
    let cases = [
        "@type Loop = Loop",
        "@type Left = Right\n  @type Right = Left",
        "@type Loop(a) = Loop(a)",
        "@type Loop = [Loop]",
    ];
    for declarations in cases {
        let source = format!("defmodule Main do\n  {declarations}\nend\n");
        let diagnostics = resolve(&parsed(&source)).expect_err("alias cycle is rejected");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "E2008"),
            "missing cycle diagnostic: {diagnostics:?}"
        );
    }
}

#[test]
fn rejects_unknown_aliases_and_wrong_type_arity() {
    let cases = [
        "@type Bad = Missing",
        "@type Pair(a, b) = a\n  @type Bad = Pair(i64)",
    ];
    for declarations in cases {
        let source = format!("defmodule Main do\n  {declarations}\nend\n");
        assert!(resolve(&parsed(&source)).is_err());
    }
}

#[test]
fn accepts_managed_recursive_structs_and_rejects_inline_layout_cycles() {
    let accepted =
        parsed("defmodule Main do\n  defstruct Node do\n    children: [Node]\n  end\nend\n");
    let resolved = resolve(&accepted).expect("List breaks the recursive layout cycle");
    assert_eq!(resolved.structs[0].name, "Node");

    let rejected = [
        "defstruct Invalid do\n    next: Invalid\n  end",
        "defstruct Left do\n    right: Right\n  end\n  defstruct Right do\n    left: Left\n  end",
        "defstruct Invalid do\n    next: {i64, Invalid}\n  end",
        "@type Wrapped = Invalid\n  defstruct Invalid do\n    next: Wrapped\n  end",
    ];
    for declarations in rejected {
        let source = format!("defmodule Main do\n  {declarations}\nend\n");
        let diagnostics = resolve(&parsed(&source)).expect_err("inline cycle is rejected");
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "E2010"),
            "missing finite-layout diagnostic: {diagnostics:?}"
        );
    }
}

#[test]
fn collects_and_validates_complete_user_implementation_metadata() {
    let source = "defmodule Main do\n  defstruct Box do\n    value: i64\n  end\n  defprotocol Render do\n    type Output\n    def render(value: Box) -> i64\n  end\n  defimpl Render, for: Box do\n    type Output = i64\n    def render(value: Box) -> i64 do\n      0\n    end\n  end\nend\n";
    let program = resolve(&parsed(source)).expect("complete implementation resolves");
    assert_eq!(program.implementations.len(), 1);
    let method = program.implementations[0].method_declarations[0].1;
    assert!(
        program
            .functions
            .iter()
            .any(|function| function.id == method)
    );
    assert!(program.debug_tree().contains("impl i0 Render for Box"));

    let incomplete = source.replace("    type Output = i64\n", "");
    let diagnostics =
        resolve(&parsed(&incomplete)).expect_err("missing associated type is rejected");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2019")
    );
}

#[test]
fn validates_derives_orphans_constraints_and_overlapping_generic_heads() {
    let derived = resolve(&parsed(
        "defmodule Main do\n  @derive [Eq, Ord, Show, Hash]\n  defstruct Box(a) do\n    value: a\n  end\nend\n",
    ))
    .expect("core derives resolve");
    assert_eq!(derived.structs[0].derives, ["Eq", "Ord", "Show", "Hash"]);

    let overlapping = parsed(
        "defmodule Main do\n  defstruct Pair(a, b) do\n    left: a\n    right: b\n  end\n  defprotocol P do\n  end\n  defimpl P, for: Pair(a, a) do\n  end\n  defimpl P, for: Pair(i32, b) do\n  end\nend\n",
    );
    let diagnostics = resolve(&overlapping).expect_err("generic heads overlap at Pair(i32, i32)");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2016")
    );

    let disjoint = parsed(
        "defmodule Main do\n  defstruct Pair(a, b) do\n    left: a\n    right: b\n  end\n  defprotocol P do\n  end\n  defimpl P, for: Pair(i32, i32) do\n  end\n  defimpl P, for: Pair(i32, i64) do\n  end\nend\n",
    );
    resolve(&disjoint).expect("different concrete heads are coherent");

    let orphan = parsed(
        "defmodule Main do\n  defimpl Eq, for: i32 do\n    def eq(left: i32, right: i32) -> bool do\n      true\n    end\n  end\nend\n",
    );
    let diagnostics = resolve(&orphan).expect_err("core primitive implementation is orphaned");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2032")
    );
}

#[test]
fn requires_exact_substituted_protocol_method_signatures() {
    let source = "defmodule Main do\n  defstruct Box(a) do\n    value: a\n  end\n  defprotocol Extract do\n    type Item\n    def extract(value: Self) -> Item\n  end\n  defimpl Extract, for: Box(a) do\n    type Item = a\n    def extract(value: Box(a)) -> a do\n      value.value\n    end\n  end\nend\n";
    resolve(&parsed(source)).expect("Self and associated types substitute exactly");

    let wrong = source.replace(
        "def extract(value: Box(a)) -> a do",
        "def extract(value: Box(a)) -> i64 do",
    );
    let diagnostics =
        resolve(&parsed(&wrong)).expect_err("inexact substituted signature is rejected");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2036")
    );
}

#[test]
fn resolves_cross_module_types_functions_and_visibility() {
    let library = parsed(
        "defmodule Library do\n  defstruct Box do\n    value: i64\n  end\n  def public(value: Box) -> Box do\n    value\n  end\n  defp hidden(value: Box) -> Box do\n    value\n  end\nend\n",
    );
    let main = parsed(
        "defmodule Main do\n  def keep(value: Library.Box) -> Library.Box do\n    value\n  end\nend\n",
    );
    let modules = resolve_package(&[library, main]).expect("package names resolve together");
    assert_ne!(modules[0].functions[0].id, modules[1].functions[0].id);
    let span = modules[1].module_span;
    assert_eq!(
        resolve_package_function(&modules, "Main", "Library.public", span)
            .expect("public function")
            .name,
        "public"
    );
    assert_eq!(
        resolve_package_function(&modules, "Main", "Library.hidden", span)
            .expect_err("private function")
            .code,
        "E2027"
    );
}

#[test]
fn rejects_cross_module_alias_cycles() {
    let first = parsed("defmodule First do\n  @type A = Second.B\nend\n");
    let second = parsed("defmodule Second do\n  @type B = First.A\nend\n");
    let diagnostics = resolve_package(&[first, second]).expect_err("package alias cycle");
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "E2008")
    );
}

#[test]
fn resolves_core_and_local_protocol_constraints_in_a_separate_namespace() {
    let source = parsed(
        "defmodule Main do\n  defprotocol Marker do\n  end\n  def core(value: a) -> a when a: Eq do\n    value\n  end\n  def local(value: a) -> a when a: Marker do\n    value\n  end\nend\n",
    );

    let resolved = resolve(&source).expect("protocol constraints resolve");

    assert_eq!(resolved.protocols[0].name, "Marker");
    assert_eq!(resolved.functions[0].constraints[0].protocol, "Eq");
    assert_eq!(resolved.functions[1].constraints[0].protocol, "Marker");
}

#[test]
fn rejects_unknown_protocols_and_constraint_parameters() {
    let cases = [
        "def value(input: a) -> a when a: Missing do\n    input\n  end",
        "def value(input: a) -> a when b: Eq do\n    input\n  end",
    ];
    for declaration in cases {
        let source = format!("defmodule Main do\n  {declaration}\nend\n");
        assert!(resolve(&parsed(&source)).is_err());
    }
}
