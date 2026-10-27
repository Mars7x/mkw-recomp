//! Single-register SIMD lane transfers and replicated loads.
use super::{reg, Op, Reg};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Writeback {
    None,
    Immediate(u8),
    Register(Reg),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Structure {
    pub load: bool,
    pub bytes: u8,
    pub vector: u8,
    pub base: Reg,
    pub lane: u8,
    /// Zero for a lane transfer, otherwise the broadcast destination byte count.
    pub replicate: u8,
    pub writeback: Writeback,
}
pub fn decode(w: u32) -> Option<Result<Op, String>> {
    if w & 0xbf000000 != 0x0d000000 {
        return None;
    }
    let invalid = |reason: &str| Some(Err(reason.to_owned()));
    let post = w & (1 << 23) != 0;
    let load = w & (1 << 22) != 0;
    let q = ((w >> 30) & 1) as u8;
    let rm = (w >> 16) & 31;
    let opcode = (w >> 13) & 7;
    let s = ((w >> 12) & 1) as u8;
    let size = ((w >> 10) & 3) as u8;
    if w & (1 << 21) != 0 || opcode & 1 != 0 {
        return invalid("unsupported multiple-register single structure");
    }
    if !post && rm != 0 {
        return invalid("reserved nonpost structure register");
    }
    let (bytes, lane, replicate) = match opcode {
        0 => (1, q * 8 + s * 4 + size, 0),
        2 if size & 1 == 0 => (2, q * 4 + s * 2 + size / 2, 0),
        4 if size == 0 => (4, q * 2 + s, 0),
        4 if size == 1 && s == 0 => (8, q, 0),
        6 if load && s == 0 => (1 << size, 0, 8 << q),
        _ => return invalid("reserved single structure element/replication"),
    };
    Some(Ok(Op::Structure(Structure {
        load,
        bytes,
        lane,
        replicate,
        vector: (w & 31) as u8,
        base: reg((w >> 5) & 31, true),
        writeback: if !post {
            Writeback::None
        } else if rm == 31 {
            Writeback::Immediate(bytes)
        } else {
            Writeback::Register(reg(rm, false))
        },
    })))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lane_bounds_replication_and_reserved_forms() {
        for (w, bytes, lane, replicate) in [
            (0x0d408020, 4, 0, 0),
            (0x4d409020, 4, 3, 0),
            (0x4d408420, 8, 1, 0),
            (0x4d405020, 2, 6, 0),
            (0x4d401c20, 1, 15, 0),
            (0x0d40cc20, 8, 0, 8),
            (0x4d40c820, 4, 0, 16),
        ] {
            let Op::Structure(v) = crate::decode(w, 0).unwrap().op else {
                panic!()
            };
            assert_eq!((v.bytes, v.lane, v.replicate), (bytes, lane, replicate));
        }
        for w in [
            0x0d608020, 0x0d40a020, 0x0d408820, 0x0d409420, 0x0d404420, 0x0d40d020, 0x0d00c020,
            0x0d418020,
        ] {
            assert!(crate::decode(w, 0).is_err(), "{w:x}");
        }
        let Op::Structure(v) = crate::decode(0x4ddf83ff, 0).unwrap().op else {
            panic!()
        };
        assert_eq!(
            (v.base, v.vector, v.writeback),
            (Reg::Sp, 31, Writeback::Immediate(4))
        );
    }
}
