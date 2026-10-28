# Explicit page-owned module images

`module_pages::Plan::new(&modules)` validates an opt-in page layout before allocating backing storage. The plan borrows immutable module payloads; `materialize()` returns independent owned memory suitable for checked relocation and later sealing. The existing `map_modules` diagnostic loader retains its exact-byte behavior.

This profile requires page-aligned module bases and nonempty segment offsets, exactly three NSO segments, matching declared/payload lengths, and the standard text/read-only/data permission classes. Execute-only text remains unreadable. Each segment extends to its final 4 KiB page with zero padding and the same permissions. Declared BSS is combined with data before rounding, so their shared page is charged once. Unclaimed address-space gaps remain unmapped.

All size/address arithmetic is checked. The existing 512 MiB per-region limit also applies after padding. Overlapping page ownership rejects across segments and modules, even when permissions match. All plans are validated before materialization; allocation failure returns an error without modifying caller-owned memory. Repeated materializations do not share mutable guest buffers.

The resulting page-exact memory composes with `OwnedMemoryBudget` and the existing bounded heap. This establishes an explicit owned layout, not actual Switch 2 physical-memory accounting, kernel resource limits, virtual-memory services or target authorization. Callers must verify source identity and link policy separately. No runtime routes, original entry points or automatic service bindings are installed.

## Validation

Six additive debug/release tests cover payloads, BSS, zero padding, boundary accesses, gaps, permissions including execute-only text, malformed metadata, overflow, ownership collisions, independent copies, sealing and heap/accounting composition. Forty synthetic native load/store budget cases run at O0/O2 with fatal UBSan. Existing diagnostic-loader tests, native headers, QEMU checks and compiler acceptance rules remain unchanged; the release gate includes the new suite.

A private, identity-checked supplied-module probe constructs four images in 12 regions, compares every payload and verifies zero BSS/padding before relocation. It applies and reads back 309,844 relocation writes. Import selections match the exact-byte mapper under the same existing diagnostic rtld-first process order. The initial probe used the dependency-only policy and correctly rejected an unresolved import; the comparison was corrected to use the existing diagnostic policy on both layouts. No production link rule changed.

No game-derived native object or original function is loaded or executed by this check. Images, identities, addresses and detailed evidence remain private. Compiler coverage gain is expected to be zero: the next work is an explicit process/resource query contract, current-process ownership and safe composition with existing GetInfo entropy services.
