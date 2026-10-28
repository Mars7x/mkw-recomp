# Actor-bound initial thread queries

`ThreadQueries::registry_for_actor` binds current-thread priority/ID queries directly to an arbitration actor's pinned thread identity and existing bridge owner. It avoids a separate query binding assembled before transferring bridge ownership into the actor. Existing bridge-based binding and default registries are unchanged.

Construction resolves the actor's original real handle in the selected table, validates the domain and memory generation under the existing state-to-memory lock order, and requires the same pinned thread object. A wrong table, closed handle, shutdown domain or replaced memory refuses. No new monitor owner, thread identity, handle or SDK thread object is allocated.

After construction, closing the real handle preserves the current-thread pin; queries using the closed real handle retain the existing invalid-handle result. Current-thread pseudo queries require this exact live bridge owner. Other actors and replacement bridges cannot inherit the binding. Dropping the actor retires its owner even while the registry retains an object reference.

This is the existing initial-thread query profile. GetPriority reports the registered initial priority; it does not report arbitration's mutable base/effective priority or Linux scheduling state. Dynamic scheduling semantics remain a separate integration. No original execution authority, TLS initialization or bootstrap register configuration is supplied.

Five additive tests pass in debug and release. Synthetic native O0/O2 tests with fatal UBSan cover identity/budget boundaries, real-handle closure, cross-actor refusals, owner capacity, lifetime, wrong tables, shutdown, stale memory and 32 wrapper/fault cases. Ten guarded PAC-entry cases verify the same identity and instruction-budget semantics through `Actor::run_guarded`, with no spare owner slots. Existing tests and QEMU checks remain unchanged.

Fresh supplied static inspection retains the bounded 208-byte ThreadManager implementation and its actual QueryMemory, priority, thread setup and ID dependencies. It does not execute that implementation or synthesize SDK thread structures. Actual bootstrap/TLS/PAC/target configuration and remote-region behavior remain unresolved; compiler survey gain is expected to remain zero.
