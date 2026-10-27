use mkw_arm64::vector_compare::{Compare, Relation};
pub fn emit(op: Compare) -> String {
    let mask = if op.bytes == 8 { u64::MAX } else { 0xffffffff };
    let bits = op.bytes * 8;
    let rhs = op.rhs.unwrap_or(0);
    let mut out=format!("{{if(!c->floating){{c->fault_status=3;return 3;}}uint64_t left[2]={{c->vregs[{}][0],c->vregs[{}][1]}},right[2]={{c->vregs[{rhs}][0],c->vregs[{rhs}][1]}},output[2]={{0,0}};uint32_t fpsr=c->fpsr;(void)right;for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t a=(left[half]>>shift)&UINT64_C({mask}),b={};\n",op.lhs,op.lhs,op.total/op.bytes,if op.rhs.is_some(){format!("(right[half]>>shift)&UINT64_C({mask})")}else{"UINT64_C(0)".into()});
    if op.absolute {
        let magnitude = mask >> 1;
        out += &format!("a&=UINT64_C({magnitude});b&=UINT64_C({magnitude});\n");
    }
    let operation = if op.relation == Relation::Equal { 4 } else { 5 };
    let predicate = match op.relation {
        Relation::Equal => "result==6",
        Relation::GreaterEqual => "result==6||result==2",
        Relation::Greater => "result==2",
        Relation::LessEqual => "result==6||result==8",
        Relation::Less => "result==8",
    };
    out+=&format!("uint64_t result=0;int status=c->floating({operation},{},c->fpcr,fpsr,a,b,0,&result,&fpsr);if(status){{c->fault_status=status;return status;}}uint64_t value=({predicate})?UINT64_C({mask}):UINT64_C(0);output[half]|=value<<shift;}}c->vregs[{}][0]=output[0];c->vregs[{}][1]=output[1];c->fpsr=fpsr;}}\n",op.bytes,op.dst,op.dst);
    out
}
