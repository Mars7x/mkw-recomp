# Explicit zero/infinite condition-variable services

`Actor::condition_services(Abi::Nx64OwnedUntimedCondition39, table)` provides
SVC 0x1c/0x1d through actor-bound delayed service intents. It requires the existing
priority policy and exact live bridge/memory generation. Existing arbitration
registries still contain only 0x1a/0x1b. Compose registries explicitly before
applying the final service allowlist. No registration is automatic.

The admitted reference subset uses addresses below 2^39 and wait timeouts zero
or minus one. Finite and other unselected timeouts, unsupported tags/lifecycles,
cycles and signal donation chains reaching pending condition membership refuse.
This does not verify Switch 2 address, shadow-stack, timer or scheduling policy.

## Wait and completion

Wait consumes full X0 mutex, full X1 key rounded down to four-byte alignment,
low W2 tag and signed full X3 timeout. Unaligned admitted mutexes return
InvalidAddress. Indefinite waits require the caller's real tag; zero-timeout
release does not use that tag or allocate another ticket.

The caller's mutex wait queue determines ownership handoff independently of the
word's current tag. An existing successor completes successfully before the key
hint is attempted and the mutex word is written. Failed key-hint writes are
ignored. A failed mutex write returns InvalidCurrentMemory to the caller while
retaining the successful successor handoff. This differs from both host atomicity
and SVC 0x1b's error delivery. Successful zero-timeout release returns TimedOut;
an indefinite wait registers condition membership atomically with release and
sleeps without holding the memory mutex.

Completion writes zero-extended W0 and clears the caller's local monitor.
Unrelated registers remain intact. Cancellation/shutdown does not fabricate a
kernel termination or timeout result; unsupported teardown preserves input
registers and retains already completed effects. Replacing memory requires
explicit cancellation/shutdown; replacement alone does not notify sleepers.

## Signal, reacquisition and bounded ordering

Signal consumes full X0 key rounded down to four-byte alignment and signed low
W1 count. Nonpositive counts select all. It has a void reference ABI: every input
register is preserved, while the caller's local monitor is cleared on completion.

Each selected waiter atomically acquires an empty mutex or sets its waiter bit
and joins the previous owner's queue. Typed owner resolution supports alias
handles. A missing/wrong-type owner delivers InvalidState to the selected waiter;
a valid typed but unbound owner refuses as outside the profile. Failed mutex
access delivers InvalidCurrentMemory. Key-hint clearing is attempted when no
matching condition waiters remain, with failed writes ignored. Signaling alone
does not imply that a waiter completed or acquired the mutex.

A queue/word plan preflights all unsupported states before mutation. Handled
kernel errors remain committed waiter results. The profile excludes any proposed
donation chain reaching a pending condition waiter, because priority reinsertion
can change the pinned kernel reference's saved tree iterator. Under this subset,
remaining condition priorities cannot change during the signal and ordered
selection is stable. Ownership chains ending in runnable actors, free mutexes
and earlier completed acquisitions remain supported. The separate host API keeps
its documented dynamic selection and stronger failure atomicity.

## Evidence and remaining work

Nine additive tests pass focused debug/release checks. Native O0/O2 fixtures with
fatal UBSan cover **112 request/budget traces**, foreign/denied bindings, **64
blocking acquisition rounds**, four two-waiter broadcasts, eight handled-error
or ignored-key-fault completions, and two shutdown cases. Unit checks cover
successor success despite release failure, transactional refusal, identity and
ticket exhaustion, handled-error delivery, and local versus unrelated monitors.
Existing tests, QEMU transport checks, native headers and acceptance rules remain
unchanged and required. QEMU is not a kernel-service oracle.

Primary semantic source hashes/licenses are recorded in
[the arbitration reference](arbitration-svc-reference.json) and
[the event result reference](EVENT_SERVICES.md). The implementation is original;
upstream code is not copied or linked. Pinned primary references are
[condition SVC validation](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/svc/kern_svc_condition_variable.cpp),
[wait/signal operations](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/kern_k_condition_variable.cpp)
and [result definitions](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libvapours/include/vapours/results/svc_results.hpp).

Fresh supplied static checks retain six condition-variable bodies /364 bytes and
two exact SVC/RET wrappers. Expected compiler-survey gain is zero. Finite waits
still require CNTPCT_EL0 and consistent guest deadlines; actual initial TLS,
boot arguments, target/PAC and lifecycle configuration remain unresolved. No
original function or library has executed/loaded; no boot or frame is claimed.
