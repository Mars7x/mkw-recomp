//! Source witnesses only. A slot's initial contents are not a call-effect proof.
use super::{Image, SourceStamp};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportError {
    InvalidAddress,
    Truncated,
    UnsupportedForm,
    ArithmeticOverflow,
    VeneerCycle,
    VeneerLimit,
}
/// Owned exact source for zero to eight direct B veneers followed by
/// ADRP x16; LDR x17,[x16,#offset]; ADD x16,x16,#offset; BR x17.
/// This says nothing about relocation type, binding, slot permissions/value,
/// callee identity, normal return, or runtime admission. It cannot mint NoReturn.
#[derive(Clone)]
pub struct ImportThunk {
    image: Image,
    sources: Vec<SourceStamp>,
    slot: u64,
}
impl ImportThunk {
    pub fn entry(&self) -> u64 {
        self.sources[0].entry
    }
    pub fn stub_entry(&self) -> u64 {
        self.sources.last().unwrap().entry
    }
    /// Effective address of the 64-bit load, also the final x16 value.
    /// x17 receives the loaded value; x30 is unchanged by these instructions.
    pub fn slot(&self) -> u64 {
        self.slot
    }
    pub fn sources(&self) -> &[SourceStamp] {
        &self.sources
    }
    /// Independent images cannot impersonate this witness's source namespace.
    pub fn belongs_to(&self, image: &Image) -> bool {
        std::sync::Arc::ptr_eq(&self.image.data, &image.data)
    }
}
impl Image {
    /// Recognize only the exact bounded form above, using checked source and
    /// address arithmetic. The slot may be outside text; it is never read here.
    pub fn import_thunk(&self, entry: u64, max_veneers: usize) -> Result<ImportThunk, ImportError> {
        if max_veneers > 8 {
            return Err(ImportError::VeneerLimit);
        }
        let mut at = entry;
        let mut seen = BTreeSet::new();
        let mut sources = Vec::new();
        loop {
            if !seen.insert(at) {
                return Err(ImportError::VeneerCycle);
            }
            let first = self.import_bytes(at, 4)?;
            let word = u32::from_le_bytes(first.try_into().unwrap());
            if word & 0xfc00_0000 == 0x1400_0000 {
                if sources.len() == max_veneers {
                    return Err(ImportError::VeneerLimit);
                }
                sources.push(self.import_stamp(at, first));
                let displacement = ((word & 0x03ff_ffff) as i64) << 38 >> 36;
                at = at
                    .checked_add_signed(displacement)
                    .ok_or(ImportError::ArithmeticOverflow)?;
                continue;
            }
            let bytes = self.import_bytes(at, 16)?;
            let words: Vec<_> = bytes
                .chunks_exact(4)
                .map(|w| u32::from_le_bytes(w.try_into().unwrap()))
                .collect();
            let [adrp, ldr, add, br] = words[..] else {
                unreachable!()
            };
            if adrp & 0x9f00_001f != 0x9000_0010
                || ldr & 0xffc0_03ff != 0xf940_0211
                || add & 0xffc0_03ff != 0x9100_0210
                || br != 0xd61f_0220
            {
                return Err(ImportError::UnsupportedForm);
            }
            let load_offset = ((ldr >> 10) & 4095) as u64 * 8;
            if load_offset != ((add >> 10) & 4095) as u64 {
                return Err(ImportError::UnsupportedForm);
            }
            let imm21 = (((adrp >> 5) & 0x7ffff) << 2) | ((adrp >> 29) & 3);
            let page_delta = ((imm21 as i64) << 43 >> 43) * 4096;
            let slot = (at & !4095)
                .checked_add_signed(page_delta)
                .and_then(|page| page.checked_add(load_offset))
                .ok_or(ImportError::ArithmeticOverflow)?;
            sources.push(self.import_stamp(at, bytes));
            return Ok(ImportThunk {
                image: self.clone(),
                sources,
                slot,
            });
        }
    }
    fn import_bytes(&self, at: u64, count: usize) -> Result<&[u8], ImportError> {
        if at % 4 != 0 || at < self.data.base {
            return Err(ImportError::InvalidAddress);
        }
        let offset =
            usize::try_from(at - self.data.base).map_err(|_| ImportError::InvalidAddress)?;
        if offset >= self.data.bytes.len() {
            return Err(ImportError::InvalidAddress);
        }
        self.data
            .bytes
            .get(
                offset
                    ..offset
                        .checked_add(count)
                        .ok_or(ImportError::ArithmeticOverflow)?,
            )
            .ok_or(ImportError::Truncated)
    }
    fn import_stamp(&self, entry: u64, bytes: &[u8]) -> SourceStamp {
        SourceStamp {
            image_digest: self.data.digest,
            text_base: self.data.base,
            entry,
            bytes: bytes.len(),
            body_digest: Sha256::digest(bytes).into(),
        }
    }
}
