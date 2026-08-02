#include <gc.h>
#include <errno.h>
#include <inttypes.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
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

enum ElIoKind {
  EL_IO_NOT_FOUND,
  EL_IO_PERMISSION_DENIED,
  EL_IO_ALREADY_EXISTS,
  EL_IO_INVALID_INPUT,
  EL_IO_IS_DIRECTORY,
  EL_IO_NOT_DIRECTORY,
  EL_IO_CLOSED,
  EL_IO_BROKEN_PIPE,
  EL_IO_OUT_OF_SPACE,
  EL_IO_OTHER,
};

enum ElIoOperation {
  EL_IO_OPEN_READ,
  EL_IO_CREATE,
  EL_IO_APPEND,
  EL_IO_READ,
  EL_IO_WRITE,
  EL_IO_FLUSH,
  EL_IO_CLOSE,
};

typedef struct {
  uint32_t operation;
  uint32_t kind;
  int64_t code;
  uint32_t has_code;
} ElIoError;

typedef struct {
  FILE *file;
  uint32_t closed;
  uint32_t owned;
  uint32_t readable;
  uint32_t writable;
} ElFileStream;

static ElFileStream el_stdin_stream;
static ElFileStream el_stdout_stream;
static ElFileStream el_stderr_stream;

typedef struct {
  uint8_t *data;
  size_t size;
} ElRuntimeString;

typedef struct ElRuntimeStringList {
  ElRuntimeString item;
  struct ElRuntimeStringList *next;
} ElRuntimeStringList;

static ElRuntimeString *el_process_arguments;
static size_t el_process_argument_count;
static ElRuntimeString *el_process_environment;
static size_t el_process_environment_count;

extern char **environ;

static ElRuntimeString el_snapshot_text(const char *source) {
  const size_t size = strlen(source);
  uint8_t *data = (uint8_t *)GC_malloc_atomic(size == 0 ? 1 : size);
  if (data == NULL) __el_runtime_fail(EL_FAILURE_ALLOCATION_EXHAUSTED, 0, 0, 0);
  if (size != 0) (void)memcpy(data, source, size);
  return (ElRuntimeString){data, size};
}

void __el_runtime_process_snapshot(int argc, const char *const *argv) {
  el_process_argument_count = argc > 1 ? (size_t)(argc - 1) : 0;
  if (el_process_argument_count != 0) {
    el_process_arguments = (ElRuntimeString *)GC_malloc(
        el_process_argument_count * sizeof(ElRuntimeString));
    if (el_process_arguments == NULL)
      __el_runtime_fail(EL_FAILURE_ALLOCATION_EXHAUSTED, 0, 0, 0);
    for (size_t index = 0; index < el_process_argument_count; index += 1)
      el_process_arguments[index] = el_snapshot_text(argv[index + 1]);
  }
  size_t count = 0;
  while (environ != NULL && environ[count] != NULL) count += 1;
  el_process_environment_count = count;
  if (count != 0) {
    el_process_environment =
        (ElRuntimeString *)GC_malloc(count * sizeof(ElRuntimeString));
    if (el_process_environment == NULL)
      __el_runtime_fail(EL_FAILURE_ALLOCATION_EXHAUSTED, 0, 0, 0);
    for (size_t index = 0; index < count; index += 1)
      el_process_environment[index] = el_snapshot_text(environ[index]);
  }
}

void *__el_runtime_process_arguments(size_t *invalid_index) {
  *invalid_index = SIZE_MAX;
  for (size_t index = 0; index < el_process_argument_count; index += 1) {
    const ElRuntimeString text = el_process_arguments[index];
    if (__el_runtime_utf8_validate(text.data, text.size) != text.size) {
      *invalid_index = index;
      return NULL;
    }
  }
  ElRuntimeStringList *list = NULL;
  for (size_t index = el_process_argument_count; index != 0; index -= 1) {
    ElRuntimeStringList *node =
        (ElRuntimeStringList *)GC_malloc(sizeof(ElRuntimeStringList));
    if (node == NULL) __el_runtime_fail(EL_FAILURE_ALLOCATION_EXHAUSTED, 0, 0, 0);
    node->item = el_process_arguments[index - 1];
    node->next = list;
    list = node;
  }
  return list;
}

