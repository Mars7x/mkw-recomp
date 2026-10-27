use mkw_arm64::vector_fp::{VectorFp, VectorFpKind};
pub fn emit(op: VectorFp) -> String {
    let mask = if op.bytes == 8 { u64::MAX } else { 0xffffffff };
    let lanes = op.total / op.bytes;
    let bits = op.bytes * 8;
    let (callback, operation) = match op.kind {
        VectorFpKind::Arithmetic(n) => ("floating", u32::from(n)),
        VectorFpKind::Conversion(n) => ("convert", u32::from(n)),
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
        VectorFpKind::PairwiseAdd|VectorFpKind::PairwiseMinMax(_)=>unreachable!(),
        VectorFpKind::Arithmetic(_)=>out+=&format!("uint64_t b={right_lane}&UINT64_C({mask}),acc=(old[half]>>shift)&UINT64_C({mask});int status=c->floating({operation},{},c->fpcr,fpsr,a,b,acc,&result,&fpsr);\n",op.bytes),
    }
    }
    out += "if(status){c->fault_status=status;return status;}\n";
    out+=&format!("output[half]|=(result&UINT64_C({mask}))<<shift;}}c->vregs[{}][0]=output[0];c->vregs[{}][1]=output[1];c->fpsr=fpsr;}}\n",op.dst,op.dst);
    out
}
