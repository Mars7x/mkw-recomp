# Bounded heap service reference

`heap_services::HeapServices` explicitly binds SVC 0x01 to an existing bounded
heap capability and coherent memory authority. The caller must select
`Abi::Nx64ZeroedBoundedHeap`. Nothing installs this registry by default or
establishes actual Switch 2 configuration.

The selected NX reference consumes full X1, requires 2 MiB alignment and size
strictly below 8 GiB, and reports InvalidSize for violations. Requests exceeding
the explicitly reserved capacity report OutOfMemory. Success returns W0=0 and
the real 64-bit heap base in X1, including zero-size and no-op requests. Reference
errors preserve X1. The host backing remains bounded by the existing 512 MiB
limit; there is no invented physical pool quota, alias accounting or resource
limit success. Only zero-filled heap growth is supported.

## Transaction and ownership

The service produces a private delayed resize intent. Constructing or dropping
that packet has no memory effect. Duplicate intents and mixing an intent with
memory outputs reject. The final commit checks the live owner, exact memory
container and capability generation under the existing memory mutex before
resizing. Register results and local-monitor clearing follow under that same
lock. Invalid packets preserve heap, registers and monitor state. Completed
reference errors clear the caller's local monitor like other completed SVCs.
Other owners lose reservations only when a changed heap granule overlaps them.

The SDK wrapper separately stores X1 through its saved application output
pointer after SVC returns. That later store can fault after a successful resize;
the service does not dereference application X0 or promise whole-wrapper
atomicity. A fresh identity-checked static probe confirms this transport in the
supplied binary without executing it.

## Evidence and limitations

Seven new tests cover lazy packets, namespace/generation and size failure
atomicity, 72 native request/budget traces, 32 wrapper/budget/fault traces,
zeroed regrowth, retained prefixes, monitor ownership and 256 concurrent
serialized requests. Native synthetic fixtures run at O0/O2 with fatal UBSan.
Existing debug/release, QEMU, supplied-binary and publication checks remain
required and unchanged. Linux/QEMU is not a Horizon heap-service oracle.

Primary references are pinned and hashed in [heap-services-reference.json](heap-services-reference.json).
The NX alignment/limit and error results are semantic references from
[Atmosphere's SetHeapSize implementation](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/svc/kern_svc_physical_memory.cpp)
and its NX board layout. Error-register preservation follows
[nxnative's SVC handler](https://github.com/seedds/nxnative/blob/c8ab8166f68296dc712a4ec13b892535d9d790c6/dm/host/hle_svc.c).
The Rust implementation is original; no upstream implementation is copied or
linked. GPL reference source remains private; nxnative is MIT licensed.

This removes one bounded startup dependency, with zero compiler-survey gain.
Actual target/PAC configuration, heap QueryMemory, argument-memory provenance,
SDK locking/error paths and title-installed TLS allocation callbacks remain.
No original game/SDK function has executed and no boot or frame is claimed.
