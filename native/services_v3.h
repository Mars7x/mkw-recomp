#ifndef MKW_SERVICES_V3_H
#define MKW_SERVICES_V3_H
#include "calls_v2.h"
#define MKW_SERVICES_V3_VERSION 3u
#define MKW_EXIT_THREAD 9
/* Unchanged CPU-v10 frame; explicit table-v3 adds a typed kernel boundary.
 * SVC is charged by generated code. Handled completion clears the local monitor.
 * ABI9 prefix calls remain empty. No Horizon service is enabled implicitly. */
typedef int (*MkwSvcV10)(void *,MkwCpuV10 *,uint32_t);
typedef struct MkwServicesV3 {
 MkwExclusiveV2 base;
 MkwSvcV10 svc;
 void *svc_context;
} MkwServicesV3;
static inline int mkw_services_v3_valid(const MkwCpuV10 *cpu) {
 if(!cpu||!cpu->exclusive||!cpu->exclusive_context)return 0;
 const MkwExclusiveV1 *ops=cpu->exclusive;
 if(ops->version!=MKW_SERVICES_V3_VERSION||ops->size!=sizeof(MkwServicesV3)||!ops->load||!ops->store||!ops->clear||!ops->memory||!ops->ordered_memory||!ops->atomic_memory||!ops->barrier)return 0;
 const MkwCpu *base=&cpu->base;
 const MkwServicesV3 *table=(const MkwServicesV3 *)ops;
 return table->base.call&&table->base.call_context&&table->svc&&table->svc_context==table->base.call_context&&base->memory==ops->memory&&base->ordered_memory==ops->ordered_memory&&base->atomic_memory==ops->atomic_memory&&base->barrier==ops->barrier&&base->memory_context==cpu->exclusive_context&&!base->call&&!base->call_context;
}
static inline int mkw_call_services_v10(MkwCpuV10 *cpu,uint64_t target,uint64_t *budget) {
 if(!mkw_services_v3_valid(cpu))return 3;
 const MkwServicesV3 *table=(const MkwServicesV3 *)cpu->exclusive;
 return table->base.call(table->base.call_context,cpu,target,budget);
}
static inline int mkw_svc_v10(MkwCpuV10 *cpu,uint32_t number) {
 if(number>65535||!mkw_services_v3_valid(cpu))return 3;
 const MkwServicesV3 *table=(const MkwServicesV3 *)cpu->exclusive;
 return table->svc(table->svc_context,cpu,number);
}
#endif
