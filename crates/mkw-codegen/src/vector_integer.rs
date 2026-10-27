use mkw_arm64::vector_integer::{Kind, Reduction, VectorInteger};
fn mask(bytes: u8) -> u64 {
    if bytes == 8 {
        u64::MAX
    } else {
        (1u64 << (bytes * 8)) - 1
    }
}
pub fn emit(op: VectorInteger) -> String {
    match op {
        VectorInteger::Lanes {
            bytes,
            total,
            dst,
            lhs,
            rhs,
            kind,
        } => {
            let bits = bytes * 8;
            let mask = mask(bytes);
            let sign = 1u64 << (bits - 1);
            let right = rhs.unwrap_or(0);
            let rhs = if rhs.is_some() {
                format!("(right[half]>>shift)&UINT64_C({mask})")
            } else {
                "UINT64_C(0)".into()
            };
            let expression = match kind {
                Kind::Add => "a+b".into(),
                Kind::Subtract => "a-b".into(),
                _ => {
                    let predicate = match kind {
                        Kind::Equal => "a==b".into(),
                        Kind::Test => "(a&b)!=0".into(),
                        Kind::UnsignedGe => "a>=b".into(),
                        Kind::UnsignedGt => "a>b".into(),
                        Kind::SignedGe => format!("(a^UINT64_C({sign}))>=(b^UINT64_C({sign}))"),
                        Kind::SignedGt => format!("(a^UINT64_C({sign}))>(b^UINT64_C({sign}))"),
                        Kind::SignedLe => format!("(a^UINT64_C({sign}))<=(b^UINT64_C({sign}))"),
                        Kind::SignedLt => format!("(a^UINT64_C({sign}))<(b^UINT64_C({sign}))"),
                        _ => unreachable!(),
                    };
                    format!("({predicate})?UINT64_C({mask}):UINT64_C(0)")
                }
            };
            format!("{{uint64_t left[2]={{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},right[2]={{c->vregs[{right}][0],c->vregs[{right}][1]}},output[2]={{0,0}};(void)right;for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t a=(left[half]>>shift)&UINT64_C({mask}),b={rhs};uint64_t value={expression};output[half]|=(value&UINT64_C({mask}))<<shift;}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n",total/bytes)
        }
        VectorInteger::Reduce {
            bytes,
            total,
            dst,
            src,
            kind,
        } => {
            let bits = bytes * 8;
            let input_mask = mask(bytes);
            let output_mask = mask(if kind == Reduction::Sum {
                bytes
            } else {
                bytes * 2
            });
            let value = if kind == Reduction::SignedLong {
                let sign = 1u64 << (bits - 1);
                format!("(value^UINT64_C({sign}))-UINT64_C({sign})")
            } else {
                "value".into()
            };
            format!("{{uint64_t source[2]={{c->vregs[{src}][0],c->vregs[{src}][1]}},sum=0;for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits};uint64_t value=(source[offset/64]>>(offset%64))&UINT64_C({input_mask});sum+={value};}}c->vregs[{dst}][0]=sum&UINT64_C({output_mask});c->vregs[{dst}][1]=0;}}\n",total/bytes)
        }
    }
}
