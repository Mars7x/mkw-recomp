# Runtime services

Synthetic translated callers now invoke five explicit read-only filesystem APIs and an opt-in five-operation event SVC reference profile through checked native boundaries. No original game service call has executed, and no unsupported service receives a fabricated success result.

The private NPDM manifest records declared service permissions and kernel capability words. The import map records unresolved symbols and binding/type information. These are requirements evidence, not a call trace. Static SVC opcode locations are diagnostic observations; linear scanning can include data.

The runtime implements owned mappings/BSS, permissions, scoped relocations, transactional transfers, a serialized plain/ordered/atomic memory bridge and a registered native-call dispatcher with iterative tail routing, explicit diagnostic data barriers and a bounded FP control/arithmetic profile for synthetic tests. Explicit guest software thread-register storage now exists, but actual TLS templates/allocation and guest thread scheduling do not. No real startup ABI is connected. The filesystem profile uses an explicitly preconfigured read-only mount, not an emulated MountRom call. Process entry, rtld startup, constructors, destructors, exception delivery, TLS, scheduling, full wait/event services, IPC, clocks, other SDK services, saves, audio, input, graphics and networking remain unimplemented.

Load-time scopes and relocation readback now pass. Before bootstrap, implement the startup ABI and verify the actual PAC feature profile. Explicit authenticated-return lowering exists with a diagnostic backend only. Keep the original SDK execution path available where practical; introduce native implementations only after identifying actual requirements. Offline gameplay and same-device multiplayer retain priority over online services.

## Pinned upstream reuse and bounded runtime integration (October 27)

See [UPSTREAM_REUSE.md](UPSTREAM_REUSE.md) and upstream-reuse-audit.json for pinned nxnative/LASRecomp/nx2android source findings, licenses, portability costs and ranked execution/frame opportunities. The selected nxnative-derived five-API read-only filesystem profile now uses checked host targets on the coherent v10/table-v2 router. Five exact supplied provider descriptors cover ten references; a synthetic native wrapper reads 1,342 supplied RomFS bytes matching an independent stream. This is opt-in service transport, not proof of the retail SDK object layout or original game startup. Six call, four filesystem and two hook tests add to existing checks; no original code executes. Default survey gain stays zero (214,521); conditional v1/v2 totals stay 214,585. Prior acceptance and correctness checks remain required. Runtime configuration, thread/TLS ownership and Switch 2 graphics/shader identification now take priority over further survey-only work.

## Scoped translated workers (October 27)

[THREADING.md](THREADING.md) describes the new predeclared native worker batch: checked explicit stack/TLS/TLR ranges, owner reservation before launch, per-thread coherent frames/router contexts, scoped library lifetimes, launch-abort cleanup and joins with architectural snapshots. Six added tests pass debug/release, including O0/O2 fatal-UBSan native fixtures, 16,000 shared LL/SC increments per optimization and 24 serial QEMU comparisons per optimization. The default survey remains 214,521; conditional v1/v2 counts remain unchanged. This does not emulate dynamic Horizon thread creation/start, waits, priorities or scheduling, initialize SDK TLS, authorize original execution or install an actual target/PAC/FP profile. Existing tests, acceptance and privacy protections remain required.

## Explicit service-table-v3 transport (October 27)

See [SVC.md](SVC.md): unchanged CPU-v10 layout, explicit typed SVC callback and X0–X7 W/X reply packets, checked atomic memory outputs, completion-time local-monitor clearing, terminal thread-exit propagation and scoped worker attachment. No Horizon service is enabled implicitly. Eight tests add to all prior checks; old headers and 68 existing test/support files remain unchanged. The strict survey remains 214,521 (+0); v1/v2 remain 214,585 and v3 conditionally compiles 214,664 including 79 new SVC wrappers. No original function executes. Dynamic lifecycle/handles/waits, real target/TLS/PAC configuration and original startup remain open.

## Owned event primitive (October 27)

[EVENTS.md](EVENTS.md) describes paired manual-reset event state, endpoint rights, lifetime/quota management, monotonic handles and bounded polling. Five new tests add state/concurrency and O0/O2 fatal-UBSan native-boundary evidence. No production SVC mapping, numeric kernel result claim, blocking wait/clock/cancellation or scheduler is installed. Existing compiler counts and all previous tests/ABI contracts remain unchanged; original execution remains gated. Preserve the distinction between this nonempty poll helper and the kernel's valid zero-handle WaitSynchronization case.

## Explicit event SVC reference bindings (October 27)

[EVENT_SERVICES.md](EVENT_SERVICES.md) describes five opt-in reference-shaped event handlers with real owned state, typed W0/W1/W2 results and no default activation. Three O0/O2 fatal-UBSan native tests add budget, namespace, lifetime, error and concurrent-client evidence. These bindings are event-only; waits, clocks, cancellation, threads/process objects and actual retail target verification remain open. Compiler counts, prior tests/headers and original-execution gates remain unchanged.
