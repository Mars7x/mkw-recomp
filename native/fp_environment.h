/* Original scoped adapter state for the attributed SoftFloat backend. */
#ifndef MKW_FP_ENVIRONMENT_H
#define MKW_FP_ENVIRONMENT_H
#include "softfloat.h"
typedef struct {uint_fast8_t rounding,tininess,flags;} MkwFpEnvironment;
static inline MkwFpEnvironment mkw_fp_begin(uint32_t fpcr){
    MkwFpEnvironment saved={softfloat_roundingMode,softfloat_detectTininess,softfloat_exceptionFlags};
    static const uint_fast8_t rounding[4]={softfloat_round_near_even,softfloat_round_max,softfloat_round_min,softfloat_round_minMag};
    softfloat_roundingMode=rounding[(fpcr>>22)&3];softfloat_detectTininess=softfloat_tininess_beforeRounding;softfloat_exceptionFlags=0;return saved;
}
static inline uint32_t mkw_fp_end(MkwFpEnvironment saved,uint32_t fpsr){
    unsigned f=softfloat_exceptionFlags;uint32_t raised=((f&16)>>4)|((f&8)>>2)|(f&4)|((f&2)<<2)|((f&1)<<4);
    softfloat_roundingMode=saved.rounding;softfloat_detectTininess=saved.tininess;softfloat_exceptionFlags=saved.flags;return fpsr|raised;
}
#endif
