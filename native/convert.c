/* Original integer/FP conversion adapter around the BSD SoftFloat subset. */
#include "cpu.h"
#include "fp_environment.h"
#include <string.h>
/* Exact binary64 multiplication by a positive power of two. Overflow here
 * only denotes an already out-of-range integer result; no FP multiply flags. */
static uint64_t scale_up_f64(uint64_t value,unsigned scale){
    uint64_t sign=value&UINT64_C(0x8000000000000000),fraction=value&UINT64_C(0x000fffffffffffff);
    unsigned exponent=(unsigned)((value>>52)&2047);
    if(exponent==2047||(!exponent&&!fraction))return value;
    if(!exponent){
        while(scale&&!(fraction&UINT64_C(0x0010000000000000))){fraction<<=1;scale--;}
        if(!(fraction&UINT64_C(0x0010000000000000)))return sign|fraction;
        fraction&=UINT64_C(0x000fffffffffffff);exponent=1;
    }
    exponent+=scale;
    if(exponent>=2047)return sign|UINT64_C(0x7ff0000000000000);
    return sign|((uint64_t)exponent<<52)|fraction;
}
/* 0/1: signed/unsigned integer -> FP, using FPCR rounding.
 * 2..11: FP -> signed/unsigned integer, rounding nearest-even,+inf,-inf,zero,ties-away.
 * Unsupported controls refuse without modifying outputs. */
int mkw_convert(uint32_t operation,uint32_t intbits,uint32_t bytes,uint32_t fpcr,uint32_t fpsr,uint64_t input,uint64_t*result,uint32_t*status){
    if((intbits!=32&&intbits!=64)||(bytes!=4&&bytes!=8)||(fpcr&~UINT32_C(0x02c00000)))return 3;
    if(operation>=MKW_CONVERT_FIXED_SCALE){
        unsigned base=operation&255,scale=(operation>>MKW_CONVERT_SCALE_SHIFT)&127;
        if(!scale||scale>intbits||(base!=0&&base!=1&&base!=8&&base!=9)||operation!=(MKW_CONVERT_FIXED_SCALE|(scale<<MKW_CONVERT_SCALE_SHIFT)|base))return 3;
        uint64_t value=0;uint32_t flags=fpsr;int rc;
        if(base<2){
            rc=mkw_convert(base,intbits,bytes,fpcr,fpsr,input,&value,&flags);if(rc)return rc;
            unsigned fraction_bits=bytes==4?23:52,exponent_mask=bytes==4?255:2047;
            unsigned exponent=(unsigned)((value>>fraction_bits)&exponent_mask);
            /* Nonzero integer/2^scale remains normal for all supported widths. */
            if(exponent)value=(value&~((uint64_t)exponent_mask<<fraction_bits))|((uint64_t)(exponent-scale)<<fraction_bits);
        }else{
            value=input;
            if(bytes==4){MkwFpEnvironment saved=mkw_fp_begin(fpcr);float32_t a={(uint32_t)input};value=f32_to_f64(a).v;flags=mkw_fp_end(saved,fpsr);}
            value=scale_up_f64(value,scale);
            rc=mkw_convert(base,intbits,8,fpcr,flags,value,&value,&flags);if(rc)return rc;
        }
        *result=value;*status=flags;return 0;
    }
    if(operation>11)return 3;
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
