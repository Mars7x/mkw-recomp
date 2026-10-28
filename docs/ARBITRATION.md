# Owned arbitration backend

`arbitration::Arbitration` supplies a bounded host API for blocking on tagged
mutex words in the existing coherent memory authority. FIFO remains the default;
[logical priority inheritance](PRIORITY_ARBITRATION.md) is an explicit optional
policy. It is a prerequisite for
measured SDK critical-section slow paths, not a Horizon SVC implementation,
scheduler or verified Switch 2 runtime. A separate, explicitly selected
[bounded SVC reference binding](ARBITRATION_SVC.md) now uses this mechanism;
its kernel-result semantics do not replace the host API contracts.

## Explicit ownership and operations

Construct a domain with one memory generation and a capacity of 1–1024 actors
and retained wait tickets. Binding consumes a live `Bridge` and pins an existing
`ThreadIdentity` from the typed handle table. Duplicate thread bindings reject.
An `Actor` owns its bridge; dropping it cancels dependent waits before releasing
the execution owner. Closing the original table handle preserves the pin.

Native synthetic work can attach through the actor. `prepare_wait` checks the
owner's tag plus waiter bit under the domain-state lock and coherent memory lock,
then registers a ticket before releasing either. A changed tag requires no wait.
Tickets block on a predicate using a condition variable, with the memory mutex
released. Notifications before blocking cannot be lost. One actor cannot have
multiple pending waits; dependency cycles reject before mutation.

Under the default FIFO policy, `unlock` chooses the earliest registered pending waiter belonging to this owner
and address. It writes the new owner's real tag, retains the waiter bit if needed,
and transfers the remaining same-address waiters to that owner. All changes
commit while both locks remain held. A foreign tag, invalid address, stale memory
or failed write changes neither queue nor guest memory. Writes use the existing
coherent backend and invalidate only overlapping LL/SC reservations. This is a
host handoff, not an exception return; it does not independently clear a caller's
local monitor.

Dropping an unconsumed ticket cancels its registration. A clonable cancellation
capability also wakes an already blocked wait. Cancellation and handoff have one
winner; completed transfers are never rolled back. Tickets retain capacity until
consumed or dropped, and sequence numbers never wrap or reuse. Owner loss and
explicit domain shutdown wake pending waits without fabricating guest-memory
cleanup. Callers must cancel or shut down before tearing down/replacing memory;
replacement alone is not a notification source. A later continuation against a
replaced generation rejects. Shutdown refuses new domain work but does not
preempt a native attachment already executing.

## Why a bounded backend first

Fresh identity-checked static inspection identifies the SDK current-thread
getter and two critical-section bodies, totaling 240 source bytes. They consume
the real thread handle from the initialized thread object, use native CAS fast
paths, and retain SVC 0x1a/0x1b and terminal error paths. Uncontended execution does
not justify omitting those paths or inventing initialized TLS/thread objects.

The pinned [nxnative thread runtime](https://github.com/seedds/nxnative/blob/c8ab8166f68296dc712a4ec13b892535d9d790c6/dm/host/hle_thread.c)
is useful MIT reference material, but its direct host-pointer atomics, global
wait queue and Darwin scheduling/context code cannot be transplanted into our
checked x86-64 Linux memory authority. The pinned
[kernel ownership semantics](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/kern_k_condition_variable.cpp)
select waiters belonging to the current owner. Kernel scheduling integration,
termination handling, address policy, write-failure delivery and SVC transaction
integration require additional work. The default backend explicitly chooses FIFO and
strong failure atomicity; it must not be registered as a full kernel service.
Condition variables, guest clocks and deadlines are also outside its scope.

Source hashes and licenses are in [arbitration-reference.json](arbitration-reference.json).
The implementation is original Rust; GPL semantic reference sources remain
private and no upstream implementation is copied or linked. The existing runtime
architecture and acceptance rules are preserved.

## Validation

Eight unit tests cover ownership/address separation, reparenting, handoff before
blocking, real blocking/shutdown, cancellation, cycles, stale generations,
capacity/exhaustion, readonly faults, retained thread pins and 64 cancellation
versus handoff races. Two native integration tests run at O0/O2 with fatal UBSan:
16 CAS budget traces, overlapping/unaffected LL/SC checks, and 128 real worker
contention/handoff rounds using translated acquire/release atomics and protected
updates. Existing native/QEMU, release, supplied-binary and privacy checks remain
required. QEMU is an ISA cross-check, not a Horizon scheduling oracle.

No original function executes. Survey gain is expected to be zero; actual
startup synchronization, target/PAC configuration and TLS/argument provenance
remain higher-value dependencies than pursuing complete instruction coverage.

[Owned host condition waits](CONDITION_WAITS.md) add an explicit priority-only
release/register/reacquire backend. Their stronger host failure atomicity is not
the SVC 0x1c/0x1d error contract; those bindings remain unsupported.
