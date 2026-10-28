# Explicit guarded entry composition

`guarded_entry::run` executes one registered trusted synthetic entry on the
calling host thread. It composes the existing v4 source-aware router, coherent
memory, explicit service registry and owned diagnostic PAC backend. It accepts
an explicit `Worker` description and returns only the existing architectural
`Outcome` snapshot. Native library/helper trust and original-execution gates
remain separate; this function cannot authorize game execution.

Before reserving a monitor owner or entering native code it checks the exact
registered entry, aligned return marker, aligned stack bounds, nonempty bounded
stack/TLS/TLR ranges, both thread-register pointers, read/write mappings and
pairwise disjoint storage. The frame receives the registered module base, eight
arguments, return marker, stack top and explicitly supplied software thread
registers. It does not invent SDK thread fields, initialize TLS templates or run
constructors. Memory contents are supplied by the caller and remain intact
until guest operations change them.

PAC, memory and route attachments live inside the execution scope. Return,
fault and budget exhaustion release the monitor owner; normal completion at a
wrong return PC is rejected. Completed guest memory effects remain on failure.
Returned snapshots contain no callback or context pointers. Storage disjointness
is checked within this invocation; callers remain responsible for ownership
across other simultaneous invocations. No host thread/stack allocation,
scheduler, affinity, lifecycle or FP backend is introduced.

Four new tests pass focused debug/release checks with O0/O2 fatal UBSan:

- 156 exact budget/error cases compose explicit TLS/TLR accesses, paired guest
  stack transport, PAC and two entropy requests. Fault and budget paths retain
  completed memory writes and release the single-owner quota.
- 32 preflight refusals cover registration, alignment, empty/oversized/overflowing
  or overlapping ranges, invalid thread pointers, mapping permissions and owner
  quota. No guest store occurs before successful preflight.
- Wrong-return and memory-fault cases preserve completed stores and free the
  owner for subsequent entries.
- Three synthetic Linux/QEMU runs check PAC, paired stack operations and TPIDR_EL0
  transport; 66 native budget traces compare their results at both optimizations.
  QEMU has its own PAC keys/profile; this is transport evidence, not a Horizon
  TLS initialization or Switch 2 compatibility oracle.

## Entry with an existing arbitration actor

`Actor::run_guarded` uses the same entry/storage validation, frame setup, scoped
PAC/router attachments and return checks with the actor's existing bridge and
memory. It does not allocate another monitor owner. Actor attachment rechecks
liveness, generation and pending-wait state before entering native code. A mutable
actor borrow prevents concurrent entry through the same actor.

This allows an actor-bound arbitration service registry to compose with guarded
entry. Foreign actor registries still refuse. Return, fault, budget exhaustion
and preflight rejection retain the actor and its owner; dropping the actor remains
responsible for its lifetime. The default `guarded_entry::run` still creates and
releases a fresh bridge. Neither path initializes SDK TLS or grants original-code
execution authority. Storage ownership across concurrent actors remains the
caller's responsibility.

Three additive tests cover ten O0/O2 blocking PAC/SVC budget combinations,
36 storage/entry preflight refusals, repeated successful entry with the owner
quota full, foreign service identity, shutdown/stale generation, and completed
service effects surviving budget exhaustion or wrong return. Synthetic C uses
fatal UBSan. Existing entry tests, including QEMU PAC/thread-register transport,
remain unchanged and required.

## Measured remaining boundary

A private all-instruction audit of the 16 previously compiled normal/proof
fragments finds six ordinary calls outside that set. Five initial SDK import
bindings concern abort preparation, mutex ownership/locking and abort observers;
the remaining local wrapper issues SVC 0x26. Inspecting the five selected exports
finds thread-register accesses in abort preparation, indirect observer callbacks
and read-lock dependencies. Four are strict-liftable; mutex locking still has a
terminal-call graph dependency. Initial linkage is diagnostic, not attested.

The 16 objects therefore are not a complete failure-runtime closure. The new
runner supplies checked composition, not those missing routines or a guessed
SDK thread layout. Actual observer/thread state, synchronization, service and
PAC/target configuration remain higher-value work than broad survey expansion.
No original function or library is executed/loaded. Compiler counts are unchanged:
214,521 strict; 214,585 v1/v2; 214,664 v3; 218,749 guarded v4.
