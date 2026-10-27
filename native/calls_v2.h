#ifndef MKW_CALLS_V2_H
#define MKW_CALLS_V2_H
#include "exclusive_v10.h"
#define MKW_EXCLUSIVE_V2_VERSION 2u
/* CPU v10 layout is unchanged. Version/size explicitly distinguish the extended
 * table; legacy exclusive-v1 functions refuse this table before dereferencing.
 * ABI-v9 prefix call/call_context remain empty. */
typedef int (*MkwCallV10)(void *,MkwCpuV10 *,uint64_t,uint64_t *);
typedef struct MkwExclusiveV2 {
 MkwExclusiveV1 base;
 MkwCallV10 call;
 void *call_context;
} MkwExclusiveV2;
static inline int mkw_calls_v2_valid(const MkwCpuV10 *cpu) {
 if(!cpu||!cpu->exclusive||!cpu->exclusive_context)return 0;
 const MkwExclusiveV1 *ops=cpu->exclusive;
 if(ops->version!=MKW_EXCLUSIVE_V2_VERSION||ops->size!=sizeof(MkwExclusiveV2)||!ops->load||!ops->store||!ops->clear||!ops->memory||!ops->ordered_memory||!ops->atomic_memory||!ops->barrier)return 0;
 const MkwCpu *base=&cpu->base;
 const MkwExclusiveV2 *table=(const MkwExclusiveV2 *)ops;
 return table->call&&table->call_context&&base->memory==ops->memory&&base->ordered_memory==ops->ordered_memory&&base->atomic_memory==ops->atomic_memory&&base->barrier==ops->barrier&&base->memory_context==cpu->exclusive_context&&!base->call&&!base->call_context;
}
static inline int mkw_call_v10(MkwCpuV10 *cpu,uint64_t target,uint64_t *budget) {
 if(!mkw_calls_v2_valid(cpu))return 3;
 const MkwExclusiveV2 *table=(const MkwExclusiveV2 *)cpu->exclusive;
 return table->call(table->call_context,cpu,target,budget);
}
#endif
