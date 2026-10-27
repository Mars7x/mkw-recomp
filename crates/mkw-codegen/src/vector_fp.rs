use mkw_arm64::vector_fp::{VectorFp, VectorFpKind};
pub fn emit(op: VectorFp) -> String {
    if matches!(
        op.kind,
        VectorFpKind::ReciprocalSqrtEstimate | VectorFpKind::ReciprocalSqrtStep
    ) {
        return emit_rsqrt(op);
    }
    if op.kind == VectorFpKind::ReciprocalEstimate {
        return emit_estimate(op);
    }
    if let VectorFpKind::ReduceMinMax(operation) = op.kind {
        return format!("{{if(!c->floating){{c->fault_status=3;return 3;}}uint64_t source[2]={{c->vregs[{}][0],c->vregs[{}][1]}},pairs[2]={{0,0}},result=0;uint32_t fpsr=c->fpsr;for(unsigned pair=0;pair<2;pair++){{int status=c->floating({operation},4,c->fpcr,fpsr,source[pair]&UINT64_C(4294967295),source[pair]>>32,0,&pairs[pair],&fpsr);if(status){{c->fault_status=status;return status;}}}}int status=c->floating({operation},4,c->fpcr,fpsr,pairs[0],pairs[1],0,&result,&fpsr);if(status){{c->fault_status=status;return status;}}c->vregs[{}][0]=result&UINT64_C(4294967295);c->vregs[{}][1]=0;c->fpsr=fpsr;}}\n",op.lhs,op.lhs,op.dst,op.dst);
    }

    let mask = if op.bytes == 8 { u64::MAX } else { 0xffffffff };
    let lanes = op.total / op.bytes;
    let bits = op.bytes * 8;
    let (callback, operation) = match op.kind {
        VectorFpKind::Arithmetic(n) => ("floating", u32::from(n)),
        VectorFpKind::Conversion(n) => ("convert", u32::from(n)),
        VectorFpKind::ReduceMinMax(_)
        | VectorFpKind::ReciprocalEstimate
        | VectorFpKind::ReciprocalSqrtEstimate
        | VectorFpKind::ReciprocalSqrtStep => unreachable!(),
        VectorFpKind::PairwiseAdd => ("floating", 0),
        VectorFpKind::PairwiseMinMax(n) => ("floating", u32::from(n)),
        VectorFpKind::ScaledConversion {
            operation,
            fractional_bits,
        } => (
            "convert",
            u32::from(operation)
                | mkw_arm64::floating::CONVERT_FIXED_SCALE
                | (u32::from(fractional_bits) << mkw_arm64::floating::CONVERT_SCALE_SHIFT),
        ),
    };
    let mut out=format!("{{if(!c->{callback}){{c->fault_status=3;return 3;}}uint64_t left[2]={{c->vregs[{}][0],c->vregs[{}][1]}},right[2]={{c->vregs[{}][0],c->vregs[{}][1]}},old[2]={{c->vregs[{}][0],c->vregs[{}][1]}},output[2]={{0,0}};uint32_t fpsr=c->fpsr;(void)right;(void)old;\n",op.lhs,op.lhs,op.rhs,op.rhs,op.dst,op.dst);
    out+=&format!("for(unsigned lane=0;lane<{lanes};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t result=0;\n");
    if matches!(
        op.kind,
        VectorFpKind::PairwiseAdd | VectorFpKind::PairwiseMinMax(_)
    ) {
        if lanes == 1 {
            out += "const uint64_t *source=left;unsigned pair=0;";
        } else {
            out += &format!(
                "const uint64_t *source=lane<{}?left:right;unsigned pair=(lane%{})*{};",
                lanes / 2,
                lanes / 2,
                bits * 2
            );
        }
        out+=&format!("uint64_t a=(source[pair/64]>>(pair%64))&UINT64_C({mask}),b=(source[(pair+{bits})/64]>>((pair+{bits})%64))&UINT64_C({mask});int status=c->floating({operation},{},c->fpcr,fpsr,a,b,0,&result,&fpsr);\n",op.bytes);
    } else {
        out += &format!("uint64_t a=(left[half]>>shift)&UINT64_C({mask});\n");
        let right_lane = if let Some(lane) = op.rhs_lane {
            let offset = u32::from(lane) * u32::from(bits);
            format!("(right[{}]>>{})", offset / 64, offset % 64)
        } else {
            "(right[half]>>shift)".into()
        };
        match op.kind {
        VectorFpKind::Conversion(_)|VectorFpKind::ScaledConversion{..}=>out+=&format!("int status=c->convert({operation},{bits},{},c->fpcr,fpsr,a,&result,&fpsr);\n",op.bytes),
        VectorFpKind::PairwiseAdd|VectorFpKind::PairwiseMinMax(_)|VectorFpKind::ReduceMinMax(_)|VectorFpKind::ReciprocalEstimate|VectorFpKind::ReciprocalSqrtEstimate|VectorFpKind::ReciprocalSqrtStep=>unreachable!(),
        VectorFpKind::Arithmetic(_)=>out+=&format!("uint64_t b={right_lane}&UINT64_C({mask}),acc=(old[half]>>shift)&UINT64_C({mask});int status=c->floating({operation},{},c->fpcr,fpsr,a,b,acc,&result,&fpsr);\n",op.bytes),
    }
    }
    out += "if(status){c->fault_status=status;return status;}\n";
    out+=&format!("output[half]|=(result&UINT64_C({mask}))<<shift;}}c->vregs[{}][0]=output[0];c->vregs[{}][1]=output[1];c->fpsr=fpsr;}}\n",op.dst,op.dst);
    out
}

