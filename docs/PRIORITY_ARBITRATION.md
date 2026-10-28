# Logical priority inheritance for owned arbitration

`Arbitration::with_policy(..., Policy::InheritedPriority)` explicitly selects
logical priority donation and priority-ordered handoff. `Arbitration::new` still
selects FIFO, with no donation. Neither policy registers SVCs, changes Linux
thread priorities or authorizes original game execution.

Each actor starts with the initial priority from its pinned typed thread identity.
`Actor::priorities` reports the domain-local base and effective values.
`set_base_priority` is explicit host control over values 0–63, including for a
blocked actor. It does not alter the underlying `ThreadIdentity` or the existing
initial-priority SVC reference registry. Guest permission, termination overrides
and pinned scheduling behavior remain outside this API.

## Donation and ordering

A pending wait donates its caller's effective priority to its owner. An owner's
effective priority is the minimum of its base priority and every pending donor,
across all owned lock addresses. Changes propagate along the owner's outgoing
wait chain. Cancellation, ticket drop and ownership transfer restore affected
priorities; completed tickets no longer donate. Shutdown removes all pending
donations. Cycles remain rejected before registration.

Handoff selects the highest-priority pending waiter for the current owner and
address; numerically lower values have higher priority. Equal-priority waiters
follow insertion order. When a blocked actor's effective priority changes, its
wait is removed and reinserted after existing equal-priority waits. Its immutable
ticket identity is preserved. Reordering requires no new identity allocation,
so identity exhaustion cannot prevent cancellation or restoration.

Priority, queue and ownership changes use the existing domain mutex. Handoff's
guest-memory write remains inside the coherent memory transaction, with failed
writes leaving donation and queue state intact. The propagation loop is iterative
and bounded by the actor limit; no recursive host stack or background scheduler
is introduced. Inherited priority is logical state, not a promise about Linux
CPU scheduling, preemption or cross-core handoff timing.

## Evidence and reuse boundary

Pinned primary references document the donation/restoration chain and ordered
waiter tree:
[thread implementation](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/kern_k_thread.cpp)
and [waiter comparator](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/include/mesosphere/kern_k_thread.hpp).
The underlying tree insertion was also inspected to confirm equal-priority
insertion direction. [priority-reference.json](priority-reference.json) records
source identities and licenses. GPL and BSD reference sources remain private;
the implementation is original Rust with no copied or linked upstream code.

A fresh identity-checked supplied-binary probe retains the three relevant SDK
critical-section/current-thread bodies, 240 source bytes, and their SVC 0x1a/0x1b
and terminal-error dependencies. This change supplies one required host mechanism.
It does not establish complete kernel compatibility, initialize actual SDK thread
objects/TLS, bypass error paths or turn that source inventory into runnable closure.

## Validation and remaining work

Eight additive tests pass: six public host tests, one native integration test and
one internal exhaustion test. An independent reachability oracle checks all
125 acyclic four-actor ownership forests across 81 priority assignments: **10,125
configurations**, checking every actor before and after each edge cancellation.
Other checks cover transitive/multiple-lock donations, priority changes while
blocked, equal-priority reinsertion, a 64-actor chain, stale generations,
unchanged FIFO behavior, identity exhaustion and separation from initial thread
queries. **32 native contention/handoff rounds** execute synthetic translated
atomics and protected updates at O0/O2 with fatal UBSan.

All prior tests and native headers remain unchanged. The existing full native,
QEMU, supplied-binary and publication gates remain required; QEMU is not a Horizon
scheduler oracle. Survey gain is expected to be zero. SVC admission/completion,
kernel write-failure delivery, lifecycle/pinning/termination policy and actual
TLS/argument/target/PAC configuration remain the next dependencies. No original
function executes and no boot, rendering or playability is claimed.
