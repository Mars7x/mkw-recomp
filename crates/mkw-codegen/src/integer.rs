use super::{r, ty, write};
use mkw_arm64::{integer::MultiplyKind, Op, Width};
pub fn emit(op: Op) -> Result<String, String> {
    Ok(match op {
        Op::Multiply {
            width,
            dst,
            lhs,
            rhs,
            acc,
            sub,
            kind,
        } => {
            let mut out = format!(
                "{{uint64_t a=({})({}),b=({})({});uint64_t result;\n",
                ty(width),
                r(lhs),
                ty(width),
                r(rhs)
            );
            match kind {
                MultiplyKind::Low=>out+="result=a*b;\n",
                MultiplyKind::UnsignedLong=>out+="result=(uint64_t)(uint32_t)a*(uint64_t)(uint32_t)b;\n",
                MultiplyKind::SignedLong=>out+="a=((a&UINT64_C(0xffffffff))^UINT64_C(0x80000000))-UINT64_C(0x80000000);b=((b&UINT64_C(0xffffffff))^UINT64_C(0x80000000))-UINT64_C(0x80000000);result=a*b;\n",
                MultiplyKind::UnsignedHigh|MultiplyKind::SignedHigh=>{
                    out+="uint64_t a0=(uint32_t)a,a1=a>>32,b0=(uint32_t)b,b1=b>>32;uint64_t t=a1*b0+((a0*b0)>>32);uint64_t carry=t>>32;t=a0*b1+(uint32_t)t;result=a1*b1+carry+(t>>32);\n";
                    if kind==MultiplyKind::SignedHigh {out+="result-=(a>>63)?b:0;result-=(b>>63)?a:0;\n";}
                }
            }
            if !matches!(kind, MultiplyKind::UnsignedHigh | MultiplyKind::SignedHigh) {
                out += &format!("result={}{}result;\n", r(acc), if sub { "-" } else { "+" });
            }
            out += &write(dst, width, "result");
            out += "}\n";
            out
        }
        Op::Divide {
            width,
            dst,
            lhs,
            rhs,
            signed,
        } => {
            let t = ty(width);
            let bits = if width == Width::W32 { 32 } else { 64 };
            let mut out = format!("{{{t} a=({t})({}),b=({t})({}),result=0;\n", r(lhs), r(rhs));
            if signed {
                out+=&format!("if(b){{unsigned sa=(unsigned)(a>>{}),sb=(unsigned)(b>>{});{t} ma=sa?({t})(0-a):a,mb=sb?({t})(0-b):b;result=ma/mb;if(sa!=sb)result=({t})(0-result);}}\n",bits-1,bits-1);
            } else {
                out += "if(b)result=a/b;\n";
            }
            out += &write(dst, width, "result");
            out += "}\n";
            out
        }
        _ => return Err("not a multiply/divide operation".into()),
    })
}
