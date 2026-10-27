use mkw_arm64::simd_shift::{Kind, Shift};
fn mask(bytes: u8) -> u64 {
    if bytes == 8 {
        u64::MAX
    } else {
        (1u64 << (bytes * 8)) - 1
    }
}
pub fn emit(op: Shift) -> String {
    let bits = op.bytes * 8;
    let input_mask = mask(op.bytes);
    let sign = 1u64 << (bits - 1);
    let n = op.amount;
    let widen = matches!(op.kind, Kind::UnsignedWiden | Kind::SignedWiden);
    let out_bytes = if widen { op.bytes * 2 } else { op.bytes };
    let out_mask = mask(out_bytes);
    let count = if widen {
        8 / op.bytes
    } else {
        op.total / op.bytes
    };
    let start = if op.upper { 64 } else { 0 };
    let value = match op.kind {
        Kind::Left => format!("a<<{n}"),
        Kind::UnsignedRight => {
            if n == bits {
                "UINT64_C(0)".into()
            } else {
                format!("a>>{n}")
            }
        }
        Kind::SignedRight => {
            if n == bits {
                format!("(a&UINT64_C({sign}))?UINT64_C({input_mask}):UINT64_C(0)")
            } else {
                format!(
                    "(a>>{n})|((a&UINT64_C({sign}))?UINT64_C({}):UINT64_C(0))",
                    input_mask ^ (input_mask >> n)
                )
            }
        }
        Kind::InsertLeft => format!("(a<<{n})|(old&UINT64_C({}))", (1u64 << n) - 1),
        Kind::InsertRight => {
            if n == bits {
                "old".into()
            } else {
                format!(
                    "(a>>{n})|(old&UINT64_C({}))",
                    input_mask ^ (input_mask >> n)
                )
            }
        }
        Kind::UnsignedWiden => format!("a<<{n}"),
        Kind::SignedWiden => format!("((a^UINT64_C({sign}))-UINT64_C({sign}))<<{n}"),
    };
    format!("{{uint64_t source[2]={{c->vregs[{}][0],c->vregs[{}][1]}},previous[2]={{c->vregs[{}][0],c->vregs[{}][1]}},output[2]={{0,0}};for(unsigned lane=0;lane<{count};lane++){{unsigned from={start}+lane*{bits},to=lane*{};uint64_t a=(source[from/64]>>(from%64))&UINT64_C({input_mask}),old=(previous[to/64]>>(to%64))&UINT64_C({out_mask});(void)a;(void)old;uint64_t value={value};output[to/64]|=(value&UINT64_C({out_mask}))<<(to%64);}}c->vregs[{}][0]=output[0];c->vregs[{}][1]=output[1];}}\n",op.src,op.src,op.dst,op.dst,out_bytes*8,op.dst,op.dst)
}