uint32_t __el_runtime_process_get_env(const uint8_t *name, size_t name_size,
                                      uint8_t **value, size_t *value_size) {
  *value = NULL;
  *value_size = 0;
  if (memchr(name, 0, name_size) != NULL || memchr(name, '=', name_size) != NULL)
    return 2;
  for (size_t index = 0; index < el_process_environment_count; index += 1) {
    const ElRuntimeString entry = el_process_environment[index];
    const uint8_t *separator = (const uint8_t *)memchr(entry.data, '=', entry.size);
    if (separator == NULL) continue;
    const size_t entry_name_size = (size_t)(separator - entry.data);
    if (entry_name_size != name_size || memcmp(entry.data, name, name_size) != 0)
      continue;
    const uint8_t *data = separator + 1;
    const size_t size = entry.size - entry_name_size - 1;
    if (__el_runtime_utf8_validate(data, size) != size) return 3;
    *value = (uint8_t *)data;
    *value_size = size;
    return 0;
  }
  return 1;
}

static uint32_t el_io_kind(int code) {
  switch (code) {
    case ENOENT: return EL_IO_NOT_FOUND;
    case EACCES: return EL_IO_PERMISSION_DENIED;
    case EEXIST: return EL_IO_ALREADY_EXISTS;
    case EINVAL: return EL_IO_INVALID_INPUT;
#ifdef EISDIR
    case EISDIR: return EL_IO_IS_DIRECTORY;
#endif
#ifdef ENOTDIR
    case ENOTDIR: return EL_IO_NOT_DIRECTORY;
#endif
#ifdef EPIPE
    case EPIPE: return EL_IO_BROKEN_PIPE;
#endif
#ifdef ENOSPC
    case ENOSPC: return EL_IO_OUT_OF_SPACE;
#endif
    default: return EL_IO_OTHER;
  }
}

static void *el_io_error(uint32_t operation, uint32_t kind, int code,
                         int has_code) {
  ElIoError *error = (ElIoError *)GC_malloc_atomic(sizeof(ElIoError));
  if (error == NULL) __el_runtime_fail(EL_FAILURE_ALLOCATION_EXHAUSTED, 0, 0, 0);
  error->operation = operation;
  error->kind = kind;
  error->code = (int64_t)code;
  error->has_code = has_code != 0;
  return error;
}

static void *el_errno_error(uint32_t operation, int code) {
  return el_io_error(operation, el_io_kind(code), code, 1);
}

static void *el_closed_error(uint32_t operation) {
  return el_io_error(operation, EL_IO_CLOSED, 0, 0);
}

uint32_t __el_runtime_error_kind(const void *value) {
  return ((const ElIoError *)value)->kind;
}

uint32_t __el_runtime_error_operation(const void *value) {
  return ((const ElIoError *)value)->operation;
}

int64_t __el_runtime_error_code(const void *value, uint32_t *has_code) {
  const ElIoError *error = (const ElIoError *)value;
  *has_code = error->has_code;
  return error->code;
}

static void el_console_bytes(FILE *file, const uint8_t *data, size_t size) {
  size_t offset = 0;
  while (offset < size) {
    errno = 0;
    const size_t count = fwrite(data + offset, 1, size - offset, file);
    if (count != 0) {
      offset += count;
    } else if (ferror(file) && errno == EINTR) {
      clearerr(file);
    } else {
      __el_runtime_fail(7, 0, 0, 0);
    }
  }
}

void __el_runtime_console_write(const uint8_t *data, size_t size,
                                uint32_t use_stderr, uint32_t newline) {
  FILE *file = use_stderr != 0 ? stderr : stdout;
  el_console_bytes(file, data, size);
  if (newline != 0) el_console_bytes(file, (const uint8_t *)"\n", 1);
}

void __el_runtime_console_error(const void *value, uint32_t use_stderr,
                                uint32_t newline) {
  static const char *const operations[] = {
      "open_read", "create", "append", "read", "write", "flush", "close"};
  static const char *const kinds[] = {
      "not_found", "permission_denied", "already_exists", "invalid_input",
      "is_directory", "not_directory", "closed", "broken_pipe",
      "out_of_space", "other"};
  const ElIoError *error = (const ElIoError *)value;
  char text[160];
  const int length = error->has_code != 0
      ? snprintf(text, sizeof(text), "%s: %s (%lld)", operations[error->operation],
                 kinds[error->kind], (long long)error->code)
      : snprintf(text, sizeof(text), "%s: %s", operations[error->operation],
                 kinds[error->kind]);
  if (length < 0 || (size_t)length >= sizeof(text)) __el_runtime_fail(7, 0, 0, 0);
  __el_runtime_console_write((const uint8_t *)text, (size_t)length, use_stderr,
                             newline);
}

