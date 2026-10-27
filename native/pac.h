#ifndef MKW_PAC_H
#define MKW_PAC_H
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Explicit diagnostic profile only: legacy PAuth, 39-bit VA, no TBI, QARMA5.
 * Not selected by the game runtime: T239 algorithm/feature profile unverified.
 * Operation: 0 sign, 1 authenticate, 2 strip, 3 authenticate-and-return.
 * Key: 0 IA, 1 IB. Keys use high/low architectural register ordering.
 * Status: 0 success, 3 unsupported, 4 authentication/branch fault. */
typedef struct {uint64_t high[2],low[2];} MkwPac39;
int mkw_pac39(void *context,uint32_t operation,uint32_t key,uint64_t pointer,uint64_t modifier,uint64_t *result);
uint64_t mkw_qarma5(uint64_t data,uint64_t modifier,uint64_t high,uint64_t low);
#ifdef __cplusplus
}
#endif
#endif
