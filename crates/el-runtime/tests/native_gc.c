#include <stddef.h>
#include <stdint.h>

#include "../src/native/runtime.h"

typedef struct Node {
  struct Node *next;
  uint64_t value;
} Node;

static Node *managed_global;
static volatile unsigned char observed_byte;

extern void GC_gcollect(void);
extern size_t GC_get_heap_size(void);

static Node *prepend(Node *next, uint64_t value) {
  Node *node = __el_runtime_alloc_scanned(sizeof(Node), 0, 0, 0);
  node->next = next;
  node->value = value;
  return node;
}

static int verify_temporary_allocations_are_reclaimable(void) {
  size_t round;
  size_t warmed_heap = 0;

  for (round = 0; round < 6; ++round) {
    size_t allocation;
    for (allocation = 0; allocation < 2048; ++allocation) {
      unsigned char *bytes = __el_runtime_alloc_atomic(2048, 0, 0, 0);
      bytes[0] = (unsigned char)(allocation + round);
      observed_byte ^= bytes[0];
    }
    GC_gcollect();
    if (round == 1) warmed_heap = GC_get_heap_size();
    if (round > 1 && GC_get_heap_size() > warmed_heap + 8u * 1024u * 1024u) {
      return 20;
    }
  }
  return 0;
}

int main(void) {
  Node *root = NULL;
  size_t index;

  __el_runtime_init();
  __el_runtime_register_managed_globals(&managed_global, sizeof(managed_global));
  for (index = 0; index < 4096; ++index) {
    unsigned char *bytes;
    root = prepend(root, (uint64_t)index);
    bytes = __el_runtime_alloc_atomic(257, 0, 0, 0);
    bytes[index % 257] = (unsigned char)(index & 0xffu);
  }
  managed_global = root;

  for (index = 4096; index != 0; --index) {
    if (root == NULL || root->value != (uint64_t)(index - 1)) return 10;
    root = root->next;
  }
  if (root != NULL) return 11;

  root = managed_global;
  for (index = 0; index < 4096; ++index) {
    (void)__el_runtime_alloc_atomic(1024, 0, 0, 0);
  }
  if (root == NULL || root->value != 4095u) return 12;
  root = NULL;
  managed_global = NULL;
  return verify_temporary_allocations_are_reclaimable();
}