void __el_runtime_integer_to_string(uint64_t value, uint32_t kind,
                                    uint8_t **data, size_t *size,
                                    uint32_t file, uint64_t start,
                                    uint64_t end) {
  char temporary[32];
  const int length = kind == 2
      ? snprintf(temporary, sizeof(temporary), "%s", value != 0 ? "true" : "false")
      : kind == 1
          ? snprintf(temporary, sizeof(temporary), "%" PRId64, (int64_t)value)
          : snprintf(temporary, sizeof(temporary), "%" PRIu64, value);
  if (length < 0 || (size_t)length >= sizeof(temporary)) {
    __el_runtime_fail(EL_FAILURE_ALLOCATION_EXHAUSTED, file, start, end);
  }
  uint8_t *copy = __el_runtime_alloc_atomic((uint64_t)(length == 0 ? 1 : length),
                                            file, start, end);
  if (length > 0) memcpy(copy, temporary, (size_t)length);
  *data = copy;
  *size = (size_t)length;
}

static size_t el_string_find(const uint8_t *data, size_t size,
                             const uint8_t *pattern, size_t pattern_size,
                             size_t offset) {
  if (pattern_size == 0) return offset <= size ? offset : SIZE_MAX;
  if (offset > size || pattern_size > size - offset) return SIZE_MAX;
  const size_t last = size - pattern_size;
  for (size_t index = offset; index <= last; index += 1) {
    if (data[index] == pattern[0] &&
        memcmp(data + index, pattern, pattern_size) == 0) {
      return index;
    }
  }
  return SIZE_MAX;
}

uint32_t __el_runtime_string_contains(const uint8_t *data, size_t size,
                                      const uint8_t *pattern,
                                      size_t pattern_size) {
  return el_string_find(data, size, pattern, pattern_size, 0) != SIZE_MAX;
}

static void el_string_split_append(ElRuntimeStringList **head,
                                   ElRuntimeStringList **tail,
                                   const uint8_t *data, size_t size,
                                   uint32_t file, uint64_t start,
                                   uint64_t end) {
  uint8_t *copy = __el_runtime_alloc_atomic((uint64_t)(size == 0 ? 1 : size),
                                            file, start, end);
  if (size > 0) memcpy(copy, data, size);
  ElRuntimeStringList *node = __el_runtime_alloc_scanned(
      (uint64_t)sizeof(ElRuntimeStringList), file, start, end);
  node->item.data = copy;
  node->item.size = size;
  node->next = NULL;
  if (*tail == NULL) {
    *head = node;
  } else {
    (*tail)->next = node;
  }
  *tail = node;
}

void *__el_runtime_string_split(const uint8_t *data, size_t size,
                                const uint8_t *separator,
                                size_t separator_size, uint32_t file,
                                uint64_t start, uint64_t end) {
  ElRuntimeStringList *head = NULL;
  ElRuntimeStringList *tail = NULL;
  if (separator_size == 0) {
    el_string_split_append(&head, &tail, data, size, file, start, end);
    return head;
  }
  size_t offset = 0;
  for (;;) {
    const size_t found =
        el_string_find(data, size, separator, separator_size, offset);
    if (found == SIZE_MAX) {
      el_string_split_append(&head, &tail, data + offset, size - offset, file,
                             start, end);
      return head;
    }
    el_string_split_append(&head, &tail, data + offset, found - offset, file,
                           start, end);
    offset = found + separator_size;
  }
}

static ElFileStream *el_stream(FILE *file, int owned, int readable,
                               int writable) {
  ElFileStream *stream = (ElFileStream *)GC_malloc(sizeof(ElFileStream));
  if (stream == NULL) __el_runtime_fail(EL_FAILURE_ALLOCATION_EXHAUSTED, 0, 0, 0);
  stream->file = file;
  stream->closed = 0;
  stream->owned = owned != 0;
  stream->readable = readable != 0;
  stream->writable = writable != 0;
  return stream;
}

