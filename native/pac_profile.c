/* Additive metadata for the existing diagnostic helper; no target auto-selection. */
#include "pac.h"
#include "cpu.h"
#include <stddef.h>
uint32_t mkw_pac39_callback_abi(void) { return MKW_CPU_ABI_VERSION; }
/* Version 1: legacy PAuth, 39-bit VA, no TBI, QARMA5, IA/IB only. */
uint32_t mkw_pac39_profile(void) { return 1; }
uint32_t mkw_pac39_context_size(void) { return sizeof(MkwPac39); }
uint32_t mkw_pac39_low_offset(void) { return offsetof(MkwPac39, low); }
