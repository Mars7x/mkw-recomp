#ifndef MKW_CPU_H
#define MKW_CPU_H
#include <stdint.h>
#include <stddef.h>
#define MKW_CPU_ABI_VERSION 5u
/* Internal transfer request: dispatcher routes c->pc without changing LR. */
#define MKW_EXIT_TAIL 8
#define MKW_ORDER_ACQUIRE_RCPC 1u
#define MKW_ORDER_ACQUIRE_RCSC 2u
#define MKW_ORDER_RELEASE 3u
typedef struct MkwCpu MkwCpu;
/* Data barriers: DMB=0, DSB=1, followed by architectural option bits. */
typedef int (*MkwBarrier)(void*,uint32_t,uint32_t);
/* FP operations add/sub/mul/div/compare/compare-signaling/fused-add = 0..6.
 * Output buffers commit only on success. Unsupported FPCR profiles refuse. */
typedef int (*MkwFloating)(uint32_t,uint32_t,uint32_t,uint32_t,uint64_t,uint64_t,uint64_t,uint64_t*,uint32_t*);
typedef int (*MkwOrderedMemory)(void*,uint32_t,int,uint64_t,uint64_t*,unsigned);
/* Atomic order: relaxed=0, acquire=1, release=2, acquire/release=3.
 * Operations: add,clear,xor,set,smax,smin,umax,umin,swap,cas = 0..9.
 * CAS input is expected; replacement is desired. Output is zero-extended old. */
typedef int (*MkwAtomicMemory)(void*,uint32_t,uint32_t,uint64_t,uint64_t,uint64_t,uint64_t*,unsigned);
typedef int (*MkwCall)(void*,MkwCpu*,uint64_t,uint64_t*);
typedef int (*MkwPAuth)(void*,uint32_t,uint32_t,uint64_t,uint64_t,uint64_t*);
/* Memory callbacks transfer 1/2/4/8/16/32 little-endian bytes; buffer has ceil(bytes/8) words after full validation.
 * Call callbacks share the budget and restore caller module_base. */
typedef int (*MkwMemory)(void*,int,uint64_t,uint64_t*,unsigned);
struct MkwCpu { uint64_t x[31], sp, pc, module_base; uint32_t n,z,c,v;
 MkwPAuth pauth; void *pac_context; MkwMemory memory; void *memory_context;
 uint64_t fault_address; int fault_status; MkwCall call; void *call_context; MkwOrderedMemory ordered_memory; uint64_t vregs[32][2]; MkwAtomicMemory atomic_memory; MkwBarrier barrier; uint32_t fpcr,fpsr; MkwFloating floating; };
int mkw_guest(MkwCpu *c, uint64_t budget);
int mkw_guest_budget(MkwCpu *c, uint64_t *budget);

#endif
