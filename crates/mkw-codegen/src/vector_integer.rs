use mkw_arm64::vector_integer::{Accumulate, Kind, Reduction, VectorInteger};
fn mask(bytes: u8) -> u64 {
    if bytes == 8 {
        u64::MAX
    } else {
        (1u64 << (bytes * 8)) - 1
    }
}
pub fn emit(op: VectorInteger) -> String {
    match op {
        VectorInteger::WidenMultiply {
            bytes,
            dst,
            lhs,
            rhs,
            upper,
            signed,
            accumulate,
            rhs_lane,
        } => {
            let bits = bytes * 8;
            let outbits = bits * 2;
            let input_mask = mask(bytes);
            let output_mask = mask(bytes * 2);
            let start = if upper { 64 } else { 0 };
            let sign = 1u64 << (bits - 1);
            let selected = if let Some(lane) = rhs_lane {
                format!(
                    "(right[{}]>>{})",
                    u32::from(lane) * u32::from(bits) / 64,
                    u32::from(lane) * u32::from(bits) % 64
                )
            } else {
                "(right[offset/64]>>(offset%64))".into()
            };
            let extend = if signed {
                format!("a=(a^UINT64_C({sign}))-UINT64_C({sign});b=(b^UINT64_C({sign}))-UINT64_C({sign});")
            } else {
                String::new()
            };
            let value = match accumulate {
                Accumulate::Product => "a*b",
                Accumulate::Add => "prior+a*b",
                Accumulate::Subtract => "prior-a*b",
            };
            // Unsigned arithmetic defines modulo 2^64 even for signed products;
            // input sign extension and the widened output mask select the ISA bits.
            format!("{{uint64_t left[2]={{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},right[2]={{c->vregs[{rhs}][0],c->vregs[{rhs}][1]}},old[2]={{c->vregs[{dst}][0],c->vregs[{dst}][1]}},output[2]={{0,0}};for(unsigned lane=0;lane<{};lane++){{unsigned offset={start}+lane*{bits},out=lane*{outbits};uint64_t a=(left[offset/64]>>(offset%64))&UINT64_C({input_mask}),b={selected}&UINT64_C({input_mask}),prior=(old[out/64]>>(out%64))&UINT64_C({output_mask});(void)prior;{extend}uint64_t value=({value})&UINT64_C({output_mask});output[out/64]|=value<<(out%64);}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n",8/bytes)
        }

        VectorInteger::MultiplyHigh {
            bytes,
            total,
            dst,
            lhs,
            rhs,
            rounding,
            rhs_lane,
        } => {
            let bits = bytes * 8;
            let sign = 1u64 << (bits - 1);
            let mask = mask(bytes);
            let increment = if rounding { sign } else { 0 };
            let rhs_expression = if let Some(lane) = rhs_lane {
                let offset = u32::from(lane) * u32::from(bits);
                format!("(right[{}]>>{})", offset / 64, offset % 64)
            } else {
                "(right[half]>>shift)".into()
            };
            // H/S operands fit int64. min*min is the only saturating case and
            // is removed before doubling; all remaining doubled/rounded products
            // fit int64. Unsigned shift plus the output mask avoids signed shifts.
            format!("{{uint64_t left[2]={{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},right[2]={{c->vregs[{rhs}][0],c->vregs[{rhs}][1]}},output[2]={{0,0}};uint32_t fpsr=c->fpsr;for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t a=(left[half]>>shift)&UINT64_C({mask}),b={rhs_expression}&UINT64_C({mask}),value;if(a==UINT64_C({sign})&&b==UINT64_C({sign})){{value=UINT64_C({});fpsr|=UINT32_C(134217728);}}else{{int64_t sa=(int64_t)(a^UINT64_C({sign}))-INT64_C({sign}),sb=(int64_t)(b^UINT64_C({sign}))-INT64_C({sign});int64_t scaled=(sa*sb)*INT64_C(2)+INT64_C({increment});value=((uint64_t)scaled>>{bits})&UINT64_C({mask});}}output[half]|=value<<shift;}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];c->fpsr=fpsr;}}\n",total/bytes,sign-1)
        }

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
                Kind::Negate => "UINT64_C(0)-a".into(),
                Kind::UnsignedMax => "a>b?a:b".into(),
                Kind::UnsignedMin => "a<b?a:b".into(),
                Kind::SignedMax => format!("(a^UINT64_C({sign}))>(b^UINT64_C({sign}))?a:b"),
                Kind::SignedMin => format!("(a^UINT64_C({sign}))<(b^UINT64_C({sign}))?a:b"),
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
            format!("{{uint64_t left[2]={{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},right[2]={{c->vregs[{right}][0],c->vregs[{right}][1]}},output[2]={{0,0}};(void)right;for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t a=(left[half]>>shift)&UINT64_C({mask}),b={rhs};(void)b;uint64_t value={expression};output[half]|=(value&UINT64_C({mask}))<<shift;}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n",total/bytes)
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
            if matches!(
                kind,
                Reduction::SignedMax
                    | Reduction::SignedMin
                    | Reduction::UnsignedMax
                    | Reduction::UnsignedMin
            ) {
                let bias = if matches!(kind, Reduction::SignedMax | Reduction::SignedMin) {
                    1u64 << (bits - 1)
                } else {
                    0
                };
                let compare = if matches!(kind, Reduction::SignedMax | Reduction::UnsignedMax) {
                    ">"
                } else {
                    "<"
                };
                return format!("{{uint64_t source[2]={{c->vregs[{src}][0],c->vregs[{src}][1]}},selected=source[0]&UINT64_C({input_mask});for(unsigned lane=1;lane<{};lane++){{unsigned offset=lane*{bits};uint64_t value=(source[offset/64]>>(offset%64))&UINT64_C({input_mask});if((value^UINT64_C({bias})){compare}(selected^UINT64_C({bias})))selected=value;}}c->vregs[{dst}][0]=selected;c->vregs[{dst}][1]=0;}}\n",total/bytes);
            }
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
