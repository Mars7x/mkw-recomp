use super::{r, write};
use mkw_arm64::{exclusive::Exclusive, Reg, Width};
fn general(reg: Reg) -> bool {
    matches!(reg, Reg::Gpr(0..=30) | Reg::Zero)
}
fn base(reg: Reg) -> bool {
    matches!(reg, Reg::Gpr(0..=30) | Reg::Sp)
}
pub fn emit(op: Exclusive) -> Result<String, String> {
    let mut out = String::new();
    match op {
        Exclusive::Clear { immediate } => {
            if immediate > 15 {
                return Err("invalid CLREX immediate".into());
            }
            out += "{int status=frame->exclusive->clear(frame->exclusive_context);if(status){c->fault_status=status;return status;}}\n";
        }
        Exclusive::Load {
            bytes,
            acquire,
            base: rn,
            dst,
        } => {
            if !matches!(bytes, 1 | 2 | 4 | 8) || !base(rn) || !general(dst) {
                return Err("invalid single exclusive load IR".into());
            }
            out += &format!(
                "{{uint64_t address={},data[2]={{0,0}};c->fault_address=address;\n",
                r(rn)
            );
            if rn == Reg::Sp {
                out += "if(c->sp&15){c->fault_status=1;return 1;}\n";
            }
            out += &format!("int status=frame->exclusive->load(frame->exclusive_context,address,{bytes},{},data,2);if(status){{c->fault_status=status;return status;}}\n",u32::from(acquire));
            out += &write(
                dst,
                if bytes == 8 { Width::X64 } else { Width::W32 },
                "data[0]",
            );
            out += "}\n";
        }
        Exclusive::Store {
            bytes,
            release,
            base: rn,
            src,
            status: rs,
        } => {
            if !matches!(bytes, 1 | 2 | 4 | 8)
                || !base(rn)
                || !general(src)
                || !general(rs)
                || rs == src
                || (rn != Reg::Sp && rs == rn)
            {
                return Err("invalid/constrained single exclusive store IR".into());
            }
            out += &format!("{{uint64_t address={},data[2]={{{},0}};uint32_t stored=0;c->fault_address=address;\n",r(rn),r(src));
            if rn == Reg::Sp {
                out += "if(c->sp&15){c->fault_status=1;return 1;}\n";
            }
            out += &format!("int status=frame->exclusive->store(frame->exclusive_context,address,{bytes},{},data,2,&stored);if(status){{c->fault_status=status;return status;}}if(stored>1){{c->fault_status=3;return 3;}}\n",u32::from(release));
            out += &write(rs, Width::W32, "1u-stored");
            out += "}\n";
        }
    }
    Ok(out)
}
