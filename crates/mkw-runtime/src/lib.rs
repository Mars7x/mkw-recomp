//! Owned checked guest memory, relocation planning and explicit native/HLE bridges.
//! Original startup remains gated; no instruction-fetch interpreter or raw guest-pointer casts.
pub mod abi;
pub mod atomics;
pub mod bridge;
pub mod dispatch;
pub mod event_services;
pub mod events;
pub mod exclusive;
pub mod exclusive_calls;
pub mod exclusive_entry;
pub mod exclusive_native;
pub mod floating;
pub mod guarded;
pub mod guarded_entry;
pub mod host_call;
pub mod kernel_handles;
mod linker;
pub mod owned_events;
pub mod pauth;
pub mod process_entropy;
#[cfg(target_os = "linux")]
pub mod romfs_hle;
#[cfg(target_os = "linux")]
pub mod romfs_hooks;
pub mod services;
pub mod table_event_services;
pub mod thread_queries;
pub mod thread_runner;
pub use linker::LinkScopes;
use mkw_formats::{Nso, Result};
use serde::Serialize;
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GuestAddr(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permissions {
    pub read: bool,
    pub write: bool,
    pub execute: bool,
}
impl Permissions {
    pub const RW: Self = Self {
        read: true,
        write: true,
        execute: false,
    };
    pub const R: Self = Self {
        read: true,
        write: false,
        execute: false,
    };
    pub const RX: Self = Self {
        read: true,
        write: false,
        execute: true,
    };
}
#[derive(Debug)]
struct Region {
    base: GuestAddr,
    bytes: Vec<u8>,
    permissions: Permissions,
    name: String,
}
#[derive(Debug, Default)]
pub struct Memory {
    regions: BTreeMap<GuestAddr, Region>,
    sealed: bool,
}
impl Memory {
    pub fn map(
        &mut self,
        base: GuestAddr,
        bytes: Vec<u8>,
        permissions: Permissions,
        name: &str,
    ) -> Result<()> {
        if self.sealed {
            return Err("mapping changes after seal are not implemented".into());
        }
        if bytes.is_empty() {
            return Ok(());
        }
        let end = base
            .0
            .checked_add(bytes.len() as u64)
            .ok_or("mapping address overflow")?;
        if bytes.len() > mkw_formats::MAX_IMAGE {
            return Err("mapping size limit".into());
        }
        for r in self.regions.values() {
            if base.0 < r.base.0 + r.bytes.len() as u64 && r.base.0 < end {
                return Err(format!("mapping overlaps {}", r.name));
            }
        }
        self.regions.insert(
            base,
            Region {
                base,
                bytes,
                permissions,
                name: name.into(),
            },
        );
        Ok(())
    }
    fn region(&self, at: GuestAddr, n: usize) -> Result<(&Region, usize)> {
        let (_, r) = self
            .regions
            .range(..=at)
            .next_back()
            .ok_or_else(|| format!("unmapped guest address {:#x}", at.0))?;
        let off = usize::try_from(at.0 - r.base.0).map_err(|_| "address offset overflow")?;
        if off.checked_add(n).ok_or("access overflow")? > r.bytes.len() {
            return Err(format!(
                "guest access {:#x}+{n:#x} crosses {}",
                at.0, r.name
            ));
        }
        Ok((r, off))
    }
    pub fn read(&self, at: GuestAddr, n: usize) -> Result<&[u8]> {
        let (r, o) = self.region(at, n)?;
        if !r.permissions.read {
            return Err("guest read permission violation".into());
        }
        Ok(&r.bytes[o..o + n])
    }
    pub fn write(&mut self, at: GuestAddr, data: &[u8]) -> Result<()> {
        let (r, o) = self.region(at, data.len())?;
        if !r.permissions.write {
            return Err("guest write permission violation".into());
        }
        let key = r.base;
        self.regions.get_mut(&key).unwrap().bytes[o..o + data.len()].copy_from_slice(data);
        Ok(())
    }
    /// Validate all spans before a transfer, preserving destination/state on a fault.
    fn spans(
        &self,
        at: GuestAddr,
        len: usize,
        write: bool,
    ) -> Result<Vec<(GuestAddr, usize, usize)>> {
        at.0.checked_add(len as u64)
            .ok_or("guest transfer address overflow")?;
        let mut spans = Vec::new();
        let mut offset = 0;
        while offset < len {
            let address = GuestAddr(at.0 + offset as u64);
            let (r, start) = self.region(address, 1)?;
            if if write {
                !r.permissions.write
            } else {
                !r.permissions.read
            } {
                return Err("guest transfer permission violation".into());
            }
            let count = (len - offset).min(r.bytes.len() - start);
            spans.push((r.base, start, count));
            offset += count;
        }
        Ok(spans)
    }
    pub fn read_into(&self, at: GuestAddr, destination: &mut [u8]) -> Result<()> {
        let spans = self.spans(at, destination.len(), false)?;
        let mut offset = 0;
        for (base, start, count) in spans {
            destination[offset..offset + count]
                .copy_from_slice(&self.regions[&base].bytes[start..start + count]);
            offset += count;
        }
        Ok(())
    }
    pub fn write_bytes(&mut self, at: GuestAddr, source: &[u8]) -> Result<()> {
        let spans = self.spans(at, source.len(), true)?;
        let mut offset = 0;
        for (base, start, count) in spans {
            self.regions.get_mut(&base).unwrap().bytes[start..start + count]
                .copy_from_slice(&source[offset..offset + count]);
            offset += count;
        }
        Ok(())
    }
    fn initialize(&mut self, at: GuestAddr, data: &[u8]) -> Result<()> {
        if self.sealed {
            return Err("loader initialization after memory seal".into());
        }
        let (r, o) = self.region(at, data.len())?;
        let key = r.base;
        self.regions.get_mut(&key).unwrap().bytes[o..o + data.len()].copy_from_slice(data);
        Ok(())
    }
    pub fn seal(&mut self) {
        self.sealed = true;
    }
    pub fn read_u64(&self, at: GuestAddr) -> Result<u64> {
        Ok(u64::from_le_bytes(self.read(at, 8)?.try_into().unwrap()))
    }
    pub fn mapped_bytes(&self) -> usize {
        self.regions.values().map(|r| r.bytes.len()).sum()
    }
    pub fn region_count(&self) -> usize {
        self.regions.len()
    }
}
#[derive(Debug)]
pub struct LoadedModule {
    pub name: String,
    pub base: GuestAddr,
    pub nso: Nso,
}
/// Map independent owned copies. BSS comes from the NSO header, not guessed gaps.
pub fn map_modules(modules: &[LoadedModule]) -> Result<Memory> {
    let mut memory = Memory::default();
    for m in modules {
        for s in &m.nso.segments {
            let permissions = Permissions {
                read: s.permissions.as_bytes()[0] == b'R',
                write: s.permissions.as_bytes()[1] == b'W',
                execute: s.permissions.as_bytes()[2] == b'X',
            };
            memory.map(
                GuestAddr(
                    m.base
                        .0
                        .checked_add(s.memory_offset as u64)
                        .ok_or("module base overflow")?,
                ),
                s.bytes.clone(),
                permissions,
                &format!("{}.{}", m.name, s.name),
            )?;
        }
        let data = &m.nso.segments[2];
        let bss = data.memory_offset + data.size;
        memory.map(
            GuestAddr(
                m.base
                    .0
                    .checked_add(bss as u64)
                    .ok_or("BSS address overflow")?,
            ),
            vec![0; m.nso.bss_size],
            Permissions::RW,
            &format!("{}.bss", m.name),
        )?;
    }
    Ok(memory)
}
#[derive(Debug, Serialize)]
pub struct SymbolBinding {
    pub consumer: usize,
    pub provider: Option<usize>,
    pub symbol: String,
    pub destination: u64,
    pub value: u64,
}
#[derive(Debug)]
pub struct RelocationPlan {
    writes: Vec<(GuestAddr, u64)>,
    pub bindings: Vec<SymbolBinding>,
}
impl RelocationPlan {
    pub fn verify_applied(&self, memory: &Memory) -> Result<()> {
        for (address, expected) in &self.writes {
            if memory.read_u64(*address)? != *expected {
                return Err(format!("relocation readback mismatch at {:#x}", address.0));
            }
        }
        Ok(())
    }
    pub fn len(&self) -> usize {
        self.writes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
    }
    /// Validate every destination before committing any write. No partial plan on error.
    pub fn apply(&self, memory: &mut Memory) -> Result<()> {
        if memory.sealed {
            return Err("cannot relocate sealed memory".into());
        }
        for (at, _) in &self.writes {
            memory.region(*at, 8)?;
        }
        for (at, value) in &self.writes {
            memory.initialize(*at, &value.to_le_bytes())?;
        }
        Ok(())
    }
}
/// Default tooling policy: requester/dependency closure. Process-global lookup
/// requires a caller-specified order through plan_relocations_scoped.
pub fn plan_relocations(modules: &[LoadedModule], memory: &Memory) -> Result<RelocationPlan> {
    let scopes = LinkScopes::dependencies(modules)?;
    plan_relocations_scoped(modules, memory, &scopes)
}
pub fn plan_relocations_scoped(
    modules: &[LoadedModule],
    memory: &Memory,
    scopes: &LinkScopes,
) -> Result<RelocationPlan> {
    let resolver = linker::Resolver::new(modules, scopes)?;
    let mut bindings = Vec::new();
    let mut writes = BTreeMap::new();
    for (consumer, m) in modules.iter().enumerate() {
        let Some(info) = &m.nso.mod0 else { continue };
        let address = |off: u64| -> Result<GuestAddr> {
            Ok(GuestAddr(
                m.base
                    .0
                    .checked_add(off)
                    .ok_or("relocation address overflow")?,
            ))
        };
        for &off in &info.relr_offsets {
            let at = address(off)?;
            let old = memory.read_u64(at)?;
            if writes.insert(at, old.wrapping_add(m.base.0)).is_some() {
                return Err("duplicate relocation destination".into());
            }
        }
        for r in &info.relocations {
            let at = address(r.offset)?;
            let value = match r.kind {
                1027 => {
                    if r.symbol != 0 {
                        return Err("relative relocation has symbol".into());
                    }
                    m.base.0.wrapping_add(r.addend as u64)
                }
                257 | 1025 | 1026 => {
                    let s = info
                        .symbols
                        .get(r.symbol as usize)
                        .ok_or("relocation symbol index outside table")?;
                    let (value, provider) = resolver.resolve(consumer, s, r.symbol)?;
                    bindings.push(SymbolBinding {
                        consumer,
                        provider,
                        symbol: s.name.clone(),
                        destination: at.0,
                        value,
                    });
                    value.wrapping_add(r.addend as u64)
                }
                other => {
                    return Err(format!(
                        "unsupported AArch64 relocation {other} in {}",
                        m.name
                    ))
                }
            };
            memory.region(at, 8)?;
            if writes.insert(at, value).is_some() {
                return Err("duplicate relocation destination".into());
            }
        }
    }
    Ok(RelocationPlan {
        writes: writes.into_iter().collect(),
        bindings,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_permissions_and_aliases() {
        let mut m = Memory::default();
        m.map(GuestAddr(0x1000), vec![0; 16], Permissions::RW, "data")
            .unwrap();
        m.write(GuestAddr(0x1001), &[1, 2, 3]).unwrap();
        assert_eq!(m.read(GuestAddr(0x1002), 2).unwrap(), [2, 3]);
        assert!(m.read(GuestAddr(0xfff), 1).is_err());
        assert!(m.write(GuestAddr(0x100f), &[0; 2]).is_err());
        assert!(m
            .map(GuestAddr(0x1008), vec![0; 16], Permissions::RW, "overlap")
            .is_err());
        m.map(GuestAddr(0x2000), vec![9; 8], Permissions::R, "ro")
            .unwrap();
        assert!(m.write(GuestAddr(0x2000), &[1]).is_err());
        m.seal();
        assert!(m.initialize(GuestAddr(0x1000), &[3]).is_err());
    }
    #[test]
    fn no_partial_relocations() {
        let mut m = Memory::default();
        m.map(GuestAddr(8), vec![0; 8], Permissions::RW, "data")
            .unwrap();
        let p = RelocationPlan {
            writes: vec![(GuestAddr(8), 123), (GuestAddr(999), 4)],
            bindings: vec![],
        };
        assert!(p.apply(&mut m).is_err());
        assert_eq!(m.read_u64(GuestAddr(8)).unwrap(), 0);
    }
    #[test]
    fn unsigned_address_overflow() {
        assert!(Memory::default()
            .map(GuestAddr(u64::MAX - 3), vec![0; 8], Permissions::RW, "bad")
            .is_err())
    }
}

#[cfg(test)]
mod relocation_tests {
    use super::*;
    use mkw_formats::{Mod0, Relocation, Symbol};
    pub(crate) fn module(name: &str, base: u64) -> LoadedModule {
        let mut raw = vec![0u8; 288];
        raw[..4].copy_from_slice(b"NSO0");
        for (i, size) in [8u32, 8, 16].iter().enumerate() {
            for (at, value) in [
                (16 + i * 16, 256 + (i * 8) as u32),
                (20 + i * 16, (i * 0x1000) as u32),
                (24 + i * 16, *size),
            ] {
                raw[at..at + 4].copy_from_slice(&value.to_le_bytes())
            }
        }
        let mut nso = Nso::parse(&raw).unwrap();
        nso.mod0 = Some(Mod0 {
            offset: 0,
            dynamic: 0,
            bss_start: 0,
            bss_end: 0,
            eh_start: 0,
            eh_end: 0,
            module_object: 0,
            tags: BTreeMap::new(),
            needed: vec![],
            symbols: vec![Symbol {
                name: "".into(),
                value: 0,
                size: 0,
                info: 0,
                other: 0,
                section: 0,
            }],
            relocations: vec![],
            relocation_counts: BTreeMap::new(),
            relr_offsets: vec![],
        });
        LoadedModule {
            name: name.into(),
            base: GuestAddr(base),
            nso,
        }
    }
    #[test]
    fn relr_and_imports_link_without_guest_pointer_casts() {
        let mut a = module("a", 0x10000);
        let mut b = module("b", 0x20000);
        a.nso.mod0.as_mut().unwrap().relr_offsets.push(0x2000);
        a.nso.segments[2].bytes[..8].copy_from_slice(&0x88u64.to_le_bytes());
        a.nso.mod0.as_mut().unwrap().symbols.push(Symbol {
            name: "f".into(),
            value: 0,
            size: 0,
            info: 0x12,
            other: 0,
            section: 0,
        });
        a.nso.mod0.as_mut().unwrap().relocations.push(Relocation {
            offset: 0x2008,
            kind: 1026,
            symbol: 1,
            addend: 4,
        });
        b.nso.mod0.as_mut().unwrap().symbols.push(Symbol {
            name: "f".into(),
            value: 4,
            size: 4,
            info: 0x12,
            other: 0,
            section: 1,
        });
        let modules = [a, b];
        let mut mem = map_modules(&modules).unwrap();
        let scopes = LinkScopes::process_order(&modules, &[0, 1]).unwrap();
        let p = plan_relocations_scoped(&modules, &mem, &scopes).unwrap();
        assert_eq!(p.len(), 2);
        p.apply(&mut mem).unwrap();
        assert_eq!(mem.read_u64(GuestAddr(0x12000)).unwrap(), 0x10088);
        assert_eq!(mem.read_u64(GuestAddr(0x12008)).unwrap(), 0x20008);
    }
    #[test]
    fn unresolved_import_rejects_whole_plan() {
        let mut a = module("a", 0x10000);
        let m = a.nso.mod0.as_mut().unwrap();
        m.symbols.push(Symbol {
            name: "missing".into(),
            value: 0,
            size: 0,
            info: 0x12,
            other: 0,
            section: 0,
        });
        m.relocations.push(Relocation {
            offset: 0x2000,
            kind: 257,
            symbol: 1,
            addend: 0,
        });
        let modules = [a];
        let mem = map_modules(&modules).unwrap();
        assert!(plan_relocations(&modules, &mem)
            .unwrap_err()
            .contains("unresolved import"));
        assert_eq!(mem.read_u64(GuestAddr(0x12000)).unwrap(), 0);
    }
    #[test]
    fn protected_self_binding_and_explicit_interposition() {
        let mut a = module("a", 0x10000);
        let mut b = module("b", 0x20000);
        for m in [&mut a, &mut b] {
            let d = m.nso.mod0.as_mut().unwrap();
            d.symbols.push(Symbol {
                name: "f".into(),
                value: 4,
                size: 4,
                info: 0x12,
                other: 3,
                section: 1,
            });
            d.relocations.push(Relocation {
                offset: 0x2000,
                kind: 257,
                symbol: 1,
                addend: 0,
            });
        }
        let mut modules = [a, b];
        let mut mem = map_modules(&modules).unwrap();
        plan_relocations(&modules, &mem)
            .unwrap()
            .apply(&mut mem)
            .unwrap();
        assert_eq!(mem.read_u64(GuestAddr(0x12000)).unwrap(), 0x10004);
        modules[0].nso.mod0.as_mut().unwrap().symbols[1].other = 0;
        let scopes = LinkScopes::process_order(&modules, &[1, 0]).unwrap();
        let plan = plan_relocations_scoped(&modules, &mem, &scopes).unwrap();
        plan.apply(&mut mem).unwrap();
        assert_eq!(mem.read_u64(GuestAddr(0x12000)).unwrap(), 0x20004);
    }
}
