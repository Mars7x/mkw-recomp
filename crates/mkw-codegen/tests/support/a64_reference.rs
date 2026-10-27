//! Independently authored synthetic ELF construction, without an assembler or game code.
//! QEMU executes these fixtures only as an optional software reference oracle.
#![allow(dead_code)]
fn put16(bytes: &mut [u8], at: usize, v: u16) {
    bytes[at..at + 2].copy_from_slice(&v.to_le_bytes());
}
fn put32(bytes: &mut [u8], at: usize, v: u32) {
    bytes[at..at + 4].copy_from_slice(&v.to_le_bytes());
}
fn put64(bytes: &mut [u8], at: usize, v: u64) {
    bytes[at..at + 8].copy_from_slice(&v.to_le_bytes());
}
fn immediate(words: &mut Vec<u32>, reg: u32, value: u64) {
    for slot in 0..4 {
        words.push(
            (if slot == 0 { 0xd2800000 } else { 0xf2800000 })
                | (slot << 21)
                | ((((value >> (slot * 16)) & 65535) as u32) << 5)
                | reg,
        );
    }
}
pub const INITIAL_X: [u64; 3] = [0xfedcba9876543210, 0x89abcdefdead8031, 0x0123456789abcdef];
pub const INITIAL_V: [[u64; 2]; 4] = [
    [0x0123456789abcdef, 0xfedcba9876543210],
    [0x80ff7f0181feaa55, 0x102030408090a0b0],
    [0x1111222233334444, 0xaaaabbbbccccdddd],
    [0xff00810080017ffe, 0x7654321089abcdef],
];
pub fn elf(cases: &[u32]) -> Vec<u8> {
    build_elf(cases, false)
}
pub fn atomic_elf(cases: &[u32]) -> Vec<u8> {
    build_elf(cases, true)
}
fn build_elf(cases: &[u32], atomic: bool) -> Vec<u8> {
    const BASE: u64 = 0x400000;
    const TEXT: usize = 4096;
    let mut code = Vec::new();
    immediate(&mut code, 28, 0); // patched output pointer
    let mut loads = Vec::new();
    for &word in cases {
        for (index, reg) in [0u32, 1, 2, 31].into_iter().enumerate() {
            loads.push((code.len(), index * 16, 0x9c000000 | reg));
            code.push(0);
        }
        for reg in 0..3 {
            loads.push((code.len(), 64 + reg * 8, 0x58000000 | reg as u32));
            code.push(0);
        }
        immediate(&mut code, 27, 99);
        code.push(0x9100037f); // ADD SP,X27,#0
        immediate(&mut code, 27, 0xb0000000);
        code.push(0xd51b421b); // MSR NZCV,X27
        if atomic {
            loads.push((code.len(), 64, 0x58000019));
            code.push(0); // initial word
            code.push(0xf9000059); // STR X25,[X2]
        }
        code.push(word);
        if atomic {
            code.push(0xf9400059);
            code.push(0x9e670320);
        } // memory -> D0
        code.push(0x3d800380); // STR Q0,[X28]
        code.push(0xf9000b80); // STR X0,[X28,#16]
        code.push(0xd53b421b); // MRS X27,NZCV
        code.push(0xf9000f9b); // STR X27,[X28,#24]
        code.push(0x9100839c); // ADD X28,X28,#32
    }
    let write_at = code.len();
    immediate(&mut code, 0, 1);
    immediate(&mut code, 1, 0);
    immediate(&mut code, 2, cases.len() as u64 * 32);
    immediate(&mut code, 8, 64);
    code.push(0xd4000001);
    immediate(&mut code, 0, 0);
    immediate(&mut code, 8, 93);
    code.push(0xd4000001);
    let pool = TEXT + code.len() * 4;
    let output = (pool + 96 + 15) & !15;
    let output_va = BASE + output as u64;
    let mut pointer = Vec::new();
    immediate(&mut pointer, 28, output_va);
    code[..4].copy_from_slice(&pointer);
    pointer.clear();
    immediate(&mut pointer, 1, output_va);
    code[write_at + 4..write_at + 8].copy_from_slice(&pointer);
    for (at, offset, opcode) in loads {
        let delta = (pool + offset) as i64 - (TEXT + at * 4) as i64;
        assert!(delta.abs() < 1024 * 1024);
        assert_eq!(delta % 4, 0);
        code[at] = opcode | (((delta / 4) as u32 & 0x7ffff) << 5);
    }
    let mut bytes = vec![0u8; output + cases.len() * 32];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    put16(&mut bytes, 16, 2);
    put16(&mut bytes, 18, 183);
    put32(&mut bytes, 20, 1);
    put64(&mut bytes, 24, BASE + TEXT as u64);
    put64(&mut bytes, 32, 64);
    put16(&mut bytes, 52, 64);
    put16(&mut bytes, 54, 56);
    put16(&mut bytes, 56, 1);
    put32(&mut bytes, 64, 1);
    put32(&mut bytes, 68, 7);
    put64(&mut bytes, 80, BASE);
    put64(&mut bytes, 88, BASE);
    let len = bytes.len() as u64;
    put64(&mut bytes, 96, len);
    put64(&mut bytes, 104, len);
    put64(&mut bytes, 112, 4096);
    for (i, word) in code.into_iter().enumerate() {
        put32(&mut bytes, TEXT + i * 4, word);
    }
    let mut initial_x = INITIAL_X;
    if atomic {
        initial_x[2] = BASE + (pool + 88) as u64;
    }
    let data = INITIAL_V.into_iter().flatten().chain(initial_x);
    for (i, value) in data.enumerate() {
        put64(&mut bytes, pool + i * 8, value);
    }
    bytes
}

