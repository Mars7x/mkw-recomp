use mkw_arm64::vector_fp::{VectorFp, VectorFpKind};
pub fn emit(op: VectorFp) -> String {
    let mask = if op.bytes == 8 { u64::MAX } else { 0xffffffff };
    let lanes = op.total / op.bytes;
    let bits = op.bytes * 8;
    let (callback, operation) = match op.kind {
        VectorFpKind::Arithmetic(n) => ("floating", n),
        VectorFpKind::Conversion(n) => ("convert", n),
    };
    let mut out=format!("{{if(!c->{callback}){{c->fault_status=3;return 3;}}uint64_t left[2]={{c->vregs[{}][0],c->vregs[{}][1]}},right[2]={{c->vregs[{}][0],c->vregs[{}][1]}},old[2]={{c->vregs[{}][0],c->vregs[{}][1]}},output[2]={{0,0}};uint32_t fpsr=c->fpsr;(void)right;(void)old;\n",op.lhs,op.lhs,op.rhs,op.rhs,op.dst,op.dst);
    out+=&format!("for(unsigned lane=0;lane<{lanes};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t a=(left[half]>>shift)&UINT64_C({mask}),result=0;\n");
    match op.kind {
        VectorFpKind::Conversion(_)=>out+=&format!("int status=c->convert({operation},{bits},{},c->fpcr,fpsr,a,&result,&fpsr);\n",op.bytes),
        VectorFpKind::Arithmetic(_)=>out+=&format!("uint64_t b=(right[half]>>shift)&UINT64_C({mask}),acc=(old[half]>>shift)&UINT64_C({mask});int status=c->floating({operation},{},c->fpcr,fpsr,a,b,acc,&result,&fpsr);\n",op.bytes),
    }
    out += "if(status){c->fault_status=status;return status;}\n";
    out+=&format!("output[half]|=(result&UINT64_C({mask}))<<shift;}}c->vregs[{}][0]=output[0];c->vregs[{}][1]=output[1];c->fpsr=fpsr;}}\n",op.dst,op.dst);
    out
}
