#!/usr/bin/env python3
"""Generate the private C grapheme tables from the pinned Unicode inputs."""

from __future__ import annotations

import hashlib
import pathlib
import re


ROOT = pathlib.Path(__file__).resolve().parents[1]
DATA = ROOT / "runtime" / "unicode" / "17.0.0"
OUTPUT = ROOT / "crates" / "el-runtime" / "src" / "native" / "unicode_grapheme_data.inc"
TEST_OUTPUT = ROOT / "crates" / "el-runtime" / "tests" / "unicode_grapheme_test_data.inc"

GCB_VALUES = {
    "CR": "EL_GCB_CR",
    "LF": "EL_GCB_LF",
    "Control": "EL_GCB_CONTROL",
    "Extend": "EL_GCB_EXTEND",
    "ZWJ": "EL_GCB_ZWJ",
    "Regional_Indicator": "EL_GCB_REGIONAL_INDICATOR",
    "Prepend": "EL_GCB_PREPEND",
    "SpacingMark": "EL_GCB_SPACING_MARK",
    "L": "EL_GCB_L",
    "V": "EL_GCB_V",
    "T": "EL_GCB_T",
    "LV": "EL_GCB_LV",
    "LVT": "EL_GCB_LVT",
}


def codepoint_range(text: str) -> tuple[int, int]:
    bounds = text.strip().split("..")
    start = int(bounds[0], 16)
    return start, int(bounds[-1], 16)


def merge(ranges: list[tuple[int, int, str]]) -> list[tuple[int, int, str]]:
    merged: list[tuple[int, int, str]] = []
    for start, end, value in sorted(ranges):
        if merged and merged[-1][1] + 1 == start and merged[-1][2] == value:
            previous = merged[-1]
            merged[-1] = (previous[0], end, value)
        else:
            merged.append((start, end, value))
    return merged


def read_versioned(name: str) -> str:
    text = (DATA / name).read_text(encoding="utf-8")
    header = "\n".join(text.splitlines()[:10])
    expected = "# Version: 17.0" if name == "emoji-data.txt" else "17.0.0"
    if expected not in header:
        raise ValueError(f"{name} is not Unicode 17.0.0 data")
    return text


def parse_gcb() -> list[tuple[int, int, str]]:
    ranges = []
    for line in read_versioned("GraphemeBreakProperty.txt").splitlines():
        match = re.match(r"^([0-9A-F.]+)\s*;\s*([A-Za-z_]+)\s*#", line)
        if match:
            ranges.append((*codepoint_range(match.group(1)), GCB_VALUES[match.group(2)]))
    return merge(ranges)


def parse_incb() -> list[tuple[int, int, str]]:
    ranges = []
    values = {
        "Consonant": "EL_INCB_CONSONANT",
        "Extend": "EL_INCB_EXTEND",
        "Linker": "EL_INCB_LINKER",
    }
    for line in read_versioned("DerivedCoreProperties.txt").splitlines():
        match = re.match(
            r"^([0-9A-F.]+)\s*;\s*InCB\s*;\s*(Consonant|Extend|Linker)\s*#",
            line,
        )
        if match:
            ranges.append((*codepoint_range(match.group(1)), values[match.group(2)]))
    return merge(ranges)


def parse_extended_pictographic() -> list[tuple[int, int, str]]:
    ranges = []
    for line in read_versioned("emoji-data.txt").splitlines():
        match = re.match(r"^([0-9A-F.]+)\s*;\s*Extended_Pictographic\s*#", line)
        if match:
            ranges.append((*codepoint_range(match.group(1)), "1"))
    return merge(ranges)


def checksum(name: str) -> str:
    return hashlib.sha256((DATA / name).read_bytes()).hexdigest()


def table(name: str, ranges: list[tuple[int, int, str]]) -> str:
    rows = "\n".join(
        f"  {{0x{start:06x}u, 0x{end:06x}u, {value}}},"
        for start, end, value in ranges
    )
    return (
        f"static const ElUnicodeRange {name}[] = {{\n{rows}\n}};\n"
        f"static const size_t {name}_count = sizeof({name}) / sizeof({name}[0]);\n"
    )


def conformance_data() -> str:
    all_bytes: list[int] = []
    all_boundaries: list[int] = []
    cases: list[tuple[int, int, int, int, int]] = []
    for line_number, line in enumerate(read_versioned("GraphemeBreakTest.txt").splitlines(), 1):
        body = line.split("#", 1)[0].strip()
        if not body:
            continue
        encoded = bytearray()
        boundaries: list[int] = []
        for token in body.split():
            if token == "÷":
                boundaries.append(len(encoded))
            elif token != "×":
                encoded.extend(chr(int(token, 16)).encode("utf-8"))
        byte_start = len(all_bytes)
        boundary_start = len(all_boundaries)
        all_bytes.extend(encoded)
        all_boundaries.extend(boundaries)
        cases.append((byte_start, len(encoded), boundary_start, len(boundaries), line_number))

    byte_rows = "\n".join(
        "  " + ", ".join(f"0x{value:02x}u" for value in all_bytes[index:index + 16]) + ","
        for index in range(0, len(all_bytes), 16)
    )
    boundary_rows = "\n".join(
        "  " + ", ".join(f"{value}u" for value in all_boundaries[index:index + 16]) + ","
        for index in range(0, len(all_boundaries), 16)
    )
    case_rows = "\n".join(
        f"  {{{byte_start}u, {byte_count}u, {boundary_start}u, {boundary_count}u, {line_number}u}},"
        for byte_start, byte_count, boundary_start, boundary_count, line_number in cases
    )
    return f"""/* Generated from Unicode 17.0.0 GraphemeBreakTest.txt. */
typedef struct {{
  uint32_t byte_start;
  uint32_t byte_count;
  uint32_t boundary_start;
  uint32_t boundary_count;
  uint32_t line_number;
}} ElGraphemeTestCase;

static const uint8_t el_grapheme_test_bytes[] = {{
{byte_rows}
}};
static const uint32_t el_grapheme_test_boundaries[] = {{
{boundary_rows}
}};
static const ElGraphemeTestCase el_grapheme_test_cases[] = {{
{case_rows}
}};
static const size_t el_grapheme_test_case_count =
    sizeof(el_grapheme_test_cases) / sizeof(el_grapheme_test_cases[0]);
"""


def main() -> None:
    inputs = [
        "GraphemeBreakProperty.txt",
        "DerivedCoreProperties.txt",
        "emoji-data.txt",
    ]
    header = "\n".join(f" * {name}: {checksum(name)}" for name in inputs)
    generated = f"""/* Generated by tools/generate_unicode_grapheme_tables.py.
 * Unicode version: 17.0.0
 * SHA-256 inputs:
{header}
 */

typedef struct {{
  uint32_t start;
  uint32_t end;
  uint8_t value;
}} ElUnicodeRange;

{table("el_gcb_ranges", parse_gcb())}
{table("el_incb_ranges", parse_incb())}
{table("el_extended_pictographic_ranges", parse_extended_pictographic())}
"""
    OUTPUT.write_text(generated, encoding="utf-8")
    TEST_OUTPUT.write_text(conformance_data(), encoding="utf-8")


if __name__ == "__main__":
    main()
