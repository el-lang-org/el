#![cfg(feature = "llvm")]

#[cfg(feature = "managed-runtime")]
use el_codegen::link_host_managed_executable;
use el_codegen::{emit_host_object_with_profile, link_host_objects};
use el_driver::{BuildProfile, analyze_source};
use el_ir::{executable_reachability_roots, monomorphize};
use el_span::SourceMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "el-native-e2e-test-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create native test directory");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove native test directory");
    }
}

fn compile_runtime_failure_stub(directory: &Path) -> PathBuf {
    let source = directory.join("runtime.c");
    let object = directory.join("runtime.o");
    fs::write(
        &source,
        "#include <stdint.h>\n#include <stdlib.h>\nvoid __el_runtime_init(void) {}\nvoid __el_runtime_fail(uint32_t category, uint32_t file, uint64_t start, uint64_t end) {\n  (void)file; (void)start; (void)end;\n  _Exit((int)(100u + category));\n}\n",
    )
    .expect("write runtime test support");
    let compiler = std::env::var_os("CC").unwrap_or_else(|| OsString::from("cc"));
    let output = Command::new(&compiler)
        .arg("-std=c11")
        .arg("-c")
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .output()
        .expect("start host C compiler");
    assert!(
        output.status.success(),
        "runtime support compilation failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    object
}

#[test]
fn utf8_string_byte_size_runs_in_development_and_release() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let source = "defmodule Main do\n  def main() -> i32 do\n    if String.byte_size(\"é🙂\") == 7 and String.byte_size(\"\") == 0 do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-string-byte-size"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "UTF-8 byte length must count encoded bytes in {label}"
        );
    }
}

#[test]
fn immutable_byte_views_share_backing_and_check_bounds_in_both_profiles() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let success = "defmodule Main do\n  def main() -> i32 do\n    data = String.bytes(\"é🙂\")\n    middle = Bytes.slice(data, 1, 5)\n    tail = Bytes.slice(middle, 2, 3)\n    if Bytes.byte_size(data) == 7 and Bytes.byte_size(middle) == 5 and Bytes.byte_size(tail) == 3 do\n      42\n    else\n      0\n    end\n  end\nend\n";
    let out_of_bounds = "defmodule Main do\n  def main() -> i32 do\n    Bytes.slice(String.bytes(\"abc\"), 2, 2)\n    0\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-byte-views"),
                success,
                profile,
            )
            .code(),
            Some(42),
            "byte views preserve byte lengths in {label}"
        );
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-byte-view-bounds"),
                out_of_bounds,
                profile,
            )
            .code(),
            Some(105),
            "Bytes.slice uses index_out_of_bounds in {label}"
        );
    }
}

#[test]
fn byte_indexing_reads_u8_and_checks_bounds_in_both_profiles() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let success = "defmodule Main do\n  def main() -> i32 do\n    data = String.bytes(\"é🙂\")\n    accent = String.bytes(\"é\")\n    tail = Bytes.slice(data, 3, 4)\n    if data[0] == 101 and data[1] == 204 and tail[0] == 240 and accent[0] > 100 do\n      42\n    else\n      0\n    end\n  end\nend\n";
    let out_of_bounds = "defmodule Main do\n  def main() -> i32 do\n    data = String.bytes(\"abc\")\n    data[3]\n    0\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-byte-index"),
                success,
                profile,
            )
            .code(),
            Some(42),
            "byte indexing reads unsigned UTF-8 bytes in {label}"
        );
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-byte-index-bounds"),
                out_of_bounds,
                profile,
            )
            .code(),
            Some(105),
            "bytes indexing uses index_out_of_bounds in {label}"
        );
    }
}

fn build_and_run(
    directory: &Path,
    runtime: &Path,
    name: &str,
    source: &str,
    profile: BuildProfile,
) -> ExitStatus {
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.el", source);
    let generic = analyze_source(file, source).expect("source reaches Generic Core");
    let roots = executable_reachability_roots(&generic).expect("select executable entry");
    let concrete = monomorphize(&generic, &roots).expect("monomorphize executable");
    let object = directory.join(format!("{name}.o"));
    let executable = directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));

    emit_host_object_with_profile(&concrete, &object, profile.codegen_profile())
        .expect("emit host object");
    link_host_objects(&[object.as_path(), runtime], &executable).expect("link native executable");
    Command::new(executable)
        .status()
        .expect("run native executable")
}

#[cfg(feature = "gc-stress-test")]
fn build_and_run_managed(
    directory: &Path,
    name: &str,
    source: &str,
    profile: BuildProfile,
) -> ExitStatus {
    let executable = build_managed_executable(directory, name, source, profile);
    Command::new(executable)
        .status()
        .expect("run managed native executable")
}

