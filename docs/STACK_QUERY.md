# Owned stack QueryMemory reference profile

`stack_query::StackQuery` implements SVC 0x06 only for one explicitly declared
stack region. Construction requires an exact, page-aligned, nonempty owned RW
mapping. Subranges, extended ranges, read-only and executable mappings reject.
No mapping is rounded or enlarged. Region names do not assign stack semantics.

The handler binds both the shared memory container and its private generation
identity. Replacing the entire memory object inside the same mutex invalidates
that binding. The existing service packet commit checks the call owner's identity
again under the memory lock; replacement during a call cannot commit old-owner
outputs into a new memory authority. Sealed mappings have no remap/resize API.

For X2 inside that stack, the handler writes a 40-byte little-endian MemoryInfo at
X0: actual base/size, reference Stack state 11, attributes 0, RW permission 3,
IPC/device reference counts 0 and padding 0. W0 returns success and W1 page flags
0. Unknown addresses refuse with the existing unsupported diagnostic; no Free,
Inaccessible, heap, code or complete virtual address-space behavior is invented.
These zero fields describe this limited owned-memory model, not retail state.

All memory/register outputs use the existing checked packet. Invalid or read-only
output ranges produce the existing memory diagnostic, not a claimed Horizon
pointer-error code. A completed packet clears the local exclusive monitor;
refused or faulted packets preserve it and registers/output bytes. A subsequent
wrapper store can fault after the service has committed its MemoryInfo; the whole
wrapper is not an atomic transaction. Registry composition and the final process
allowlist remain explicit.

Six added tests cover exact mapping geometry, namespace and generation changes,
permissions, 280 native boundary/output/budget cases, 32 wrapper budget/fault cases,
register widths, monitor completion/refusal and partial cross-region write faults.
They run at O0/O2 with fatal UBSan in debug and release test suites. Existing
native/QEMU semantic checks remain required. Linux/QEMU is not a Horizon kernel
query oracle; this change adds no new kernel-compatibility claim.

A private identity-checked supplied-binary probe confirms the selected 24-byte
wrapper's X0/X2 input and W1 output transport, including its separate page-info
store. The measured thread initializer also needs priority and ID queries already
provided by the explicit initial-thread profile. No original function is loaded
or executed. Actual initial-thread/boot-parameter provenance, ELF TLS and TLR
initialization, target/PAC/signature configuration, error-path synchronization and
observer bindings remain unresolved. This is a bounded service integration, not
an SDK initializer replacement or permission to execute it.

Semantic references are pinned in `stack-query-reference.json`. Atmosphere GPL
sources were inspected privately for ABI facts only; no implementation was copied
or linked. The new Rust implementation is original. References:
[QueryMemory kernel entry](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/svc/kern_svc_query_memory.cpp),
[query region information](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/kern_k_page_table_base.cpp).
