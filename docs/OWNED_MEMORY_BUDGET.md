# Owned memory budget

`OwnedMemoryBudget::new(memory, allowance)` binds an explicitly supplied host allowance to one sealed `ExclusiveMemory` generation. `snapshot()` reports the allowance, current visible bytes, full committed backing capacity and allowance minus visible bytes. It takes the same mutex as native memory effects and heap resize, so each snapshot observes one committed state.

Every mapping must have a page-aligned base and page-sized length. All fixed mappings count, regardless of read/write/execute permissions. The heap counts only its currently visible prefix as usage; its entire reserved backing counts toward committed capacity. Construction requires enough allowance for that full capacity. Every currently permitted resize therefore remains within the allowance without changing existing heap behavior. All additions and subtraction are checked. Empty sealed memory is permitted with a nonzero page-sized allowance.

Snapshots do not read guest payloads, change memory, consume owner slots or invalidate exclusive reservations. Clones retain the same generation binding. Replacing the memory inside its container invalidates old capabilities; poisoned locks refuse. Ordinary mapping changes remain sealed and unsupported.

This is a host accounting foundation, **not a physical-memory model or GetInfo service**. It assumes separately owned mappings without guest aliases or shared backing. It does not discover host RAM, invent kernel resource quotas, round incomplete pages, classify SDK mappings, or infer secure/system memory overhead. A caller-supplied allowance is not evidence of the actual target's available memory. Existing eager heap backing and its 512 MiB cap remain unchanged.

## Validation

Five additive tests cover page/capacity/allowance validation, live growth and shrink, failed resizes, monitor preservation, stale generations, poisoned locks, and 2,048 concurrent snapshots during 1,024 resize operations. Forty-eight synthetic native SVC heap request/budget cases run at O0/O2 with fatal UBSan and compare accounting with committed heap state. The same suite runs in debug and release. Existing native and QEMU checks remain required; this change adds no CPU instruction semantics or original-code execution.

## Next integration

Supplied static startup inspection identifies total/used non-system process-memory queries before heap sizing and TLS allocator installation. The pinned NX reference computes those values from normal mappings, code, main-thread stack and resource/secure-memory state. The owned budget supplies a consistent local usage primitive, but does not establish that complete accounting model. Before exposing process queries, define the explicit supported process/resource contract, generation and current-process ownership, subtype/error ordering, and composition with the entropy handler on the shared GetInfo service number. Do not substitute a frozen usage snapshot for live heap changes or interpret host available RAM as a guest process limit.

The reference research remains semantic-only; no upstream implementation was copied into this module. Actual Switch 2 configuration, target identity, PAC, TLS and complete startup closure remain gates before original execution.