#[cfg(feature = "gc-stress-test")]
fn build_managed_executable(
    directory: &Path,
    name: &str,
    source: &str,
    profile: BuildProfile,
) -> PathBuf {
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.el", source);
    let generic = analyze_source(file, source).expect("source reaches Generic Core");
    let roots = executable_reachability_roots(&generic).expect("select executable entry");
    let concrete = monomorphize(&generic, &roots).expect("monomorphize executable");
    let object = directory.join(format!("{name}.o"));
    let executable = directory.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));

    emit_host_object_with_profile(&concrete, &object, profile.codegen_profile())
        .expect("emit managed host object");
    link_host_managed_executable(&[object.as_path()], &executable)
        .expect("link managed native executable");
    executable
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn named_function_values_call_indirectly_in_both_gc_stress_profiles() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def retain(text: string) -> string do\n    Rune.to_string('🙂')\n    text\n  end\n  def identity(value: a) -> a do\n    value\n  end\n  def apply(function: (string) -> string, value: string) -> string do\n    function(value)\n  end\n  def main() -> i32 do\n    first: (string) -> string = retain\n    second: (string) -> string = identity\n    text = apply(second, apply(first, \"é🙂\"))\n    if String.byte_size(text) == 7 do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-function-values"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "indirect calls must preserve managed arguments in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn enum_count_at_and_to_list_preserve_order_and_managed_items() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def list_at(values: [string]) -> bool do\n    match Enum.at(values, 1) do\n      some: {:some, string} -> match some do\n        {:some, value} -> value == \"bb\"\n      end\n      _ -> false\n    end\n  end\n  def array_missing(values: [string; 2]) -> bool do\n    match Enum.at(values, 2) do\n      some: {:some, string} -> false\n      _ -> true\n    end\n  end\n  def slice_at(values: Slice(string)) -> bool do\n    match Enum.at(values, 0) do\n      some: {:some, string} -> match some do\n        {:some, value} -> value == \"a\"\n      end\n      _ -> false\n    end\n  end\n  def byte_at(values: bytes) -> bool do\n    match Enum.at(values, 1) do\n      some: {:some, u8} -> match some do\n        {:some, value} -> value == 66\n      end\n      _ -> false\n    end\n  end\n  def map_at(values: Map(i32, i32)) -> bool do\n    match Enum.at(values, 0) do\n      some: {:some, {i32, i32}} -> match some do\n        {:some, {key, value}} -> key == 1 and value == 50\n      end\n      _ -> false\n    end\n  end\n  def main() -> i32 do\n    list: [string] = [\"a\", \"bb\"]\n    array: [string; 2] = #[\"a\", \"bb\"]\n    slice = Slice.from_array(array)\n    data = String.bytes(\"AB\")\n    map: Map(i32, i32) = %{1 => 50, 2 => 60}\n    list_copy = Enum.to_list(list)\n    array_list = Enum.to_list(array)\n    slice_list = Enum.to_list(slice)\n    byte_list = Enum.to_list(data)\n    map_list = Enum.to_list(map)\n    Rune.to_string('🙂')\n    if Enum.count(list) == 2 and Enum.count(array) == 2 and Enum.count(slice) == 2 and Enum.count(data) == 2 and Enum.count(map) == 2 and list_at(list) and array_missing(array) and slice_at(slice) and byte_at(data) and map_at(map) and list_copy == [\"a\", \"bb\"] and array_list == [\"a\", \"bb\"] and slice_list == [\"a\", \"bb\"] and byte_list == [65, 66] and map_list == [{1, 50}, {2, 60}] do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(&temp.0, &format!("{label}-enum-traversal"), source, profile,)
                .code(),
            Some(42),
            "Enum traversal must preserve order, absence, and managed items in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn enum_each_any_and_all_short_circuit_and_root_managed_items() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def retain(value: string) -> unit do\n    Rune.to_string('🙂')\n    String.byte_size(value)\n    unit\n  end\n  def any_first(value: i32) -> bool do\n    if value == 1 do\n      true\n    else\n      1 / 0 == 0\n    end\n  end\n  def all_first_false(value: i32) -> bool do\n    if value == 0 do\n      false\n    else\n      1 / 0 == 0\n    end\n  end\n  def byte_a(value: u8) -> bool do\n    if value == 65 do\n      true\n    else\n      1 / 0 == 0\n    end\n  end\n  def pair_positive(value: {i32, string}) -> bool do\n    match value do\n      {key, text} -> key > 0 and String.byte_size(text) > 0\n    end\n  end\n  def main() -> i32 do\n    list: [string] = [\"a\", \"bb\"]\n    array: [i32; 2] = #[1, 2]\n    false_first: [i32; 2] = #[0, 1]\n    slice = Slice.from_array(false_first)\n    data = String.bytes(\"AB\")\n    map: Map(i32, string) = %{1 => \"one\", 2 => \"two\"}\n    empty: [i32] = []\n    Enum.each(list, retain)\n    if Enum.any(array, any_first) and Enum.all(slice, all_first_false) == false and Enum.any(data, byte_a) and Enum.all(map, pair_positive) and Enum.any(empty, any_first) == false and Enum.all(empty, any_first) do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(&temp.0, &format!("{label}-enum-visits"), source, profile).code(),
            Some(42),
            "Enum visitors must preserve roots, empty identities, and short-circuit in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn enum_reduce_is_strict_left_to_right_and_roots_the_accumulator() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def ordered(total: i32, value: i32) -> i32 do\n    if total == 0 and value == 1 do\n      1\n    else\n      if total == 1 and value == 2 do\n        42\n      else\n        1 / 0\n      end\n    end\n  end\n  def ordered_byte(total: i32, value: u8) -> i32 do\n    if total == 0 and value == 65 do\n      1\n    else\n      if total == 1 and value == 66 do\n        42\n      else\n        1 / 0\n      end\n    end\n  end\n  def ordered_pair(total: i32, value: {i32, string}) -> i32 do\n    match value do\n      {key, _} -> ordered(total, key)\n    end\n  end\n  def last(previous: string, value: string) -> string do\n    Rune.to_string('🙂')\n    value\n  end\n  def main() -> i32 do\n    strings: [string] = [\"a\", \"bb\"]\n    array: [i32; 2] = #[1, 2]\n    slice = Slice.from_array(array)\n    data = String.bytes(\"AB\")\n    map: Map(i32, string) = %{1 => \"one\", 2 => \"two\"}\n    empty: [i32] = []\n    final = Enum.reduce(strings, \"initial\", last)\n    if final == \"bb\" and Enum.reduce(array, 0 :: i32, ordered) == 42 and Enum.reduce(slice, 0 :: i32, ordered) == 42 and Enum.reduce(data, 0 :: i32, ordered_byte) == 42 and Enum.reduce(map, 0 :: i32, ordered_pair) == 42 and Enum.reduce(empty, 42 :: i32, ordered) == 42 do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(&temp.0, &format!("{label}-enum-reduce"), source, profile).code(),
            Some(42),
            "Enum.reduce must preserve order and managed accumulators in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn enum_filter_preserves_order_and_managed_items() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def keep_text(value: string) -> bool do\n    Rune.to_string('🙂')\n    String.byte_size(value) > 1\n  end\n  def positive(value: i32) -> bool do\n    value > 0\n  end\n  def byte_a(value: u8) -> bool do\n    value == 65\n  end\n  def positive_pair(value: {i32, string}) -> bool do\n    match value do\n      {key, _} -> key > 0\n    end\n  end\n  def main() -> i32 do\n    list: [string] = [\"a\", \"bb\", \"ccc\"]\n    array: [i32; 3] = #[1, 0, 2]\n    slice = Slice.from_array(array)\n    data = String.bytes(\"ABA\")\n    map: Map(i32, string) = %{1 => \"one\", 0 => \"zero\", 2 => \"two\"}\n    if Enum.filter(list, keep_text) == [\"bb\", \"ccc\"] and Enum.filter(array, positive) == [1, 2] and Enum.filter(slice, positive) == [1, 2] and Enum.filter(data, byte_a) == [65, 65] and Enum.filter(map, positive_pair) == [{1, \"one\"}, {2, \"two\"}] do\n      42\n    else\n      0\n    end\n  end\nend\n";
    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(&temp.0, &format!("{label}-enum-filter"), source, profile).code(),
            Some(42)
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn enum_map_preserves_order_and_roots_managed_results() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def keep_text(value: string) -> string do\n    Rune.to_string('🙂')\n    value\n  end\n  def positive(value: i32) -> bool do\n    value > 0\n  end\n  def byte_a(value: u8) -> bool do\n    value == 65\n  end\n  def pair_key(value: {i32, string}) -> i32 do\n    match value do\n      {key, _} -> key\n    end\n  end\n  def main() -> i32 do\n    list: [string] = [\"a\", \"bb\", \"ccc\"]\n    array: [i32; 3] = #[1, 0, 2]\n    slice = Slice.from_array(array)\n    data = String.bytes(\"ABA\")\n    map: Map(i32, string) = %{1 => \"one\", 0 => \"zero\", 2 => \"two\"}\n    empty: [i32] = []\n    if Enum.map(list, keep_text) == [\"a\", \"bb\", \"ccc\"] and Enum.map(array, positive) == [true, false, true] and Enum.map(slice, positive) == [true, false, true] and Enum.map(data, byte_a) == [true, false, true] and Enum.map(map, pair_key) == [1, 0, 2] and Enum.map(empty, positive) == [] do\n      42\n    else\n      0\n    end\n  end\nend\n";
    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(&temp.0, &format!("{label}-enum-map"), source, profile).code(),
            Some(42)
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn managed_lists_survive_every_allocation_collection_in_all_root_positions() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  @type Held = [i32] | :none\n  def build(count: i32, tail: [i32]) -> [i32] do\n    if count == 0 do\n      tail\n    else\n      build(count - 1, [count | tail])\n    end\n  end\n  def length(values: [i32], count: i32) -> i32 do\n    match values do\n      [] -> count\n      [_ | tail] -> length(tail, count + 1)\n    end\n  end\n  def observe(values: [i32]) -> unit do\n    match values do\n      [] -> unit\n      [_ | _] -> unit\n    end\n  end\n  def preserve(values: [i32]) -> [i32] do\n    mut held: [i32] = values\n    defer do\n      observe(held)\n    end\n    if true do\n      defer observe(held)\n      held\n    else\n      []\n    end\n  end\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      temporary: [i32] = [1, 2, 3, 4, 5, 6, 7, 8]\n      observe(temporary)\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    graph: [i32] = preserve(build(42, []))\n    held: Held = graph\n    pressure(512)\n    match held do\n      values: [i32] -> length(values, 0)\n      _ -> 0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-managed-list-stress"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "reachable list graph must survive collection at every allocation in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn list_reverse_preserves_nested_managed_items_under_gc_stress() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def head(values: [i32]) -> i32 do\n    match values do\n      [value | _] -> value\n      [] -> 0\n    end\n  end\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      temporary: [i32] = [1, 2, 3, 4]\n      head(temporary)\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    values: [[i32]] = [[42], [1]]\n    reversed = List.reverse(values)\n    pressure(256)\n    original = match values do\n      [first | _] -> head(first)\n      [] -> 0\n    end\n    answer = match reversed do\n      [_ | tail] -> match tail do\n        [item | _] -> head(item)\n        [] -> 0\n      end\n      [] -> 0\n    end\n    if original == 42 do\n      answer\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-list-reverse-stress"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "List.reverse must preserve nested managed items and order in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn bytes_list_conversions_are_fresh_and_survive_gc_stress() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def head(values: [u8]) -> u8 do\n    match values do\n      [value | _] -> value\n      [] -> 0\n    end\n  end\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      temporary: [u8] = Bytes.to_list(Bytes.from_list([1, 2, 3, 4]))\n      head(temporary)\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    original: [u8] = [42, 127, 255]\n    data = Bytes.from_list(original)\n    copy = Bytes.to_list(data)\n    empty = Bytes.from_list([])\n    empty_list: [u8] = Bytes.to_list(empty)\n    pressure(256)\n    if original == copy and data[0] == 42 and data[2] == 255 and Bytes.byte_size(empty) == 0 and empty_list == [] do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-bytes-list-conversion-stress"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "byte/list conversions preserve values and roots in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn rune_to_string_encodes_every_utf8_width_under_gc_stress() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      String.byte_size(Rune.to_string('🙂'))\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    ascii = Rune.to_string('A')\n    two = Rune.to_string('é')\n    three = Rune.to_string('€')\n    four = Rune.to_string('🙂')\n    rune_keys: Map(rune, i32) = %{'A' => 1, 'A' => 2}\n    pressure(256)\n    ascii_bytes = String.bytes(ascii)\n    two_bytes = String.bytes(two)\n    three_bytes = String.bytes(three)\n    four_bytes = String.bytes(four)\n    if String.byte_size(ascii) == 1 and ascii_bytes[0] == 65 and String.byte_size(two) == 2 and two_bytes[0] == 195 and two_bytes[1] == 169 and String.byte_size(three) == 3 and three_bytes[0] == 226 and three_bytes[1] == 130 and three_bytes[2] == 172 and String.byte_size(four) == 4 and four_bytes[0] == 240 and four_bytes[1] == 159 and four_bytes[2] == 153 and four_bytes[3] == 130 and Map.size(rune_keys) == 1 do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-rune-to-string-stress"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "Rune.to_string must encode exact UTF-8 and retain storage in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn string_codepoints_decode_eagerly_in_order_under_gc_stress() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      temporary = String.codepoints(\"Aé€🙂\")\n      temporary == ['A', 'e', '́', '€', '🙂']\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    codepoints = String.codepoints(\"Aé€🙂\")\n    empty = String.codepoints(\"\")\n    pressure(256)\n    if codepoints == ['A', 'e', '́', '€', '🙂'] and empty == [] do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-string-codepoints-stress"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "String.codepoints must preserve scalar order and roots in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn string_length_uses_unicode_17_extended_graphemes() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def main() -> i32 do\n    if String.length(\"\") == 0 and String.length(\"A\") == 1 and String.length(\"é\") == 1 and String.length(\"🇸🇬\") == 1 and String.length(\"👩‍👩‍👧‍👦\") == 1 and String.length(\"Aé🇸🇬👩‍👩‍👧‍👦\") == 4 do\n      42\n    else\n      0\n    end\n  end\nend\n";
    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-string-grapheme-length"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "String.length must use pinned grapheme boundaries in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn eager_graphemes_and_lazy_text_views_retain_source_under_gc_stress() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def is_smile(value: rune) -> bool do\n    value == '🙂'\n  end\n  def is_flag(value: string) -> bool do\n    value == \"🇸🇬\"\n  end\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      String.graphemes(\"Aé🇸🇬👩‍👩‍👧‍👦\")\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    graphemes = String.graphemes(\"Aé🇸🇬👩‍👩‍👧‍👦\")\n    codepoints = Enum.to_list(String.codepoint_view(\"Aé🙂\"))\n    lazy_graphemes = Enum.to_list(String.grapheme_view(\"Aé🇸🇬👩‍👩‍👧‍👦\"))\n    empty_codepoints = Enum.to_list(String.codepoint_view(\"\"))\n    empty_graphemes = Enum.to_list(String.grapheme_view(\"\"))\n    pressure(256)\n    if graphemes == [\"A\", \"é\", \"🇸🇬\", \"👩‍👩‍👧‍👦\"] and lazy_graphemes == graphemes and codepoints == ['A', 'e', '́', '🙂'] and empty_codepoints == [] and empty_graphemes == [] and Enum.count(String.codepoint_view(\"Aé🙂\")) == 4 and Enum.count(String.grapheme_view(\"Aé🇸🇬👩‍👩‍👧‍👦\")) == 4 and Enum.any(String.codepoint_view(\"A🙂\"), is_smile) and Enum.any(String.grapheme_view(\"é🇸🇬\"), is_flag) do\n      42\n    else\n      0\n    end\n  end\nend\n";
    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-string-lazy-views-stress"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "eager graphemes and lazy views must preserve boundaries and roots in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn lazy_text_views_retain_allocated_string_backing() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      Rune.to_string('A')\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    text = Rune.to_string('🙂')\n    codepoints = Enum.to_list(String.codepoint_view(text))\n    graphemes = Enum.to_list(String.grapheme_view(text))\n    pressure(256)\n    if codepoints == ['🙂'] and graphemes == [\"🙂\"] do\n      42\n    else\n      0\n    end\n  end\nend\n";
    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-allocated-string-lazy-views"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "lazy views must retain allocated string backing in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn string_from_bytes_validates_utf8_and_reports_first_invalid_offsets() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def valid_payload(value: {:ok, string}) -> string do\n    match value do\n      {:ok, text} -> text\n    end\n  end\n  def error_payload(value: {:error, String.Utf8Error}) -> usize do\n    match value do\n      {:error, reason} -> String.utf8_error_offset(reason)\n    end\n  end\n  def decode(data: bytes) -> string do\n    match String.from_bytes(data) do\n      value: {:ok, string} -> valid_payload(value)\n      _ -> \"\"\n    end\n  end\n  def error_offset(data: bytes) -> usize do\n    match String.from_bytes(data) do\n      value: {:error, String.Utf8Error} -> error_payload(value)\n      _ -> 999\n    end\n  end\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      String.from_bytes(Bytes.from_list([65, 195, 169]))\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    source = Bytes.from_list([101, 204, 129, 240, 159, 153, 130])\n    text = decode(source)\n    pressure(256)\n    if String.byte_size(text) == 7 and String.bytes(text)[0] == 101 and error_offset(Bytes.from_list([97, 128])) == 1 and error_offset(Bytes.from_list([97, 194])) == 1 and error_offset(Bytes.from_list([224, 128, 128])) == 0 and error_offset(Bytes.from_list([237, 160, 128])) == 0 and error_offset(Bytes.from_list([244, 144, 128, 128])) == 0 and error_offset(Bytes.from_list([240, 159, 65, 130])) == 0 do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-utf8-validation-stress"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "String.from_bytes must validate and report stable offsets in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn buffers_preserve_value_snapshots_and_match_string_utf8_validation() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def valid_payload(value: {:ok, string}) -> string do\n    match value do\n      {:ok, text} -> text\n    end\n  end\n  def error_payload(value: {:error, String.Utf8Error}) -> usize do\n    match value do\n      {:error, reason} -> String.utf8_error_offset(reason)\n    end\n  end\n  def buffer_text(buffer: Buffer) -> string do\n    match Buffer.to_string(buffer) do\n      value: {:ok, string} -> valid_payload(value)\n      _ -> \"\"\n    end\n  end\n  def buffer_error(buffer: Buffer) -> usize do\n    match Buffer.to_string(buffer) do\n      value: {:error, String.Utf8Error} -> error_payload(value)\n      _ -> 999\n    end\n  end\n  def bytes_error(data: bytes) -> usize do\n    match String.from_bytes(data) do\n      value: {:error, String.Utf8Error} -> error_payload(value)\n      _ -> 999\n    end\n  end\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      Buffer.to_bytes(Buffer.append_string(Buffer.new(), \"temporary🙂\"))\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    empty = Buffer.new()\n    original = Buffer.append_string(empty, \"hello\")\n    spaced = Buffer.append_byte(original, 32)\n    complete = Buffer.append_bytes(spaced, String.bytes(\"world\"))\n    snapshot = Buffer.to_bytes(complete)\n    later = Buffer.append_byte(complete, 33)\n    invalid_bytes = Bytes.from_list([97, 128])\n    invalid = Buffer.append_bytes(Buffer.new(), invalid_bytes)\n    text = buffer_text(complete)\n    pressure(256)\n    if Buffer.byte_size(empty) == 0 and Bytes.byte_size(Buffer.to_bytes(empty)) == 0 and String.byte_size(buffer_text(empty)) == 0 and Buffer.byte_size(original) == 5 and Buffer.byte_size(complete) == 11 and Buffer.byte_size(later) == 12 and Bytes.byte_size(snapshot) == 11 and snapshot[0] == 104 and snapshot[10] == 100 and String.byte_size(text) == 11 and String.bytes(text)[5] == 32 and buffer_error(invalid) == 1 and buffer_error(invalid) == bytes_error(invalid_bytes) do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-buffer-snapshot-stress"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "Buffer snapshots and UTF-8 validation must remain stable in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn arbitrary_bit_views_pack_msb_first_and_preserve_backing() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def some_payload(value: {:some, bytes}) -> bytes do\n    match value do\n      {:some, data} -> data\n    end\n  end\n  def aligned(value: bits) -> bytes do\n    match Bits.to_bytes(value) do\n      result: {:some, bytes} -> some_payload(result)\n      none: :none -> Bytes.from_list([])\n    end\n  end\n  def is_none(value: {:some, bytes} | :none) -> bool do\n    match value do\n      none: :none -> true\n      _ -> false\n    end\n  end\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      Bits.to_bytes(Bits.slice(Bytes.to_bits(Bytes.from_list([178, 108, 240])), 3, 16))\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    source = Bytes.from_list([178, 108, 240])\n    bits = Bytes.to_bits(source)\n    view = Bits.slice(bits, 3, 16)\n    packed = aligned(view)\n    original = aligned(bits)\n    empty = aligned(Bytes.to_bits(Bytes.from_list([])))\n    pressure(256)\n    if Bits.bit_size(bits) == 24 and Bits.bit_size(view) == 16 and bits[0] and bits[1] == false and view[0] and view[1] == false and Bytes.byte_size(packed) == 2 and packed[0] == 147 and packed[1] == 103 and original[0] == 178 and original[2] == 240 and Bytes.byte_size(empty) == 0 and is_none(Bits.to_bytes(Bits.slice(bits, 1, 15))) do\n      42\n    else\n      0\n    end\n  end\nend\n";
    let source = source
        .replace(
            "none: :none -> Bytes.from_list([])",
            "_ -> Bytes.from_list([])",
        )
        .replace(
            "none: :none -> true\n      _ -> false",
            "some: {:some, bytes} -> false\n      _ -> true",
        );
    let out_of_bounds = "defmodule Main do\n  def main() -> i32 do\n    bits = Bytes.to_bits(Bytes.from_list([128]))\n    bits[8]\n    0\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-arbitrary-bits-stress"),
                &source,
                profile,
            )
            .code(),
            Some(42),
            "bit views must index and repack MSB-first in {label}"
        );
        assert_ne!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-bits-out-of-bounds"),
                out_of_bounds,
                profile,
            )
            .code(),
            Some(0),
            "bit indexing must fail out of bounds in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn byte_aligned_bitstring_construction_preserves_order_and_checks_sizes() {
    let temp = TempDir::new();
    let mut expected = vec![
        0x12, 0x12, 0x34, 0x34, 0x12, 0x12, 0x34, 0x56, 0x12, 0x34, 0x56, 0x78, 0x01, 0x02, 0x03,
        0x04, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
        0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0xff, 0xfe,
    ];
    if cfg!(target_endian = "little") {
        expected.extend([0x02, 0x01]);
    } else {
        expected.extend([0x01, 0x02]);
    }
    expected.extend([0xaa, 0xbb]);
    let byte_checks = expected
        .iter()
        .enumerate()
        .map(|(index, byte)| format!("built[{index}] == {byte}"))
        .collect::<Vec<_>>()
        .join(" and ");
    let source = format!(
        "defmodule Main do\n  def packet(data: bytes) -> bytes do\n    <<0x12::unsigned-big-size(8), 0x1234::unsigned-big-size(16), 0x1234::unsigned-little-size(16), 0x123456::unsigned-big-size(24), 0x12345678::unsigned-big-size(32), 0x0102030405::unsigned-big-size(40), 0x010203040506::unsigned-big-size(48), 0x01020304050607::unsigned-big-size(56), 0x0102030405060708::unsigned-big-size(64), 0 - 2::signed-big-size(16), 0x0102::unsigned-native-size(16), data::bytes>>\n  end\n  def main() -> i32 do\n    built = packet(Bytes.from_list([170, 187]))\n    empty = <<>>\n    if Bytes.byte_size(built) == {} and {byte_checks} and Bytes.byte_size(empty) == 0 do\n      42\n    else\n      0\n    end\n  end\nend\n",
        expected.len()
    );
    let size_mismatch = "defmodule Main do\n  def build(data: bytes, size: usize) -> bytes do\n    <<data::bytes-size(size)>>\n  end\n  def main() -> i32 do\n    build(Bytes.from_list([1]), 2)\n    0\n  end\nend\n";
    let integer_mismatch = "defmodule Main do\n  def build(value: i32) -> bytes do\n    <<value::unsigned-big-size(8)>>\n  end\n  def main() -> i32 do\n    build(256)\n    0\n  end\nend\n";
    let signed_integer_mismatch = "defmodule Main do\n  def build(value: i32) -> bytes do\n    <<value::signed-big-size(16)>>\n  end\n  def main() -> i32 do\n    build(32768)\n    0\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-bitstring-construction"),
                &source,
                profile,
            )
            .code(),
            Some(42),
            "bitstring construction must preserve exact bytes in {label}"
        );
        for (case, failing_source) in [
            ("size", size_mismatch),
            ("integer", integer_mismatch),
            ("signed-integer", signed_integer_mismatch),
        ] {
            assert_eq!(
                build_and_run_managed(
                    &temp.0,
                    &format!("{label}-bitstring-{case}-mismatch"),
                    failing_source,
                    profile,
                )
                .code(),
                Some(1),
                "bitstring {case} mismatch must fail in {label}"
            );
        }
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn byte_aligned_bitstring_patterns_decode_capture_and_fail_normally() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def parse(packet: bytes, size: usize) -> i32 do\n    match packet do\n      <<18::unsigned-big-size(8), signed::signed-little-size(16), prefix::bytes-size(size), rest::bytes-size(Bytes.byte_size(prefix))>> ->\n        pressure = Bytes.to_list(rest)\n        if signed == 513 and prefix[0] == 170 and prefix[1] == 187 and rest[0] == 204 and rest[1] == 221 do\n          match pressure do\n            [204 | [221 | []]] -> 42\n            _ -> 1\n          end\n        else\n          2\n        end\n      _ -> 0\n    end\n  end\n  def tail_size(packet: bytes) -> usize do\n    match packet do\n      <<7::unsigned-big-size(8), tail::bytes>> -> Bytes.byte_size(tail)\n      _ -> 99\n    end\n  end\n  def is_empty(packet: bytes) -> i32 do\n    match packet do\n      <<>> -> 1\n      _ -> 0\n    end\n  end\n  def signed_negative(packet: bytes) -> i32 do\n    match packet do\n      <<-2::signed-big-size(16)>> -> 1\n      _ -> 0\n    end\n  end\n  def unsigned_max(packet: bytes) -> i32 do\n    match packet do\n      <<value::unsigned-big-size(64)>> -> if value == 0xffffffffffffffff do\n        1\n      else\n        0\n      end\n      _ -> 0\n    end\n  end\n  def native_roundtrip(packet: bytes) -> i32 do\n    match packet do\n      <<value::unsigned-native-size(16)>> -> if value == 0x1234 do\n        1\n      else\n        0\n      end\n      _ -> 0\n    end\n  end\n  def main() -> i32 do\n    valid = <<18::unsigned-big-size(8), 513::signed-little-size(16), Bytes.from_list([170, 187])::bytes, Bytes.from_list([204, 221])::bytes>>\n    literal_mismatch = <<19::unsigned-big-size(8), 513::signed-little-size(16), Bytes.from_list([170, 187, 204, 221])::bytes>>\n    short = Bytes.from_list([18, 1, 2, 170, 187, 204])\n    leftover = Bytes.from_list([18, 1, 2, 170, 187, 204, 221, 238])\n    negative = <<0 - 2::signed-big-size(16)>>\n    if parse(valid, 2) == 42 and parse(literal_mismatch, 2) == 0 and parse(short, 2) == 0 and parse(leftover, 2) == 0 and tail_size(Bytes.from_list([7, 1, 2, 3])) == 3 and is_empty(<<>>) == 1 and is_empty(Bytes.from_list([1])) == 0 and signed_negative(negative) == 1 and unsigned_max(Bytes.from_list([255, 255, 255, 255, 255, 255, 255, 255])) == 1 and native_roundtrip(<<0x1234::unsigned-native-size(16)>>) == 1 do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-bitstring-patterns"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "bitstring patterns must decode and fail normally in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn bitstring_patterns_decode_every_v1_integer_width_and_byte_order() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def decode(packet: bytes) -> i32 do\n    match packet do\n      <<a::unsigned-big-size(8), b::unsigned-big-size(24), c::unsigned-big-size(32), d::unsigned-big-size(40), e::unsigned-big-size(48), f::unsigned-big-size(56), g::unsigned-big-size(64), h::unsigned-little-size(24)>> -> if a == 0x12 and b == 0x123456 and c == 0x12345678 and d == 0x0102030405 and e == 0x010203040506 and f == 0x01020304050607 and g == 0x0102030405060708 and h == 0x123456 do\n        42\n      else\n        1\n      end\n      _ -> 0\n    end\n  end\n  def main() -> i32 do\n    decode(<<0x12::unsigned-big-size(8), 0x123456::unsigned-big-size(24), 0x12345678::unsigned-big-size(32), 0x0102030405::unsigned-big-size(40), 0x010203040506::unsigned-big-size(48), 0x01020304050607::unsigned-big-size(56), 0x0102030405060708::unsigned-big-size(64), 0x123456::unsigned-little-size(24)>>)\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-bitstring-pattern-widths"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "bitstring patterns must decode every width and byte order in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn managed_slices_retain_nested_backing_and_copy_in_both_profiles() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def head(values: [i32]) -> i32 do\n    match values do\n      [value | _] -> value\n      [] -> 0\n    end\n  end\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      temporary: [i32] = [1, 2, 3, 4]\n      head(temporary)\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    array: [[i32]; 2] = #[[1], [42]]\n    whole = Slice.from_array(array)\n    part = Slice.subslice(whole, 1, 1)\n    copy = Slice.copy(part)\n    pressure(256)\n    head(copy[0])\n  end\nend\n";
    let out_of_bounds = "defmodule Main do\n  def main() -> i32 do\n    values: [i32; 2] = #[1, 2]\n    Slice.subslice(Slice.from_array(values), 1, 2)[0]\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-managed-slice-stress"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "slice views and copies must retain nested managed elements in {label}"
        );
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-managed-slice-bounds"),
                out_of_bounds,
                profile,
            )
            .code(),
            Some(1),
            "managed runtime must terminate nonzero on subslice bounds failure in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn managed_map_literals_and_size_survive_gc_stress_in_both_profiles() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def pressure(count: i32) -> unit do\n    mut remaining: i32 = count\n    while remaining > 0 do\n      temporary: [i32] = [1, 2, 3, 4]\n      remaining := remaining - 1\n    end\n  end\n  def main() -> i32 do\n    values: Map(i32, [i32]) = %{1 => [1], 2 => [2], 1 => [42], 3 => [3]}\n    pressure(256)\n    if Map.size(values) == 3 do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-managed-map-size"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "managed map nodes and duplicate replacement must survive collection at every allocation in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn immutable_map_put_remove_and_reinsert_preserve_sizes_under_gc_stress() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def head(values: [i32]) -> i32 do\n    match values do\n      [value | _] -> value\n      [] -> 0\n    end\n  end\n  def some_value(value: {:some, [i32]}) -> i32 do\n    match value do\n      {:some, values} -> head(values)\n    end\n  end\n  def fetched(map: Map(i32, [i32]), key: i32) -> i32 do\n    match Map.fetch(map, key) do\n      some: {:some, [i32]} -> some_value(some)\n      _ -> 0\n    end\n  end\n  def main() -> i32 do\n    empty: Map(i32, [i32]) = Map.new()\n    from_empty = Map.put(empty, 7, [7])\n    original: Map(i32, [i32]) = %{1 => [1], 2 => [2], 3 => [3]}\n    replaced = Map.put(original, 2, [42])\n    appended = Map.put(replaced, 4, [4])\n    removed = Map.remove(appended, 2)\n    reinserted = Map.put(removed, 2, [22])\n    absent_removed = Map.remove(reinserted, 99)\n    if Map.size(empty) == 0 and Map.size(from_empty) == 1 and Map.size(original) == 3 and Map.size(replaced) == 3 and Map.size(appended) == 4 and Map.size(removed) == 3 and Map.size(reinserted) == 4 and Map.size(absent_removed) == 4 and fetched(from_empty, 7) == 7 and fetched(original, 2) == 2 and fetched(replaced, 2) == 42 and fetched(removed, 2) == 0 and fetched(reinserted, 2) == 22 do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-immutable-map-updates"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "map updates and fetches must preserve source values and cardinality in {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn managed_map_values_survive_each_api_operation() {
    let temp = TempDir::new();
    let prefix = "defmodule Main do\n  def head(values: [i32]) -> i32 do\n    match values do\n      [value | _] -> value\n      [] -> 0\n    end\n  end\n  def some_value(value: {:some, [i32]}) -> i32 do\n    match value do\n      {:some, values} -> head(values)\n    end\n  end\n  def fetched(map: Map(i32, [i32]), key: i32) -> i32 do\n    match Map.fetch(map, key) do\n      some: {:some, [i32]} -> some_value(some)\n      _ -> 0\n    end\n  end\n  def main() -> i32 do\n";
    let cases = [
        (
            "literal",
            "    map: Map(i32, [i32]) = %{1 => [42]}\n    fetched(map, 1)\n",
        ),
        (
            "put-empty",
            "    map: Map(i32, [i32]) = Map.put(Map.new(), 7, [42])\n    fetched(map, 7)\n",
        ),
        (
            "replace",
            "    map: Map(i32, [i32]) = Map.put(%{1 => [1], 2 => [2]}, 2, [42])\n    fetched(map, 2)\n",
        ),
        (
            "remove",
            "    map: Map(i32, [i32]) = Map.remove(%{1 => [1], 2 => [42]}, 1)\n    fetched(map, 2)\n",
        ),
        (
            "reinsert",
            "    map: Map(i32, [i32]) = Map.put(Map.remove(%{1 => [1], 2 => [2]}, 1), 1, [42])\n    fetched(map, 1)\n",
        ),
    ];
    for (label, body) in cases {
        let source = format!("{prefix}{body}  end\nend\n");
        assert_eq!(
            build_and_run_managed(&temp.0, label, &source, BuildProfile::Development).code(),
            Some(42),
            "managed map value failed after {label}"
        );
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn seeded_composite_maps_preserve_order_and_ignore_order_for_equality() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def ordered(values: [{{i32, i32}, i32}]) -> bool do\n    match values do\n      [{{2, 2}, 20} | tail] -> match tail do\n        [{{1, 1}, 13} | rest] -> match rest do\n          [] -> true\n          [_ | _] -> false\n        end\n        [_ | _] -> false\n        [] -> false\n      end\n      [_ | _] -> false\n      [] -> false\n    end\n  end\n  def main() -> i32 do\n    literal: Map({i32, i32}, i32) = %{{1, 1} => 10, {2, 2} => 20, {1, 1} => 11}\n    replaced = Map.put(literal, {1, 1}, 12)\n    reordered = Map.put(Map.remove(replaced, {1, 1}), {1, 1}, 13)\n    equal_different_order: Map({i32, i32}, i32) = %{{1, 1} => 13, {2, 2} => 20}\n    unequal_value: Map({i32, i32}, i32) = %{{1, 1} => 99, {2, 2} => 20}\n    if ordered(Enum.to_list(reordered)) and reordered == equal_different_order and reordered != unequal_value and Map.size(literal) == 2 do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        let executable = build_managed_executable(
            &temp.0,
            &format!("{label}-seeded-composite-map"),
            source,
            profile,
        );
        for seed in ["1", "2", "18446744073709551615"] {
            assert_eq!(
                Command::new(&executable)
                    .env("EL_MAP_HASH_SEED", seed)
                    .status()
                    .expect("run map conformance executable")
                    .code(),
                Some(42),
                "map order and equality must be stable for seed {seed} in {label}"
            );
        }
    }
}

