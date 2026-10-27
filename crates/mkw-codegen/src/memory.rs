use super::{r, write};
use mkw_arm64::{
    memory::{AddressMode, Transfer, ValueReg},
    Width,
};
fn read(value: ValueReg) -> (String, String) {
    match value {
        ValueReg::Gpr(reg) => (r(reg), "UINT64_C(0)".into()),
        ValueReg::Vector(n) => (format!("c->vregs[{n}][0]"), format!("c->vregs[{n}][1]")),
    }
}
fn extend(expr: &str, option: u8) -> String {
    match option {
        2 => format!("({expr}&UINT64_C(0xffffffff))"),
        3 | 7 => expr.into(),
        6 => format!("((({expr}&UINT64_C(0xffffffff))^UINT64_C(0x80000000))-UINT64_C(0x80000000))"),
        _ => unreachable!("decoded extension"),
    }
}
pub fn emit(op: Transfer) -> String {
    let count = if op.second.is_some() { 2 } else { 1 };
    let total = usize::from(op.bytes) * count;
    let mut out = String::from("{\n");
    let (address, writeback) = match op.address {
        AddressMode::Literal(at) => (format!("c->module_base+UINT64_C({at})"), false),
        AddressMode::Offset(off) => (format!("{}+UINT64_C({})", r(op.base), off as u64), false),
        AddressMode::PreIndex(off) => {
            out += &format!(
                "uint64_t updated_base={}+UINT64_C({});\n",
                r(op.base),
                off as u64
            );
            ("updated_base".into(), true)
        }
        AddressMode::PostIndex(off) => {
            out += &format!(
                "uint64_t updated_base={}+UINT64_C({});\n",
                r(op.base),
                off as u64
            );
            (r(op.base), true)
        }
        AddressMode::Register {
            index,
            extend: option,
            shift,
        } => (
            format!("{}+({}<<{shift})", r(op.base), extend(&r(index), option)),
            false,
        ),
    };
    out+=&format!("uint64_t address={address};uint64_t values[4]={{0}};uint64_t elements[2][2]={{ {{0,0}},{{0,0}} }};c->fault_address=address;\n");
    if !op.load {
        for (i, value) in std::iter::once(op.first).chain(op.second).enumerate() {
            let (lo, hi) = read(value);
            out += &format!("elements[{i}][0]={lo};elements[{i}][1]={hi};\n");
        }
        out+=&format!("for(unsigned e=0;e<{count};e++)for(unsigned b=0;b<{};b++){{unsigned offset=e*{}+b;values[offset/8]|=((elements[e][b/8]>>((b%8)*8))&UINT64_C(255))<<((offset%8)*8);}}\n",op.bytes,op.bytes);
    }
    out+=&format!("if(!c->memory){{c->fault_status=3;return 3;}}int status=c->memory(c->memory_context,{},address,values,{total});if(status){{c->fault_status=status;return status;}}\n",u8::from(!op.load));
    if op.load {
        out+=&format!("for(unsigned e=0;e<{count};e++)for(unsigned b=0;b<{};b++){{unsigned offset=e*{}+b;elements[e][b/8]|=((values[offset/8]>>((offset%8)*8))&UINT64_C(255))<<((b%8)*8);}}\n",op.bytes,op.bytes);
        for (i, value) in std::iter::once(op.first).chain(op.second).enumerate() {
            match value {
                ValueReg::Gpr(reg) => {
                    let expression = if op.signed {
                        let sign = 1u64 << (op.bytes * 8 - 1);
                        format!("((elements[{i}][0]^UINT64_C({sign}))-UINT64_C({sign}))")
                    } else {
                        format!("elements[{i}][0]")
                    };
                    out += &write(reg, op.width, &expression);
                }
                ValueReg::Vector(n) => {
                    out += &format!(
                        "c->vregs[{n}][0]=elements[{i}][0];c->vregs[{n}][1]=elements[{i}][1];\n"
                    )
                }
            }
        }
    }
    if writeback {
        out += &write(op.base, Width::X64, "updated_base");
    }
    out += "}\n";
    out
}
