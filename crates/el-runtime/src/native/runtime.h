#ifndef EL_RUNTIME_H
#define EL_RUNTIME_H

#include <stddef.h>
#include <stdint.h>

#define EL_PRIVATE_ABI_VERSION 6u

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
void *__el_runtime_file_open(const uint8_t *path, size_t size, uint32_t mode,
                             void **error);
void *__el_runtime_file_close(void *stream);
uint8_t *__el_runtime_reader_read(void *stream, size_t maximum, size_t *size,
                                  uint32_t *eof, void **error);
void *__el_runtime_writer_write(void *stream, const uint8_t *data, size_t size);
void *__el_runtime_writer_flush(void *stream);
void *__el_runtime_stdin(void);
void *__el_runtime_stdout(void);
void *__el_runtime_stderr(void);
uint32_t __el_runtime_error_kind(const void *error);
uint32_t __el_runtime_error_operation(const void *error);
int64_t __el_runtime_error_code(const void *error, uint32_t *has_code);
void __el_runtime_process_snapshot(int argc, const char *const *argv);
void *__el_runtime_process_arguments(size_t *invalid_index);
uint32_t __el_runtime_process_get_env(const uint8_t *name, size_t name_size,
                                      uint8_t **value, size_t *value_size);
void __el_runtime_console_write(const uint8_t *data, size_t size,
                                uint32_t use_stderr, uint32_t newline);
void __el_runtime_console_error(const void *error, uint32_t use_stderr,
                                uint32_t newline);
void __el_runtime_integer_to_string(uint64_t value, uint32_t kind,
                                    uint8_t **data, size_t *size,
                                    uint32_t file, uint64_t start,
                                    uint64_t end);
uint32_t __el_runtime_string_contains(const uint8_t *data, size_t size,
                                      const uint8_t *pattern,
                                      size_t pattern_size);
void *__el_runtime_string_split(const uint8_t *data, size_t size,
                                const uint8_t *separator,
                                size_t separator_size, uint32_t file,
                                uint64_t start, uint64_t end);

#endif