pub struct FloatingCase {
    pub word: u32,
    pub left: u64,
    pub right: u64,
    pub fpcr: u32,
    pub accumulate: u64,
    pub nzcv: u32,
    pub fpsr: u32,
}
pub fn floating_elf(cases: &[FloatingCase]) -> Vec<u8> {
    const BASE: u64 = 0x400000;
    const TEXT: usize = 4096;
    let mut code = Vec::new();
    immediate(&mut code, 28, 0);
    let mut loads = Vec::new();
    for (i, case) in cases.iter().enumerate() {
        for reg in 0..4 {
            loads.push((code.len(), i * 80 + reg * 16, 0x9c000000 | reg as u32));
            code.push(0);
        }
        loads.push((code.len(), i * 80 + 64, 0x5800001b));
        code.push(0);
        code.push(0xd51b441b); // MSR FPCR
        loads.push((code.len(), i * 80 + 72, 0x5800001b));
        code.push(0);
        code.push(0xd51b443b); // MSR FPSR
        immediate(&mut code, 27, u64::from(case.nzcv));
        code.push(0xd51b421b);
        code.push(case.word);
        code.push(0x3d800380); // Q0 result
        code.push(0xd53b443b);
        code.push(0xf9000b9b); // FPSR -> output+16
        code.push(0xd53b421b);
        code.push(0xf9000f9b); // NZCV -> output+24
        code.push(0x9100839c);
    }
    let write_at = code.len();
    immediate(&mut code, 0, 1);
    immediate(&mut code, 1, 0);
    immediate(&mut code, 2, cases.len() as u64 * 32);
    immediate(&mut code, 8, 64);
    code.push(0xd4000001);
    immediate(&mut code, 0, 0);
    immediate(&mut code, 8, 93);
    code.push(0xd4000001);
    let pool = TEXT + code.len() * 4;
    let output = (pool + cases.len() * 80 + 15) & !15;
    let va = BASE + output as u64;
    let mut pointer = Vec::new();
    immediate(&mut pointer, 28, va);
    code[..4].copy_from_slice(&pointer);
    pointer.clear();
    immediate(&mut pointer, 1, va);
    code[write_at + 4..write_at + 8].copy_from_slice(&pointer);
    for (at, offset, op) in loads {
        let delta = (pool + offset) as i64 - (TEXT + at * 4) as i64;
        assert!(delta.abs() < 1024 * 1024);
        code[at] = op | (((delta / 4) as u32 & 0x7ffff) << 5);
    }
    let mut bytes = vec![0u8; output + cases.len() * 32];
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    put16(&mut bytes, 16, 2);
    put16(&mut bytes, 18, 183);
    put32(&mut bytes, 20, 1);
    put64(&mut bytes, 24, BASE + TEXT as u64);
    put64(&mut bytes, 32, 64);
    put16(&mut bytes, 52, 64);
    put16(&mut bytes, 54, 56);
    put16(&mut bytes, 56, 1);
    put32(&mut bytes, 64, 1);
    put32(&mut bytes, 68, 7);
    put64(&mut bytes, 80, BASE);
    put64(&mut bytes, 88, BASE);
    let len = bytes.len() as u64;
    put64(&mut bytes, 96, len);
    put64(&mut bytes, 104, len);
    put64(&mut bytes, 112, 4096);
    for (i, w) in code.into_iter().enumerate() {
        put32(&mut bytes, TEXT + i * 4, w);
    }
    for (i, c) in cases.iter().enumerate() {
        for (j, value) in [
            INITIAL_V[0][0],
            INITIAL_V[0][1],
            c.left,
            INITIAL_V[1][1],
            c.right,
            INITIAL_V[2][1],
            c.accumulate,
            INITIAL_V[3][1],
            u64::from(c.fpcr),
            u64::from(c.fpsr),
        ]
        .into_iter()
        .enumerate()
        {
            put64(&mut bytes, pool + i * 80 + j * 8, value);
        }
    }
    bytes
}
