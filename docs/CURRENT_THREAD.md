# Scoped current-thread query binding

`ThreadQueries::registry_for_current_thread(&bridge, handle)` adds an explicit
current-thread binding to the existing initial priority/ID reference profile.
The supplied handle must name a real typed `ThreadIdentity` in this table. The
bridge must have a live monitor owner in its current memory generation. Binding
pins that identity; it does not allocate a replacement ID, rewrite entry arguments,
initialize an SDK thread object or start a host thread.

Only this registry's current-thread pseudo handle (low W1 == 0xffff8000) gains
resolution. Its memory container, authority generation and owner identity must all
match the active call. A second bridge over the same memory, a different memory
namespace, or replacement memory cannot inherit the binding. Dropping the bridge
releases its owner, so retaining the registry does not keep execution permission
alive. Caller-selected registries remain explicit capabilities, not a global
scheduler or a claim that a host thread is permanently assigned an identity.

Closing the original table handle leaves the pinned current identity alive until
the registry is dropped. Queries using that closed ordinary handle still fail.
Real-handle queries retain the existing table semantics; they are not restricted
to the current owner. Default `registry()` still rejects both pseudo handles.
The current-process pseudo handle remains unsupported even in a bound registry.
Final allowlists, duplicate-handler rejection and monitor clearing/refusal retain
their existing behavior. No priority inheritance, process/thread exit, arbitrary
kernel scheduling or retail Switch 2 authorization is supplied.

Three added native tests run at O0/O2 with fatal UBSan in debug/release: exact
priority/ID budgets and W-handle truncation; pin and final lifetime; closed,
foreign and wrong-kind handles; same-memory alternate owners, new owners after
drop, foreign memory and replaced generations; pseudo-process refusal, allowlist
restriction and monitor effects. A fourth additive guarded test promotes the
previous private fixture: 126 O0/O2 budget/error traces compose stack QueryMemory,
real-handle priority/ID queries and diagnostic PAC. That guarded fixture uses
real handles; it does not claim that the high-level guarded entry helper installs
a current-thread binding. Low-level guarded/service routes can use the explicit
registry with the bridge they already own.

The selected supplied initial constructor uses the real handle stored by
`nnosInitialize`, so pseudo resolution alone does not unblock that constructor.
Fresh static probes retain the measured initial-handle flow, separate ELF TLS/TLR
slot, mutable initializer/destructor pointers and another argument-region memory
query outside the stack-only capability. No callback is omitted, arbitrary TLS
size selected, argument mapping invented or original function executed.

The pseudo handle's reference behavior follows the already pinned MIT
[nxnative thread service implementation](https://github.com/seedds/nxnative/blob/c8ab8166f68296dc712a4ec13b892535d9d790c6/dm/host/hle_thread.c):
its current-thread token selects the current thread object for the priority/ID
getters. Our binding uses owned runtime identities instead of its pthread-local
lookup. No upstream implementation is copied or linked. Existing reference
identities/licenses remain in `thread-queries-reference.json`; this addition
makes no new retail compatibility claim. Existing native/QEMU suites remain
required; Linux/QEMU does not provide a Horizon query oracle.
