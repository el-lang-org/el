#include <gc.h>
#include <stdio.h>
#include <stdlib.h>

#include "runtime.h"

enum { EL_FAILURE_ALLOCATION_EXHAUSTED = 6 };

static int el_runtime_initialized;

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
  el_runtime_initialized = 1;
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