fn emit_estimate(op: VectorFp) -> String {
    let mut out=format!("{{if(!c->floating||(c->fpcr&~UINT32_C(0x02c00000))){{c->fault_status=3;return 3;}}uint64_t left[2]={{c->vregs[{}][0],c->vregs[{}][1]}},output[2]={{0,0}};uint32_t fpsr=c->fpsr;for(unsigned lane=0;lane<2;lane++){{unsigned half=lane/2,shift=(lane%2)*32;uint32_t a=(uint32_t)(left[half]>>shift),result=0;",op.lhs,op.lhs);
    out += r#"uint32_t sign=a&UINT32_C(0x80000000),magnitude=a&UINT32_C(0x7fffffff),fraction=a&UINT32_C(0x7fffff);int exponent=(int)(a>>23)&255;
if(exponent==255){if(fraction){if(!(fraction&UINT32_C(0x400000)))fpsr|=1;result=(c->fpcr&UINT32_C(0x02000000))?UINT32_C(0x7fc00000):(a|UINT32_C(0x400000));}else result=sign;}
else if(!magnitude){result=sign|UINT32_C(0x7f800000);fpsr|=2;}
else if(magnitude<UINT32_C(0x200000)){unsigned rounding=(c->fpcr>>22)&3;int infinity=rounding==0||(rounding==1&&!sign)||(rounding==2&&sign);result=sign|(infinity?UINT32_C(0x7f800000):UINT32_C(0x7f7fffff));fpsr|=20;}
else{if(!exponent){while(!(fraction&UINT32_C(0x800000))){fraction*=2;exponent--;}exponent++;fraction&=UINT32_C(0x7fffff);}unsigned bin=256+(fraction>>15),denominator=2*bin+1;unsigned estimate=(262144+denominator/2)/denominator;int output_exponent=253-exponent;uint32_t significand=(estimate-256)<<15;if(output_exponent<=0){significand=(UINT32_C(0x800000)|significand)>>(1-output_exponent);output_exponent=0;}result=sign|((uint32_t)output_exponent<<23)|significand;}
"#;
    out+=&format!("output[half]|=(uint64_t)result<<shift;}}c->vregs[{}][0]=output[0];c->vregs[{}][1]=output[1];c->fpsr=fpsr;}}\n",op.dst,op.dst);
    out
}

