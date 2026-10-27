/* Original integer/FP conversion adapter around the BSD SoftFloat subset. */
#include "cpu.h"
#include "fp_environment.h"
#include <string.h>
/* 0/1: signed/unsigned integer -> FP, using FPCR rounding.
 * 2..11: FP -> signed/unsigned integer, rounding nearest-even,+inf,-inf,zero,ties-away.
 * Unsupported controls refuse without modifying outputs. */
int mkw_convert(uint32_t operation,uint32_t intbits,uint32_t bytes,uint32_t fpcr,uint32_t fpsr,uint64_t input,uint64_t*result,uint32_t*status){
    if(operation>11||(intbits!=32&&intbits!=64)||(bytes!=4&&bytes!=8)||(fpcr&~UINT32_C(0x02c00000)))return 3;
    MkwFpEnvironment saved=mkw_fp_begin(fpcr);uint64_t value=0;
    if(operation<2){
        if(intbits==32){uint32_t raw=(uint32_t)input;int32_t signed_value;memcpy(&signed_value,&raw,sizeof raw);
            if(bytes==4)value=operation?ui32_to_f32(raw).v:i32_to_f32(signed_value).v;
            else value=operation?ui32_to_f64(raw).v:i32_to_f64(signed_value).v;
        }else{int64_t signed_value;memcpy(&signed_value,&input,sizeof input);
            if(bytes==4)value=operation?ui64_to_f32(input).v:i64_to_f32(signed_value).v;
            else value=operation?ui64_to_f64(input).v:i64_to_f64(signed_value).v;
        }
    }else{
        static const uint_fast8_t modes[5]={softfloat_round_near_even,softfloat_round_max,softfloat_round_min,softfloat_round_minMag,softfloat_round_near_maxMag};
        uint_fast8_t rounding=modes[(operation-2)/2];unsigned unsign=operation&1;
        if(bytes==4){float32_t a={(uint32_t)input};
            if(intbits==32)value=unsign?f32_to_ui32(a,rounding,true):(uint32_t)f32_to_i32(a,rounding,true);
            else value=unsign?f32_to_ui64(a,rounding,true):(uint64_t)f32_to_i64(a,rounding,true);
        }else{float64_t a={input};
            if(intbits==32)value=unsign?f64_to_ui32(a,rounding,true):(uint32_t)f64_to_i32(a,rounding,true);
            else value=unsign?f64_to_ui64(a,rounding,true):(uint64_t)f64_to_i64(a,rounding,true);
        }
    }
    uint32_t flags=mkw_fp_end(saved,fpsr);*result=value;*status=flags;return 0;
}