#[cfg(feature = "gc-stress-test")]
#[test]
fn every_standard_composite_map_key_uses_structural_hash_and_equality() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def main() -> i32 do\n    list_keys: Map([i32], i32) = %{[1, 2] => 1, [1, 2] => 2}\n    array_keys: Map([i32; 2], i32) = %{#[1, 2] => 1, #[1, 2] => 2}\n    first_array: [i32; 2] = #[1, 2]\n    second_array: [i32; 2] = #[1, 2]\n    first_slice = Slice.from_array(first_array)\n    second_slice = Slice.from_array(second_array)\n    slice_keys: Map(Slice(i32), i32) = %{first_slice => 1, second_slice => 2}\n    first_bytes = String.bytes(\"same\")\n    second_bytes = String.bytes(\"same\")\n    byte_keys: Map(bytes, i32) = %{first_bytes => 1, second_bytes => 2}\n    string_keys: Map(string, i32) = %{\"same\" => 1, \"same\" => 2}\n    inner_left: Map(i32, i32) = %{1 => 10, 2 => 20}\n    inner_right: Map(i32, i32) = %{2 => 20, 1 => 10}\n    outer_left: Map(i32, Map(i32, i32)) = %{7 => inner_left}\n    outer_right: Map(i32, Map(i32, i32)) = %{7 => inner_right}\n    if Map.size(list_keys) == 1 and Map.size(array_keys) == 1 and Map.size(slice_keys) == 1 and Map.size(byte_keys) == 1 and Map.size(string_keys) == 1 and outer_left == outer_right do\n      42\n    else\n      0\n    end\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run_managed(
                &temp.0,
                &format!("{label}-all-composite-map-keys"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "all standard composite keys must use structural Eq/Hash in {label}"
        );
    }
}

