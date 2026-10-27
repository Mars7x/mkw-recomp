use super::{r, ty, write};
use mkw_arm64::{integer::MultiplyKind, Op, Width};
pub fn emit(op: Op) -> Result<String, String> {
    Ok(match op {
        Op::Crc32 {
            dst,
            lhs,
            rhs,
            bytes,
            castagnoli,
        } => {
            let polynomial = if castagnoli {
                0x82f63b78u32
            } else {
                0xedb88320u32
            };
            let mut out=format!("{{uint32_t crc=(uint32_t)({});uint64_t data={};for(unsigned bit=0;bit<{};bit++){{uint32_t low=(crc^(uint32_t)data)&1;crc=(crc>>1)^(UINT32_C({polynomial})&(UINT32_C(0)-low));data>>=1;}}\n",r(lhs),r(rhs),u32::from(bytes)*8);
            out += &write(dst, Width::W32, "crc");
            out += "}\n";
            out
        }

        Op::IntegerUnary {
            width,
            dst,
            src,
            operation,
        } => {
            let bits = if width == Width::W32 { 32 } else { 64 };
            let mut out = format!("{{uint64_t value=({})({}),result=0;\n", ty(width), r(src));
            match operation {
                0=>out+=&format!("for(unsigned i=0;i<{bits};i++)result|=((value>>i)&1)<<({bits}-1-i);\n"),
                1..=3=>{let group=1u32<<operation;let bytes=bits/8;out+=&format!("for(unsigned b=0;b<{bytes};b++){{unsigned to=(b/{group})*{group}+({group}-1-b%{group});result|=((value>>(b*8))&255)<<(to*8);}}\n");},
                4=>out+=&format!("for(unsigned bit={bits};bit>0;bit--){{if((value>>(bit-1))&1)break;result++;}}\n"),
                5=>out+=&format!("uint64_t sign=value>>{};for(unsigned bit={};bit>0;bit--){{if(((value>>(bit-1))&1)!=sign)break;result++;}}\n",bits-1,bits-1),
                _=>return Err("invalid integer unary operation".into()),
            }
            out += &write(dst, width, "result");
            out += "}\n";
            out
        }
        Op::CarryArithmetic {
            width,
            dst,
            lhs,
            rhs,
            sub,
            flags,
        } => {
            let t = ty(width);
            let bits = if width == Width::W32 { 32 } else { 64 };
            let mut out = format!(
                "{{{t} a=({t})({}),b=({t})({}),carry=({t})(c->c&1u);\n",
                r(lhs),
                r(rhs)
            );
            if sub {
                out += "b=~b;\n";
            }
            out += &format!("{t} sum=({t})(a+b),result=({t})(sum+carry);\n");
            if flags {
                out+=&format!("c->n=(uint32_t)(result>>{});c->z=(result==0);c->c=(sum<a)||(result<sum);c->v=(uint32_t)((~(a^b)&(a^result))>>{});\n",bits-1,bits-1);
            }
            out += &write(dst, width, "result");
            out += "}\n";
            out
        }
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
        _ => return Err("not an integer arithmetic/bit operation".into()),
    })
}
