//! SIMD lane/replicated transfers and multiple-register structures.
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
    pub registers: u8,
    pub load: bool,
    pub bytes: u8,
    pub vector: u8,
    pub base: Reg,
    pub lane: u8,
    /// Zero for a lane transfer, otherwise the broadcast destination byte count.
    pub replicate: u8,
    pub writeback: Writeback,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Interleaved {
    pub load: bool,
    pub bytes: u8,
    pub vector_bytes: u8,
    pub registers: u8,
    pub interleaved: bool,
    pub vector: u8,
    pub base: Reg,
    pub writeback: Writeback,
}
pub fn decode(w: u32) -> Option<Result<Op, String>> {
    if w & 0xbf000000 == 0x0c000000 {
        let post = w & (1 << 23) != 0;
        let load = w & (1 << 22) != 0;
        let q = w & (1 << 30) != 0;
        let rm = (w >> 16) & 31;
        let size = ((w >> 10) & 3) as u8;
        let (registers, interleaved) = match (w >> 12) & 15 {
            0 => (4, true),
            2 => (4, false),
            4 => (3, true),
            6 => (3, false),
            7 => (1, false),
            8 => (2, true),
            10 => (2, false),
            _ => return Some(Err("reserved multiple structure opcode".into())),
        };
        if w & (1 << 21) != 0 {
            return Some(Err("unsupported multiple structure opcode".into()));
        }
        if (!post && rm != 0) || (size == 3 && !q && interleaved) {
            return Some(Err(
                "reserved interleaved structure arrangement/register".into()
            ));
        }
        let vector_bytes = if q { 16 } else { 8 };
        return Some(Ok(Op::InterleavedStructure(Interleaved {
            load,
            bytes: 1 << size,
            vector_bytes,
            registers,
            interleaved,
            vector: (w & 31) as u8,
            base: reg((w >> 5) & 31, true),
            writeback: if !post {
                Writeback::None
            } else if rm == 31 {
                Writeback::Immediate(vector_bytes * registers)
            } else {
                Writeback::Register(reg(rm, false))
            },
        })));
    }
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
    let registers = 1 + (((opcode & 1) as u8) << 1) + (((w >> 21) & 1) as u8);
    if !post && rm != 0 {
        return invalid("reserved nonpost structure register");
    }
    let (bytes, lane, replicate) = match opcode & 6 {
        0 => (1, q * 8 + s * 4 + size, 0),
        2 if size & 1 == 0 => (2, q * 4 + s * 2 + size / 2, 0),
        4 if size == 0 => (4, q * 2 + s, 0),
        4 if size == 1 && s == 0 => (8, q, 0),
        6 if load && s == 0 => (1 << size, 0, 8 << q),
        _ => return invalid("reserved single structure element/replication"),
    };
    Some(Ok(Op::Structure(Structure {
        registers,
        load,
        bytes,
        lane,
        replicate,
        vector: (w & 31) as u8,
        base: reg((w >> 5) & 31, true),
        writeback: if !post {
            Writeback::None
        } else if rm == 31 {
            Writeback::Immediate(bytes * registers)
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
            0x0d408820, 0x0d409420, 0x0d404420, 0x0d40d020, 0x0d00c020, 0x0d418020,
        ] {
            assert!(crate::decode(w, 0).is_err(), "{w:x}");
        }
        for (word, count) in [(0x0d608020, 2), (0x0d40a020, 3)] {
            let Op::Structure(v) = crate::decode(word, 0).unwrap().op else {
                panic!()
            };
            assert_eq!(v.registers, count);
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

#[cfg(test)]
mod interleaved_tests {
    use super::*;
    #[test]
    fn two_structure_fields_and_reserved_forms() {
        for q in 0..2 {
            for size in 0..4 {
                for load in 0..2 {
                    for post in 0..2 {
                        let w = 0x0c0083ff
                            | (q << 30)
                            | (size << 10)
                            | (load << 22)
                            | (post << 23)
                            | ((if post != 0 { 31 } else { 0 }) << 16);
                        if q == 0 && size == 3 {
                            assert!(crate::decode(w, 0).is_err());
                            continue;
                        }
                        let Op::InterleavedStructure(v) = crate::decode(w, 0).unwrap().op else {
                            panic!()
                        };
                        assert_eq!(
                            (v.bytes, v.vector_bytes, v.vector, v.base, v.load),
                            (1 << size, 8 << q, 31, Reg::Sp, load != 0)
                        );
                        assert_eq!(
                            v.writeback,
                            if post != 0 {
                                Writeback::Immediate(16 << q)
                            } else {
                                Writeback::None
                            }
                        );
                    }
                }
            }
        }
        let Op::InterleavedStructure(v) = crate::decode(0x4cc18c3f, 0).unwrap().op else {
            panic!()
        };
        assert_eq!(
            (v.base, v.writeback),
            (Reg::Gpr(1), Writeback::Register(Reg::Gpr(1)))
        );
        for w in [0x0c418020, 0x0c608020, 0x4c40c020, 0x0c408c20] {
            assert!(crate::decode(w, 0).is_err(), "{w:x}");
        }
    }
}

#[cfg(test)]
mod multiple_tests {
    use super::*;
    #[test]
    fn multiple_register_counts_layouts_and_post_steps() {
        for (opcode, count, interleaved) in [
            (0, 4, true),
            (2, 4, false),
            (4, 3, true),
            (6, 3, false),
            (7, 1, false),
            (8, 2, true),
            (10, 2, false),
        ] {
            for q in 0..2 {
                for size in 0..4 {
                    let w = 0x0cdf03ff | (opcode << 12) | (q << 30) | (size << 10);
                    if interleaved && q == 0 && size == 3 {
                        assert!(crate::decode(w, 0).is_err());
                        continue;
                    }
                    let Op::InterleavedStructure(v) = crate::decode(w, 0).unwrap().op else {
                        panic!()
                    };
                    assert_eq!(
                        (v.registers, v.interleaved, v.writeback),
                        (count, interleaved, Writeback::Immediate((8 << q) * count))
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod multi_lane_tests {
    use super::*;
    #[test]
    fn lane_and_replicated_counts_and_poststeps() {
        for count in 1u32..=4 {
            for q in 0..2 {
                for size in 0..4 {
                    let w = 0x0ddfc3ff
                        | (q << 30)
                        | (size << 10)
                        | (((count - 1) & 1) << 21)
                        | (((count - 1) >> 1) << 13);
                    let Op::Structure(v) = crate::decode(w, 0).unwrap().op else {
                        panic!()
                    };
                    assert_eq!(
                        (v.registers, v.replicate, v.writeback),
                        (
                            count as u8,
                            8 << q,
                            Writeback::Immediate((1 << size) * count as u8)
                        )
                    );
                }
            }
        }
    }
}