#[cfg(feature = "allocation-failure-test")]
#[test]
fn managed_allocation_failure_reports_origin_and_skips_cleanup() {
    let temp = TempDir::new();
    let source = "defmodule Main do\n  def cleanup() -> unit do\n    1 / 0\n    unit\n  end\n  def main() -> i32 do\n    defer cleanup()\n    values: [i32] = [1]\n    0\n  end\nend\n";
    let mut sources = SourceMap::new();
    let file = sources.add_file("src/main.el", source);
    let generic = analyze_source(file, source).expect("source reaches Generic Core");
    let roots = executable_reachability_roots(&generic).expect("select executable entry");
    let concrete = monomorphize(&generic, &roots).expect("monomorphize executable");
    let object = temp.0.join("allocation-failure.o");
    let executable = temp.0.join(format!(
        "allocation-failure{}",
        std::env::consts::EXE_SUFFIX
    ));
    emit_host_object_with_profile(&concrete, &object, BuildProfile::Release.codegen_profile())
        .expect("emit allocation-failure fixture");
    link_host_managed_executable(&[object.as_path()], &executable)
        .expect("link allocation-failure fixture");

    let output = Command::new(executable)
        .output()
        .expect("run allocation-failure fixture");
    let literal_start = source.rfind("[1]").expect("list literal") as u64;
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success());
    assert_eq!(
        stderr,
        format!(
            "EL runtime failure 6 at file 0:{literal_start}..{}\n",
            literal_start + 3
        )
    );
    assert!(
        !stderr.contains("failure 2"),
        "allocation failure must terminate without running deferred cleanup"
    );
}

