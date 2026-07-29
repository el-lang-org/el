#ifndef EL_RUNTIME_H
#define EL_RUNTIME_H

#include <stddef.h>
#include <stdint.h>

#define EL_PRIVATE_ABI_VERSION 4u

_Noreturn void __el_runtime_fail(uint32_t category, uint32_t file,
                                 uint64_t start, uint64_t end);
void __el_runtime_init(void);
void *__el_runtime_alloc_scanned(uint64_t size, uint32_t file, uint64_t start,
                                 uint64_t end);
void *__el_runtime_alloc_atomic(uint64_t size, uint32_t file, uint64_t start,
                                uint64_t end);
void __el_runtime_register_managed_globals(void *start, size_t size);
uintptr_t __el_runtime_hash_seed(void);
size_t __el_runtime_utf8_validate(const uint8_t *data, size_t size);
size_t __el_runtime_grapheme_next(const uint8_t *data, size_t size,
                                  size_t offset);
size_t __el_runtime_grapheme_count(const uint8_t *data, size_t size);

#endif
