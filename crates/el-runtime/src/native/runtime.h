#ifndef EL_RUNTIME_H
#define EL_RUNTIME_H

#include <stddef.h>
#include <stdint.h>

#define EL_PRIVATE_ABI_VERSION 1u

_Noreturn void __el_runtime_fail(uint32_t category, uint32_t file,
                                 uint64_t start, uint64_t end);
void __el_runtime_init(void);
void *__el_runtime_alloc_scanned(uint64_t size, uint32_t file, uint64_t start,
                                 uint64_t end);
void *__el_runtime_alloc_atomic(uint64_t size, uint32_t file, uint64_t start,
                                uint64_t end);
void __el_runtime_register_managed_globals(void *start, size_t size);

#endif