#[test]
fn development_and_release_preserve_arithmetic_exit_semantics() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let arithmetic = "defmodule Main do\n  def multiply(value: i32, factor: i32) -> i32 do\n    value * factor\n  end\n  def main() -> i32 do\n    multiply(6, 7)\n  end\nend\n";
    let overflow = "defmodule Main do\n  def main() -> i32 do\n    2147483647 + 1\n  end\nend\n";

    for (label, profile) in [
        ("debug", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-arithmetic"),
                arithmetic,
                profile,
            )
            .code(),
            Some(42)
        );
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-overflow"),
                overflow,
                profile,
            )
            .code(),
            Some(101),
            "integer_overflow must retain runtime category 1 in both profiles"
        );
    }
}

#[test]
fn struct_values_preserve_copy_and_reconstruction_semantics_natively() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let source = "defmodule Main do\n  defstruct Pair(a) do\n    first: a\n    second: i32\n  end\n  def main() -> i32 do\n    mut pair: Pair(i32) = %Pair{second: 1, first: 40}\n    copy = pair\n    pair.second := 2\n    copy.first + copy.second + pair.second - 1\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-struct"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "struct copies must remain unchanged after root reconstruction in {label}"
        );
    }
}

#[test]
fn fixed_array_indexing_is_checked_in_development_and_release() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let success = "defmodule Main do\n  def get(values: [i32; 3], index: usize) -> i32 do\n    values[index]\n  end\n  def main() -> i32 do\n    get(#[1, 42, 3], 1)\n  end\nend\n";
    let out_of_bounds = "defmodule Main do\n  def main() -> i32 do\n    values: [i32; 2] = #[1, 2]\n    values[2]\n  end\nend\n";

    for (label, profile) in [
        ("development", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-array-index"),
                success,
                profile,
            )
            .code(),
            Some(42),
            "fixed-array indexing must select the requested element in {label}"
        );
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-array-bounds"),
                out_of_bounds,
                profile,
            )
            .code(),
            Some(105),
            "index_out_of_bounds must retain runtime category 5 in {label}"
        );
    }
}

