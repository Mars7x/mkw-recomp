//! Consistent opt-in process queries over one owned memory configuration.
use crate::{
    memory_catalog::MemoryCatalog,
    memory_query::{self, MemoryQuery},
    owned_memory_budget::OwnedMemoryBudget,
    process_entropy::ProcessEntropy,
    process_memory::{self, ProcessMemory},
    process_regions::{ProcessRegions, Span},
    services::Services,
};

pub enum Abi {
    /// Private-page accounting and catalog queries with explicit reservations.
    /// Uses checked host output faults, not retail kernel error ordering.
    Nx64PrivateOwnedProcess,
    /// Explicit host success protocol for remote GetInfo29/30. Requires a
    /// nonzero 64KiB-aligned reservation; no retail error/bounds claim.
    Nx64PrivateOwnedProcessWithObservedRemote64K,
}
/// Build GetInfo and QueryMemory together only when their capabilities refer to
/// the same container/generation, address space and declared stack role. ASLR
/// remains an explicit enclosing domain. This grants no execution authority and
/// adds remote queries only under the explicit observed-success variant. No
/// thread identity or kernel resource model is supplied.
pub fn registry(
    abi: Abi,
    budget: OwnedMemoryBudget,
    regions: ProcessRegions,
    catalog: MemoryCatalog,
    aslr: Span,
    entropy: Option<&ProcessEntropy>,
) -> Result<Services, String> {
    if !catalog.matches_budget(&budget) || !regions.matches_budget(&budget) {
        return Err("process query capabilities have different memory identities".into());
    }
    let layout = regions
        .snapshot()
        .map_err(|e| format!("process region snapshot: {e:?}"))?;
    if !catalog.agrees_with_regions(layout) {
        return Err("process query address space or mapping roles disagree".into());
    }
    let query = MemoryQuery::new(memory_query::Abi::Nx64PrivateCatalog, catalog)
        .map_err(|e| format!("process memory catalog: {e:?}"))?
        .registry();
    let info = ProcessMemory::new(process_memory::Abi::Nx64IsolatedPrivatePages, budget)
        .with_regions(regions, aslr)
        .map_err(|e| format!("process region configuration: {e:?}"))?;
    let info = match abi {
        Abi::Nx64PrivateOwnedProcess => info,
        Abi::Nx64PrivateOwnedProcessWithObservedRemote64K => info
            .with_observed_remote_region()
            .map_err(|e| format!("observed remote region configuration: {e:?}"))?,
    };
    let info = match entropy {
        Some(e) => info.registry_with_entropy(e),
        None => info.registry(),
    };
    info.combined(&query)
}
