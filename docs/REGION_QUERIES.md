# Explicit current-process region queries

`ProcessMemory::with_regions` adds GetInfo types 2/3 (alias), 4/5 (heap), 12/13 (ASLR) and 14/15 (stack) to the existing explicitly selected NX reference profile. The default registry is unchanged. Types 29/30 (remote) remain unsupported: their observed Switch 2 call sites do not establish complete service semantics.

Construction requires the catalog and accounting capabilities to identify the same memory container and generation. It rejects stale catalogs. ASLR bounds are separate explicit input: a nonempty, page-aligned, checked span contained within the address space and enclosing all five host reservations. The disjoint code reservation is not substituted for ASLR. This conservative enclosing-domain contract is host configuration, not target layout discovery.

Calls validate memory generation and live owner. Supported queries validate full-width subtype before classifying the low 32-bit handle; only the current-process pseudo-handle is supported. InfoType uses its low 32 bits. Nonzero subtype produces the existing invalid-combination result; unmodeled handles refuse. Successful replies write W0=0 and full-width X1. Reservation sizes do not change when the heap commitment changes. Existing service completion, exclusive-monitor and instruction-budget rules apply. A subsequent wrapper store can fault after a successful query.

One GetInfo handler explicitly dispatches region, accounting and optional entropy cases. Duplicate service registration still rejects; there is no fallback after handler effects. The entire composed profile remains memory-generation scoped. No original routes or target configuration gates are activated.

The semantic reference is Atmosphere revision `6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8`, `kern_svc_info.cpp` and `kern_k_page_table_base.cpp`. This GPL-2.0 source is a private semantic reference; no implementation is copied or linked. Its ASLR queries return the alias-code domain, which contains other allocation regions for the inspected 39/42-bit layout. It does not establish retail Switch 2 remote-region behavior.

Four additive tests cover constructor containment/overflow/identity checks, native request widths and error ordering, monitor completion, missing/foreign/stale/restricted profiles, entropy/accounting composition and wrapper writeback faults. Native tests run at O0/O2 with fatal UBSan, including 80 request/budget traces and 32 wrapper/fault traces. Prior tests and acceptance rules remain unchanged.

Supplied static evidence identifies eight of these supported InfoTypes among ten distinct types in three bounded startup helpers. The two remote InfoTypes still block that complete path. This is compatibility with an explicitly configured host reference profile, not demonstrated original execution. Survey coverage is expected to remain unchanged; actual boot parameters, TLS, PAC and target identity remain unresolved.
