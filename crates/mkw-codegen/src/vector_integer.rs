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
        VectorInteger::MultiplyLow {
            bytes,
            total,
            dst,
            lhs,
            rhs,
            rhs_lane,
        } => {
            let bits = bytes * 8;
            let mask = mask(bytes);
            let right = if let Some(lane) = rhs_lane {
                let offset = u32::from(lane) * u32::from(bits);
                format!("(right[{}]>>{})", offset / 64, offset % 64)
            } else {
                "(right[half]>>shift)".into()
            };
            format!("{{uint64_t left[2]={{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},right[2]={{c->vregs[{rhs}][0],c->vregs[{rhs}][1]}},output[2]={{0,0}};for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t a=(left[half]>>shift)&UINT64_C({mask}),b={right}&UINT64_C({mask});output[half]|=((a*b)&UINT64_C({mask}))<<shift;}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n",total/bytes)
        }

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

pub fn emit_halving(op: mkw_arm64::vector_integer::Halving) -> String {
    let mkw_arm64::vector_integer::Halving {
        bytes,
        total,
        dst,
        lhs,
        rhs,
        signed,
        subtract,
    } = op;
    let bits = bytes * 8;
    let mask = mask(bytes);
    let sign = 1u64 << (bits - 1);
    let calculation = if signed {
        format!("int64_t a=(int64_t)(ua^UINT64_C({sign}))-(int64_t)UINT64_C({sign}),b=(int64_t)(ub^UINT64_C({sign}))-(int64_t)UINT64_C({sign}),wide=a{}b;uint64_t value=(uint64_t)(wide/2-(wide<0&&wide%2!=0));",if subtract{"-"}else{"+"})
    } else {
        "uint64_t value=(ua+ub)/2;".into()
    };
    format!("{{uint64_t left[2]={{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},right[2]={{c->vregs[{rhs}][0],c->vregs[{rhs}][1]}},output[2]={{0,0}};for(unsigned lane=0;lane<{};lane++){{unsigned bit=lane*{bits},half=bit/64,shift=bit%64;uint64_t ua=(left[half]>>shift)&UINT64_C({mask}),ub=(right[half]>>shift)&UINT64_C({mask});{calculation}output[half]|=(value&UINT64_C({mask}))<<shift;}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}
",total/bytes)
}

pub fn emit_mla(op: mkw_arm64::vector_integer::MultiplyAccumulate) -> String {
    let mkw_arm64::vector_integer::MultiplyAccumulate {
        bytes,
        total,
        dst,
        lhs,
        rhs,
        rhs_lane,
    } = op;
    let bits = bytes * 8;
    let mask = mask(bytes);
    let right = if let Some(lane) = rhs_lane {
        let offset = u32::from(lane) * u32::from(bits);
        format!("(right[{}]>>{})", offset / 64, offset % 64)
    } else {
        "(right[half]>>shift)".into()
    };
    format!("{{uint64_t left[2]={{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},right[2]={{c->vregs[{rhs}][0],c->vregs[{rhs}][1]}},old[2]={{c->vregs[{dst}][0],c->vregs[{dst}][1]}},output[2]={{0,0}};for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t a=(left[half]>>shift)&UINT64_C({mask}),b={right}&UINT64_C({mask}),prior=(old[half]>>shift)&UINT64_C({mask});output[half]|=((prior+a*b)&UINT64_C({mask}))<<shift;}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n",total/bytes)
}

pub fn emit_pairwise_minmax(op: mkw_arm64::vector_integer::PairwiseMinMax) -> String {
    let mkw_arm64::vector_integer::PairwiseMinMax {
        bytes,
        total,
        dst,
        lhs,
        rhs,
        signed,
        minimum,
    } = op;
    let bits = bytes * 8;
    let mask = mask(bytes);
    let sign = 1u64 << (bits - 1);
    let pairs = total / bytes / 2;
    let compare = if signed {
        format!("int64_t a=(int64_t)(ua^UINT64_C({sign}))-(int64_t)UINT64_C({sign}),b=(int64_t)(ub^UINT64_C({sign}))-(int64_t)UINT64_C({sign});")
    } else {
        "uint64_t a=ua,b=ub;".into()
    };
    format!("{{uint64_t left[2]={{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},right[2]={{c->vregs[{rhs}][0],c->vregs[{rhs}][1]}},output[2]={{0,0}};for(unsigned lane=0;lane<{};lane++){{const uint64_t *source=lane<{pairs}?left:right;unsigned pair=(lane%{pairs})*{bits}*2,offset=lane*{bits};uint64_t ua=(source[pair/64]>>(pair%64))&UINT64_C({mask}),ub=(source[(pair+{bits})/64]>>((pair+{bits})%64))&UINT64_C({mask});{compare}uint64_t value=a{}b?ua:ub;output[offset/64]|=value<<(offset%64);}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n",total/bytes,if minimum{"<"}else{">"})
}
pub fn emit_pairwise_long_sum(op: mkw_arm64::vector_integer::PairwiseLongSum) -> String {
    let mkw_arm64::vector_integer::PairwiseLongSum {
        bytes,
        total,
        dst,
        src,
        signed,
    } = op;
    let bits = bytes * 8;
    let input_mask = mask(bytes);
    let output_mask = mask(bytes * 2);
    let sign = 1u64 << (bits - 1);
    let convert = if signed {
        format!("int64_t a=(int64_t)(ua^UINT64_C({sign}))-(int64_t)UINT64_C({sign}),b=(int64_t)(ub^UINT64_C({sign}))-(int64_t)UINT64_C({sign});")
    } else {
        "int64_t a=(int64_t)ua,b=(int64_t)ub;".into()
    };
    format!("{{uint64_t source[2]={{c->vregs[{src}][0],c->vregs[{src}][1]}},output[2]={{0,0}};for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits}*2,half=offset/64,shift=offset%64;uint64_t ua=(source[half]>>shift)&UINT64_C({input_mask}),ub=(source[half]>>(shift+{bits}))&UINT64_C({input_mask});{convert}output[half]|=((uint64_t)(a+b)&UINT64_C({output_mask}))<<shift;}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n",total/bytes/2)
}
pub fn emit_saturating_immediate(
    op: mkw_arm64::vector_integer::SaturatingShiftImmediate,
) -> String {
    let mkw_arm64::vector_integer::SaturatingShiftImmediate {
        bytes,
        total,
        dst,
        src,
        signed,
        amount,
    } = op;
    let bits = bytes * 8;
    let mask = mask(bytes);
    let sign = 1u64 << (bits - 1);
    let input = if signed {
        format!("(__int128)(raw^UINT64_C({sign}))-(__int128)UINT64_C({sign})")
    } else {
        "(__int128)raw".into()
    };
    let minimum = if signed {
        format!("-(((__int128)1)<<{})", bits - 1)
    } else {
        "0".into()
    };
    let maximum = format!(
        "(((__int128)1)<<{})-1",
        if signed { bits - 1 } else { bits }
    );
    format!("{{uint64_t source[2]={{c->vregs[{src}][0],c->vregs[{src}][1]}},output[2]={{0,0}};uint32_t saturated=0;for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits},half=offset/64,shift=offset%64;uint64_t raw=(source[half]>>shift)&UINT64_C({mask});__int128 value=({input})*(((__int128)1)<<{amount}),low={minimum},high={maximum};if(value<low){{value=low;saturated=1;}}if(value>high){{value=high;saturated=1;}}output[half]|=((uint64_t)value&UINT64_C({mask}))<<shift;}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];if(saturated)c->fpsr|=UINT32_C(0x08000000);}}\n",total/bytes)
}

pub fn emit_polynomial(v: mkw_arm64::vector_integer::PolynomialMultiply) -> String {
    let half = u8::from(v.upper);
    format!("{{uint64_t a=c->vregs[{}][{half}],b=c->vregs[{}][{half}];__uint128_t product=0;for(unsigned bit=0;bit<64;bit++){{if((b>>bit)&1)product^=(__uint128_t)a<<bit;}}c->vregs[{}][0]=(uint64_t)product;c->vregs[{}][1]=(uint64_t)(product>>64);}}\n",v.lhs,v.rhs,v.dst,v.dst)
}
pub fn emit_widen_single(dst: u8, src: u8, upper: bool) -> String {
    let half = u8::from(upper);
    format!("{{uint64_t source=c->vregs[{src}][{half}],output[2]={{0,0}};uint32_t fpsr=c->fpsr;if(!c->floating){{c->fault_status=3;return 3;}}for(unsigned lane=0;lane<2;lane++){{int status=c->floating(14,4,c->fpcr,fpsr,(source>>(lane*32))&UINT64_C(0xffffffff),0,0,&output[lane],&fpsr);if(status){{c->fault_status=status;return status;}}}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];c->fpsr=fpsr;}}\n")
}

pub fn emit_unsigned_abs(v: mkw_arm64::vector_integer::UnsignedAbsDifference) -> String {
    let bits = v.bytes * 8;
    let width = if v.long { bits * 2 } else { bits };
    let count = if v.long {
        8 / v.bytes
    } else {
        v.total / v.bytes
    };
    let start = if v.upper { 64 } else { 0 };
    let mask = (1u64 << bits) - 1;
    format!("{{uint64_t a[2]={{c->vregs[{}][0],c->vregs[{}][1]}},b[2]={{c->vregs[{}][0],c->vregs[{}][1]}},output[2]={{0,0}};for(unsigned lane=0;lane<{count};lane++){{unsigned input={start}+lane*{bits},dest=lane*{width};uint64_t left=(a[input/64]>>(input%64))&UINT64_C({mask}),right=(b[input/64]>>(input%64))&UINT64_C({mask});uint64_t result=left>=right?left-right:right-left;output[dest/64]|=result<<(dest%64);}}c->vregs[{}][0]=output[0];c->vregs[{}][1]=output[1];}}\n",v.lhs,v.lhs,v.rhs,v.rhs,v.dst,v.dst)
}

pub fn emit_hash(v: mkw_arm64::vector_integer::SecureHash) -> String {
    use mkw_arm64::vector_integer::HashKind::*;
    let mut s=format!("{{uint32_t d[4],n[4],m[4],output[4]={{0,0,0,0}};for(unsigned lane=0;lane<4;lane++){{d[lane]=(uint32_t)(c->vregs[{}][lane/2]>>((lane%2)*32));n[lane]=(uint32_t)(c->vregs[{}][lane/2]>>((lane%2)*32));m[lane]=(uint32_t)(c->vregs[{}][lane/2]>>((lane%2)*32));}}(void)d;(void)n;(void)m;",v.dst,v.lhs,v.rhs);
    match v.kind {
        Sha1Rotate=>s+="output[0]=(n[0]>>2)|(n[0]<<30);",
        Sha1Schedule0=>s+="output[0]=d[0]^d[2]^m[0];output[1]=d[1]^d[3]^m[1];output[2]=d[2]^n[0]^m[2];output[3]=d[3]^n[1]^m[3];",
        Sha1Schedule1=>s+="for(unsigned lane=0;lane<4;lane++){uint32_t value=d[lane]^(lane<3?n[lane+1]:output[0]);output[lane]=(value<<1)|(value>>31);}",
        Sha1Choose|Sha1Parity|Sha1Majority=>{
            let f=match v.kind{Sha1Choose=>"(b&third)^(~b&e)",Sha1Parity=>"b^third^e",Sha1Majority=>"(b&third)^(b&e)^(third&e)",_=>unreachable!()};
            s+=&format!("uint32_t a=d[0],b=d[1],third=d[2],e=d[3],last=n[0];for(unsigned round=0;round<4;round++){{uint32_t temp=((a<<5)|(a>>27))+({f})+last+m[round];last=e;e=third;third=(b>>2)|(b<<30);b=a;a=temp;}}output[0]=a;output[1]=b;output[2]=third;output[3]=e;");
        }
        Sha256Schedule0=>s+="for(unsigned lane=0;lane<4;lane++){uint32_t value=lane<3?d[lane+1]:n[0];uint32_t sigma=((value>>7)|(value<<25))^((value>>18)|(value<<14))^(value>>3);output[lane]=d[lane]+sigma;}",
        Sha256Schedule1=>s+="for(unsigned lane=0;lane<4;lane++){uint32_t value=lane<2?m[lane+2]:output[lane-2];uint32_t sigma=((value>>17)|(value<<15))^((value>>19)|(value<<13))^(value>>10);output[lane]=d[lane]+(lane<3?n[lane+1]:m[0])+sigma;}",
        Sha256Hash|Sha256Hash2=>{
            let(a,b)=if v.kind==Sha256Hash{("d","n")}else{("n","d")};
            s+=&format!("uint32_t state[8]={{{a}[0],{a}[1],{a}[2],{a}[3],{b}[0],{b}[1],{b}[2],{b}[3]}};for(unsigned round=0;round<4;round++){{uint32_t a=state[0],b=state[1],c=state[2],e=state[4],f=state[5],g=state[6];uint32_t sigma1=((e>>6)|(e<<26))^((e>>11)|(e<<21))^((e>>25)|(e<<7)),sigma0=((a>>2)|(a<<30))^((a>>13)|(a<<19))^((a>>22)|(a<<10));uint32_t t1=state[7]+sigma1+((e&f)^(~e&g))+m[round],t2=sigma0+((a&b)^(a&c)^(b&c)),next_e=state[3]+t1;for(unsigned i=7;i>0;i--)state[i]=state[i-1];state[0]=t1+t2;state[4]=next_e;}}for(unsigned lane=0;lane<4;lane++)output[lane]=state[lane+{}];",if v.kind==Sha256Hash{0}else{4});
        }
    }
    s+=&format!("c->vregs[{}][0]=(uint64_t)output[0]|((uint64_t)output[1]<<32);c->vregs[{}][1]=(uint64_t)output[2]|((uint64_t)output[3]<<32);}}\n",v.dst,v.dst);
    s
}

pub fn emit_rounded_register(v: mkw_arm64::vector_integer::RoundedRegisterShift) -> String {
    let bits = v.bytes * 8;
    let m = mask(v.bytes);
    let sign = 1u64 << (bits - 1);
    let saturation = if v.saturating {
        format!("__int128 value=(__int128)(raw^UINT64_C({sign}))-((__int128)1<<{}),minimum=-((__int128)1<<{}),maximum=((__int128)1<<{})-1,wide;if(count>={bits}){{wide=value<0?minimum:value>0?maximum:0;if(value)fpsr|=UINT32_C(0x08000000);}}else{{wide=value*((__int128)1<<count);if(wide<minimum){{wide=minimum;fpsr|=UINT32_C(0x08000000);}}if(wide>maximum){{wide=maximum;fpsr|=UINT32_C(0x08000000);}}}}result=(uint64_t)wide&UINT64_C({m});",bits-1,bits-1,bits-1)
    } else {
        format!("result=count>={bits}?0:(raw<<count)&UINT64_C({m});")
    };
    format!("{{uint64_t a[2]={{c->vregs[{}][0],c->vregs[{}][1]}},b[2]={{c->vregs[{}][0],c->vregs[{}][1]}},out[2]={{0,0}};uint32_t fpsr=c->fpsr;for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{bits};uint64_t raw=(a[offset/64]>>(offset%64))&UINT64_C({m}),count_raw=b[offset/64]>>(offset%64),result=0;int count=(int)(count_raw&255);if(count>=128)count-=256;if(count>=0){{{saturation}}}else{{unsigned amount=(unsigned)(-count);if(amount<{bits}){{uint64_t extension=(raw&UINT64_C({sign}))?(UINT64_C({m})^((UINT64_C(1)<<({bits}-amount))-1)):0;result=((raw>>amount)|extension)+((raw>>(amount-1))&1);result&=UINT64_C({m});}}}}out[offset/64]|=result<<(offset%64);}}c->vregs[{}][0]=out[0];c->vregs[{}][1]=out[1];c->fpsr=fpsr;}}\n",v.lhs,v.lhs,v.rhs,v.rhs,16/v.bytes,v.dst,v.dst)
}
pub fn emit_rounded_narrow(v: mkw_arm64::vector_integer::RoundedShiftNarrow) -> String {
    let input_bits = v.bytes * 16;
    let bits = v.bytes * 8;
    let input_mask = mask(v.bytes * 2);
    let out_mask = mask(v.bytes);
    let start = if v.upper { 64 } else { 0 };
    let low = if v.upper {
        format!("c->vregs[{}][0]", v.dst)
    } else {
        "0".into()
    };
    format!("{{uint64_t source[2]={{c->vregs[{}][0],c->vregs[{}][1]}},out[2]={{{low},0}};for(unsigned lane=0;lane<{};lane++){{unsigned offset=lane*{input_bits},dest={start}+lane*{bits};uint64_t raw=(source[offset/64]>>(offset%64))&UINT64_C({input_mask});__uint128_t rounded=((__uint128_t)raw+((__uint128_t)1<<({}-1)))>>{};out[dest/64]|=((uint64_t)rounded&UINT64_C({out_mask}))<<(dest%64);}}c->vregs[{}][0]=out[0];c->vregs[{}][1]=out[1];}}\n",v.src,v.src,8/v.bytes,v.amount,v.amount,v.dst,v.dst)
}
pub fn emit_long_pair_accumulate(dst: u8, src: u8) -> String {
    format!("{{uint64_t source[2]={{c->vregs[{src}][0],c->vregs[{src}][1]}},old[2]={{c->vregs[{dst}][0],c->vregs[{dst}][1]}},out[2]={{0,0}};for(unsigned lane=0;lane<4;lane++){{unsigned offset=lane*32;uint64_t raw=source[offset/64]>>(offset%64);int64_t a=(int64_t)((raw&65535)^32768)-32768,b=(int64_t)(((raw>>16)&65535)^32768)-32768;uint64_t value=((old[offset/64]>>(offset%64))+(uint64_t)(a+b))&UINT64_C(0xffffffff);out[offset/64]|=value<<(offset%64);}}c->vregs[{dst}][0]=out[0];c->vregs[{dst}][1]=out[1];}}\n")
}
pub fn emit_saturating_long(v: mkw_arm64::vector_integer::SaturatingLongAccumulate) -> String {
    let bits = v.bytes * 8;
    let wide_bits = bits * 2;
    let m = mask(v.bytes);
    let wm = mask(v.bytes * 2);
    let sign = 1u64 << (bits - 1);
    let ws = 1u64 << (wide_bits - 1);
    let start = if v.upper { 64 } else { 0 };
    let right = if let Some(lane) = v.rhs_lane {
        let offset = u32::from(lane) * u32::from(bits);
        format!("(b[{}]>>{})", offset / 64, offset % 64)
    } else {
        "(b[offset/64]>>(offset%64))".into()
    };
    format!("{{uint64_t a[2]={{c->vregs[{}][0],c->vregs[{}][1]}},b[2]={{c->vregs[{}][0],c->vregs[{}][1]}},old[2]={{c->vregs[{}][0],c->vregs[{}][1]}},out[2]={{0,0}};uint32_t fpsr=c->fpsr;for(unsigned lane=0;lane<{};lane++){{unsigned offset={start}+lane*{bits},dest=lane*{wide_bits};uint64_t ar=(a[offset/64]>>(offset%64))&UINT64_C({m}),br={right}&UINT64_C({m}),cr=(old[dest/64]>>(dest%64))&UINT64_C({wm});__int128 left=(__int128)(ar^UINT64_C({sign}))-((__int128)1<<{}),rhs=(__int128)(br^UINT64_C({sign}))-((__int128)1<<{}),acc=(__int128)(cr^UINT64_C({ws}))-((__int128)1<<{}),minimum=-((__int128)1<<{}),maximum=((__int128)1<<{})-1,value=left*rhs*2;if(value>maximum){{value=maximum;fpsr|=UINT32_C(0x08000000);}}if(value<minimum){{value=minimum;fpsr|=UINT32_C(0x08000000);}}value+=acc;if(value>maximum){{value=maximum;fpsr|=UINT32_C(0x08000000);}}if(value<minimum){{value=minimum;fpsr|=UINT32_C(0x08000000);}}out[dest/64]|=((uint64_t)value&UINT64_C({wm}))<<(dest%64);}}c->vregs[{}][0]=out[0];c->vregs[{}][1]=out[1];c->fpsr=fpsr;}}\n",v.lhs,v.lhs,v.rhs,v.rhs,v.dst,v.dst,8/v.bytes,bits-1,bits-1,wide_bits-1,wide_bits-1,wide_bits-1,v.dst,v.dst)
}
