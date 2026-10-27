//! Read-only, bounded binary parsing. Guest addresses are module-relative, never host pointers.
pub mod capabilities;
pub mod unwind;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub type Result<T> = std::result::Result<T, String>;
pub const MAX_IMAGE: usize = 512 * 1024 * 1024;
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
pub fn sha256(b: &[u8]) -> String {
    hex(&Sha256::digest(b))
}
pub fn slice(b: &[u8], off: usize, len: usize) -> Result<&[u8]> {
    b.get(off..off.checked_add(len).ok_or("range overflow")?)
        .ok_or_else(|| format!("range {off:#x}+{len:#x} outside {} bytes", b.len()))
}
pub fn u32le(b: &[u8], o: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(slice(b, o, 4)?.try_into().unwrap()))
}
pub fn u64le(b: &[u8], o: usize) -> Result<u64> {
    Ok(u64::from_le_bytes(slice(b, o, 8)?.try_into().unwrap()))
}
fn cstr(b: &[u8], o: usize) -> Result<String> {
    let rest = b.get(o..).ok_or("string offset outside table")?;
    let n = rest
        .iter()
        .position(|x| *x == 0)
        .ok_or("unterminated string")?;
    Ok(String::from_utf8_lossy(&rest[..n]).into_owned())
}

/// Reuse the system's established LZ4 decoder; no dependency on developer headers/symlinks.
pub fn lz4_block(src: &[u8], size: usize) -> Result<Vec<u8>> {
    if size > MAX_IMAGE || src.len() > i32::MAX as usize {
        return Err("LZ4 size exceeds limit".into());
    }
    let mut dst = vec![0u8; size];
    // SAFETY: library lives through the call; ABI is LZ4's public C API. Both pointers
    // reference valid buffers of the exact signed lengths passed. The safe decoder
    // checks malformed input and never writes beyond dstCapacity.
    let n = unsafe {
        let lib = libloading::Library::new("liblz4.so.1").map_err(|e| e.to_string())?;
        let f = lib
            .get::<unsafe extern "C" fn(*const u8, *mut u8, i32, i32) -> i32>(
                b"LZ4_decompress_safe\0",
            )
            .map_err(|e| e.to_string())?;
        f(
            src.as_ptr(),
            dst.as_mut_ptr(),
            src.len() as i32,
            size as i32,
        )
    };
    if n < 0 || n as usize != size {
        return Err(format!("LZ4 produced {n}, expected {size}"));
    }
    Ok(dst)
}

/// Only ordinary Zstandard frames. Zbic is deliberately not assumed equivalent.
pub fn zstd_frame(src: &[u8], limit: usize) -> Result<Vec<u8>> {
    // SAFETY: documented Zstandard C ABI, borrowed input and owned output. Library and
    // symbols stay live for all calls. Declared output is checked before allocation.
    unsafe {
        let lib = libloading::Library::new("libzstd.so.1").map_err(|e| e.to_string())?;
        let size = lib
            .get::<unsafe extern "C" fn(*const u8, usize) -> u64>(b"ZSTD_getFrameContentSize\0")
            .map_err(|e| e.to_string())?(src.as_ptr(), src.len());
        if size > limit as u64 {
            return Err(format!(
                "zstd size unknown, invalid, or exceeds {limit}-byte inspection limit: {size}"
            ));
        }
        let mut dst = vec![0u8; size as usize];
        let n = lib
            .get::<unsafe extern "C" fn(*mut u8, usize, *const u8, usize) -> usize>(
                b"ZSTD_decompress\0",
            )
            .map_err(|e| e.to_string())?(
            dst.as_mut_ptr(), dst.len(), src.as_ptr(), src.len()
        );
        let err = lib
            .get::<unsafe extern "C" fn(usize) -> u32>(b"ZSTD_isError\0")
            .map_err(|e| e.to_string())?(n);
        if err != 0 || n != dst.len() {
            return Err(format!(
                "zstd decompression failed (dictionary, framing, corruption, or size): {n}"
            ));
        }
        Ok(dst)
    }
}

