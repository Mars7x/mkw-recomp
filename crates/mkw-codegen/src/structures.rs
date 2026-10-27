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

pub fn emit_interleaved(op: mkw_arm64::structures::Interleaved) -> String {
    let next = (op.vector + 1) % 32;
    let bits = op.bytes * 8;
    let mask = if op.bytes == 8 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let mut out=format!("{{uint64_t address={},value[4]={{0,0,0,0}},old[2][2]={{{{c->vregs[{}][0],c->vregs[{}][1]}} ,{{c->vregs[{next}][0],c->vregs[{next}][1]}}}};c->fault_address=address;(void)old;\n",r(op.base),op.vector,op.vector);
    if op.writeback != Writeback::None {
        let step = match op.writeback {
            Writeback::Immediate(n) => format!("UINT64_C({n})"),
            Writeback::Register(reg) => r(reg),
            Writeback::None => unreachable!(),
        };
        out += &format!("uint64_t updated_base=address+{step};\n");
    }
    if !op.load {
        out+=&format!("for(unsigned lane=0;lane<{};lane++)for(unsigned slot=0;slot<2;slot++){{unsigned from=lane*{bits},to=(lane*2+slot)*{bits};uint64_t element=(old[slot][from/64]>>(from%64))&UINT64_C({mask});value[to/64]|=element<<(to%64);}}\n",op.vector_bytes/op.bytes);
    }
    out+=&format!("if(!c->memory){{c->fault_status=3;return 3;}}int status=c->memory(c->memory_context,{},address,value,{});if(status){{c->fault_status=status;return status;}}\n",u8::from(!op.load),op.vector_bytes*2);
    if op.load {
        out+=&format!("uint64_t output[2][2]={{{{0,0}},{{0,0}}}};for(unsigned lane=0;lane<{};lane++)for(unsigned slot=0;slot<2;slot++){{unsigned from=(lane*2+slot)*{bits},to=lane*{bits};uint64_t element=(value[from/64]>>(from%64))&UINT64_C({mask});output[slot][to/64]|=element<<(to%64);}}c->vregs[{}][0]=output[0][0];c->vregs[{}][1]=output[0][1];c->vregs[{next}][0]=output[1][0];c->vregs[{next}][1]=output[1][1];\n",op.vector_bytes/op.bytes,op.vector,op.vector);
    }
    if op.writeback != Writeback::None {
        out += &write(op.base, Width::X64, "updated_base");
    }
    out + "}\n"
}