#[test]
fn native_control_flow_preserves_loops_short_circuiting_and_early_returns() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let factorial = "defmodule Main do\n  def main() -> i32 do\n    mut n: i32 = 5\n    mut result: i32 = 1\n    while n > 1 do\n      result := result * n\n      n := n - 1\n    end\n    if false and 1 / 0 == 0 do\n      return 1\n    end\n    if true or 1 / 0 == 0 do\n      return result\n    end\n    0\n  end\nend\n";

    for (label, profile) in [
        ("debug", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-control-flow"),
                factorial,
                profile,
            )
            .code(),
            Some(120),
            "short-circuited division must not run and nested return must preserve factorial"
        );
    }
}

#[test]
fn native_pipelines_insert_and_evaluate_their_input_first() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let succeeds = "defmodule Main do\n  def multiply(value: i32, factor: i32) -> i32 do\n    value * factor\n  end\n  def identity(value: a) -> a do\n    value\n  end\n  def main() -> i32 do\n    6 |> multiply(7) |> identity()\n  end\nend\n";
    let ordered_failure = "defmodule Main do\n  def fail_input() -> i32 do\n    2147483647 + 1\n  end\n  def combine(left: i32, right: i32) -> i32 do\n    left + right\n  end\n  def main() -> i32 do\n    fail_input() |> combine(1 / 0)\n  end\nend\n";

    for (label, profile) in [
        ("debug", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-pipeline"),
                succeeds,
                profile,
            )
            .code(),
            Some(42)
        );
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-pipeline-order"),
                ordered_failure,
                profile,
            )
            .code(),
            Some(101),
            "input overflow must occur before explicit-argument division by zero"
        );
    }
}

