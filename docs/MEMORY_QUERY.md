# Explicit private-catalog QueryMemory

`MemoryQuery` is an opt-in SVC 6 handler over `MemoryCatalog`. `Nx64PrivateCatalog` requires a zero-based declared address space and explicitly models private pages with zero attributes, IPC/device counts and page flags. It does not discover a target layout or establish retail compatibility.

The full X2 query address selects a coherent catalog entry. The handler writes a 40-byte LP64 MemoryInfo to X0, returns W0=0 and W1=0, and preserves the other registers. State values are Free=0, Code=3, CodeData=4, Normal=5, Stack=11 and ThreadLocal=12; permission bits reflect actual mappings. Heap growth/shrink changes the Normal prefix through the existing catalog. Adjacent identical mappings can share an entry.

For addresses at or above the declared address-space end, the selected profile returns Inaccessible=16, base=end, size=unsigned wrapping negative end, and zero permissions/attributes/counts. This follows the inspected reference's outside-address behavior while restricting construction to a zero-based space. ASLR bounds are not substituted for address-space bounds.

Every call validates memory container, generation and live owner. The catalog snapshot supplies a linearization point; packet commit rechecks the owner and validates the entire output range before writing. Invalid output memory produces the existing host memory fault and preserves the output/register packet. This deliberately retains the checked host contract rather than claiming the kernel's safe-user-pointer/result-code ordering. A later wrapper page-info store may fault after a completed MemoryInfo write. Existing instruction budgets and exclusive-monitor completion rules remain unchanged.

The stack-only QueryMemory handler is unchanged. Registering both handlers still rejects a duplicate SVC. No original routes or target configuration gates are enabled.

Semantic references are the previously pinned Atmosphere revision `6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8`, `kern_svc_query_memory.cpp`, `kern_k_page_table_base.cpp` and memory-state enum. These GPL-2.0 files remain private semantic references; no implementation is copied or linked. Linux QEMU is not an oracle for Horizon kernel services; existing QEMU transport checks remain required.

Five additive tests pass in debug and release. Native O0/O2 tests with fatal UBSan cover 96 state/address/budget cases, 32 output/fault cases, 32 wrapper cases, scoped refusals and 1,024 concurrent queries during heap resizing. Tests distinguish states sharing RW permissions, cross mapping boundaries, preserve monitors on refusal and retain completed SVC effects on later wrapper faults.

A private synthetic native probe queries twelve supplied module mappings plus a synthetic stack, an uncommitted gap and an outside address. It retains payload/zero-fill and relocation verification. The declared states and bounds are host configuration; only synthetic code is executed. Actual bootstrap/TLS/PAC/target configuration and remote-region semantics remain unresolved.