void *__el_runtime_file_open(const uint8_t *path, size_t size, uint32_t mode,
                             void **error) {
  *error = NULL;
  if (memchr(path, 0, size) != NULL) {
    const uint32_t operation = mode == 0 ? EL_IO_OPEN_READ :
                               mode == 1 ? EL_IO_CREATE : EL_IO_APPEND;
    *error = el_io_error(operation, EL_IO_INVALID_INPUT, 0, 0);
    return NULL;
  }
  char *native = (char *)GC_malloc_atomic(size + 1);
  if (native == NULL) __el_runtime_fail(EL_FAILURE_ALLOCATION_EXHAUSTED, 0, 0, 0);
  (void)memcpy(native, path, size);
  native[size] = '\0';
  const char *mode_text = mode == 0 ? "rb" : mode == 1 ? "wb" : "ab";
  FILE *file;
  do {
    errno = 0;
    file = fopen(native, mode_text);
  } while (file == NULL && errno == EINTR);
  if (file == NULL) {
    const uint32_t operation = mode == 0 ? EL_IO_OPEN_READ :
                               mode == 1 ? EL_IO_CREATE : EL_IO_APPEND;
    *error = el_errno_error(operation, errno);
    return NULL;
  }
  return el_stream(file, 1, mode == 0, mode != 0);
}

void *__el_runtime_file_close(void *value) {
  ElFileStream *stream = (ElFileStream *)value;
  if (stream == NULL || stream->closed) return el_closed_error(EL_IO_CLOSE);
  if (!stream->owned) return el_io_error(EL_IO_CLOSE, EL_IO_INVALID_INPUT, 0, 0);
  int result;
  do {
    errno = 0;
    result = fclose(stream->file);
  } while (result != 0 && errno == EINTR);
  stream->closed = 1;
  stream->file = NULL;
  return result == 0 ? NULL : el_errno_error(EL_IO_CLOSE, errno);
}

uint8_t *__el_runtime_reader_read(void *value, size_t maximum, size_t *size,
                                  uint32_t *eof, void **error) {
  ElFileStream *stream = (ElFileStream *)value;
  *size = 0;
  *eof = 0;
  *error = NULL;
  if (stream == NULL || stream->closed) {
    *error = el_closed_error(EL_IO_READ);
    return NULL;
  }
  if (!stream->readable) {
    *error = el_io_error(EL_IO_READ, EL_IO_INVALID_INPUT, 0, 0);
    return NULL;
  }
  const size_t allocation = maximum == 0 ? 1 : maximum;
  uint8_t *data = (uint8_t *)GC_malloc_atomic(allocation);
  if (data == NULL) __el_runtime_fail(EL_FAILURE_ALLOCATION_EXHAUSTED, 0, 0, 0);
  if (maximum == 0) return data;
  for (;;) {
    errno = 0;
    const size_t count = fread(data, 1, maximum, stream->file);
    if (count != 0) {
      *size = count;
      return data;
    }
    if (feof(stream->file)) {
      *eof = 1;
      return NULL;
    }
    if (ferror(stream->file) && errno == EINTR) {
      clearerr(stream->file);
      continue;
    }
    *error = el_errno_error(EL_IO_READ, errno);
    clearerr(stream->file);
    return NULL;
  }
}

void *__el_runtime_writer_write(void *value, const uint8_t *data, size_t size) {
  ElFileStream *stream = (ElFileStream *)value;
  if (stream == NULL || stream->closed) return el_closed_error(EL_IO_WRITE);
  if (!stream->writable) return el_io_error(EL_IO_WRITE, EL_IO_INVALID_INPUT, 0, 0);
  size_t offset = 0;
  while (offset < size) {
    errno = 0;
    const size_t count = fwrite(data + offset, 1, size - offset, stream->file);
    if (count != 0) {
      offset += count;
    } else if (ferror(stream->file) && errno == EINTR) {
      clearerr(stream->file);
    } else {
      clearerr(stream->file);
      return el_errno_error(EL_IO_WRITE, errno);
    }
  }
  return NULL;
}

void *__el_runtime_writer_flush(void *value) {
  ElFileStream *stream = (ElFileStream *)value;
  if (stream == NULL || stream->closed) return el_closed_error(EL_IO_FLUSH);
  if (!stream->writable) return el_io_error(EL_IO_FLUSH, EL_IO_INVALID_INPUT, 0, 0);
  int result;
  do {
    errno = 0;
    result = fflush(stream->file);
  } while (result != 0 && errno == EINTR);
  return result == 0 ? NULL : el_errno_error(EL_IO_FLUSH, errno);
}

void *__el_runtime_stdin(void) {
  el_stdin_stream = (ElFileStream){stdin, 0, 0, 1, 0};
  return &el_stdin_stream;
}

void *__el_runtime_stdout(void) {
  el_stdout_stream = (ElFileStream){stdout, 0, 0, 0, 1};
  return &el_stdout_stream;
}

void *__el_runtime_stderr(void) {
  el_stderr_stream = (ElFileStream){stderr, 0, 0, 0, 1};
  return &el_stderr_stream;
}