#[test]
fn native_defer_registration_preserves_call_timing_and_capture_snapshots() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let immediate_call = "defmodule Main do\n  def cleanup(value: i32) -> unit do\n    unit\n  end\n  def fail_input() -> i32 do\n    2147483647 + 1\n  end\n  def main() -> i32 do\n    defer cleanup(fail_input())\n    1 / 0\n  end\nend\n";
    let captured_block = "defmodule Main do\n  def numerator() -> i32 do\n    1\n  end\n  def denominator(value: i32) -> i32 do\n    value - 1\n  end\n  def use(value: i32) -> unit do\n    numerator() / denominator(value)\n    unit\n  end\n  def main() -> i32 do\n    mut value: i32 = 1\n    defer do\n      use(value)\n    end\n    value := 2\n    0\n  end\nend\n";

    for (label, profile) in [
        ("debug", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-defer-call-registration"),
                immediate_call,
                profile,
            )
            .code(),
            Some(101),
            "deferred call input overflow must run before the later division"
        );
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-defer-block-capture"),
                captured_block,
                profile,
            )
            .code(),
            Some(102),
            "deferred block must observe the captured value 1 rather than the later value 2"
        );
    }
}

#[test]
fn native_cleanup_cfg_preserves_results_returns_lifo_and_loop_scopes() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let saved_result = "defmodule Main do\n  def cleanup() -> unit do\n    unit\n  end\n  def main() -> i32 do\n    defer cleanup()\n    if true do\n      defer cleanup()\n      42\n    else\n      0\n    end\n  end\nend\n";
    let nested_return_lifo = "defmodule Main do\n  def maximum() -> i32 do\n    2147483647\n  end\n  def one() -> i32 do\n    1\n  end\n  def numerator() -> i32 do\n    1\n  end\n  def zero() -> i32 do\n    0\n  end\n  def overflow() -> unit do\n    maximum() + one()\n    unit\n  end\n  def divide_by_zero() -> unit do\n    numerator() / zero()\n    unit\n  end\n  def main() -> i32 do\n    defer divide_by_zero()\n    if true do\n      defer divide_by_zero()\n      defer overflow()\n      return 42\n    end\n    0\n  end\nend\n";
    let per_iteration = "defmodule Main do\n  def maximum() -> i32 do\n    2147483647\n  end\n  def one() -> i32 do\n    1\n  end\n  def numerator() -> i32 do\n    1\n  end\n  def zero() -> i32 do\n    0\n  end\n  def overflow() -> unit do\n    maximum() + one()\n    unit\n  end\n  def main() -> i32 do\n    mut count: i32 = 2\n    while count > 1 do\n      defer overflow()\n      count := count - 1\n    end\n    numerator() / zero()\n  end\nend\n";

    for (label, profile) in [
        ("debug", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-cleanup-saved-result"),
                saved_result,
                profile,
            )
            .code(),
            Some(42),
            "fallthrough cleanup must preserve nested block results"
        );
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-cleanup-return-lifo"),
                nested_return_lifo,
                profile,
            )
            .code(),
            Some(101),
            "the last inner action must run first on a nested return"
        );
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-cleanup-loop-scope"),
                per_iteration,
                profile,
            )
            .code(),
            Some(101),
            "loop-body cleanup must run before control reaches the next expression"
        );
    }
}

