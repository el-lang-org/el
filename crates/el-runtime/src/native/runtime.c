#include <gc.h>
#include <stdio.h>
#include <stdlib.h>
#include <time.h>

#include "runtime.h"

enum { EL_FAILURE_ALLOCATION_EXHAUSTED = 6 };

static int el_runtime_initialized;
static uintptr_t el_hash_seed;

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
