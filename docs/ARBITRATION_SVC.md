# Explicit owned lock-arbitration SVC reference

`Actor::arbitration_services(Abi::Nx64OwnedArbitration39, table)` creates an
explicit SVC 0x1a/0x1b registry bound to that actor, its coherent memory generation
and a typed handle table. It requires `Policy::InheritedPriority`; FIFO domains
refuse. Final service allowlists still apply. Nothing installs this automatically
or verifies Switch 2 target configuration.

The selected reference scope is ordinary owned guest memory below 2^39. It has
no shadow-stack or high/kernel-VA policy, guest clock, host scheduling guarantee,
termination ABI or dynamic thread lifecycle. Matching lock requests require a
live bound owner and the caller actor's real tag. Unsupported lifecycle/tag or
cycle/capacity states refuse rather than invent kernel results. A valid typed
thread without an active actor is unsupported, not reported as an invalid handle.

## Transport and completion

Lock consumes low W0 as the owner handle, full X1 as the address and low W2 as
the requested tag. Unlock consumes full X0 as the address. Completed operations
write only W0, zero-extending into X0, and clear the caller's local exclusive
monitor. Unrelated registers and reservations remain intact except for actual
overlapping memory writes.

A lock first reads the checked aligned word. If it differs from the requested
owner handle with the waiter bit, the call completes successfully before looking
up the owner or inspecting the requested tag. A matching word requires a real
thread object in the supplied table; absent/wrong-type handles return
InvalidHandle. Contention registers a genuine wait and propagates priority.
The host thread sleeps with the memory mutex released. Notifications before the
host starts sleeping remain recorded in the wait predicate.

Reference unlock selects the current owner's highest-priority waiter at that
address, independently of the word's existing tag. It writes the selected tag,
transfers remaining waiters and completes the selected wait. With no waiter it
writes zero. A failed write still completes the selected wait with
InvalidCurrentMemory and transfers remaining ownership, matching the inspected
reference behavior. This is distinct from the existing host `Actor::unlock`,
whose stronger failure atomicity and tag checks remain unchanged.

Unaligned admitted addresses return InvalidAddress; failed user-memory accesses
return InvalidCurrentMemory. Addresses outside the explicit window refuse.
Host cancellation, owner loss or shutdown stop a blocked SVC with an unsupported
host fault, preserving its input registers rather than fabricating a guest
termination/timeout result. Already completed handoff effects are not rolled back.
Callers must cancel/shut down before replacing memory; replacement alone does not
notify sleepers. No preemption of already executing native attachments is claimed.

## Transaction boundary

The bounded handler constructs a private delayed intent without effects. Intents
cannot mix with heap resizing, memory outputs, register outputs or another intent.
Private intent storage is boxed; the public reply shape and native ABI are unchanged.

The typed commit path runs before the ordinary service memory lock, taking locks
in domain-state then memory order. It checks the active actor, exact bridge owner,
container and generation before mutation. Waiting releases the memory guard;
completion reacquires and validates it before the result and monitor transition.
A pending-ticket guard removes interrupted waits and restores donations. Handlers
receive no mutable CPU or arbitrary commit callback. Ordinary service packets
retain their previous behavior.

## Evidence and limits

Pinned primary sources cover
[lock SVC validation](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/svc/kern_svc_lock.cpp)
and [wait/handoff semantics](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/kern_k_condition_variable.cpp).
[arbitration-svc-reference.json](arbitration-svc-reference.json) records hashes and
licenses. The Rust implementation is original; upstream implementation is neither
copied nor linked, and reference source remains private.

A fresh exact-source probe confirms the selected supplied SVC/RET wrappers for
both operations without executing them. Their critical-section callers retain
actual current-thread/TLS, PAC, atomic fast-path and terminal-error dependencies.
No original function has executed; this is a bounded service integration, not a
complete runnable startup closure.

Eight additive tests cover packet laziness/mixing, owner/namespace/generation and
registry-lifetime refusal, default FIFO rejection, monitor completion/refusal,
priority transfer and reference write-failure delivery. Native O0/O2 fixtures
with fatal UBSan include **96 request/budget traces**, **32 two-waiter priority
handoff rounds**, four blocked-write-fault cases and four teardown cases. Addresses
above 4 GiB and high input-register bits exercise the actual transport widths.
Existing native/QEMU, release, supplied-binary and privacy checks remain required
and unchanged. QEMU is an ISA cross-check, not a Horizon scheduling oracle.

Expected compiler-survey gain is zero. Full lifecycle/termination and scheduling
policy, condition-variable waits, initial thread/TLS/argument provenance and
actual target/PAC configuration remain dependencies. No boot or frame is claimed.
