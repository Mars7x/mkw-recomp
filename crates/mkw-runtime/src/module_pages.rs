//! Explicit page-owned module images. The default exact-byte mapper is unchanged.
use crate::{GuestAddr, LoadedModule, Memory, Permissions};
use mkw_formats::{Result, MAX_IMAGE};

struct Mapping<'a> {
    base: GuestAddr,
    size: usize,
    payload: &'a [u8],
    permissions: Permissions,
    name: String,
}

/// Validated, immutable plan borrowing the original payloads until materialized.
/// Zero padding belongs to its segment's permissions; data and BSS share one
/// allocation. No address-space gaps or process resource facts are invented.
pub struct Plan<'a> {
    mappings: Vec<Mapping<'a>>,
    bytes: u64,
}
impl<'a> Plan<'a> {
    pub fn new(modules: &'a [LoadedModule]) -> Result<Self> {
        let mut mappings = Vec::new();
        let mut bytes = 0u64;
        for module in modules {
            if module.base.0 % 4096 != 0 || module.nso.segments.len() != 3 {
                return Err("page image requires aligned base and three NSO segments".into());
            }
            for (index, segment) in module.nso.segments.iter().enumerate() {
                let (permissions, expected) = match index {
                    0 if segment.permissions == "--X" => (
                        Permissions {
                            read: false,
                            write: false,
                            execute: true,
                        },
                        "--X",
                    ),
                    0 => (Permissions::RX, "R-X"),
                    1 => (Permissions::R, "R--"),
                    _ => (Permissions::RW, "RW-"),
                };
                if segment.bytes.len() != segment.size || segment.permissions != expected {
                    return Err("page image segment payload or permissions mismatch".into());
                }
                let size = segment
                    .size
                    .checked_add(if index == 2 { module.nso.bss_size } else { 0 })
                    .ok_or("page image BSS overflow")?;
                if size == 0 {
                    continue;
                }
                if segment.memory_offset % 4096 != 0 || size > MAX_IMAGE {
                    return Err("page image segment alignment or size limit".into());
                }
                let size = size
                    .checked_add(4095)
                    .ok_or("page image rounding overflow")?
                    & !4095;
                if size > MAX_IMAGE {
                    return Err("page image padded size limit".into());
                }
                let base = module
                    .base
                    .0
                    .checked_add(segment.memory_offset as u64)
                    .ok_or("page image base overflow")?;
                base.checked_add(size as u64)
                    .ok_or("page image end overflow")?;
                bytes = bytes
                    .checked_add(size as u64)
                    .ok_or("page image accounting overflow")?;
                mappings.push(Mapping {
                    base: GuestAddr(base),
                    size,
                    payload: &segment.bytes,
                    permissions,
                    name: format!("{}.page-segment-{index}", module.name),
                });
            }
        }
        mappings.sort_by_key(|m| m.base);
        for pair in mappings.windows(2) {
            if pair[0].base.0 + pair[0].size as u64 > pair[1].base.0 {
                return Err("page image ownership collision".into());
            }
        }
        Ok(Self { mappings, bytes })
    }

    /// Check that each planned module mapping still has its exact initial bytes,
    /// zero BSS/padding, permissions and allocation extent. Additional mappings
    /// (for example a stack) are allowed. This is a read-only, point-in-time
    /// content check, not proof of load history, identity or execution authority.
    /// In particular, recheck under the same ownership/lock as startup use.
    pub fn verify_pristine(&self, memory: &Memory) -> Result<()> {
        for mapping in &self.mappings {
            let region = memory
                .regions
                .get(&mapping.base)
                .ok_or("initial module mapping missing")?;
            if region.heap_visible.is_some()
                || region.bytes.len() != mapping.size
                || region.permissions != mapping.permissions
            {
                return Err("initial module mapping extent or permissions differ".into());
            }
            if region.bytes[..mapping.payload.len()] != *mapping.payload
                || region.bytes[mapping.payload.len()..]
                    .iter()
                    .any(|&b| b != 0)
            {
                return Err("module image differs from initial payload or zero fill".into());
            }
        }
        Ok(())
    }

    pub fn mapped_bytes(&self) -> u64 {
        self.bytes
    }
    pub fn region_count(&self) -> usize {
        self.mappings.len()
    }

    /// Allocate independent zeroed pages and copy only declared segment bytes.
    /// The returned memory is unsealed for checked relocation. No original code
    /// is invoked, no routes registered, and no target authorization supplied.
    pub fn materialize(&self) -> Result<Memory> {
        let mut memory = Memory::default();
        for mapping in &self.mappings {
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(mapping.size)
                .map_err(|e| format!("page image allocation: {e}"))?;
            bytes.resize(mapping.size, 0);
            bytes[..mapping.payload.len()].copy_from_slice(mapping.payload);
            memory.map(mapping.base, bytes, mapping.permissions, &mapping.name)?;
        }
        Ok(memory)
    }
}
