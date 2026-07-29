#include <gc.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#include "runtime.h"

enum { EL_FAILURE_ALLOCATION_EXHAUSTED = 6 };

static int el_runtime_initialized;
static uintptr_t el_hash_seed;

enum ElGraphemeBreak {
  EL_GCB_OTHER,
  EL_GCB_CR,
  EL_GCB_LF,
  EL_GCB_CONTROL,
  EL_GCB_EXTEND,
  EL_GCB_ZWJ,
  EL_GCB_REGIONAL_INDICATOR,
  EL_GCB_PREPEND,
  EL_GCB_SPACING_MARK,
  EL_GCB_L,
  EL_GCB_V,
  EL_GCB_T,
  EL_GCB_LV,
  EL_GCB_LVT,
};

enum ElIndicConjunctBreak {
  EL_INCB_NONE,
  EL_INCB_CONSONANT,
  EL_INCB_EXTEND,
  EL_INCB_LINKER,
};

#include "unicode_grapheme_data.inc"

_Noreturn void __el_runtime_fail(uint32_t category, uint32_t file,
                                 uint64_t start, uint64_t end) {
  (void)fprintf(stderr, "EL runtime failure %u at file %u:%llu..%llu\n", category,
                file, (unsigned long long)start, (unsigned long long)end);
  (void)fflush(stderr);
  _Exit(1);
}

void __el_runtime_init(void) {
  if (el_runtime_initialized) return;
  GC_INIT();
  const char *override = getenv("EL_MAP_HASH_SEED");
  if (override != NULL && override[0] != '\0') {
    el_hash_seed = (uintptr_t)strtoull(override, NULL, 0);
  } else {
    struct timespec now;
    (void)timespec_get(&now, TIME_UTC);
    el_hash_seed = (uintptr_t)&el_runtime_initialized ^
                   (uintptr_t)now.tv_sec ^ ((uintptr_t)now.tv_nsec << 1u);
  }
  if (el_hash_seed == 0) el_hash_seed = UINTPTR_MAX;
  el_runtime_initialized = 1;
}

uintptr_t __el_runtime_hash_seed(void) {
  if (!el_runtime_initialized) __el_runtime_init();
  return el_hash_seed;
}

static int el_utf8_continuation(uint8_t byte) {
  return byte >= 0x80u && byte <= 0xbfu;
}

size_t __el_runtime_utf8_validate(const uint8_t *data, size_t size) {
  size_t offset = 0;
  while (offset < size) {
    const uint8_t first = data[offset];
    if (first <= 0x7fu) {
      offset += 1;
    } else if (first >= 0xc2u && first <= 0xdfu) {
      if (size - offset < 2 || !el_utf8_continuation(data[offset + 1])) return offset;
      offset += 2;
    } else if (first == 0xe0u) {
      if (size - offset < 3 || data[offset + 1] < 0xa0u ||
          data[offset + 1] > 0xbfu || !el_utf8_continuation(data[offset + 2])) return offset;
      offset += 3;
    } else if ((first >= 0xe1u && first <= 0xecu) ||
               (first >= 0xeeu && first <= 0xefu)) {
      if (size - offset < 3 || !el_utf8_continuation(data[offset + 1]) ||
          !el_utf8_continuation(data[offset + 2])) return offset;
      offset += 3;
    } else if (first == 0xedu) {
      if (size - offset < 3 || data[offset + 1] < 0x80u ||
          data[offset + 1] > 0x9fu || !el_utf8_continuation(data[offset + 2])) return offset;
      offset += 3;
    } else if (first == 0xf0u) {
      if (size - offset < 4 || data[offset + 1] < 0x90u ||
          data[offset + 1] > 0xbfu || !el_utf8_continuation(data[offset + 2]) ||
          !el_utf8_continuation(data[offset + 3])) return offset;
      offset += 4;
    } else if (first >= 0xf1u && first <= 0xf3u) {
      if (size - offset < 4 || !el_utf8_continuation(data[offset + 1]) ||
          !el_utf8_continuation(data[offset + 2]) ||
          !el_utf8_continuation(data[offset + 3])) return offset;
      offset += 4;
    } else if (first == 0xf4u) {
      if (size - offset < 4 || data[offset + 1] < 0x80u ||
          data[offset + 1] > 0x8fu || !el_utf8_continuation(data[offset + 2]) ||
          !el_utf8_continuation(data[offset + 3])) return offset;
      offset += 4;
    } else {
      return offset;
    }
  }
  return size;
}

