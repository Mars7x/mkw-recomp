use super::{r, write};
use mkw_arm64::{
    structures::{Structure, Writeback},
    Width,
};
pub fn emit(op: Structure) -> String {
    let mut out=format!("{{uint64_t address={},value[1]={{0}},old[2]={{c->vregs[{}][0],c->vregs[{}][1]}};c->fault_address=address;\n",r(op.base),op.vector,op.vector);
    if op.writeback != Writeback::None {
        let step = match op.writeback {
            Writeback::Immediate(n) => format!("UINT64_C({n})"),
            Writeback::Register(reg) => r(reg),
            Writeback::None => unreachable!(),
        };
        out += &format!("uint64_t updated_base=address+{step};\n");
    }
    let offset = u32::from(op.lane) * u32::from(op.bytes) * 8;
    let mask = if op.bytes == 8 {
        u64::MAX
    } else {
        (1u64 << (op.bytes * 8)) - 1
    };
    if !op.load {
        out += &format!(
            "value[0]=(old[{}]>>{})&UINT64_C({mask});\n",
            offset / 64,
            offset % 64
        );
    }
    out+=&format!("if(!c->memory){{c->fault_status=3;return 3;}}int status=c->memory(c->memory_context,{},address,value,{});if(status){{c->fault_status=status;return status;}}\n",u8::from(!op.load),op.bytes);
    if op.load {
        if op.replicate != 0 {
            out+=&format!("old[0]=0;old[1]=0;for(unsigned b=0;b<{};b+={})old[b/8]|=(value[0]&UINT64_C({mask}))<<((b%8)*8);\n",op.replicate,op.bytes);
        } else {
            out += &format!(
                "old[{}]=(old[{}]&~UINT64_C({}))|((value[0]&UINT64_C({mask}))<<{});\n",
                offset / 64,
                offset / 64,
                mask << (offset % 64),
                offset % 64
            );
        }
        out += &format!(
            "c->vregs[{}][0]=old[0];c->vregs[{}][1]=old[1];\n",
            op.vector, op.vector
        );
    }
    if op.writeback != Writeback::None {
        out += &write(op.base, Width::X64, "updated_base");
    }
    out += "}\n";
    out
}
