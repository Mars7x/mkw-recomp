use mkw_arm64::vector_integer::{Accumulate, Kind, NarrowKind, Reduction, VectorInteger};
fn mask(bytes: u8) -> u64 {
    if bytes == 8 {
        u64::MAX
    } else {
        (1u64 << (bytes * 8)) - 1
    }
}
pub fn emit(op: VectorInteger) -> String {
    match op {
        VectorInteger::PairwiseAdd {
            bytes,
            total,
            dst,
            lhs,
            rhs,
        } => {
            let bits = bytes * 8;
            let mask = mask(bytes);
            let pairs = if rhs.is_some() { total / bytes / 2 } else { 1 };
            let right = rhs.unwrap_or(lhs);
            let sources = if rhs.is_some() { 2 } else { 1 };
            format!("{{uint64_t source[2][2]={{{{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},{{c->vregs[{right}][0],c->vregs[{right}][1]}}}},output[2]={{0,0}};for(unsigned side=0;side<{sources};side++)for(unsigned pair=0;pair<{pairs};pair++){{unsigned first=pair*{bits}*2,second=first+{bits},out=(side*{pairs}+pair)*{bits};uint64_t a=(source[side][first/64]>>(first%64))&UINT64_C({mask}),b=(source[side][second/64]>>(second%64))&UINT64_C({mask});output[out/64]|=((a+b)&UINT64_C({mask}))<<(out%64);}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n")
        }

        VectorInteger::RightShift {
            bytes,
            total,
            dst,
            src,
            amount,
            signed,
            rounding,
            accumulate,
        } => {
            let bits = bytes * 8;
            let mask = mask(bytes);
            let sign = 1u64 << (bits - 1);
            let shifted = if amount == bits {
                if signed {
                    format!("(a&UINT64_C({sign}))?UINT64_C({mask}):UINT64_C(0)")
                } else {
                    "UINT64_C(0)".into()
                }
            } else if signed {
                format!(
                    "(a>>{amount})|((a&UINT64_C({sign}))?UINT64_C({}):UINT64_C(0))",
                    mask ^ (mask >> amount)
                )
            } else {
                format!("a>>{amount}")
            };
            let round = if rounding {
                format!("value+=((a>>{})&UINT64_C(1));", amount - 1)
            } else {
                String::new()
            };
            let add = if accumulate { "value+=prior;" } else { "" };
            format!("{{uint64_t source[2]={{c->vregs[{src}][0],c->vregs[{src}][1]}},old[2]={{c->vregs[{dst}][0],c->vregs[{dst}][1]}},output[2]={{0,0}};for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t a=(source[half]>>shift)&UINT64_C({mask}),prior=(old[half]>>shift)&UINT64_C({mask});(void)a;(void)prior;uint64_t value={shifted};{round}{add}output[half]|=(value&UINT64_C({mask}))<<shift;}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n",total/bytes)
        }

        VectorInteger::WidenArithmetic {
            bytes,
            dst,
            lhs,
            rhs,
            upper,
            signed,
            wide_lhs,
            subtract,
        } => {
            let bits = bytes * 8;
            let outbits = bits * 2;
            let input_mask = mask(bytes);
            let output_mask = mask(bytes * 2);
            let sign = 1u64 << (bits - 1);
            let start = if upper { 64 } else { 0 };
            let left_offset = if wide_lhs { "out" } else { "offset" };
            let left_mask = if wide_lhs { output_mask } else { input_mask };
            let extend = if signed {
                format!(
                    "{}b=(b^UINT64_C({sign}))-UINT64_C({sign});",
                    if wide_lhs {
                        String::new()
                    } else {
                        format!("a=(a^UINT64_C({sign}))-UINT64_C({sign});")
                    }
                )
            } else {
                String::new()
            };
            let value = if subtract { "a-b" } else { "a+b" };
            format!("{{uint64_t left[2]={{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},right[2]={{c->vregs[{rhs}][0],c->vregs[{rhs}][1]}},output[2]={{0,0}};for(unsigned lane=0;lane<{};lane++){{unsigned offset={start}+lane*{bits},out=lane*{outbits},left_offset={left_offset};uint64_t a=(left[left_offset/64]>>(left_offset%64))&UINT64_C({left_mask}),b=(right[offset/64]>>(offset%64))&UINT64_C({input_mask});{extend}uint64_t value=({value})&UINT64_C({output_mask});output[out/64]|=value<<(out%64);}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n",8/bytes)
        }

        VectorInteger::RegisterShift {
            bytes,
            total,
            dst,
            lhs,
            rhs,
            signed,
        } => {
            let bits = bytes * 8;
            let mask = mask(bytes);
            let sign = 1u64 << (bits - 1);
            let negative = if signed {
                format!("(a&UINT64_C({sign}))!=0")
            } else {
                "0".into()
            };
            format!("{{uint64_t left[2]={{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},right[2]={{c->vregs[{rhs}][0],c->vregs[{rhs}][1]}},output[2]={{0,0}};for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t a=(left[half]>>shift)&UINT64_C({mask}),b=(right[half]>>shift)&UINT64_C(255),value;int amount=(int)b;if(amount>=128)amount-=256;if(amount>=0){{value=amount>={bits}?0:(a<<amount)&UINT64_C({mask});}}else{{unsigned count=(unsigned)(-amount);if(count>={bits})value={negative}?UINT64_C({mask}):0;else{{value=a>>count;if({negative})value|=UINT64_C({mask})^(UINT64_C({mask})>>count);}}}}output[half]|=value<<shift;}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n",total/bytes)
        }

        VectorInteger::SaturatingShiftNarrow {
            bytes,
            dst,
            src,
            scalar,
            upper,
            kind,
            amount,
            rounding,
        } => narrow(Narrow {
            bytes,
            dst,
            src,
            scalar,
            upper,
            kind,
            amount,
            rounding,
        }),
        VectorInteger::SaturatingNarrow {
            bytes,
            dst,
            src,
            scalar,
            upper,
            kind,
        } => narrow(Narrow {
            bytes,
            dst,
            src,
            scalar,
            upper,
            kind,
            amount: 0,
            rounding: false,
        }),
        VectorInteger::Saturating {
            bytes,
            total,
            dst,
            lhs,
            rhs,
            signed,
            subtract,
        } => {
            let bits = bytes * 8;
            let mask = mask(bytes);
            let sign = 1u64 << (bits - 1);
            let result = if subtract { "a-b" } else { "a+b" };
            let overflow = if signed {
                if subtract {
                    format!("((a^b)&(a^value)&UINT64_C({sign}))!=0")
                } else {
                    format!("(~(a^b)&(a^value)&UINT64_C({sign}))!=0")
                }
            } else if subtract {
                "a<b".into()
            } else {
                "value<a".into()
            };
            let clip = if signed {
                format!(
                    "(a&UINT64_C({sign}))?UINT64_C({sign}):UINT64_C({})",
                    sign - 1
                )
            } else if subtract {
                "UINT64_C(0)".into()
            } else {
                format!("UINT64_C({mask})")
            };
            format!("{{uint64_t left[2]={{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},right[2]={{c->vregs[{rhs}][0],c->vregs[{rhs}][1]}},output[2]={{0,0}};uint32_t fpsr=c->fpsr;for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t a=(left[half]>>shift)&UINT64_C({mask}),b=(right[half]>>shift)&UINT64_C({mask}),value=({result})&UINT64_C({mask});if({overflow}){{value={clip};fpsr|=UINT32_C(134217728);}}output[half]|=value<<shift;}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];c->fpsr=fpsr;}}\n",total/bytes)
        }

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

struct Narrow {
    bytes: u8,
    dst: u8,
    src: u8,
    scalar: bool,
    upper: bool,
    kind: NarrowKind,
    amount: u8,
    rounding: bool,
}
fn narrow(
    Narrow {
        bytes,
        dst,
        src,
        scalar,
        upper,
        kind,
        amount,
        rounding,
    }: Narrow,
) -> String {
    let bits = bytes * 8;
    let inbits = bits * 2;
    let input_mask = mask(bytes * 2);
    let output_mask = mask(bytes);
    let sign = 1u64 << (inbits - 1);
    let signed_input = kind != NarrowKind::Unsigned;
    let signed_output = kind == NarrowKind::Signed;
    let max = if signed_output {
        (1u64 << (bits - 1)) - 1
    } else {
        output_mask
    };
    let min = 1u64 << (bits - 1);
    let threshold = input_mask - min + 1;
    let negative = if signed_input {
        format!("(a&UINT64_C({sign}))!=0")
    } else {
        "0".into()
    };
    let handle_negative = if signed_output {
        format!("if(a<UINT64_C({threshold})){{value=UINT64_C({min});fpsr|=UINT32_C(134217728);}}else value=a&UINT64_C({output_mask});")
    } else {
        "value=0;fpsr|=UINT32_C(134217728);".into()
    };
    let transform = if amount == 0 {
        String::new()
    } else {
        let round = if rounding {
            format!("((a>>{})&UINT64_C(1))", amount - 1)
        } else {
            "UINT64_C(0)".into()
        };
        let fill = if signed_input {
            format!(
                "((a&UINT64_C({sign}))?UINT64_C({}):UINT64_C(0))",
                input_mask ^ (input_mask >> amount)
            )
        } else {
            "UINT64_C(0)".into()
        };
        format!("uint64_t increment={round};a=((a>>{amount})|{fill});a=(a+increment)&UINT64_C({input_mask});")
    };
    let low = if upper {
        format!("c->vregs[{dst}][0]")
    } else {
        "0".into()
    };
    let start = if upper { 64 } else { 0 };
    let lanes = if scalar { 1 } else { 8 / bytes };
    format!("{{uint64_t source[2]={{c->vregs[{src}][0],c->vregs[{src}][1]}},output[2]={{{low},0}};uint32_t fpsr=c->fpsr;for(unsigned lane=0;lane<{lanes};lane++){{unsigned offset=lane*{inbits},out={start}+lane*{bits};uint64_t a=(source[offset/64]>>(offset%64))&UINT64_C({input_mask}),value;{transform}if({negative}){{{handle_negative}}}else if(a>UINT64_C({max})){{value=UINT64_C({max});fpsr|=UINT32_C(134217728);}}else value=a;output[out/64]|=value<<(out%64);}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];c->fpsr=fpsr;}}\n")
}