static uint8_t el_unicode_property(const ElUnicodeRange *ranges, size_t count,
                                   uint32_t codepoint, uint8_t fallback) {
  size_t low = 0;
  size_t high = count;
  while (low < high) {
    const size_t middle = low + (high - low) / 2;
    const ElUnicodeRange range = ranges[middle];
    if (codepoint < range.start) {
      high = middle;
    } else if (codepoint > range.end) {
      low = middle + 1;
    } else {
      return range.value;
    }
  }
  return fallback;
}

static uint32_t el_utf8_decode(const uint8_t *data, size_t *offset) {
  const uint8_t first = data[(*offset)++];
  if (first <= 0x7fu) return first;
  const uint8_t second = data[(*offset)++];
  if (first <= 0xdfu) {
    return ((uint32_t)(first & 0x1fu) << 6u) | (uint32_t)(second & 0x3fu);
  }
  const uint8_t third = data[(*offset)++];
  if (first <= 0xefu) {
    return ((uint32_t)(first & 0x0fu) << 12u) |
           ((uint32_t)(second & 0x3fu) << 6u) | (uint32_t)(third & 0x3fu);
  }
  const uint8_t fourth = data[(*offset)++];
  return ((uint32_t)(first & 0x07u) << 18u) |
         ((uint32_t)(second & 0x3fu) << 12u) |
         ((uint32_t)(third & 0x3fu) << 6u) | (uint32_t)(fourth & 0x3fu);
}

typedef struct {
  uint8_t previous_gcb;
  uint8_t incb_state;
  size_t regional_indicator_count;
  int extended_pictographic_sequence;
  int zwj_after_extended_pictographic;
} ElGraphemeState;

static uint8_t el_gcb(uint32_t codepoint) {
  return el_unicode_property(el_gcb_ranges, el_gcb_ranges_count, codepoint,
                             EL_GCB_OTHER);
}

static uint8_t el_incb(uint32_t codepoint) {
  return el_unicode_property(el_incb_ranges, el_incb_ranges_count, codepoint,
                             EL_INCB_NONE);
}

static int el_extended_pictographic(uint32_t codepoint) {
  return el_unicode_property(el_extended_pictographic_ranges,
                             el_extended_pictographic_ranges_count, codepoint,
                             0) != 0;
}

static void el_grapheme_state_accept(ElGraphemeState *state, uint32_t codepoint,
                                     uint8_t gcb) {
  const uint8_t incb = el_incb(codepoint);
  state->zwj_after_extended_pictographic =
      gcb == EL_GCB_ZWJ && state->extended_pictographic_sequence;
  if (el_extended_pictographic(codepoint)) {
    state->extended_pictographic_sequence = 1;
  } else if (gcb != EL_GCB_EXTEND) {
    state->extended_pictographic_sequence = 0;
  }

  if (incb == EL_INCB_CONSONANT) {
    state->incb_state = 1;
  } else if (state->incb_state != 0 && incb == EL_INCB_LINKER) {
    state->incb_state = 2;
  } else if (incb != EL_INCB_EXTEND && incb != EL_INCB_LINKER) {
    state->incb_state = 0;
  }

  if (gcb == EL_GCB_REGIONAL_INDICATOR) {
    state->regional_indicator_count += 1;
  } else {
    state->regional_indicator_count = 0;
  }
  state->previous_gcb = gcb;
}

