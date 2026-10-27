# Concrete dependency order

1. Verify the actual PAC/address regime and bind native registration to exact module identity/executable permissions before executing original functions. Scoped relocation readback and synthetic checked-memory/native dispatch now pass; rtld startup, final RelRo and TLS remain incomplete.
2. Extend lowering from the refreshed census: FP conversions, remaining arithmetic/unary/vector operations and control profiles, plus scalar bit operations. Basic S/D arithmetic/compare/FMADD, legacy FP state, H/S/D FCSEL and normal-memory DMB/DSB now pass diagnostic tests. Raw SIMD register bits and scalar LSE RMW/CAS now pass native diagnostic tests. BR/tail routing, CCMP/CCMN and integer multiply/divide are implemented; complete indirect-target discovery/binding remains open. Common scalar/SIMD memory forms and integer logical/bitfield operations are implemented. RCpc/RCsc acquire/release tags, serialized diagnostic memory, TBZ/TBNZ, BLR, shifted arithmetic and conditional selects are implemented. Actual weak-memory equivalence is not established.
3. Evaluate/adapt the MIT LAS portable lifter behind the checked memory/CPU ABI. Retain native semantic tests and reject unsupported PAC/FP/SIMD/atomic behavior. Add authenticated-return and guest-memory instruction tests before executing game functions.
4. Generate multiple private native translation units tied to exact module identities. Compile and measure; complete translation coverage is a separate gate from startup.
5. Implement the observed startup, module initialization, TLS/thread and required service path, with explicit unsupported diagnostics. Observe repeatable real initialization milestones.
6. Trace real graphics bootstrap/submission and shader payloads. Establish Ampere semantics, texture layouts and synchronization before Vulkan draws. Validate a genuine game-generated frame.
7. Integrate SDL3 input/audio, VFS and saves, then verify title/menu progression and a playable race. Expand modes only from observed behavior.
8. Profile correct gameplay before optimization, enhancements or Flatpak packaging.

The long-term acceptance criteria remain the user's original phases. The foundation does not relax them or replace the original game with a recreation.

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