#[test]
fn native_unrecoverable_failure_bypasses_pending_cleanup() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let source = "defmodule Main do\n  def maximum() -> i32 do\n    2147483647\n  end\n  def one() -> i32 do\n    1\n  end\n  def numerator() -> i32 do\n    1\n  end\n  def zero() -> i32 do\n    0\n  end\n  def divide_by_zero() -> unit do\n    numerator() / zero()\n    unit\n  end\n  def main() -> i32 do\n    defer divide_by_zero()\n    maximum() + one()\n  end\nend\n";

    for (label, profile) in [
        ("debug", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-failure-skips-cleanup"),
                source,
                profile,
            )
            .code(),
            Some(101),
            "main overflow must terminate before pending division-by-zero cleanup"
        );
    }
}

#[test]
fn native_string_backed_tagged_result_parser_preserves_discriminants_and_payloads() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let tagged = "defmodule Main do\n  @type Parsed = {:ok, string} | :error\n  def parse(input: string, valid: bool) -> Parsed do\n    if valid do\n      {:ok, input}\n    else\n      :error\n    end\n  end\n  def payload(value: {:ok, string}) -> i32 do\n    match value do\n      {:ok, text} -> 2\n    end\n  end\n  def atom_value(value: :error) -> i32 do\n    match value do\n      :error -> 40\n    end\n  end\n  def unwrap(value: Parsed) -> i32 do\n    match value do\n      ok: {:ok, string} -> payload(ok)\n      _ -> atom_value(:error)\n    end\n  end\n  def main() -> i32 do\n    unwrap(parse(\"forty-two\", true)) + unwrap(parse(\"ignored\", false))\n  end\nend\n";

    for (label, profile) in [
        ("debug", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-tagged-parser"),
                tagged,
                profile,
            )
            .code(),
            Some(42),
            "string-backed tagged parser results must survive calls and exhaustive matches"
        );
    }
}

#[test]
fn native_exhaustive_i64_string_match_preserves_static_string_payloads() {
    let temp = TempDir::new();
    let runtime = compile_runtime_failure_stub(&temp.0);
    let source = "defmodule Main do\n  @type Scalar = i64 | string\n  def choose(text: bool) -> Scalar do\n    if text do\n      \"forty-two\"\n    else\n      42\n    end\n  end\n  def classify(value: Scalar) -> i32 do\n    match value do\n      number: i64 -> 40\n      text: string -> 2\n    end\n  end\n  def main() -> i32 do\n    classify(choose(false)) + classify(choose(true))\n  end\nend\n";

    for (label, profile) in [
        ("debug", BuildProfile::Development),
        ("release", BuildProfile::Release),
    ] {
        assert_eq!(
            build_and_run(
                &temp.0,
                &runtime,
                &format!("{label}-i64-string-union"),
                source,
                profile,
            )
            .code(),
            Some(42),
            "both exhaustive union alternatives must preserve their payload ABI"
        );
    }
}
