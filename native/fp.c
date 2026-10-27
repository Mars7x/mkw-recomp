/* Original ABI adapter around the attributed BSD SoftFloat subset. */
#include "cpu.h"
#include "fp_environment.h"
static uint64_t exponent_mask(unsigned bytes){return bytes==4?UINT64_C(0x7f800000):UINT64_C(0x7ff0000000000000);}
static uint64_t fraction_mask(unsigned bytes){return bytes==4?UINT64_C(0x007fffff):UINT64_C(0x000fffffffffffff);}
static int nan_bits(uint64_t value,unsigned bytes){return (value&exponent_mask(bytes))==exponent_mask(bytes)&&(value&fraction_mask(bytes))!=0;}
unsigned mkw_fp_abi_version(void){return MKW_CPU_ABI_VERSION;}
/* Operations: add=0, sub=1, mul=2, div=3, quiet compare=4, signaling compare=5, fused multiply-add=6, negate=7, absolute=8, square-root=9.
 * Result is raw IEEE bits or NZCV nibble. Nonzero status leaves outputs intact.
 * Supported FPCR: rounding and default-NaN; traps/flush/alternative/merge refuse. */
int mkw_fp(uint32_t operation,uint32_t bytes,uint32_t fpcr,uint32_t fpsr,
           uint64_t left,uint64_t right,uint64_t accumulate,uint64_t *result,uint32_t *status){
    if((bytes!=4&&bytes!=8)||operation>9||(fpcr&~UINT32_C(0x02c00000)))return 3;
    if(bytes==4){left=(uint32_t)left;right=(uint32_t)right;accumulate=(uint32_t)accumulate;}
    MkwFpEnvironment saved=mkw_fp_begin(fpcr);
    uint64_t value=0;
    if(operation==7||operation==8){uint64_t sign=bytes==4?UINT64_C(0x80000000):UINT64_C(0x8000000000000000);value=operation==7?left^sign:left&~sign;
    }else if(operation==9){if(bytes==4){float32_t a={(uint32_t)left};value=f32_sqrt(a).v;}else{float64_t a={left};value=f64_sqrt(a).v;}
    }else if(operation==4||operation==5){
        uint64_t quiet=bytes==4?UINT64_C(0x00400000):UINT64_C(0x0008000000000000);
        int na=nan_bits(left,bytes),nb=nan_bits(right,bytes);
        if(na||nb){value=3;if(operation==5||(na&&!(left&quiet))||(nb&&!(right&quiet)))softfloat_exceptionFlags|=softfloat_flag_invalid;}
        else{
            uint64_t sign=bytes==4?UINT64_C(0x80000000):UINT64_C(0x8000000000000000);
            if(left==right||((left&~sign)==0&&(right&~sign)==0))value=6;
            else{int less;if((left^right)&sign)less=(left&sign)!=0;else less=(left&sign)?left>right:left<right;value=less?8:2;}
        }
    }else if(operation==6&&(nan_bits(left,bytes)||nan_bits(right,bytes)||nan_bits(accumulate,bytes))){
        /* A64 fused operations give the addend first NaN priority. The generic
         * SoftFloat binary propagation specialization alone does not express it. */
        uint64_t quiet=bytes==4?UINT64_C(0x00400000):UINT64_C(0x0008000000000000);
        uint64_t operands[3]={accumulate,left,right};int found=0;
        for(unsigned i=0;i<3;i++)if(nan_bits(operands[i],bytes)&&!(operands[i]&quiet)){value=operands[i]|quiet;softfloat_exceptionFlags|=softfloat_flag_invalid;found=1;break;}
        if(!found){
            uint64_t sign=bytes==4?UINT64_C(0x80000000):UINT64_C(0x8000000000000000);
            int inf_zero=(((left&~sign)==exponent_mask(bytes))&&!(right&~sign))||(((right&~sign)==exponent_mask(bytes))&&!(left&~sign));
            if(inf_zero){value=exponent_mask(bytes)|quiet;softfloat_exceptionFlags|=softfloat_flag_invalid;}
            else for(unsigned i=0;i<3;i++)if(nan_bits(operands[i],bytes)){value=operands[i];break;}
        }
    }else if(bytes==4){
        float32_t a={(uint32_t)left},b={(uint32_t)right},c={(uint32_t)accumulate},v;
        switch(operation){case 0:v=f32_add(a,b);break;case 1:v=f32_sub(a,b);break;case 2:v=f32_mul(a,b);break;case 3:v=f32_div(a,b);break;default:v=f32_mulAdd(a,b,c);break;}value=v.v;
    }else{
        float64_t a={left},b={right},c={accumulate},v;
        switch(operation){case 0:v=f64_add(a,b);break;case 1:v=f64_sub(a,b);break;case 2:v=f64_mul(a,b);break;case 3:v=f64_div(a,b);break;default:v=f64_mulAdd(a,b,c);break;}value=v.v;
    }
    if(operation!=4&&operation!=5&&operation!=7&&operation!=8&&(fpcr&UINT32_C(0x02000000))&&nan_bits(value,bytes))value=bytes==4?UINT64_C(0x7fc00000):UINT64_C(0x7ff8000000000000);
    uint32_t flags=mkw_fp_end(saved,fpsr);*result=value;*status=flags;return 0;
}
