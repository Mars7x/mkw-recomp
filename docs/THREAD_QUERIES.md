# Owned initial thread queries

`ThreadIdentity::register` creates an owned logical thread record in an explicit
handle table. Each record has a nonzero, monotonically allocated 64-bit identity
and a caller-configured initial priority in 0..63. It does not launch a host or
guest thread, initialize an SDK object, grant process permissions or configure
scheduling. IDs are never reused or wrapped; failed table insertion can consume
an ID but publishes no handle. Existing table quotas, typed lookups, exported
pins and cross-table import rules apply. Closing a handle does not invalidate
other owned references or mean that a running thread was terminated.

`ThreadQueries::registry` explicitly installs only GetThreadPriority (0x0c) and
GetThreadId (0x25). Both consume the low W1 handle and return W0 status; priority
uses W1 and identity uses X1. A valid lookup reports that exact owned record.
Unknown, stale, foreign or wrong-kind handles return reference InvalidHandle
with a zero value output. Pseudo current-thread/current-process handles refuse
as unsupported, with no fabricated current-thread binding. Final process
allowlist restriction and registry conflict checks remain required.

This is an **initial-state reference profile**: the priority is fixed, with no
priority inheritance, SetThreadPriority, scheduler, waits, lifecycle or current
thread binding. Registry selection does not verify Switch 2 compatibility or
authorize original execution. No SDK ThreadType fields or TLS templates are
written by these handlers.

The getter semantics and range were checked in pinned
[Atmosphere thread services](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/svc/kern_svc_thread.cpp).
The register/result convention, including zero value on an invalid handle, was
also checked in pinned
[nxnative thread bindings](https://github.com/seedds/nxnative/blob/c8ab8166f68296dc712a4ec13b892535d9d790c6/dm/host/hle_thread.c).
This original Rust implementation uses our existing typed table and packet
transport; upstream code is not copied or linked. Reference identities and
limitations are recorded in `thread-queries-reference.json`; GPL reference
source remains private. These references are not a retail Switch 2 oracle.

## Evidence and remaining dependency

Seven new tests pass focused debug/release checks. Native fixtures compile at
O0/O2 with fatal UBSan. They cover 1,024 exact priority/ID budget traces across
all 64 priorities, upper input bits, handle transfer/lifetime/type/namespace,
invalid/pseudo/denied calls, wrapper memory output, local-monitor clearing versus
refusal, 256 concurrent identity registrations, exhaustion and full-width ID
transport. Another 102 guarded PAC/entry budget traces exercise successful
initial queries, error loops and denied second queries. Existing native/QEMU
transport and supplied-binary checks remain required; no Linux test is presented
as a Horizon service oracle.

The fresh supplied export probe confirms the selected initializer uses SVC
0x06, 0x0c and 0x25. This change supplies two reference handlers; **QueryMemory
0x06, real initial-thread ownership/binding, ELF TLS, boot-parameter provenance
and actual target/PAC configuration remain open**. The SDK's own initializer
writes its fields; it is not replaced with a guessed zero-filled object.
No original code is loaded or executed. Survey gain is zero: 214,521 strict;
214,585 v1/v2; 214,664 v3; 218,749 guarded v4. Runtime prerequisites have priority
over broader survey-only work.
