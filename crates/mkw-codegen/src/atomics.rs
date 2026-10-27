use super::{r, write};
use mkw_arm64::{atomics::Atomic, Width};
pub fn emit(op: Atomic) -> String {
    let mut out=format!("{{uint64_t address={},input={},replacement={},old=0;c->fault_address=address;if(!c->atomic_memory){{c->fault_status=3;return 3;}}int status=c->atomic_memory(c->memory_context,{},{},address,input,replacement,&old,{});if(status){{c->fault_status=status;return status;}}\n",r(op.base),r(op.input),r(op.replacement),op.order as u32,op.operation as u32,op.bytes);
    out += &write(
        op.result,
        if op.bytes == 8 {
            Width::X64
        } else {
            Width::W32
        },
        "old",
    );
    out += "}\n";
    out
}
