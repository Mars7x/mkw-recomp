# Scoped translated workers

The runtime can launch a predeclared batch of synthetic CPU-v10/table-v2 workers on host threads. This advances the ownership and translated-entry part of the upstream threading plan. It is not a Horizon CreateThread/StartThread implementation, a scheduler, or authorization to run original code.

## Contract

The supervisor explicitly attests concurrent safety through an unsafe launch boundary: every reachable native target/helper must be reentrant or synchronize its host state. ABI markers alone do not prove this. The supervisor provides registered native entries, eight initial integer arguments, aligned return markers, instruction budgets and explicit guest stack/TLS/TLR storage. All ranges must be readable/writable owned normal memory and mutually disjoint within and across the batch. The software thread-register values must point inside their declared storage. The runtime checks the entire batch before entering any worker. Storage contents/layouts are supplied by the caller; no guessed SDK thread object, fixed TLS size or implicit constructor is installed.

The supervisor reserves every coherent memory-owner identity before launch. Each worker creates its own CPU frame, attachment and nested-call context on its own host thread. All workers retain the same owned memory authority. Code libraries remain borrowed until every scoped worker joins. Results contain architectural values and remaining budgets, not native callback/context pointers.

A launch latch prevents native entry until every host spawn succeeds. A partial spawn error or supervisor unwind aborts pending workers and wakes them for cleanup. Every started thread joins. Native faults retain their statuses and snapshots; siblings still finish/join. An invalid successful return PC reports an error. Completed memory effects are retained on execution errors; this is not process rollback. Drop releases every owner and reservation, including at owner-quota and launch failures.

Limits are explicit: 1–64 workers, host stacks 128 KiB–64 MiB, and each guest storage range 1 byte–16 MiB. Guest SP starts at the end of its 16-byte-aligned stack. Guest stacks are checked buffers, not host AArch64 stacks. Host scheduling/affinity and elapsed time are not presented as verified guest behavior.

## Evidence

Six new Rust tests (five integration, one launch/unwind unit test) pass debug/release. Native fixtures compile at O0/O2 with fatal UBSan. They cover:

- Eight workers with distinct software TLS/TLR pointers, checked loads/stores and stack transport; every budget 0–14 checks exact PC/SP and deferred effects.
- Four workers executing generated LDAXR/STLXR retry loops: four rounds of 4,000 increments, **16,000 increments per optimization**. Thirty-two repeated reserve/return/join cycles per optimization check owner capacity and reservation cleanup.
- **24 serial native/QEMU cases per optimization** compare values, loop status and independently computed instruction budgets across counts and integer wraparound. Concurrent host scheduling is checked against the serialized authority, not claimed equivalent to QEMU or hardware weak memory.
- Whole-batch rejection for unknown/host-only entries, bad return markers, stack alignment, missing/read-only/overlapping/overflowing storage, invalid thread pointers, and host/owner quotas. Failed preflight and owner reservation leave sibling data unchanged.
- Fault status/address propagation, wrong-return errors, sibling completion/join, pointer-free results, and supervisor unwind waking eight aborted launch waiters. The launch unit test exercises the abort mechanism; it does not force a real OS allocation failure.

Existing native, PAC, FP, memory, filesystem and survey checks remain required and unchanged. All 67 preceding crate test/support files, ABI-v9 definitions and published CPU/exclusive headers remain byte-identical. Default survey gain is **zero**; conditional v1/v2 counts remain unchanged. Only author-created synthetic code executes.

## Upstream relationship and remaining work

Lifecycle/ownership design was compared with nxnative `dm/host/thread.c` and `hle_thread.c` at `c8ab8166f68296dc712a4ec13b892535d9d790c6`. This is an independent Rust adapter around our existing coherent router. No native ARM64 context/stack code, Mach ports/QoS, fixed Switch 1 TLS constants or guest thread-object layout was copied. Existing upstream filesystem attribution is retained unchanged.

Actual target configuration, SDK TLS templates/layouts, startup ABI, typed SVC transport, dynamic creation/start/exit, handle lifetime, waits/events/cancellation, priorities/affinity and scheduler/memory-order contracts still need implementation and verification. PAC/FP backends are not installed by this runner; unsupported operations still refuse. Generic native thread launch is not evidence of a verified Horizon thread model or Mario Kart World startup. The next useful work is these runtime contracts and the observed graphics-control-format mismatch, not chasing 100% static survey coverage.

## Explicit service-table-v3 transport (October 27)

See [SVC.md](SVC.md): unchanged CPU-v10 layout, explicit typed SVC callback and X0–X7 W/X reply packets, checked atomic memory outputs, completion-time local-monitor clearing, terminal thread-exit propagation and scoped worker attachment. No Horizon service is enabled implicitly. Eight tests add to all prior checks; old headers and 68 existing test/support files remain unchanged. The strict survey remains 214,521 (+0); v1/v2 remain 214,585 and v3 conditionally compiles 214,664 including 79 new SVC wrappers. No original function executes. Dynamic lifecycle/handles/waits, real target/TLS/PAC configuration and original startup remain open.

## Owned event primitive (October 27)

[EVENTS.md](EVENTS.md) describes paired manual-reset event state, endpoint rights, lifetime/quota management, monotonic handles and bounded polling. Five new tests add state/concurrency and O0/O2 fatal-UBSan native-boundary evidence. No production SVC mapping, numeric kernel result claim, blocking wait/clock/cancellation or scheduler is installed. Existing compiler counts and all previous tests/ABI contracts remain unchanged; original execution remains gated. Preserve the distinction between this nonempty poll helper and the kernel's valid zero-handle WaitSynchronization case.

## Explicit event SVC reference bindings (October 27)

[EVENT_SERVICES.md](EVENT_SERVICES.md) describes five opt-in reference-shaped event handlers with real owned state, typed W0/W1/W2 results and no default activation. Three O0/O2 fatal-UBSan native tests add budget, namespace, lifetime, error and concurrent-client evidence. These bindings are event-only; waits, clocks, cancellation, threads/process objects and actual retail target verification remain open. Compiler counts, prior tests/headers and original-execution gates remain unchanged.
