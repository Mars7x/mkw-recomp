# Explicit owned memory-state catalog

`MemoryCatalog` creates coherent partitions of an explicitly declared guest address space. It is a host primitive, not a QueryMemory service or verified target memory map.

Construction requires sealed page-exact memory and a matching heap capability. Every fixed mapping must have exactly one declaration with identical base and full size. The caller explicitly selects Code, CodeData, Normal, Stack or ThreadLocal; mapping names and permissions do not select a state. Declarations are sorted, duplicate/missing/extra declarations reject, and permissions must be compatible with the selected host state. Code cannot be writable; CodeData is R or RW; Normal, Stack and ThreadLocal require RW. Free is never a valid fixed mapping declaration.

The declared address space must be nonempty, page-aligned and contain every mapping's full backing, including reserved heap capacity. Within this complete host model, the visible heap prefix is Normal/RW, and uncommitted addresses are Free with no permissions. Hidden backing is not reported as allocated guest memory. Snapshots coalesce adjacent identical states and permissions and cover the declared span exactly. Outside addresses have no catalog entry. No kernel attributes, IPC/device reference counts or inaccessible-region ABI are inferred.

Snapshots hold the coherent memory mutex and reject stale generations or poisoned memory. They neither consume owner slots nor change guest bytes or exclusive monitors. Heap resize remains the existing implementation; its committed prefix is reflected atomically in each snapshot. Fixed mappings remain sealed.

Five additive tests cover complete partitions, state/permission validation, coalescing, exact declarations, overflow, foreign capabilities, replacement, poison, owner/monitor preservation and concurrent resize. Concurrency testing performs 2,048 snapshots during 1,024 resizes. Eighteen synthetic native heap-service budget cases run at O0/O2 with fatal UBSan. Existing correctness checks and query handlers are unchanged.

A private supplied-input check declares host states for four page-mapped modules and a synthetic stack, verifies payload/zero-fill and existing relocation results, then checks complete partitions through heap growth/shrink. These state assignments and address bounds are explicit host configuration, not discovered retail Switch 2 facts. No original function executes. A bounded QueryMemory adapter still needs ABI output/fault behavior and explicit outside-range policy; target bootstrap/TLS/PAC gates remain unresolved.
