use super::{r, write};
use mkw_arm64::{
    structures::{Structure, Writeback},
    Width,
};
pub fn emit(op: Structure) -> String {
    let count = op.registers;
    let bits = op.bytes * 8;
    let mask = if op.bytes == 8 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let offset = u32::from(op.lane) * u32::from(bits);
    let mut out = format!(
        "{{uint64_t address={},value[4]={{0}},old[{count}][2];c->fault_address=address;\n",
        r(op.base)
    );
    for slot in 0..count {
        let reg = (op.vector + slot) % 32;
        out += &format!("old[{slot}][0]=c->vregs[{reg}][0];old[{slot}][1]=c->vregs[{reg}][1];\n");
    }
    if op.writeback != Writeback::None {
        let step = match op.writeback {
            Writeback::Immediate(n) => format!("UINT64_C({n})"),
            Writeback::Register(reg) => r(reg),
            Writeback::None => unreachable!(),
        };
        out += &format!("uint64_t updated_base=address+{step};\n");
    }
    if !op.load {
        out+=&format!("for(unsigned slot=0;slot<{count};slot++){{unsigned to=slot*{bits};uint64_t element=(old[slot][{}]>>{})&UINT64_C({mask});value[to/64]|=element<<(to%64);}}\n",offset/64,offset%64);
    }
    out+=&format!("if(!c->memory){{c->fault_status=3;return 3;}}int status=c->memory(c->memory_context,{},address,value,{});if(status){{c->fault_status=status;return status;}}\n",u8::from(!op.load),op.bytes*count);
    if op.load {
        out+=&format!("for(unsigned slot=0;slot<{count};slot++){{unsigned from=slot*{bits};uint64_t element=(value[from/64]>>(from%64))&UINT64_C({mask});");
        if op.replicate != 0 {
            out+=&format!("old[slot][0]=0;old[slot][1]=0;for(unsigned b=0;b<{};b+={})old[slot][b/8]|=element<<((b%8)*8);",op.replicate,op.bytes);
        } else {
            out += &format!(
                "old[slot][{}]=(old[slot][{}]&~UINT64_C({}))|(element<<{});",
                offset / 64,
                offset / 64,
                mask << (offset % 64),
                offset % 64
            );
        }
        out += "}\n";
        for slot in 0..count {
            let reg = (op.vector + slot) % 32;
            out +=
                &format!("c->vregs[{reg}][0]=old[{slot}][0];c->vregs[{reg}][1]=old[{slot}][1];\n");
        }
    }
    if op.writeback != Writeback::None {
        out += &write(op.base, Width::X64, "updated_base");
    }
    out + "}\n"
}

pub fn emit_interleaved(op: mkw_arm64::structures::Interleaved) -> String {
    let count = op.registers;
    let bits = op.bytes * 8;
    let lanes = op.vector_bytes / op.bytes;
    let mask = if op.bytes == 8 {
        u64::MAX
    } else {
        (1u64 << bits) - 1
    };
    let mut out=format!("{{uint64_t address={},value[8]={{0}},old[{count}][2];c->fault_address=address;(void)old;\n",r(op.base));
    for slot in 0..count {
        let reg = (op.vector + slot) % 32;
        out += &format!("old[{slot}][0]=c->vregs[{reg}][0];old[{slot}][1]=c->vregs[{reg}][1];\n");
    }
    if op.writeback != Writeback::None {
        let step = match op.writeback {
            Writeback::Immediate(n) => format!("UINT64_C({n})"),
            Writeback::Register(reg) => r(reg),
            Writeback::None => unreachable!(),
        };
        out += &format!("uint64_t updated_base=address+{step};\n");
    }
    let at = if op.interleaved {
        format!("(lane*{count}+slot)*{bits}")
    } else {
        format!("(slot*{lanes}+lane)*{bits}")
    };
    if !op.load {
        out+=&format!("for(unsigned lane=0;lane<{lanes};lane++)for(unsigned slot=0;slot<{count};slot++){{unsigned from=lane*{bits},to={at};uint64_t element=(old[slot][from/64]>>(from%64))&UINT64_C({mask});value[to/64]|=element<<(to%64);}}\n");
    }
    out+=&format!("if(!c->memory){{c->fault_status=3;return 3;}}int status=c->memory(c->memory_context,{},address,value,{});if(status){{c->fault_status=status;return status;}}\n",u8::from(!op.load),op.vector_bytes*count);
    if op.load {
        out+=&format!("uint64_t output[{count}][2]={{0}};for(unsigned lane=0;lane<{lanes};lane++)for(unsigned slot=0;slot<{count};slot++){{unsigned from={at},to=lane*{bits};uint64_t element=(value[from/64]>>(from%64))&UINT64_C({mask});output[slot][to/64]|=element<<(to%64);}}\n");
        for slot in 0..count {
            let reg = (op.vector + slot) % 32;
            out += &format!(
                "c->vregs[{reg}][0]=output[{slot}][0];c->vregs[{reg}][1]=output[{slot}][1];\n"
            );
        }
    }
    if op.writeback != Writeback::None {
        out += &write(op.base, Width::X64, "updated_base");
    }
    out + "}\n"
}
