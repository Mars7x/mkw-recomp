# Owned host condition waits

`Actor::prepare_condition_wait(mutex, key)` atomically releases an owned mutex
and registers an indefinite condition wait in the existing arbitration domain.
`Actor::signal_condition(key, count)` selects up to `count` waiters; zero selects
all. It returns the number selected, which can exceed the number completed.
These are explicit host APIs requiring `Policy::InheritedPriority`. No SVC,
clock, timeout, kernel error result or Switch 2 configuration is supplied.

A selected waiter acquires an empty mutex or joins its active owner's queue.
It completes only after acquisition or later ownership handoff. Donation follows
mutex ownership edges; condition membership has no owner edge. A condition waiter
can still own other mutexes and receive donations. Changes to its effective
priority reinsert it after existing equal-priority waiters. The host signal
policy chooses the highest current effective priority at each selection, so a
donation during a multiwaiter signal can change subsequent selection.

## Ownership and atomicity

Mutex and key must be distinct aligned writable words. The mutex must contain
the caller's real tag, optionally with the waiter bit. Active typed actors,
exact memory generation, cycle checks and the existing bounded ticket pool are
required. Transitions retain their ticket identity. Completed tickets occupy
quota until consumed or dropped. FIFO remains the default and refuses these APIs.

Prepare and signal hold domain state then coherent memory. They plan queue and
priority changes before applying them. A small word overlay makes later planned
acquisitions see earlier ownership changes in the same signal. All mapping,
owner, cycle, capacity and identity checks precede commit. Refusal leaves queue
state, memory and exclusive reservations unchanged. The same memory guard keeps
preflighted writes valid through commit. Successful writes invalidate overlapping
reservations through the ordinary coherent backend.

The key hint is set before mutex release; queue registration completes before
notification. Wait predicates retain notifications that arrive before the host
thread sleeps. Waiting releases the memory mutex. Signal clears the key hint
when no condition waiters remain. Cancellation can leave a stale positive hint;
a later signal safely reconciles it. Hint values are not the queue authority.

Cancellation, actor loss and shutdown cover both condition and mutex phases and
restore donations. Replacing memory refuses later operations and continuations;
replacement alone does not notify sleepers, so the host must cancel or shut down.
No scheduler preemption or host priority change is implied.

## Deliberate boundary before kernel bindings

This host contract has stronger failure atomicity than the inspected kernel
reference. Reference condition release can hand off a mutex before a later
write fails; hint writes can fail without aborting the operation. Reference
signal delivers some failures to selected waiters and has a void service ABI.
Its mutable tree-iterator broadcast order also requires separate analysis when
donations reinsert remaining condition waiters. The host policy here is explicit;
it is not claimed to reproduce that iterator behavior.

[Explicit bounded SVC 0x1c/0x1d bindings](CONDITION_SERVICES.md) now preserve
reference register/error ordering for a narrower admitted lifecycle/order subset.
Positive waits also need a shared guest counter/deadline model. The supplied finite-wait helper
reads currently unsupported CNTPCT_EL0; implementing host sleep alone would not
close that dependency. No original function or library has executed/loaded.

## Evidence and provenance

Nine additive unit tests cover an independent stable-order oracle over **243
priority/update configurations**, donations into condition waiters, shared-mutex
broadcast, existing-waiter release, dynamic reselection, transactional refusal,
heap-shrink faults, cancellation, owner loss, shutdown, stale generations,
capacity retention and identity exhaustion. Two native tests run **128 O0/O2
rounds** with fatal UBSan, checking protected work after reacquisition, early
signals and release/registration races. Existing native, QEMU, release, survey
and private-artifact checks remain required and unchanged.

The implementation is original. Pinned primary semantic references and their
licenses/hashes are retained in [the arbitration reference](arbitration-svc-reference.json)
and [priority reference](PRIORITY_ARBITRATION.md). Upstream source stays private;
no kernel implementation is copied or linked. The relevant primary sources are
[condition-variable operations](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/kern_k_condition_variable.cpp)
and [priority restoration](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/kern_k_thread.cpp).

Fresh static supplied-input checks retain six selected condition-variable bodies
(364 bytes) and two exact SVC/RET wrappers. This is a measured runtime dependency,
not a complete startup closure. Expected compiler-survey gain is zero. Actual
TLS/boot-argument, target/PAC and kernel lifecycle configuration remain open.
