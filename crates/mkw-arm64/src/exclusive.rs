//! Single-register exclusive decoding for an explicit future v10 profile.
//! Not called by the default decoder. Pair forms and constrained-unpredictable
//! store status/data or status/base overlaps refuse instead of selecting a value.
use super::{reg, Reg};
use serde::Serialize;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Exclusive {
    Load {
        bytes: u8,
        acquire: bool,
        base: Reg,
        dst: Reg,
    },
    Store {
        bytes: u8,
        release: bool,
        base: Reg,
        src: Reg,
        status: Reg,
    },
    Clear {
        immediate: u8,
    },
}
pub fn decode(w: u32) -> Option<Exclusive> {
    if w & 0xfffff0ff == 0xd503305f {
        return Some(Exclusive::Clear {
            immediate: ((w >> 8) & 15) as u8,
        });
    }
    let bytes = 1 << (w >> 30);
    let base = reg((w >> 5) & 31, true);
    let data = reg(w & 31, false);
    if w & 0x3fff7c00 == 0x085f7c00 {
        return Some(Exclusive::Load {
            bytes,
            acquire: w & (1 << 15) != 0,
            base,
            dst: data,
        });
    }
    if w & 0x3fe07c00 != 0x08007c00 {
        return None;
    }
    let s = (w >> 16) & 31;
    let n = (w >> 5) & 31;
    let t = w & 31;
    if s == t || (s == n && n != 31) {
        return None;
    }
    Some(Exclusive::Store {
        bytes,
        release: w & (1 << 15) != 0,
        base,
        src: data,
        status: reg(s, false),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_single_register_width_order_and_alias_is_explicit() {
        for size in 0..4 {
            for ordered in 0..2 {
                for n in 0..32 {
                    for t in 0..32 {
                        let word = 0x085f7c00 | (size << 30) | (ordered << 15) | (n << 5) | t;
                        assert_eq!(
                            decode(word),
                            Some(Exclusive::Load {
                                bytes: 1 << size,
                                acquire: ordered != 0,
                                base: reg(n, true),
                                dst: reg(t, false)
                            })
                        );
                        assert!(crate::decode(word, 0).is_err());
                        for s in 0..32 {
                            let word = 0x08007c00
                                | (size << 30)
                                | (ordered << 15)
                                | (s << 16)
                                | (n << 5)
                                | t;
                            let expected = if s == t || (s == n && n != 31) {
                                None
                            } else {
                                Some(Exclusive::Store {
                                    bytes: 1 << size,
                                    release: ordered != 0,
                                    base: reg(n, true),
                                    src: reg(t, false),
                                    status: reg(s, false),
                                })
                            };
                            assert_eq!(decode(word), expected);
                            assert!(crate::decode(word, 0).is_err());
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn clear_immediate_preserved_and_neighbours_refuse() {
        for immediate in 0..16 {
            let word = 0xd503305f | (immediate << 8);
            assert_eq!(
                decode(word),
                Some(Exclusive::Clear {
                    immediate: immediate as u8
                })
            );
            assert!(crate::decode(word, 0).is_err());
            for bit in (0..8).chain(12..32) {
                assert!(decode(word ^ (1 << bit)).is_none());
            }
        }
        let program = [0xd5033f5fu32, 0xd65f03c0]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        assert!(crate::lift_exclusive_v10(&program, 0, 0).is_ok());
        assert!(crate::lift(&program, 0, 0).is_err());
        assert!(crate::lift_exclusive_v10(&program[..4], 0, 0).is_err());
        assert!(crate::lift_exclusive_v10(&program, 1, 1).is_err());
        for tail in [0u32, 0x54000040, 0x14000002] {
            let p = [0xd5033f5fu32, tail]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect::<Vec<_>>();
            assert!(crate::lift_exclusive_v10(&p, 0, 0).is_err());
        }
        let p = [0xd5033f5fu32, 0x14000002]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        assert!(crate::lift_function_exclusive_v10(&p, 0, 0).is_ok());
        for word in [
            0x887f0040, 0xc87f0040, 0x88200040, 0xc8200040, 0x889ffc00, 0xc8dffc00, 0xb8e10040,
            0xc8e0fc41, 0xd5033bbf, 0xd5033fdf,
        ] {
            assert!(decode(word).is_none(), "{word:08x}");
        }
    }
    #[test]
    fn fixed_single_fields_do_not_admit_pair_ordered_or_reserved_forms() {
        for canonical in [0x085f7c41, 0xc85ffc41] {
            for bit in (16..30).filter(|&bit| bit != 22).chain(10..15) {
                assert!(decode(canonical ^ (1 << bit)).is_none(), "{bit}");
            }
        }
        for bit in (21..30).chain(10..15) {
            assert!(decode(0xc8007c41 ^ (1 << bit)).is_none(), "{bit}");
        }
    }
}
