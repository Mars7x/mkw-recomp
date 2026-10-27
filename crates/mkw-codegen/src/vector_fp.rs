use mkw_arm64::vector_fp::{VectorFp, VectorFpKind};
pub fn emit(op: VectorFp) -> String {
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
        VectorFpKind::ReduceMinMax(_) | VectorFpKind::ReciprocalEstimate => unreachable!(),
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
        VectorFpKind::PairwiseAdd|VectorFpKind::PairwiseMinMax(_)|VectorFpKind::ReduceMinMax(_)|VectorFpKind::ReciprocalEstimate=>unreachable!(),
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
