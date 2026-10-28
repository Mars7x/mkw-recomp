#ifndef MKW_GUARDED_V4_H
#define MKW_GUARDED_V4_H
/* Explicit guarded source contract. Runtime preflight and actual routed-target
 * checks are required; this header alone does not discharge import premises. */
#include "services_v3.h"
#define MKW_GUARDED_V4_VERSION 4u
#define MKW_ORIGIN_V4_BYTES 32u
typedef int (*MkwPreflightV4)(void *,MkwCpuV10 *,const uint8_t *,uint64_t);
typedef struct MkwGuardedV4 {
 MkwServicesV3 base;
 MkwPreflightV4 preflight;
 void *guard_context;
} MkwGuardedV4;
static inline int mkw_guarded_v4_valid(const MkwCpuV10 *cpu) {
 if(!cpu||!cpu->exclusive||!cpu->exclusive_context)return 0;
 const MkwExclusiveV1 *ops=cpu->exclusive;
 if(ops->version!=MKW_GUARDED_V4_VERSION||ops->size!=sizeof(MkwGuardedV4)||!ops->load||!ops->store||!ops->clear||!ops->memory||!ops->ordered_memory||!ops->atomic_memory||!ops->barrier)return 0;
 const MkwGuardedV4 *table=(const MkwGuardedV4 *)ops;
 const MkwCpu *base=&cpu->base;
 return table->base.base.call&&table->base.base.call_context&&table->base.svc&&table->preflight&&table->guard_context==table->base.svc_context&&table->guard_context==table->base.base.call_context&&base->memory==ops->memory&&base->ordered_memory==ops->ordered_memory&&base->atomic_memory==ops->atomic_memory&&base->barrier==ops->barrier&&base->memory_context==cpu->exclusive_context&&!base->call&&!base->call_context;
}
static inline int mkw_preflight_v4(MkwCpuV10 *cpu,const uint8_t *origin,uint64_t source_entry) {
 if(!origin||!mkw_guarded_v4_valid(cpu)||source_entry%4||source_entry>UINT64_MAX-cpu->base.module_base||cpu->base.pc!=cpu->base.module_base+source_entry)return 3;
 const MkwGuardedV4 *table=(const MkwGuardedV4 *)cpu->exclusive;
 return table->preflight(table->guard_context,cpu,origin,source_entry);
}
static inline int mkw_call_guarded_v4(MkwCpuV10 *cpu,uint64_t target,uint64_t *budget) {
 if(!budget||!mkw_guarded_v4_valid(cpu))return 3;
 const MkwGuardedV4 *table=(const MkwGuardedV4 *)cpu->exclusive;
 return table->base.base.call(table->guard_context,cpu,target,budget);
}
static inline int mkw_svc_guarded_v4(MkwCpuV10 *cpu,uint32_t number) {
 if(number>65535||!mkw_guarded_v4_valid(cpu))return 3;
 const MkwGuardedV4 *table=(const MkwGuardedV4 *)cpu->exclusive;
 return table->base.svc(table->guard_context,cpu,number);
}
#endif
