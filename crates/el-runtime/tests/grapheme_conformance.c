#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include "../src/native/runtime.h"
#include "unicode_grapheme_test_data.inc"

int main(void) {
  size_t case_index;
  if (el_grapheme_test_case_count <= 700u) return 1;
  for (case_index = 0; case_index < el_grapheme_test_case_count; ++case_index) {
    const ElGraphemeTestCase test = el_grapheme_test_cases[case_index];
    const uint8_t *bytes = el_grapheme_test_bytes + test.byte_start;
    size_t offset = 0;
    size_t boundary_index = 1;
    if (el_grapheme_test_boundaries[test.boundary_start] != 0u) return 2;
    while (offset < test.byte_count) {
      offset = __el_runtime_grapheme_next(bytes, test.byte_count, offset);
      if (boundary_index >= test.boundary_count ||
          offset != el_grapheme_test_boundaries[test.boundary_start + boundary_index]) {
        (void)fprintf(stderr, "boundary mismatch on GraphemeBreakTest line %u\n",
                      test.line_number);
        return 3;
      }
      boundary_index += 1;
    }
    if (boundary_index != test.boundary_count) return 4;
    if (__el_runtime_grapheme_count(bytes, test.byte_count) + 1u !=
        test.boundary_count) {
      (void)fprintf(stderr, "count mismatch on GraphemeBreakTest line %u\n",
                    test.line_number);
      return 5;
    }
  }
  return 0;
}