fn emit_rsqrt(op: VectorFp) -> String {
    let mut out=format!("{{if(!c->floating||(c->fpcr&~UINT32_C(0x02c00000))){{c->fault_status=3;return 3;}}uint64_t left[2]={{c->vregs[{}][0],c->vregs[{}][1]}},right[2]={{c->vregs[{}][0],c->vregs[{}][1]}},output[2]={{0,0}};uint32_t fpsr=c->fpsr;(void)right;for(unsigned lane=0;lane<{};lane++){{unsigned half=lane/2,shift=(lane%2)*32;uint32_t a=(uint32_t)(left[half]>>shift),result=0;",op.lhs,op.lhs,op.rhs,op.rhs,op.total/4);
    if op.kind == VectorFpKind::ReciprocalSqrtEstimate {
        out += r#"uint32_t magnitude=a&UINT32_C(0x7fffffff),fraction=a&UINT32_C(0x7fffff);int exponent=(int)(a>>23)&255;
if(exponent==255&&fraction){if(!(fraction&UINT32_C(0x400000)))fpsr|=1;result=(c->fpcr&UINT32_C(0x02000000))?UINT32_C(0x7fc00000):(a|UINT32_C(0x400000));}
else if(!magnitude){result=(a&UINT32_C(0x80000000))|UINT32_C(0x7f800000);fpsr|=2;}
else if(a&UINT32_C(0x80000000)){result=UINT32_C(0x7fc00000);fpsr|=1;}
else if(exponent==255)result=0;
else{if(!exponent){while(!(fraction&UINT32_C(0x800000))){fraction*=2;exponent--;}exponent++;fraction&=UINT32_C(0x7fffff);}unsigned bin=exponent%2?128+(fraction>>16):256+(fraction>>15);unsigned midpoint=bin<256?2*bin+1:2*((bin&~1u)+1);unsigned low=256,high=512;while(low+1<high){unsigned candidate=(low+high)/2;uint64_t boundary=2*candidate-1;if(midpoint*boundary*boundary<UINT64_C(268435456))low=candidate;else high=candidate;}result=((uint32_t)((380-exponent)/2)<<23)|((low-256)<<15);}
"#;
    } else {
        // Finite f32 inputs convert exactly to f64. A nonzero (3-a*b)/2 has
        // magnitude at least 2^-47 and below 2^255, so halving stays f64-normal.
        // The fresh inexact flag permits round-to-odd jamming before f32 rounding.
        out += r#"uint32_t b=(uint32_t)(right[half]>>shift),aa=a&UINT32_C(0x7fffffff),bb=b&UINT32_C(0x7fffffff);int status=0;
if(aa>UINT32_C(0x7f800000)||bb>UINT32_C(0x7f800000)){uint64_t value=0;status=c->floating(11,4,c->fpcr,fpsr,a,b,UINT64_C(0x3fc00000),&value,&fpsr);result=(uint32_t)value;}
else if((aa==UINT32_C(0x7f800000)&&!bb)||(!aa&&bb==UINT32_C(0x7f800000)))result=UINT32_C(0x3fc00000);
else if(aa==UINT32_C(0x7f800000)||bb==UINT32_C(0x7f800000))result=((a^b^UINT32_C(0x80000000))&UINT32_C(0x80000000))|UINT32_C(0x7f800000);
else{uint64_t da=0,db=0,wide=0,value=0;uint32_t raised=0;status=c->floating(14,4,c->fpcr,0,a,0,0,&da,&raised);if(!status)status=c->floating(14,4,c->fpcr,0,b,0,0,&db,&raised);if(!status)status=c->floating(11,8,(c->fpcr&~UINT32_C(0x00c00000))|UINT32_C(0x00c00000),0,da,db,UINT64_C(0x4008000000000000),&wide,&raised);
if(!status){if(!(wide&UINT64_C(0x7fffffffffffffff))){result=((c->fpcr>>22)&3)==2?UINT32_C(0x80000000):0;fpsr|=raised;}else{wide-=UINT64_C(0x0010000000000000);if(raised&16)wide|=1;status=c->floating(14,8,c->fpcr,fpsr|raised,wide,0,0,&value,&fpsr);result=(uint32_t)value;}}}
if(status){c->fault_status=status;return status;}
"#;
    }
    out+=&format!("output[half]|=(uint64_t)result<<shift;}}c->vregs[{}][0]=output[0];c->vregs[{}][1]=output[1];c->fpsr=fpsr;}}\n",op.dst,op.dst);
    out
}
