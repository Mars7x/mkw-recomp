//! Bounded .eh_frame_hdr/CIE/FDE extent reader. Does not execute unwind programs.
use crate::{u32le, Nso, Result};
use serde::Serialize;
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize)]
pub struct FunctionExtent {
    pub start: usize,
    pub size: usize,
    pub fde: usize,
    pub cie: usize,
    pub augmentation: String,
    pub return_register: u64,
    pub pac_key_b: bool,
}
#[derive(Clone)]
struct Cie {
    augmentation: String,
    encoding: u8,
    return_register: u64,
    pac_key_b: bool,
}
struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn byte(&mut self) -> Result<u8> {
        let b = *self.data.get(self.at).ok_or("truncated CFI data")?;
        self.at += 1;
        Ok(b)
    }
    fn skip(&mut self, n: usize) -> Result<()> {
        self.at = self.at.checked_add(n).ok_or("CFI offset overflow")?;
        if self.at > self.data.len() {
            return Err("CFI range outside record".into());
        }
        Ok(())
    }
    fn uleb(&mut self) -> Result<u64> {
        let mut out = 0u64;
        for shift in (0..70).step_by(7) {
            let b = self.byte()?;
            if shift == 63 && b > 1 {
                return Err("CFI LEB overflow".into());
            }
            out |= u64::from(b & 127) << shift;
            if b & 128 == 0 {
                return Ok(out);
            }
        }
        Err("CFI LEB overflow".into())
    }
    fn sleb(&mut self) -> Result<i64> {
        let mut out = 0u64;
        for shift in (0..70).step_by(7) {
            let b = self.byte()?;
            if shift == 63 && !matches!(b, 0 | 127) {
                return Err("CFI signed LEB overflow".into());
            }
            out |= u64::from(b & 127) << shift;
            if b & 128 == 0 {
                let bits = shift + 7;
                if bits < 64 && b & 64 != 0 {
                    out |= u64::MAX << bits
                }
                return Ok(out as i64);
            }
        }
        Err("CFI signed LEB overflow".into())
    }
    fn encoded_skip(&mut self, encoding: u8) -> Result<()> {
        match encoding & 15 {
            0 | 4 | 12 => self.skip(8),
            2 | 10 => self.skip(2),
            3 | 11 => self.skip(4),
            1 => {
                self.uleb()?;
                Ok(())
            }
            9 => {
                self.sleb()?;
                Ok(())
            }
            _ => Err("unsupported CFI pointer encoding".into()),
        }
    }
}
fn record(n: &Nso, address: usize) -> Result<&[u8]> {
    let length = n.word(address)? as usize;
    if length == 0 || length == 0xffff_ffff {
        return Err("unsupported empty/DWARF64 CFI record".into());
    }
    n.read(
        address,
        4usize.checked_add(length).ok_or("CFI size overflow")?,
    )
}
fn parse_cie(n: &Nso, address: usize) -> Result<Cie> {
    let b = record(n, address)?;
    if u32le(b, 4)? != 0 {
        return Err("FDE references non-CIE record".into());
    }
    let mut c = Cursor { data: b, at: 8 };
    let version = c.byte()?;
    if !matches!(version, 1 | 3) {
        return Err("unsupported CIE version".into());
    }
    let mut aug = Vec::new();
    loop {
        let b = c.byte()?;
        if b == 0 {
            break;
        }
        if aug.len() >= 64 {
            return Err("CIE augmentation limit".into());
        }
        aug.push(b)
    }
    let augmentation = String::from_utf8(aug).map_err(|_| "non-UTF8 CIE augmentation")?;
    if c.uleb()? == 0 {
        return Err("zero CIE code alignment".into());
    }
    c.sleb()?;
    let return_register = if version == 1 {
        u64::from(c.byte()?)
    } else {
        c.uleb()?
    };
    let mut encoding = 0;
    let mut pac_key_b = false;
    if !augmentation.starts_with('z') {
        return Err("unsupported non-z CIE augmentation".into());
    }
    let len = usize::try_from(c.uleb()?).map_err(|_| "augmentation size overflow")?;
    let end = c.at.checked_add(len).ok_or("augmentation overflow")?;
    if end > b.len() {
        return Err("augmentation outside CIE".into());
    }
    c.data = &b[..end];
    for a in augmentation.bytes().skip(1) {
        match a {
            b'R' => encoding = c.byte()?,
            b'L' => {
                c.byte()?;
            }
            b'P' => {
                let enc = c.byte()?;
                c.encoded_skip(enc)?
            }
            b'B' => pac_key_b = true,
            b'S' => {}
            _ => return Err(format!("unsupported CIE augmentation {augmentation}")),
        }
    }
    if c.at != end {
        return Err("unconsumed CIE augmentation data".into());
    }
    Ok(Cie {
        augmentation,
        encoding,
        return_register,
        pac_key_b,
    })
}
pub fn functions(n: &Nso) -> Result<Vec<FunctionExtent>> {
    let Some(info) = &n.mod0 else {
        return Ok(vec![]);
    };
    if info.eh_start == info.eh_end {
        return Ok(vec![]);
    }
    let h = n.read(info.eh_start, info.eh_end - info.eh_start)?;
    if h.get(..4) != Some(&[1, 0x1b, 3, 0x3b]) {
        return Err("unsupported unwind header encoding".into());
    }
    let count = u32le(h, 8)? as usize;
    crate::slice(h, 12, count.checked_mul(8).ok_or("unwind count overflow")?)?;
    let mut cies = BTreeMap::new();
    let mut out = Vec::with_capacity(count);
    let text = &n.segments[0];
    let mut previous = None;
    for i in 0..count {
        let relative = |offset| -> Result<usize> {
            usize::try_from(info.eh_start as i64 + u32le(h, offset)? as i32 as i64)
                .map_err(|_| "negative unwind index address".into())
        };
        let indexed_start = relative(12 + i * 8)?;
        let fde = relative(16 + i * 8)?;
        let data = record(n, fde)?;
        let delta = u32le(data, 4)? as usize;
        let cie = fde
            .checked_add(4)
            .and_then(|v| v.checked_sub(delta))
            .ok_or("invalid CIE delta")?;
        if let std::collections::btree_map::Entry::Vacant(e) = cies.entry(cie) {
            e.insert(parse_cie(n, cie)?);
        }
        let c = &cies[&cie];
        if c.encoding != 0x1b {
            return Err(format!("unsupported FDE encoding {:02x}", c.encoding));
        }
        let start = usize::try_from(fde as i64 + 8 + u32le(data, 8)? as i32 as i64)
            .map_err(|_| "negative FDE start")?;
        let size = u32le(data, 12)? as i32;
        if size < 0 {
            return Err("negative FDE extent".into());
        }
        let size = size as usize;
        let end = start.checked_add(size).ok_or("FDE extent overflow")?;
        if start != indexed_start
            || start < text.memory_offset
            || end > text.memory_offset + text.size
            || start % 4 != 0
            || size % 4 != 0
            || previous.is_some_and(|p| p > start)
        {
            return Err("invalid indexed FDE code range/order".into());
        }
        let mut aug = Cursor { data, at: 16 };
        let extra = usize::try_from(aug.uleb()?).map_err(|_| "FDE augmentation overflow")?;
        aug.skip(extra)?;
        previous = Some(start);
        out.push(FunctionExtent {
            start,
            size,
            fde,
            cie,
            augmentation: c.augmentation.clone(),
            return_register: c.return_register,
            pac_key_b: c.pac_key_b,
        });
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_leb() {
        let mut c = Cursor {
            data: &[0xe5, 0x8e, 0x26, 0x7c],
            at: 0,
        };
        assert_eq!(c.uleb().unwrap(), 624485);
        assert_eq!(c.sleb().unwrap(), -4);
        assert!(c.uleb().is_err());
        let mut c = Cursor {
            data: &[255; 10],
            at: 0,
        };
        assert!(c.uleb().is_err());
    }
}

#[cfg(test)]
mod extent_tests {
    use super::*;
    use crate::Mod0;
    fn fixture() -> Nso {
        let mut raw = vec![0u8; 424];
        raw[..4].copy_from_slice(b"NSO0");
        for (i, (off, va, size)) in [(256u32, 0u32, 32u32), (288, 4096, 128), (416, 8192, 8)]
            .iter()
            .enumerate()
        {
            for (at, v) in [
                (16 + i * 16, *off),
                (20 + i * 16, *va),
                (24 + i * 16, *size),
            ] {
                raw[at..at + 4].copy_from_slice(&v.to_le_bytes())
            }
        }
        let mut n = Nso::parse(&raw).unwrap();
        let b = &mut n.segments[1].bytes;
        b[..4].copy_from_slice(&[1, 0x1b, 3, 0x3b]);
        b[8..12].copy_from_slice(&1u32.to_le_bytes());
        b[12..16].copy_from_slice(&(-4096i32).to_le_bytes());
        b[16..20].copy_from_slice(&64u32.to_le_bytes());
        b[32..56].copy_from_slice(&[
            20, 0, 0, 0, 0, 0, 0, 0, 1, b'z', b'R', b'B', 0, 1, 0x7c, 30, 1, 0x1b, 0x0c, 0x1f, 0,
            0, 0, 0,
        ]);
        b[64..68].copy_from_slice(&20u32.to_le_bytes());
        b[68..72].copy_from_slice(&36u32.to_le_bytes());
        b[72..76].copy_from_slice(&(-4168i32).to_le_bytes());
        b[76..80].copy_from_slice(&32u32.to_le_bytes());
        n.mod0 = Some(Mod0 {
            offset: 0,
            dynamic: 0,
            bss_start: 0,
            bss_end: 0,
            eh_start: 4096,
            eh_end: 4116,
            module_object: 0,
            tags: BTreeMap::new(),
            needed: vec![],
            symbols: vec![],
            relocations: vec![],
            relocation_counts: BTreeMap::new(),
            relr_offsets: vec![],
        });
        n
    }
    #[test]
    fn key_b_cie_and_exact_extent() {
        let extents = functions(&fixture()).unwrap();
        assert_eq!(extents.len(), 1);
        assert_eq!(
            (
                extents[0].start,
                extents[0].size,
                extents[0].return_register
            ),
            (0, 32, 30)
        );
        assert!(extents[0].pac_key_b);
    }
    #[test]
    fn malformed_index_cie_and_fde_rejected() {
        let mut n = fixture();
        n.segments[1].bytes[12..16].copy_from_slice(&(-4092i32).to_le_bytes());
        assert!(functions(&n).is_err());
        n = fixture();
        n.segments[1].bytes[64..68].copy_from_slice(&1000u32.to_le_bytes());
        assert!(functions(&n).is_err());
        n = fixture();
        n.segments[1].bytes[43] = b'X';
        assert!(functions(&n).is_err());
    }
}