#[derive(Debug, Serialize)]
pub struct Segment {
    pub name: String,
    pub file_offset: usize,
    pub memory_offset: usize,
    pub size: usize,
    pub stored_size: usize,
    pub compression: String,
    pub permissions: String,
    pub sha256: String,
    pub embedded_hash_checked: bool,
    #[serde(skip)]
    pub bytes: Vec<u8>,
}
#[derive(Debug, Serialize)]
pub struct Symbol {
    pub name: String,
    pub value: u64,
    pub size: u64,
    pub info: u8,
    pub other: u8,
    pub section: u16,
}
#[derive(Debug, Serialize)]
pub struct Relocation {
    pub offset: u64,
    pub kind: u32,
    pub symbol: u32,
    pub addend: i64,
}
#[derive(Debug, Serialize)]
pub struct Mod0 {
    pub offset: usize,
    pub dynamic: usize,
    pub bss_start: usize,
    pub bss_end: usize,
    pub eh_start: usize,
    pub eh_end: usize,
    pub module_object: usize,
    pub tags: BTreeMap<u64, Vec<u64>>,
    pub needed: Vec<String>,
    pub symbols: Vec<Symbol>,
    pub relr_offsets: Vec<u64>,
    pub relocations: Vec<Relocation>,
    pub relocation_counts: BTreeMap<u32, usize>,
}
#[derive(Debug, Serialize)]
pub struct Nso {
    pub format: String,
    pub version: u32,
    pub flags: u32,
    pub build_id: String,
    pub sha256: String,
    pub bss_size: usize,
    pub module_name: String,
    pub segments: Vec<Segment>,
    pub mod0: Option<Mod0>,
}
impl Nso {
    pub fn parse(b: &[u8]) -> Result<Self> {
        if slice(b, 0, 4)? != b"NSO0" {
            return Err("missing NSO0 signature".into());
        }
        slice(b, 0, 256)?;
        let version = u32le(b, 4)?;
        if version != 0 {
            return Err(format!("unsupported NSO version {version}"));
        }
        let flags = u32le(b, 12)?;
        if flags & !0xff != 0 {
            return Err(format!("unknown NSO flags {flags:#x}"));
        }
        if flags & 0x80 != 0 {
            return Err("Zbic flag detected: unsupported; do not decode as ordinary zstd".into());
        }
        let mut segments: Vec<Segment> = Vec::new();
        for (i, name) in ["text", "rodata", "data"].iter().enumerate() {
            let h = 16 + i * 16;
            let off = u32le(b, h)? as usize;
            let va = u32le(b, h + 4)? as usize;
            let size = u32le(b, h + 8)? as usize;
            let compressed = flags & (1 << i) != 0;
            let stored = if compressed {
                u32le(b, 96 + i * 4)? as usize
            } else {
                size
            };
            if size > MAX_IMAGE || va.checked_add(size).ok_or("segment overflow")? > MAX_IMAGE {
                return Err("segment exceeds image limit".into());
            }
            if off < 256 {
                return Err("segment overlaps NSO header".into());
            }
            for s in &segments {
                if size != 0
                    && s.size != 0
                    && va < s.memory_offset + s.size
                    && s.memory_offset < va + size
                {
                    return Err("overlapping memory segments".into());
                }
                if stored != 0
                    && s.stored_size != 0
                    && off < s.file_offset + s.stored_size
                    && s.file_offset < off + stored
                {
                    return Err("overlapping file segments".into());
                }
            }
            let src = slice(b, off, stored)?;
            let bytes = if compressed {
                lz4_block(src, size)?
            } else {
                src.to_vec()
            };
            let hash = sha256(&bytes);
            let checked = flags & (1 << (i + 3)) != 0;
            if checked && hash != hex(slice(b, 160 + i * 32, 32)?) {
                return Err(format!("{name} hash mismatch"));
            }
            segments.push(Segment {
                name: name.to_string(),
                file_offset: off,
                memory_offset: va,
                size,
                stored_size: stored,
                compression: if compressed { "lz4-block" } else { "none" }.into(),
                permissions: match i {
                    0 => {
                        if flags & 0x40 != 0 {
                            "--X"
                        } else {
                            "R-X"
                        }
                    }
                    1 => "R--",
                    _ => "RW-",
                }
                .into(),
                sha256: hash,
                embedded_hash_checked: checked,
                bytes,
            });
        }
        let bss_size = u32le(b, 60)? as usize;
        if segments[2].memory_offset + segments[2].size + bss_size > MAX_IMAGE {
            return Err("BSS exceeds image limit".into());
        }
        let no = u32le(b, 28)? as usize;
        let ns = u32le(b, 44)? as usize;
        let module_name = if ns > 0 {
            String::from_utf8_lossy(slice(b, no, ns)?)
                .trim_end_matches('\0')
                .to_string()
        } else {
            String::new()
        };
        let mut n = Self {
            format: "NSO0".into(),
            version,
            flags,
            build_id: hex(&b[64..96]),
            sha256: sha256(b),
            bss_size,
            module_name,
            segments,
            mod0: None,
        };
        let text = &n.segments[0];
        if text.size >= 8 {
            let mo = u32le(&text.bytes, 4)? as usize;
            if mo != 0 {
                n.mod0 = Some(n.parse_mod0(mo)?)
            }
        }
        Ok(n)
    }
    pub fn read(&self, va: usize, size: usize) -> Result<&[u8]> {
        for s in &self.segments {
            if va >= s.memory_offset
                && va.checked_add(size).ok_or("VA overflow")? <= s.memory_offset + s.size
            {
                return slice(&s.bytes, va - s.memory_offset, size);
            }
        }
        Err(format!("unmapped module VA {va:#x}+{size:#x}"))
    }
    pub fn word(&self, va: usize) -> Result<u32> {
        u32le(self.read(va, 4)?, 0)
    }
    fn parse_mod0(&self, m: usize) -> Result<Mod0> {
        let b = self.read(m, 28)?;
        if &b[..4] != b"MOD0" {
            return Err(format!("MOD0 signature missing at {m:#x}"));
        }
        let rel = |i: usize| -> Result<usize> {
            let v = m as i64 + u32le(b, 4 + i * 4)? as i32 as i64;
            usize::try_from(v).map_err(|_| "negative MOD0 offset".into())
        };
        let dynamic = rel(0)?;
        let bss_start = rel(1)?;
        let bss_end = rel(2)?;
        let eh_start = rel(3)?;
        let eh_end = rel(4)?;
        if bss_start > bss_end || eh_start > eh_end {
            return Err("inverted MOD0 range".into());
        }
        self.read(eh_start, eh_end - eh_start)?;
        let mut tags: BTreeMap<u64, Vec<u64>> = BTreeMap::new();
        let mut at = dynamic;
        let mut ended = false;
        for _ in 0..65536 {
            let d = self.read(at, 16)?;
            let tag = u64le(d, 0)?;
            let value = u64le(d, 8)?;
            at += 16;
            if tag == 0 {
                ended = true;
                break;
            }
            tags.entry(tag).or_default().push(value);
        }
        if !ended {
            return Err("unterminated dynamic table".into());
        }
        let get = |t| tags.get(&t).and_then(|v| v.first()).copied();
        let strtab = if let Some(st) = get(5) {
            self.read(st as usize, get(10).ok_or("missing DT_STRSZ")? as usize)?
        } else {
            &[]
        };
        let needed = tags
            .get(&1)
            .map(|v| {
                v.iter()
                    .map(|o| cstr(strtab, *o as usize))
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();
        let mut symbols = Vec::new();
        if let Some(st) = get(6) {
            if get(11).unwrap_or(24) != 24 {
                return Err("unsupported symbol entry size".into());
            }
            let count = if let Some(h) = get(4) {
                self.word(h as usize + 4)? as usize
            } else {
                // NSO header supplies a bounded dynamic-symbol extent when DT_HASH is absent.
                return Err(
                    "symbol count unavailable without DT_HASH (GNU hash not implemented)".into(),
                );
            };
            if count > 2_000_000 {
                return Err("symbol count limit".into());
            }
            for i in 0..count {
                let s = self.read(st as usize + i * 24, 24)?;
                symbols.push(Symbol {
                    name: cstr(strtab, u32le(s, 0)? as usize)?,
                    info: s[4],
                    other: s[5],
                    section: u16::from_le_bytes([s[6], s[7]]),
                    value: u64le(s, 8)?,
                    size: u64le(s, 16)?,
                });
            }
        }
        let mut relocations = Vec::new();
        let mut relocation_counts = BTreeMap::new();
        for (tag, sz) in [(7, 8), (23, 2)] {
            if let Some(base) = get(tag) {
                let size = get(sz).ok_or("missing relocation table size")? as usize;
                if size % 24 != 0 || get(9).unwrap_or(24) != 24 {
                    return Err("invalid RELA size".into());
                }
                if tag == 23 && get(20).unwrap_or(7) != 7 {
                    return Err("PLT relocations are not RELA".into());
                }
                let data = self.read(base as usize, size)?;
                for r in data.chunks_exact(24) {
                    let info = u64le(r, 8)?;
                    let kind = info as u32;
                    *relocation_counts.entry(kind).or_default() += 1;
                    relocations.push(Relocation {
                        offset: u64le(r, 0)?,
                        kind,
                        symbol: (info >> 32) as u32,
                        addend: u64le(r, 16)? as i64,
                    });
                }
            }
        }
        let mut relr_offsets = Vec::new();
        if let Some(base) = get(36) {
            let size = get(35).ok_or("missing DT_RELRSZ")? as usize;
            if size % 8 != 0 || get(37) != Some(8) {
                return Err("invalid RELR entry size".into());
            }
            relr_offsets = decode_relr(self.read(base as usize, size)?)?;
            for &off in &relr_offsets {
                self.read(off as usize, 8)?;
            }
        }
        Ok(Mod0 {
            relr_offsets,
            offset: m,
            dynamic,
            bss_start,
            bss_end,
            eh_start,
            eh_end,
            module_object: rel(5)?,
            tags,
            needed,
            symbols,
            relocations,
            relocation_counts,
        })
    }
}

#[derive(Debug, Serialize)]
pub struct Npdm {
    pub acid_rsa_bits: u32,
    pub acid_magic_offset: usize,
    pub signature_verified: bool,
    pub service_access: Vec<String>,
    pub kernel_capabilities: Vec<u32>,
    pub format: String,
    pub flags: u8,
    pub is_64_bit: bool,
    pub address_space_type: u8,
    pub main_thread_priority: u8,
    pub main_thread_cpu: u8,
    pub system_resource_size: u32,
    pub version: u32,
    pub stack_size: u32,
    pub title_name: String,
    pub aci0_offset: u32,
    pub aci0_size: u32,
    pub acid_offset: u32,
    pub acid_size: u32,
    pub program_id: String,
    pub sha256: String,
}
impl Npdm {
    pub fn parse(b: &[u8]) -> Result<Self> {
        if slice(b, 0, 4)? != b"META" {
            return Err("missing META".into());
        }
        slice(b, 0, 128)?;
        let ao = u32le(b, 112)?;
        let az = u32le(b, 116)?;
        let co = u32le(b, 120)?;
        let cz = u32le(b, 124)?;
        let aci = slice(b, ao as usize, az as usize)?;
        if slice(aci, 0, 4)? != b"ACI0" {
            return Err("missing ACI0".into());
        }
        let acid = slice(b, co as usize, cz as usize)?;
        let (acid_magic_offset, acid_rsa_bits) = if acid.get(768..772) == Some(b"ACID") {
            (768, 3072)
        } else if acid.get(512..516) == Some(b"ACID") {
            (512, 2048)
        } else {
            return Err("missing ACID at known S1/S2 layout".into());
        };
        slice(acid, acid_magic_offset, 64)?;
        // ACI0 declares permissions; these are inspected, not cryptographically authenticated.
        let so = u32le(aci, 40)? as usize;
        let sz = u32le(aci, 44)? as usize;
        let services = slice(aci, so, sz)?;
        let mut service_access = Vec::new();
        let mut at = 0;
        while at < services.len() {
            let control = services[at];
            at += 1;
            let len = ((control & 7) + 1) as usize;
            let name = String::from_utf8_lossy(slice(services, at, len)?);
            service_access.push(format!(
                "{}:{}",
                if control & 128 != 0 {
                    "server"
                } else {
                    "client"
                },
                name
            ));
            at += len;
        }
        let ko = u32le(aci, 48)? as usize;
        let ks = u32le(aci, 52)? as usize;
        if ks % 4 != 0 {
            return Err("unaligned kernel capabilities".into());
        }
        let kernel_capabilities = slice(aci, ko, ks)?
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        Ok(Self {
            acid_rsa_bits,
            acid_magic_offset,
            signature_verified: false,
            service_access,
            kernel_capabilities,
            format: "META/ACI0/ACID".into(),
            flags: b[12],
            is_64_bit: b[12] & 1 != 0,
            address_space_type: (b[12] >> 1) & 7,
            main_thread_priority: b[14],
            main_thread_cpu: b[15],
            system_resource_size: u32le(b, 20)?,
            version: u32le(b, 24)?,
            stack_size: u32le(b, 28)?,
            title_name: String::from_utf8_lossy(&b[32..48])
                .trim_end_matches('\0')
                .into(),
            aci0_offset: ao,
            aci0_size: az,
            acid_offset: co,
            acid_size: cz,
            program_id: format!("{:016x}", u64le(aci, 16)?),
            sha256: sha256(b),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        let mut b = vec![0u8; 280];
        b[..4].copy_from_slice(b"NSO0");
        for (i, off) in [256u32, 264, 272].iter().enumerate() {
            for (at, v) in [
                (16 + i * 16, *off),
                (20 + i * 16, (i * 4096) as u32),
                (24 + i * 16, 8),
                (96 + i * 4, 8),
            ] {
                b[at..at + 4].copy_from_slice(&v.to_le_bytes())
            }
        }
        b
    }
    #[test]
    fn valid_uncompressed() {
        let n = Nso::parse(&fixture()).unwrap();
        assert_eq!(n.segments.len(), 3);
        assert_eq!(n.read(4096, 8).unwrap(), [0; 8]);
        assert!(n.read(8, 1).is_err())
    }
    #[test]
    fn truncation_is_error() {
        let b = fixture();
        for len in 0..b.len() {
            assert!(Nso::parse(&b[..len]).is_err(), "{len}")
        }
    }
    #[test]
    fn bad_hash_is_error() {
        let mut b = fixture();
        b[12] = 8;
        assert!(Nso::parse(&b).unwrap_err().contains("hash mismatch"))
    }
    #[test]
    fn overlap_is_error() {
        let mut b = fixture();
        b[36..40].copy_from_slice(&0u32.to_le_bytes());
        assert!(Nso::parse(&b).is_err())
    }
    #[test]
    fn zbic_is_not_zstd() {
        let mut b = fixture();
        b[12] = 128;
        assert!(Nso::parse(&b).unwrap_err().contains("Zbic"))
    }
    #[test]
    fn lz4_literal_and_malformed() {
        assert_eq!(lz4_block(b"\x50hello", 5).unwrap(), b"hello");
        assert!(lz4_block(b"\xff", 20).is_err());
        assert!(lz4_block(b"\x50hello", 6).is_err())
    }
}

/// ELF64 RELR: even words locate a relocation, odd words encode the next 63 slots.
pub fn decode_relr(bytes: &[u8]) -> Result<Vec<u64>> {
    if bytes.len() % 8 != 0 {
        return Err("truncated RELR".into());
    }
    let mut next = None;
    let mut out = Vec::new();
    for word in bytes.chunks_exact(8) {
        let e = u64le(word, 0)?;
        if e & 1 == 0 {
            if e % 8 != 0 {
                return Err("unaligned RELR address".into());
            }
            out.push(e);
            next = Some(e.checked_add(8).ok_or("RELR overflow")?);
        } else {
            let base = next.ok_or("RELR bitmap before address")?;
            for bit in 1..64 {
                if e & (1u64 << bit) != 0 {
                    out.push(base.checked_add((bit - 1) * 8).ok_or("RELR overflow")?);
                }
            }
            next = Some(base.checked_add(63 * 8).ok_or("RELR overflow")?);
        }
        if out.len() > 4_000_000 {
            return Err("RELR count limit".into());
        }
    }
    Ok(out)
}
#[cfg(test)]
mod relr_tests {
    use super::*;
    #[test]
    fn bitmap() {
        let b: Vec<_> = [0x100u64, 0b1011, 0x400]
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();
        assert_eq!(decode_relr(&b).unwrap(), vec![0x100, 0x108, 0x118, 0x400]);
    }
    #[test]
    fn malformed() {
        assert!(decode_relr(&1u64.to_le_bytes()).is_err());
        assert!(decode_relr(&[1]).is_err());
        assert!(decode_relr(&2u64.to_le_bytes()).is_err());
    }
}

#[cfg(test)]
mod npdm_tests {
    use super::*;
    fn fixture(magic: usize) -> Vec<u8> {
        let mut b = vec![0u8; 128 + magic + 64 + 64];
        b[..4].copy_from_slice(b"META");
        let ao = 128 + magic + 64;
        for (o, v) in [(112, ao), (116, 64), (120, 128), (124, magic + 64)] {
            b[o..o + 4].copy_from_slice(&(v as u32).to_le_bytes())
        }
        b[128 + magic..132 + magic].copy_from_slice(b"ACID");
        b[ao..ao + 4].copy_from_slice(b"ACI0");
        b
    }
    #[test]
    fn both_acid_layouts() {
        for (off, bits) in [(512, 2048), (768, 3072)] {
            let n = Npdm::parse(&fixture(off)).unwrap();
            assert_eq!(n.acid_rsa_bits, bits);
            assert!(!n.signature_verified)
        }
    }
    #[test]
    fn truncated_acid() {
        let b = fixture(768);
        assert!(Npdm::parse(&b[..b.len() - 1]).is_err())
    }
}

#[cfg(test)]
mod zstd_tests {
    use super::*;
    #[test]
    fn ordinary_frame_and_limits() {
        // Public Zstandard framing around the independently authored text below.
        let bytes = [
            0x28, 0xb5, 0x2f, 0xfd, 0x20, 0x11, 0x89, 0x00, 0x00, 0x73, 0x79, 0x6e, 0x74, 0x68,
            0x65, 0x74, 0x69, 0x63, 0x20, 0x66, 0x69, 0x78, 0x74, 0x75, 0x72, 0x65,
        ];
        assert_eq!(zstd_frame(&bytes, 32).unwrap(), b"synthetic fixture");
        assert!(zstd_frame(&bytes, 8).is_err());
        assert!(zstd_frame(&bytes[..12], 32).is_err());
        assert!(zstd_frame(b"bad", 32).is_err());
    }
}
