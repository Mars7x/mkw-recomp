//! Plan explicit filesystem HLE entries from existing scoped symbol bindings.
//! This neither edits relocations nor authorizes game execution or validates the
//! retail SDK object layout. The selected upstream profile remains opt-in.
use crate::{
    exclusive_calls::Target,
    romfs_hle::{ReadOnlyFs, SYMBOLS},
    GuestAddr, LoadedModule, Memory, RelocationPlan,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Debug, Serialize)]
pub struct HookDescriptor {
    symbol: String,
    provider: usize,
    provider_name: String,
    provider_sha256: String,
    address: u64,
    module_base: u64,
    binding_sites: usize,
}
impl HookDescriptor {
    pub fn symbol(&self) -> &str {
        &self.symbol
    }
    pub fn address(&self) -> GuestAddr {
        GuestAddr(self.address)
    }
    pub fn binding_sites(&self) -> usize {
        self.binding_sites
    }
    pub fn bind<'code>(&self, filesystem: &Arc<ReadOnlyFs>) -> Result<Target<'code>, String> {
        filesystem.target(&self.symbol, GuestAddr(self.address), self.module_base)
    }
}
pub fn checked_hooks(
    modules: &[LoadedModule],
    plan: &RelocationPlan,
    memory: &Memory,
) -> Result<Vec<HookDescriptor>, String> {
    plan.verify_applied(memory)?;
    let mut hooks = BTreeMap::<u64, HookDescriptor>::new();
    let mut names = BTreeSet::new();
    let mut sites = BTreeSet::new();
    for b in &plan.bindings {
        if !SYMBOLS.contains(&b.symbol.as_str()) {
            continue;
        }
        if !sites.insert((b.consumer, b.destination)) {
            return Err("duplicate HLE binding site".into());
        }
        let consumer = modules
            .get(b.consumer)
            .ok_or("HLE binding consumer outside module table")?;
        let provider_index = b.provider.ok_or("selected HLE symbol has no provider")?;
        let provider = modules
            .get(provider_index)
            .ok_or("HLE binding provider outside module table")?;
        if provider.nso.sha256.len() != 64
            || !provider.nso.sha256.bytes().all(|b| b.is_ascii_hexdigit())
            || provider.base.0 % 4096 != 0
            || b.value % 4 != 0
        {
            return Err("invalid HLE provider identity/address metadata".into());
        }
        let offset = b
            .destination
            .checked_sub(consumer.base.0)
            .ok_or("HLE relocation below consumer base")?;
        let info = consumer
            .nso
            .mod0
            .as_ref()
            .ok_or("HLE consumer has no dynamic metadata")?;
        let relocations: Vec<_> = info
            .relocations
            .iter()
            .filter(|r| r.offset == offset)
            .collect();
        if relocations.len() != 1 {
            return Err("HLE binding does not have one matching relocation".into());
        }
        let r = relocations[0];
        if !matches!(r.kind, 257 | 1025 | 1026)
            || r.addend != 0
            || info
                .symbols
                .get(r.symbol as usize)
                .is_none_or(|s| s.name != b.symbol)
            || memory.read_u64(GuestAddr(b.destination))? != b.value
        {
            return Err("HLE relocation name/kind/addend/readback mismatch".into());
        }
        let relative = b
            .value
            .checked_sub(provider.base.0)
            .ok_or("HLE entry below provider base")?;
        let p = provider
            .nso
            .mod0
            .as_ref()
            .ok_or("HLE provider has no dynamic symbols")?;
        if !p.symbols.iter().any(|s| {
            s.name == b.symbol && s.section != 0 && (s.info & 15) == 2 && s.value == relative
        }) {
            return Err("selected HLE provider is not the exact exported function".into());
        }
        if !provider.nso.segments.iter().any(|s| {
            s.permissions.as_bytes().get(2) == Some(&b'X')
                && relative >= s.memory_offset as u64
                && relative.checked_add(4).is_some_and(|end| {
                    end <= s
                        .memory_offset
                        .checked_add(s.size)
                        .map(|n| n as u64)
                        .unwrap_or(0)
                })
        }) {
            return Err("selected HLE entry is outside executable segments".into());
        }
        names.insert(b.symbol.as_str());
        if let Some(hook) = hooks.get_mut(&b.value) {
            if hook.symbol != b.symbol || hook.provider != provider_index {
                return Err("conflicting HLE symbols/providers alias one entry".into());
            }
            hook.binding_sites += 1;
        } else {
            hooks.insert(
                b.value,
                HookDescriptor {
                    symbol: b.symbol.clone(),
                    provider: provider_index,
                    provider_name: provider.name.clone(),
                    provider_sha256: provider.nso.sha256.clone(),
                    address: b.value,
                    module_base: provider.base.0,
                    binding_sites: 1,
                },
            );
        }
    }
    if SYMBOLS.iter().any(|name| !names.contains(name)) {
        return Err("selected five-hook filesystem profile is not completely referenced".into());
    }
    Ok(hooks.into_values().collect())
}
