#ifndef MKW_EXCLUSIVE_V10_H
#define MKW_EXCLUSIVE_V10_H
#include "cpu.h"
#include <stddef.h>
#define MKW_CPU_V10_ABI_VERSION 10u
#define MKW_EXCLUSIVE_V1_VERSION 1u
#define MKW_EXCLUSIVE_VALUE_WORDS 2u
/* V1 transfers aligned 1/2/4/8/16 bytes in a two-word little-endian value.
 * order is 0=plain, 1=acquire(load)/release(store); other values refuse.
 * Return 0 means callback completed; stored is then 1(success) or 0(failure).
 * On callback error, output buffers remain unchanged. Contexts must come
 * from the owned backend attachment and stay alive until it is detached.
 */
typedef int (*MkwExclusiveLoadV1)(void *,uint64_t,uint32_t,uint32_t,uint64_t *,uint32_t);
typedef int (*MkwExclusiveStoreV1)(void *,uint64_t,uint32_t,uint32_t,const uint64_t *,uint32_t,uint32_t *);
typedef int (*MkwExclusiveClearV1)(void *);
typedef struct MkwExclusiveV1 {
 uint32_t version,size;
 MkwExclusiveLoadV1 load;
 MkwExclusiveStoreV1 store;
 MkwExclusiveClearV1 clear;
 MkwMemory memory;
 MkwOrderedMemory ordered_memory;
 MkwAtomicMemory atomic_memory;
 MkwBarrier barrier;
} MkwExclusiveV1;
/* A distinct native frame. The complete v9 prefix/layout remains unchanged. */
typedef struct MkwCpuV10 {
 MkwCpu base;
 const MkwExclusiveV1 *exclusive;
 void *exclusive_context;
} MkwCpuV10;
unsigned mkw_guest_v10_budget_abi_version(void);
int mkw_guest_v10_budget(MkwCpuV10 *,uint64_t *);
/* Prefix and exclusive callbacks must use one authority. Unversioned call
 * dispatch remains refused until cross-module v10 routing is implemented. */
static inline int mkw_exclusive_v10_valid(const MkwCpuV10 *cpu) {
 if(!cpu||!cpu->exclusive||!cpu->exclusive_context)return 0;
 const MkwExclusiveV1 *ops=cpu->exclusive;
 if(ops->version!=MKW_EXCLUSIVE_V1_VERSION||ops->size!=sizeof(*ops)||!ops->load||!ops->store||!ops->clear||!ops->memory||!ops->ordered_memory||!ops->atomic_memory||!ops->barrier)return 0;
 const MkwCpu *base=&cpu->base;
 return base->memory==ops->memory&&base->ordered_memory==ops->ordered_memory&&base->atomic_memory==ops->atomic_memory&&base->barrier==ops->barrier&&base->memory_context==cpu->exclusive_context&&!base->call&&!base->call_context;
}
#endif