static int el_grapheme_break(const ElGraphemeState *state,
                             uint32_t codepoint, uint8_t current) {
  const uint8_t previous = state->previous_gcb;
  if (previous == EL_GCB_CR && current == EL_GCB_LF) return 0; /* GB3 */
  if (previous == EL_GCB_CONTROL || previous == EL_GCB_CR ||
      previous == EL_GCB_LF)
    return 1; /* GB4 */
  if (current == EL_GCB_CONTROL || current == EL_GCB_CR ||
      current == EL_GCB_LF)
    return 1; /* GB5 */
  if (previous == EL_GCB_L &&
      (current == EL_GCB_L || current == EL_GCB_V ||
       current == EL_GCB_LV || current == EL_GCB_LVT))
    return 0; /* GB6 */
  if ((previous == EL_GCB_LV || previous == EL_GCB_V) &&
      (current == EL_GCB_V || current == EL_GCB_T))
    return 0; /* GB7 */
  if ((previous == EL_GCB_LVT || previous == EL_GCB_T) &&
      current == EL_GCB_T)
    return 0; /* GB8 */
  if (current == EL_GCB_EXTEND || current == EL_GCB_ZWJ) return 0; /* GB9 */
  if (current == EL_GCB_SPACING_MARK) return 0;                    /* GB9a */
  if (previous == EL_GCB_PREPEND) return 0;                        /* GB9b */
  if (el_incb(codepoint) == EL_INCB_CONSONANT && state->incb_state == 2)
    return 0; /* GB9c */
  if (el_extended_pictographic(codepoint) && previous == EL_GCB_ZWJ &&
      state->zwj_after_extended_pictographic)
    return 0; /* GB11 */
  if (previous == EL_GCB_REGIONAL_INDICATOR &&
      current == EL_GCB_REGIONAL_INDICATOR &&
      state->regional_indicator_count % 2u == 1u)
    return 0; /* GB12/GB13 */
  return 1;   /* GB999 */
}

size_t __el_runtime_grapheme_next(const uint8_t *data, size_t size,
                                  size_t offset) {
  if (offset >= size) return size;
  ElGraphemeState state = {0};
  uint32_t codepoint = el_utf8_decode(data, &offset);
  el_grapheme_state_accept(&state, codepoint, el_gcb(codepoint));
  while (offset < size) {
    const size_t current_offset = offset;
    codepoint = el_utf8_decode(data, &offset);
    const uint8_t current = el_gcb(codepoint);
    if (el_grapheme_break(&state, codepoint, current)) return current_offset;
    el_grapheme_state_accept(&state, codepoint, current);
  }
  return size;
}

size_t __el_runtime_grapheme_count(const uint8_t *data, size_t size) {
  size_t count = 0;
  size_t offset = 0;
  while (offset < size) {
    offset = __el_runtime_grapheme_next(data, size, offset);
    count += 1;
  }
  return count;
}

static void *el_allocate(uint64_t size, int atomic, uint32_t file, uint64_t start,
                         uint64_t end) {
  void *result;
  if (!el_runtime_initialized) __el_runtime_init();
  if (size > SIZE_MAX) {
    __el_runtime_fail(EL_FAILURE_ALLOCATION_EXHAUSTED, file, start, end);
  }
#ifdef EL_GC_STRESS_TEST
  GC_gcollect();
#endif
#ifdef EL_RUNTIME_ALLOCATION_FAILURE_TEST
  (void)atomic;
  result = NULL;
#else
  result = atomic ? GC_malloc_atomic((size_t)size) : GC_malloc((size_t)size);
#endif
  if (result == NULL) {
    __el_runtime_fail(EL_FAILURE_ALLOCATION_EXHAUSTED, file, start, end);
  }
  return result;
}

void *__el_runtime_alloc_scanned(uint64_t size, uint32_t file, uint64_t start,
                                 uint64_t end) {
  return el_allocate(size, 0, file, start, end);
}

void *__el_runtime_alloc_atomic(uint64_t size, uint32_t file, uint64_t start,
                                uint64_t end) {
  return el_allocate(size, 1, file, start, end);
}

void __el_runtime_register_managed_globals(void *start, size_t size) {
  if (!el_runtime_initialized) __el_runtime_init();
  if (size != 0) GC_add_roots(start, (char *)start + size);
}
